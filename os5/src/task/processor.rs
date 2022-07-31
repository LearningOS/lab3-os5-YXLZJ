//! Implementation of [`Processor`] and Intersection of control flow
//!
//! Here, the continuous operation of user apps in CPU is maintained,
//! the current running state of CPU is recorded,
//! and the replacement and transfer of control flow of different applications are executed.




use super::__switch;
use super::{fetch_task, TaskStatus};
use super::{TaskContext, TaskControlBlock};
use crate::config::MAX_SYSCALL_NUM;
use crate::mm::VirtAddr;
use crate::sync::UPSafeCell;
use crate::syscall;
use crate::timer::get_time_us;
use crate::trap::TrapContext;
use alloc::sync::Arc;
use lazy_static::*;

/// Processor management structure
pub struct Processor {
    /// The task currently executing on the current processor
    current: Option<Arc<TaskControlBlock>>,
    /// The basic control flow of each core, helping to select and switch process
    idle_task_cx: TaskContext,
}

impl Processor {
    pub fn new() -> Self {
        Self {
            current: None,
            idle_task_cx: TaskContext::zero_init(),
        }
    }
    fn get_idle_task_cx_ptr(&mut self) -> *mut TaskContext {
        &mut self.idle_task_cx as *mut _
    }
    pub fn take_current(&mut self) -> Option<Arc<TaskControlBlock>> {
        self.current.take()
    }
    pub fn current(&self) -> Option<Arc<TaskControlBlock>> {
        self.current.as_ref().map(|task| Arc::clone(task))
    }
    pub fn get_current_status(&mut self) ->TaskStatus{
        let pcb = self.current().unwrap();
        let inner=pcb.inner_exclusive_access();
        inner.task_status
    }
    pub fn set_task_priority(&mut self,prio:usize){
        let pcb = self.current().unwrap();
        let mut inner = pcb.inner_exclusive_access();
        inner.task_priority = prio;
    }
    pub fn get_already_time(&mut self)->usize{
        let now = get_time_us();
        let pcb = self.current().unwrap();
        let inner = pcb.inner_exclusive_access();
        now - inner.start_time
    }
    pub fn get_syscall_times(&mut self)->[u32;MAX_SYSCALL_NUM]{
        let pcb = self.current().unwrap();
        let inner = pcb.inner_exclusive_access();
        inner.syscall_times
    }
    pub fn syscall_add(&mut self,syscall_id:usize){
        let pcb = self.current().unwrap();
        let mut inner =pcb.inner_exclusive_access();
        inner.syscall_times[syscall_id]+=1
    }
    pub fn mmap(&mut self,_start:usize,_len:usize,_port:usize)->isize{
        let pcb = self.current().unwrap();
        let mut inner =pcb.inner_exclusive_access();
        inner.memory_set.mmap(_start, _len, _port)
    }
    pub fn munmap(&mut self,_start:usize,_len:usize)->isize{
        let pcb = self.current().unwrap();
        let mut inner =pcb.inner_exclusive_access();
        inner.memory_set.munmap(_start, _len)
    }
}

lazy_static! {
    /// PROCESSOR instance through lazy_static!
    pub static ref PROCESSOR: UPSafeCell<Processor> = unsafe { UPSafeCell::new(Processor::new()) };
}

/// The main part of process execution and scheduling
///
/// Loop fetch_task to get the process that needs to run,
/// and switch the process through __switch
pub fn run_tasks() {
    loop {
        let mut processor = PROCESSOR.exclusive_access();
        if let Some(task) = fetch_task() {
            let idle_task_cx_ptr = processor.get_idle_task_cx_ptr();
            // access coming task TCB exclusively
            let mut task_inner = task.inner_exclusive_access();
            let next_task_cx_ptr = &task_inner.task_cx as *const TaskContext;
            task_inner.task_status = TaskStatus::Running;
            drop(task_inner);
            // release coming task TCB manually
            processor.current = Some(task);
            // release processor manually
            drop(processor);
            unsafe {
                __switch(idle_task_cx_ptr, next_task_cx_ptr);
            }
        }
    }
}

/// Get current task through take, leaving a None in its place
pub fn take_current_task() -> Option<Arc<TaskControlBlock>> {
    PROCESSOR.exclusive_access().take_current()
}

/// Get a copy of the current task
pub fn current_task() -> Option<Arc<TaskControlBlock>> {
    PROCESSOR.exclusive_access().current()
}

/// Get token of the address space of current task
pub fn current_user_token() -> usize {
    let task = current_task().unwrap();
    let token = task.inner_exclusive_access().get_user_token();
    token
}

/// Get the mutable reference to trap context of current task
pub fn current_trap_cx() -> &'static mut TrapContext {
    current_task()
        .unwrap()
        .inner_exclusive_access()
        .get_trap_cx()
}

/// Return to idle control flow for new scheduling
pub fn schedule(switched_task_cx_ptr: *mut TaskContext) {
    let mut processor = PROCESSOR.exclusive_access();
    let idle_task_cx_ptr = processor.get_idle_task_cx_ptr();
    drop(processor);
    unsafe {
        __switch(switched_task_cx_ptr, idle_task_cx_ptr);
    }
}

pub fn get_current_status()->TaskStatus{
    let mut processor = PROCESSOR.exclusive_access();
    processor.get_current_status()
}

pub fn set_task_priority(prio:usize){
    let mut processor = PROCESSOR.exclusive_access();
    processor.set_task_priority(prio)
}

pub fn get_already_time()->usize{
    let mut processor = PROCESSOR.exclusive_access();
    processor.get_already_time()
}

pub fn syscall_add(syscall_id:usize){
    let mut processor = PROCESSOR.exclusive_access();
    processor.syscall_add(syscall_id)
}

pub fn get_syscall_times()->[u32;MAX_SYSCALL_NUM]{
    let mut processor = PROCESSOR.exclusive_access();
    processor.get_syscall_times()
}

pub fn mmap(_start:usize,_len:usize,_port:usize)->isize{
    let mut processor = PROCESSOR.exclusive_access();
    processor.mmap(_start, _len, _port)
}

pub fn munmap(_start:usize,_len:usize)->isize{
    let mut processor = PROCESSOR.exclusive_access();
    processor.munmap(_start, _len)
}