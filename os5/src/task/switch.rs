core::arch::global_asm!(include_str!("switch.S"));
use super::context::TaskContext;
extern "C"{
    pub fn __switch(
        cunrrent_task_cx_ptr: *mut TaskContext,
        next_task_cx_ptr:*const TaskContext);
}

