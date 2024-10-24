//! Task management implementation
//!
//! Everything about task management, like starting and switching tasks is
//! implemented here.
//!
//! A single global instance of [`TaskManager`] called `TASK_MANAGER` controls
//! all the tasks in the operating system.
//!
//! Be careful when you see `__switch` ASM function in `switch.S`. Control flow around this function
//! might not be what you expect.

mod context;
mod switch;
#[allow(clippy::module_inception)]
mod task;
use mm::address::VPNRange;
use crate::config::MAX_SYSCALL_NUM;
use crate::loader::{get_app_data, get_num_app};
use crate::mm::{self, MapPermission};
use crate::sync::UPSafeCell;
use crate::syscall::INIT_SCHEDULE_TIME;
use crate::timer::get_time_us;
use crate::trap::TrapContext;
use alloc::vec::Vec;
use lazy_static::*;
use switch::__switch;
pub use task::{TaskControlBlock, TaskStatus};
use crate::mm::VirtAddr;
pub use context::TaskContext;

/// The task manager, where all the tasks are managed.
///
/// Functions implemented on `TaskManager` deals with all task state transitions
/// and task context switching. For convenience, you can find wrappers around it
/// in the module level.
///
/// Most of `TaskManager` are hidden behind the field `inner`, to defer
/// borrowing checks to runtime. You can see examples on how to use `inner` in
/// existing functions on `TaskManager`.
pub struct TaskManager {
    /// total number of tasks
    num_app: usize,
    /// use inner value to get mutable access
    inner: UPSafeCell<TaskManagerInner>,
}

/// The task manager inner in 'UPSafeCell'
struct TaskManagerInner {
    /// task list
    tasks: Vec<TaskControlBlock>,
    /// id of current `Running` task
    current_task: usize,
}

lazy_static! {
    /// a `TaskManager` global instance through lazy_static!
    pub static ref TASK_MANAGER: TaskManager = {
        println!("init TASK_MANAGER");
        let num_app = get_num_app();
        println!("num_app = {}", num_app);
        let mut tasks: Vec<TaskControlBlock> = Vec::new();
        for i in 0..num_app {
            tasks.push(TaskControlBlock::new(get_app_data(i), i));
        }
        TaskManager {
            num_app,
            inner: unsafe {
                UPSafeCell::new(TaskManagerInner {
                    tasks,
                    current_task: 0,
                })
            },
        }
    };
}

impl TaskManager {
    /// Run the first task in task list.
    ///
    /// Generally, the first task in task list is an idle task (we call it zero process later).
    /// But in ch4, we load apps statically, so the first task is a real app.
    fn run_first_task(&self) -> ! {
        let mut inner = self.inner.exclusive_access();
        let next_task = &mut inner.tasks[0];
        next_task.task_status = TaskStatus::Running;
        let next_task_cx_ptr = &next_task.task_cx as *const TaskContext;
        let mut init_schedule_time = INIT_SCHEDULE_TIME.exclusive_access();
        init_schedule_time[0] = get_time_us() as usize;
        drop(init_schedule_time);
        drop(inner);
        let mut _unused = TaskContext::zero_init();
        // before this, we should drop local variables that must be dropped manually
        unsafe {
            __switch(&mut _unused as *mut _, next_task_cx_ptr);
        }
        panic!("unreachable in run_first_task!");
    }

    /// Change the status of current `Running` task into `Ready`.
    fn mark_current_suspended(&self) {
        let mut inner = self.inner.exclusive_access();
        let cur = inner.current_task;
        inner.tasks[cur].task_status = TaskStatus::Ready;
    }

    /// Change the status of current `Running` task into `Exited`.
    fn mark_current_exited(&self) {
        let mut inner = self.inner.exclusive_access();
        let cur = inner.current_task;
        inner.tasks[cur].task_status = TaskStatus::Exited;
    }

    /// Find next task to run and return task id.
    ///
    /// In this case, we only return the first `Ready` task in task list.
    fn find_next_task(&self) -> Option<usize> {
        let inner = self.inner.exclusive_access();
        let current = inner.current_task;
        (current + 1..current + self.num_app + 1)
            .map(|id| id % self.num_app)
            .find(|id| inner.tasks[*id].task_status == TaskStatus::Ready)
    }

    /// Get the current 'Running' task's token.
    fn get_current_token(&self) -> usize {
        let inner = self.inner.exclusive_access();
        inner.tasks[inner.current_task].get_user_token()
    }

    /// Get the current 'Running' task's trap contexts.
    fn get_current_trap_cx(&self) -> &'static mut TrapContext {
        let inner = self.inner.exclusive_access();
        inner.tasks[inner.current_task].get_trap_cx()
    }

    /// Change the current 'Running' task's program break
    pub fn change_current_program_brk(&self, size: i32) -> Option<usize> {
        let mut inner = self.inner.exclusive_access();
        let cur = inner.current_task;
        inner.tasks[cur].change_program_brk(size)
    }

    /// Switch current `Running` task to the task we have found,
    /// or there is no `Ready` task and we can exit with all applications completed
    fn run_next_task(&self) {
        if let Some(next) = self.find_next_task() {
            let mut inner = self.inner.exclusive_access();
            let current = inner.current_task;
            inner.tasks[next].task_status = TaskStatus::Running;
            inner.current_task = next;
            let current_task_cx_ptr = &mut inner.tasks[current].task_cx as *mut TaskContext;
            let next_task_cx_ptr = &inner.tasks[next].task_cx as *const TaskContext;
            let mut init_schedule_time = INIT_SCHEDULE_TIME.exclusive_access();
            if init_schedule_time[next] == 0usize {
                init_schedule_time[next] = get_time_us() as usize;
            }
            // init_schedule_time[next] = get_time_ms() as usize;
            drop(init_schedule_time);
            drop(inner);
            // before this, we should drop local variables that must be dropped manually
            unsafe {
                __switch(current_task_cx_ptr, next_task_cx_ptr);
            }
            // go back to user mode
        } else {
            panic!("All applications completed!");
        }
    }

        /// get current task id
        pub fn get_current_task(&self) -> usize {
            // self.inner.exclusive_access().current_task
            let inner = self.inner.exclusive_access();
            let current_task_id = inner.current_task;
            drop(inner);
            current_task_id
        }
    
        /// get current syscall times of current task
        pub fn get_syscall_times(&self) -> [u32; MAX_SYSCALL_NUM] {
            // self.inner.exclusive_access().tasks[self.get_current_task()].task_info.get_syscall_times()
            let inner = self.inner.exclusive_access();
            let current_task_id = inner.current_task;
            let syscall_times = inner.tasks[current_task_id].task_info.get_syscall_times();
            drop(inner);
            syscall_times
        }
    
        /// Write the syscall times of current task to TaskInfo
        pub fn add_syscall_times(&self, syscall_id: u32) {
            let mut inner = self.inner.exclusive_access();
            let current = inner.current_task;
            inner.tasks[current].task_info.add_syscall_times(syscall_id as usize);
            drop(inner);
        }
        /// Write the running status of current task to TaskInfo
        pub fn write_running_status(&self) {
            let mut inner = self.inner.exclusive_access();
            let current = inner.current_task;
            inner.tasks[current].task_info.set_status(TaskStatus::Running);
            drop(inner);
        }
    
        /// set current task time
        pub fn set_current_task_time(&self, time: usize) {
            let mut inner = self.inner.exclusive_access();
            let current = inner.current_task;
            inner.tasks[current].task_info.set_time(time);
            drop(inner);
        }
        /// Get the current task time
        // pub fn get_current_task_time(&self) -> usize {
        //     self.inner.exclusive_access().tasks[self.get_current_task()].task_info.get_time()
        // }
        pub fn get_current_task_time(&self) -> usize {
            let inner = self.inner.exclusive_access();
            let current_task_id = inner.current_task;
            let time = inner.tasks[current_task_id].task_info.get_time();
            drop(inner);
            time
        }
        /// map
        pub fn mmap(&self, start: usize, end: usize, permission: MapPermission) -> isize {
            let mut inner = self.inner.exclusive_access();
            let current = inner.current_task;
            let task_control_block = &mut inner.tasks[current];
            let start_va = VirtAddr(start);
            let end_va = VirtAddr(end);
            let vpnrange = VPNRange::new(start_va.floor(),end_va.ceil());
            for vpn in vpnrange {
                // 判断是否有pte已经被映射了，如果是，则返回错误
                if let Some(pte) = task_control_block.memory_set.translate(vpn) {
                    if pte.is_valid() {
                        return -1;
                    }
                } else {
                    continue;
                }
            }
            // 将新的映射插入到memory_set中
            task_control_block.memory_set.insert_framed_area(start_va, end_va, permission);
            for vpn in vpnrange {
                // 判断pte的映射是否成功（判断物理内存是否充足）
                if let Some(pte) = task_control_block.memory_set.translate(vpn) {
                    if pte.is_valid() == false {
                        return -1;
                    } 
                } else {
                    return -1;
                }
            }
            drop(inner);
            0
        }
        /// unmap
        pub fn munmap(&self, start: usize, end: usize) -> isize {
            let mut inner = self.inner.exclusive_access();
            let current = inner.current_task;
            let task_control_block = &mut inner.tasks[current];
            let start_va = VirtAddr(start);
            let end_va = VirtAddr(end);
            let vpnrange = VPNRange::new(start_va.floor(),end_va.ceil());
            for vpn in vpnrange {
                // 判断是否有pte已经被映射了，如果否，则返回错误
                if let Some(pte) = task_control_block.memory_set.translate(vpn) {
                    if pte.is_valid() == false {
                        return -1;
                    }   
                } else {
                    return -1;
                }
            }
            if task_control_block.memory_set.remove(start_va, start_va) == -1 {
                return -1;
            }
            drop(inner);
            0
        }
        
}

/// Run the first task in task list.
pub fn run_first_task() {
    TASK_MANAGER.run_first_task();
}

/// Switch current `Running` task to the task we have found,
/// or there is no `Ready` task and we can exit with all applications completed
fn run_next_task() {
    TASK_MANAGER.run_next_task();
}

/// Change the status of current `Running` task into `Ready`.
fn mark_current_suspended() {
    TASK_MANAGER.mark_current_suspended();
}

/// Change the status of current `Running` task into `Exited`.
fn mark_current_exited() {
    TASK_MANAGER.mark_current_exited();
}

/// Suspend the current 'Running' task and run the next task in task list.
pub fn suspend_current_and_run_next() {
    mark_current_suspended();
    run_next_task();
}

/// Exit the current 'Running' task and run the next task in task list.
pub fn exit_current_and_run_next() {
    mark_current_exited();
    run_next_task();
}

/// Get the current 'Running' task's token.
pub fn current_user_token() -> usize {
    TASK_MANAGER.get_current_token()
}

/// Get the current 'Running' task's trap contexts.
pub fn current_trap_cx() -> &'static mut TrapContext {
    TASK_MANAGER.get_current_trap_cx()
}

/// Change the current 'Running' task's program break
pub fn change_program_brk(size: i32) -> Option<usize> {
    TASK_MANAGER.change_current_program_brk(size)
}

/// Get the current task id
pub fn get_current_task() -> usize {
    TASK_MANAGER.get_current_task()
}

/// Add syscall times of current task
pub fn add_syscall_times(syscall_id: u32) {
    TASK_MANAGER.add_syscall_times(syscall_id);
}

/// Write the running status of current task
pub fn write_running_status() {
    TASK_MANAGER.write_running_status();
}

/// Set current task time
pub fn set_current_task_time(time: usize) {
    TASK_MANAGER.set_current_task_time(time);
}

/// Get current task time
pub fn get_current_task_time() -> usize {
    TASK_MANAGER.get_current_task_time()
}

/// mmap
pub fn mmap(start: usize, end: usize, permission: MapPermission) -> isize {
    TASK_MANAGER.mmap(start, end, permission)
}

/// munmap
pub fn munmap(start: usize, end: usize) -> isize {
    TASK_MANAGER.munmap(start, end)
}

