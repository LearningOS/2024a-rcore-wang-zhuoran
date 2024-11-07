//! Process management syscalls
//!
use alloc::sync::Arc;
use core::mem::{self, size_of};
use crate::{
    config::{PAGE_SIZE, BIG_STRIDE, MAX_SYSCALL_NUM},
    fs::{open_file, OpenFlags},
    mm::{translated_refmut, translated_str, MapPermission, VPNRange, VirtAddr, 
        translated_byte_buffer},
    task::{
        add_task, current_task, current_user_token, exit_current_and_run_next,
        suspend_current_and_run_next, TaskStatus, TaskControlBlock,
    },
    timer::get_time_us
};

#[repr(C)]
#[derive(Debug)]
/// time value
pub struct TimeVal {
    /// time in second
    pub sec: usize,
    /// time in microsecond
    pub usec: usize,
}

/// Task information
#[allow(dead_code)]
#[derive(Clone, Copy)]
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
/// exit current process
pub fn sys_exit(exit_code: i32) -> ! {
    trace!("kernel:pid[{}] sys_exit", current_task().unwrap().pid.0);
    exit_current_and_run_next(exit_code);
    panic!("Unreachable in sys_exit!");
}
/// yield the current process
pub fn sys_yield() -> isize {
    //trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}
/// get current process id
pub fn sys_getpid() -> isize {
    trace!("kernel: sys_getpid pid:{}", current_task().unwrap().pid.0);
    current_task().unwrap().pid.0 as isize
}
/// fork current process
pub fn sys_fork() -> isize {
    trace!("kernel:pid[{}] sys_fork", current_task().unwrap().pid.0);
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
/// execute a new program
pub fn sys_exec(path: *const u8) -> isize {
    trace!("kernel:pid[{}] sys_exec", current_task().unwrap().pid.0);
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(app_inode) = open_file(path.as_str(), OpenFlags::RDONLY) {
        let all_data = app_inode.read_all();
        let task = current_task().unwrap();
        task.exec(all_data.as_slice());
        0
    } else {
        -1
    }
}

/// If there is not a child process whose pid is same as given, return -1.
/// Else if there is a child process but it is still running, return -2.
pub fn sys_waitpid(pid: isize, exit_code_ptr: *mut i32) -> isize {
    //trace!("kernel: sys_waitpid");
    let task = current_task().unwrap();
    // find a child process

    // ---- access current PCB exclusively
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
        // ++++ temporarily access child PCB exclusively
        p.inner_exclusive_access().is_zombie() && (pid == -1 || pid as usize == p.getpid())
        // ++++ release child PCB
    });
    if let Some((idx, _)) = pair {
        let child = inner.children.remove(idx);
        // confirm that child will be deallocated after being removed from children list
        assert_eq!(Arc::strong_count(&child), 1);
        let found_pid = child.getpid();
        // ++++ temporarily access child PCB exclusively
        let exit_code = child.inner_exclusive_access().exit_code;
        // ++++ release child PCB
        *translated_refmut(inner.memory_set.token(), exit_code_ptr) = exit_code;
        found_pid as isize
    } else {
        -2
    }
    // ---- release current PCB automatically
}

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(_ts: *mut TimeVal, _tz: usize) -> isize {
    trace!(
        "kernel:pid[{}] sys_get_time NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
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
    trace!(
        "kernel:pid[{}] sys_task_info NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    let mut buffer = translated_byte_buffer(current_user_token(), _ti as *const u8, core::mem::size_of::<TaskInfo>());
    let task_control_block = current_task().unwrap();
    let task_info = task_control_block.task_info_exclusive_access();
    let syscall_times = task_info.get_syscall_times();
    let time_us = task_info.get_time();
    let time = ((time_us / 1_000_000) & 0xffff) * 1000 + (time_us % 1_000_000 ) / 1000;
    let status = TaskStatus::Running;
    let task_info = TaskInfo {
        status,
        syscall_times,
        time,
    };
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
    trace!(
        "kernel:pid[{}] sys_mmap NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
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
    // mmap(start, end, permission)
    let task_control_block = current_task().unwrap();
    let mut inner = task_control_block.inner_exclusive_access();
    let start_va = VirtAddr(start);
    let end_va = VirtAddr(end);
    let vpnrange = VPNRange::new(start_va.floor(),end_va.ceil());
    for vpn in vpnrange {
        // 判断是否有pte已经被映射了，如果是，则返回错误
        if let Some(pte) = inner.memory_set.translate(vpn) {
            if pte.is_valid() {
                drop(inner);
                return -1;
            }
        } else {
            continue;
        }
    }
    // 将新的映射插入到memory_set中
    inner.memory_set.insert_framed_area(start_va, end_va, permission);
    for vpn in vpnrange {
        // 判断pte的映射是否成功（判断物理内存是否充足）
        if let Some(pte) = inner.memory_set.translate(vpn) {
            if pte.is_valid() == false {
                drop(inner);
                return -1;
            } 
        } else {
            drop(inner);
            return -1;
        }
    }
    drop(inner);
    0 
}

/// YOUR JOB: Implement munmap.
pub fn sys_munmap(start: usize, len: usize) -> isize {
    trace!(
        "kernel:pid[{}] sys_munmap NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    if start % PAGE_SIZE != 0 {
        return -1;
    }
    let end = start + len;
    let task_control_block = current_task().unwrap();
    // let current = inner.current_task;
    // let task_control_block = &mut inner.tasks[current];
    let mut inner = task_control_block.inner_exclusive_access();
    let start_va = VirtAddr(start);
    let end_va = VirtAddr(end);
    let vpnrange = VPNRange::new(start_va.floor(),end_va.ceil());
    for vpn in vpnrange {
        // 判断是否有pte已经被映射了，如果否，则返回错误
        if let Some(pte) = inner.memory_set.translate(vpn) {
            if pte.is_valid() == false {
                drop(inner);
                return -1;
            }   
        } else {
            drop(inner);
            return -1;
        }
    }
    if inner.memory_set.remove(start_va, start_va) == -1 {
        drop(inner);
        return -1;
    }
    drop(inner);
    0
}

/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel:pid[{}] sys_sbrk", current_task().unwrap().pid.0);
    if let Some(old_brk) = current_task().unwrap().change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}

/// YOUR JOB: Implement spawn.
/// HINT: fork + exec =/= spawn
pub fn sys_spawn(_path: *const u8) -> isize {
    trace!(
        "kernel:pid[{}] sys_spawn NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    // let token = current_user_token();
    // let path = translated_str(token, _path);
    // if let Some(app_inode) = open_file(path.as_str(), OpenFlags::RDONLY) {
    //     let all_data = app_inode.read_all();
    //     let current_task = current_task().unwrap();
    //     let task:Arc<TaskControlBlock> = TaskControlBlock::new(all_data.as_slice()).into();
    //     // task.exec(all_data.as_slice());
    //     let mut parent_inner = current_task.inner_exclusive_access();
    //     parent_inner.children.push(task.clone());
    //     let new_pid = task.pid.0;
    //     add_task(task);
    //     drop(parent_inner);
    //     new_pid as isize
    // } else {
    //     -1
    // }
    // get the path of the new app
    let path = translated_str(current_user_token(), _path);
    if let Some(inode) = open_file(&path, OpenFlags::RDONLY) {
        let v: alloc::vec::Vec<u8> = inode.read_all();
        // get the app data(create a new task control block)
        let tcb=Arc::new(TaskControlBlock::new(v.as_slice()));
        let pid = tcb.getpid();
        // get the task control block of the current task(当前任务一定存在，不然不会有进程调用spawn)
        let current_tcb=current_task().unwrap();
        let mut inner =current_tcb.inner_exclusive_access();
        // add the new task to the children list of the current task
        inner.children.push(tcb.clone());
        drop(inner);
        let mut inner = tcb.inner_exclusive_access();
        inner.parent=Some(Arc::downgrade(&current_tcb));
        drop(inner);
        // for child in inner.children.iter() {
        //     println!("spwan====parent pid :{},child pid:{}",current_tcb.getpid(),child.getpid());
        // }
        // drop(current_tcb);
        // and add it to the task list（此时会发生所有权的转移，所以需要在之前就取出当前进程的pid）
        // println!("spwan====pid is {}, Num is {}",tcb.getpid(),Arc::strong_count(&tcb));
        add_task(tcb);
        pid as isize
    }else{
        -1
    }
}

/// YOUR JOB: Set task priority.
pub fn sys_set_priority(_prio: isize) -> isize {
    trace!(
        "kernel:pid[{}] sys_set_priority NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    // 设置当前进程优先级为 prio
    // 参数：prio 进程优先级，要求 prio >= 2
    // 返回值：如果输入合法则返回 prio，否则返回 -1
    if _prio < 2 {
        return -1;
    }
    let task = current_task().unwrap();
    let mut inner = task.inner_exclusive_access();
    inner.priority = _prio as usize;
    inner.pass = BIG_STRIDE / _prio as usize;
    drop(inner);
    _prio
}
