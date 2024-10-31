//!Implementation of [`TaskManager`]
use super::TaskControlBlock;
use crate::config::BIG_STRIDE;
use crate::sync::UPSafeCell;
use alloc::collections::VecDeque;
use alloc::sync::Arc;
use lazy_static::*;
use crate::task::TaskStatus;
///A array of `TaskControlBlock` that is thread-safe
pub struct TaskManager {
    ready_queue: VecDeque<Arc<TaskControlBlock>>,
}

/// A simple FIFO scheduler.
impl TaskManager {
    ///Creat an empty TaskManager
    pub fn new() -> Self {
        Self {
            ready_queue: VecDeque::new(),
        }
    }
    /// Add process back to ready queue
    pub fn add(&mut self, task: Arc<TaskControlBlock>) {
        self.ready_queue.push_back(task);
    }
    /// Take a process out of the ready queue
    pub fn fetch(&mut self) -> Option<Arc<TaskControlBlock>> {
        // self.ready_queue.pop_front()
        // 找到ready_queue中状态为Ready且优先级最高的进程（stride 最小）
        let mut index = 0;
        let mut min_stride = usize::MAX;
        for i in 0..self.ready_queue.len() {
            let inner = self.ready_queue[i].inner_exclusive_access();
            let stride = inner.stride;
            if inner.task_status == TaskStatus::Ready {
                if stride < min_stride {
                    min_stride = stride;
                    index = i;
                }
            }
        }
        if min_stride == usize::MAX {
            return None;
        }
        if let Some(task) = self.ready_queue.get(index) {
            let mut inner = task.inner_exclusive_access();
            let pass =  BIG_STRIDE / inner.priority;
            inner.pass = pass;
            inner.stride += pass;
            drop(inner);
        }
        self.ready_queue.remove(index)
    }
}

lazy_static! {
    /// TASK_MANAGER instance through lazy_static!
    pub static ref TASK_MANAGER: UPSafeCell<TaskManager> =
        unsafe { UPSafeCell::new(TaskManager::new()) };
}

/// Add process to ready queue
pub fn add_task(task: Arc<TaskControlBlock>) {
    //trace!("kernel: TaskManager::add_task");
    TASK_MANAGER.exclusive_access().add(task);
}

/// Take a process out of the ready queue
pub fn fetch_task() -> Option<Arc<TaskControlBlock>> {
    //trace!("kernel: TaskManager::fetch_task");
    TASK_MANAGER.exclusive_access().fetch()
}
