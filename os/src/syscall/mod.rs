//! Implementation of syscalls
//!
//! The single entry point to all system calls, [`syscall()`], is called
//! whenever userspace wishes to perform a system call using the `ecall`
//! instruction. In this case, the processor raises an 'Environment call from
//! U-mode' exception, which is handled as one of the cases in
//! [`crate::trap::trap_handler`].
//!
//! For clarity, each single syscall is implemented as its own function, named
//! `sys_` then the name of the syscall. You can find functions like this in
//! submodules, and you should also implement syscalls this way.

/// write syscall
const SYSCALL_WRITE: usize = 64;
/// exit syscall
const SYSCALL_EXIT: usize = 93;
/// yield syscall
const SYSCALL_YIELD: usize = 124;
/// gettime syscall
const SYSCALL_GET_TIME: usize = 169;
/// taskinfo syscall
const SYSCALL_TASK_INFO: usize = 410;

mod fs;
mod process;

use fs::*;
use lazy_static::*;
pub use process::*;
use crate::{config::MAX_APP_NUM, sync::UPSafeCell, task::TASK_MANAGER, timer::get_time_ms};

lazy_static! {
    /// Global variable: initial schedule time of each task
    pub static ref INIT_SCHEDULE_TIME: UPSafeCell<[usize; MAX_APP_NUM]> = unsafe { 
        UPSafeCell::new([0usize; MAX_APP_NUM]) 
    };
}




/// handle syscall exception with `syscall_id` and other arguments
pub fn syscall(syscall_id: usize, args: [usize; 3]) -> isize {
    // let current_tid = TASK_MANAGER.get_current_task();
    
    let init_schedule_time = INIT_SCHEDULE_TIME.exclusive_access();
    // init_schedule_time里存的时间应该是任务第一次被调度的时间，所以这里应该用当前时间减去任务第一次被调度的时间
    TASK_MANAGER.set_current_task_time(get_time_ms() - init_schedule_time[TASK_MANAGER.get_current_task()]);
    TASK_MANAGER.add_syscall_times(syscall_id as u32);
    match syscall_id {
        SYSCALL_WRITE => sys_write(args[0], args[1] as *const u8, args[2]),
        SYSCALL_EXIT => sys_exit(args[0] as i32),
        SYSCALL_YIELD => sys_yield(),
        SYSCALL_GET_TIME => sys_get_time(args[0] as *mut TimeVal, args[1]),
        SYSCALL_TASK_INFO => sys_task_info(args[0] as *mut TaskInfo),
        _ => panic!("Unsupported syscall_id: {}", syscall_id),
    }
}
