use alloc::sync::Arc;

use crate::config:: MAX_SYSCALL_NUM;
use crate::loader::get_app_data_by_name;
use crate::mm::{translated_str, translated_refmut, VirtAddr,PhysAddr,PageTable};
use crate::task::processor::{set_task_priority, get_current_status, get_already_run_time, get_syscall_times, current_task, current_user_token, mmap, munmap};
use crate::task::{exit_current_and_run_next, suspend_current_and_run_next, add_task, TaskControlBlock};
use crate::timer::get_time_us;
use crate::task::task::TaskStatus;

pub fn sys_yield() -> isize {
    suspend_current_and_run_next();
    0
}

pub fn sys_exit(exit_code: i32) -> ! {
    exit_current_and_run_next(exit_code);
    panic!("Unreachable in sys_exit!");
}

pub fn get_physical_address(token:usize,ptr:*const u8) ->usize{
    let page_table= PageTable::from_token(token);
    let va = VirtAddr::from(ptr as usize);
    let ppn = page_table.find_pte(va.floor()).unwrap().ppn();
    PhysAddr::from(ppn).0 +va.page_offset() 
}

pub fn sys_get_time(_ts: *mut TimeVal, _tz: usize) -> isize {
    let us = get_time_us();
    let real = get_physical_address(current_user_token(), _ts as *const u8) as *mut TimeVal;
    unsafe {
        *real = TimeVal {
            sec: us / 1_000_000,
            usec: us % 1_000_000,
        };
    }
    0
}

pub fn sys_getpid() -> isize {
    current_task().unwrap().pid.0 as isize
}

pub fn sys_task_info(ti: *mut TaskInfo) -> isize {
    let real = get_physical_address(current_user_token(), ti as *const u8) as *mut TaskInfo;
    unsafe {
        *real = TaskInfo{
            status:get_current_status(),
            syscall_times:get_syscall_times(),
            time:get_already_run_time()/1000,
        }
    }
    0
}

pub fn sys_fork() -> isize {
    let current_task = current_task().unwrap();
    let new_task = current_task.fork();
    let new_pid = new_task.pid.0;
    // modify trap context of new_task, because it returns immediately after switching
    let trap_cx = new_task.inner_exclusive_access().get_trap_cx();
    // we do not have to move to next instruction since we have done it before
    // for child process, fork returns 0
   trap_cx.x[10] = 0;
   // add new task to scheduler
   add_task(new_task);
   new_pid as isize
}

pub fn sys_exec(path: *const u8) -> isize {
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(data) = get_app_data_by_name(path.as_str()) {
        let task = current_task().unwrap();
        task.exec(data);
        0
    } else {
        -1
    }
}

pub fn sys_waitpid(pid: isize, exit_code_ptr: *mut i32) -> isize {
    let task = current_task().unwrap();
    // find a child process

    // ---- access current TCB exclusively
    let mut inner = task.inner_exclusive_access();
    if !inner
        .children
        .iter()
        .any(|p| pid == -1 || pid as usize == p.getpid())
    {
        return -1;
        // ---- release current PCB
    }
    let pair = inner.children.iter().enumerate().find(|(_, p)| {
        // ++++ temporarily access child PCB lock exclusively
        p.inner_exclusive_access().is_zombie() && (pid == -1 || pid as usize == p.getpid())
        // ++++ release child PCB
    });
    if let Some((idx, _)) = pair {
        let child = inner.children.remove(idx);
        // confirm that child will be deallocated after removing from children list
        assert_eq!(Arc::strong_count(&child), 1);
        let found_pid = child.getpid();
        // ++++ temporarily access child TCB exclusively
        let exit_code = child.inner_exclusive_access().exit_code;
        // ++++ release child PCB
        *translated_refmut(inner.memory_set.token(), exit_code_ptr) = exit_code;
        found_pid as isize
    } else {
        -2
    }
    // ---- release current PCB lock automatically
}

pub fn sys_spawn(_path:*const u8) ->isize{
    let token = current_user_token();
    let path = translated_str(token, _path);
    if let Some(data) = get_app_data_by_name(path.as_str()){
        let new_task:Arc<TaskControlBlock>= Arc::new(TaskControlBlock::new(data));
        let mut new_inner = new_task.inner_exclusive_access();
        let parent = current_task().unwrap();
        let mut parent_inner = parent.inner_exclusive_access();
        new_inner.children.push(new_task.clone());
        drop(new_inner);
        drop(parent_inner);
        let new_pid = new_task.pid.0;
        add_task(new_task);
        new_pid as isize
    } else {
        -1
    }
}

pub fn sys_set_priority(prio:isize)->isize{
    if prio<2{
        return -1;
    }
    set_task_priority(prio as usize);
    prio as isize
}

pub fn sys_mmap(_start: usize, _len: usize, _port: usize) -> isize {
    if (_port&!0x7 !=0)||(_port&0x7==0){
        return -1
    }
    let va = VirtAddr::from(_start);
    if va.aligned(){
        
        return mmap(_start, _len, _port);
    }else{
        return -1; 
    }
}

pub fn sys_munmap(_start: usize, _len: usize) -> isize {
    let va = VirtAddr::from(_start);
    if va.aligned(){
        return munmap(_start, _len);
    }else{
        return -1;
    }
}

#[repr(C)]
pub struct TimeVal{
    pub sec:usize,
    pub usec:usize,
}

pub struct TaskInfo {
    status: TaskStatus,
    syscall_times: [u32; MAX_SYSCALL_NUM],
    time: usize,
}