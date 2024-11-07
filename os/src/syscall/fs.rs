//! File and filesystem-related syscalls
use crate::fs::{open_file, OpenFlags, Stat};
use crate::mm::{translated_byte_buffer, translated_str, UserBuffer};
use crate::task::{current_task, current_user_token};
use core::mem;
pub fn sys_write(fd: usize, buf: *const u8, len: usize) -> isize {
    trace!("kernel:pid[{}] sys_write", current_task().unwrap().pid.0);
    let token = current_user_token();
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if let Some(file) = &inner.fd_table[fd] {
        if !file.writable() {
            return -1;
        }
        let file = file.clone();
        // release current task TCB manually to avoid multi-borrow
        drop(inner);
        file.write(UserBuffer::new(translated_byte_buffer(token, buf, len))) as isize
    } else {
        -1
    }
}

pub fn sys_read(fd: usize, buf: *const u8, len: usize) -> isize {
    trace!("kernel:pid[{}] sys_read", current_task().unwrap().pid.0);
    let token = current_user_token();
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if let Some(file) = &inner.fd_table[fd] {
        let file = file.clone();
        if !file.readable() {
            return -1;
        }
        // release current task TCB manually to avoid multi-borrow
        drop(inner);
        trace!("kernel: sys_read .. file.read");
        file.read(UserBuffer::new(translated_byte_buffer(token, buf, len))) as isize
    } else {
        -1
    }
}

pub fn sys_open(path: *const u8, flags: u32) -> isize {
    trace!("kernel:pid[{}] sys_open", current_task().unwrap().pid.0);
    let task = current_task().unwrap();
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(inode) = open_file(path.as_str(), OpenFlags::from_bits(flags).unwrap()) {
        let mut inner = task.inner_exclusive_access();
        let fd = inner.alloc_fd();
        inner.fd_table[fd] = Some(inode);
        fd as isize
    } else {
        -1
    }
}

pub fn sys_close(fd: usize) -> isize {
    trace!("kernel:pid[{}] sys_close", current_task().unwrap().pid.0);
    let task = current_task().unwrap();
    let mut inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if inner.fd_table[fd].is_none() {
        return -1;
    }
    inner.fd_table[fd].take();
    0
}

/// YOUR JOB: Implement fstat.
pub fn sys_fstat(_fd: usize, _st: *mut Stat) -> isize {
    trace!(
        "kernel:pid[{}] sys_fstat NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    /*
pub struct Stat {
    /// 文件所在磁盘驱动器号，该实验中写死为 0 即可
    pub dev: u64,
    /// inode 文件所在 inode 编号
    pub ino: u64,
    /// 文件类型
    pub mode: StatMode,
    /// 硬链接数量，初始为1
    pub nlink: u32,
    /// 无需考虑，为了兼容性设计
    pad: [u64; 7],
}
     */
    // 首先应检查fd是否合法，然后获取文件的inode，然后将inode的信息填充到st中
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    if _fd >= inner.fd_table.len() {
        return -1;
    }
    if _fd == 0 || _fd == 1 || _fd == 2 {
        return -1;
    }
    if inner.fd_table[_fd].is_none() {
        return -1;
    }
    let token = current_user_token();
    let current_task = current_task().unwrap();
    let inner = current_task.inner_exclusive_access();
    if let Some(file) = &inner.fd_table[_fd] {
        let file = file.clone();
        drop(inner);
        let mut buffer = translated_byte_buffer(token, _st as *const u8, core::mem::size_of::<Stat>());

        let stat = file.stat();

        // 考虑stat被分到多个page的情况
        let stat_bytes: [u8; mem::size_of::<Stat>()] = unsafe { core::mem::transmute(stat) };

        if buffer[0].len() < mem::size_of::<Stat>() {
            let len = buffer[0].len();
            buffer[0].copy_from_slice(&stat_bytes[..len]);
            buffer[1][..(mem::size_of::<Stat>() - len)].copy_from_slice(&stat_bytes[len..]);
        } else {
            buffer[0][..mem::size_of::<Stat>()].copy_from_slice(&stat_bytes);
        }
    } else {
        return -1;
    }
    
    0
}

/// YOUR JOB: Implement linkat.
pub fn sys_linkat(_old_name: *const u8, _new_name: *const u8) -> isize {
    trace!(
        "kernel:pid[{}] sys_linkat NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    // 首先检查错误，不能链接同名文件
    // let old_name = translated_str(current_user_token(), _old_name);
    // let new_name = translated_str(current_user_token(), _new_name);
    // if old_name == new_name || old_name.is_empty() || new_name.is_empty() {
    //     return -1;
    // }

    // linkat(&old_name, &new_name)

    //借鉴sys_open，我先得为_new_name创建一个文件
    let token = current_user_token();
    // 获取到新文件名
    let new_name = translated_str(token, _new_name);
    //只需要参考文件索引的过程，我们让新文件名指向原文件的inode即可
    //获取到根目录的inode
    let root_inode = crate::fs::ROOT_INODE.clone();
    //根据ROOT_INODE查找到原文件的inode，我们这里和新建一个file的最大区别就是不会真的创建一个inode
    let old_name = translated_str(token, _old_name);
    if let Some(old_inode) = root_inode.find(&old_name.as_str()) {
        //创建一个file，但是inode不是alloc产生，而是由old_inode，从而使得两个文件指向同一个inode
        // 需要转换一下，通过Inode类型找到它的inode_id
        root_inode.create_file_inode(
            &new_name,
            old_inode
                .get_inode_id_by_name(&root_inode, &old_name)
                .unwrap(),
        )
    } else {
        -1
    }

}

/// YOUR JOB: Implement unlinkat.
pub fn sys_unlinkat(_name: *const u8) -> isize {
    trace!(
        "kernel:pid[{}] sys_unlinkat NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    // let name = translated_str(current_user_token(), _name);
    // if name.is_empty() {
    //     return -1;
    // }
    // unlinkat(&name)

    let token = current_user_token();
    let name = translated_str(token, _name);
    //因为我们只有根目录，所以这就是文件的所在目录
    let root_inode = crate::fs::ROOT_INODE.clone();
    //找到文件的inode
    if let Some(inode) = root_inode.find(&name.as_str()) {
        //删除文件
        inode.unlink(&root_inode, &name)
    } else {
        -1
    }
}
