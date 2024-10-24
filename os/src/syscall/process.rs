//! Process management syscalls
use crate::{
    config::{MAX_SYSCALL_NUM, PAGE_SIZE}, mm::MapPermission, task::{
        change_program_brk, current_user_token, exit_current_and_run_next, mmap, munmap, suspend_current_and_run_next, TaskStatus, TASK_MANAGER
    }, timer::get_time_us
};
use crate::mm::translated_byte_buffer;
use core::mem::size_of;
use core::mem;
#[repr(C)]
#[derive(Debug)]
/// Time value
pub struct TimeVal {
    /// Second
    pub sec: usize,
    /// Microsecond
    pub usec: usize,
}

/// Task information
#[allow(dead_code)]
pub struct TaskInfo {
    /// Task status in it's life cycle
    status: TaskStatus,
    /// The numbers of syscall called by task
    syscall_times: [u32; MAX_SYSCALL_NUM],
    /// Total running time of task
    time: usize,
}

impl TaskInfo {
    /// Create a new TaskInfo
    pub fn new() -> Self {
        TaskInfo {
            status: TaskStatus::UnInit,
            syscall_times: [0; MAX_SYSCALL_NUM],
            time: 0,
        }
    }
    /// Set task status
    pub fn set_status(&mut self, status: TaskStatus) {
        self.status = status;
    }
    /// Set task running time
    pub fn set_time(&mut self, time: usize) {
        self.time = time;
    }
    /// Add syscall times
    pub fn add_syscall_times(&mut self, syscall_id: usize) {
        self.syscall_times[syscall_id] += 1;
    }

    /// get syscall times
    pub fn get_syscall_times(&self) -> [u32; MAX_SYSCALL_NUM] {
        self.syscall_times
    }

    /// set syscall times
    pub fn set_syscall_times(&mut self, syscall_times: [u32; MAX_SYSCALL_NUM]) {
        self.syscall_times = syscall_times;
    }

    /// get task time
    pub fn get_time(&self) -> usize {
        self.time
    }
}

/// task exits and submit an exit code
pub fn sys_exit(_exit_code: i32) -> ! {
    trace!("kernel: sys_exit");
    exit_current_and_run_next();
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(_ts: *mut TimeVal, _tz: usize) -> isize {
    trace!("kernel: sys_get_time");
    // 获取到实际物理地址，使得内核可以直接读写用户空间的数据
    let mut buffer = translated_byte_buffer(current_user_token(), _ts as *const u8, core::mem::size_of::<TimeVal>());
    // 考虑到 TimeVal 可能被分页，所以需要逐页拷贝
    let us = get_time_us();
    let time = TimeVal {
        sec: us / 1_000_000,
        usec: us % 1_000_000,
    };
    let time_bytes: [u8; mem::size_of::<TimeVal>()] = unsafe { mem::transmute(time) };
    
    if buffer.len() == 1 {
        // TimeVal 未被分页
        buffer[0].copy_from_slice(&time_bytes);
    } else if buffer[0].len() < 16 {
        // TimeVal 被分页, 逐页拷贝
        let len = buffer[0].len();
        buffer[0][..len].copy_from_slice(&time_bytes[..len]);
        buffer[1][..(16 - len)].copy_from_slice(&time_bytes[len..]);
    }
    0
}


/// YOUR JOB: Finish sys_task_info to pass testcases
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TaskInfo`] is splitted by two pages ?
pub fn sys_task_info(_ti: *mut TaskInfo) -> isize {
    trace!("kernel: sys_task_info NOT IMPLEMENTED YET!");
    
    let mut buffer = translated_byte_buffer(current_user_token(), _ti as *const u8, core::mem::size_of::<TaskInfo>());
    let syscall_times = TASK_MANAGER.get_syscall_times();
    let time_us = TASK_MANAGER.get_current_task_time();
    let time = ((time_us / 1_000_000) & 0xffff) * 1000 + (time_us % 1_000_000 ) / 1000;
    let status = TaskStatus::Running;
    let task_info = TaskInfo {
        status,
        syscall_times,
        time,
    };
    // 将 TaskInfo 结构体转换为字节数组写入buffer
    let task_info_byte: [u8; mem::size_of::<TaskInfo>()] = unsafe { mem::transmute(task_info) };
    if buffer[0].len() < size_of::<TaskInfo>() {
        let len = buffer[0].len();
        buffer[0].copy_from_slice(&task_info_byte[..len]);
        buffer[1][..(size_of::<TaskInfo>() - len)].copy_from_slice(&task_info_byte[len..]);
    } else {
        buffer[0][..size_of::<TaskInfo>()].copy_from_slice(&task_info_byte);
    }
    0
}

/// YOUR JOB: Implement mmap.
pub fn sys_mmap(start: usize, len: usize, port: usize) -> isize {
    trace!("kernel: sys_mmap NOT IMPLEMENTED YET!");
    // 先判断传入参数的正确性
    /*
    start 没有按页大小对齐
    port & !0x7 != 0 (port 其余位必须为0)
    port & 0x7 = 0 (这样的内存无意义)
    [start, start + len) 中存在已经被映射的页
    物理内存不足
     */
    if start % PAGE_SIZE != 0 || port & !0x7 != 0 || port & 0x7 == 0 {
        return -1;
    }
    let end = start + len;
    let permission = MapPermission::from_bits((port as u8) << 1).unwrap() | MapPermission::U;
    mmap(start, end, permission)
}


/// YOUR JOB: Implement munmap.
pub fn sys_munmap(start: usize, len: usize) -> isize {
    trace!("kernel: sys_munmap NOT IMPLEMENTED YET!");
    if start % PAGE_SIZE != 0 {
        return -1;
    }
    let end = start + len;
    munmap(start, end)
}
/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel: sys_sbrk");
    if let Some(old_brk) = change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}
