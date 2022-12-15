//! Process management syscalls

use crate::config::MAX_SYSCALL_NUM;
use crate::task::{exit_current_and_run_next, suspend_current_and_run_next, TaskStatus};
use crate::timer::get_time_us;
use crate::task::{get_current_num,get_current_time,mmap_malloc,unmap_unalloc};
use crate::task::current_user_token;
use crate::mm::{VirtAddr, PhysAddr, PageTable};
#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

#[derive(Clone, Copy)]
pub struct TaskInfo {
    pub status: TaskStatus,
    pub syscall_times: [u32; MAX_SYSCALL_NUM],
    pub time: usize,
}

pub fn sys_exit(exit_code: i32) -> ! {
    info!("[kernel] Application exited with code {}", exit_code);
    exit_current_and_run_next();
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    suspend_current_and_run_next();
    0
}

// YOUR JOB: 引入虚地址后重写 sys_get_time
pub fn sys_get_time(_ts: *mut TimeVal, _tz: usize) -> isize {
    
    let _us = get_time_us();
    let page_table = PageTable::from_token(current_user_token());
    let ptr = _ts as usize;
    let va = VirtAddr::from(ptr);
    let vpn = va.floor();
    let ppn = page_table.translate(vpn).unwrap().ppn();
    let buffers = ppn.get_bytes_array();
    let offset = va.page_offset();
    let sec = _us / 1_000_000_000;
    let usec = _us %1_000_000_000;
    buffers[0+offset] = (sec&0xff) as u8;
    buffers[1+offset] = ((sec>>8)&0xff) as u8;
    buffers[2+offset] = ((sec>>16)&0xff) as u8;
    buffers[3+offset] = ((sec>>24)&0xff) as u8;

    buffers[8+offset] = (usec&0xff) as u8;
    buffers[9+offset] = ((usec>>8)&0xff) as u8;
    buffers[10+offset] = ((usec>>16)&0xff) as u8;
    buffers[11+offset] = ((usec>>24)&0xff) as u8;
    
    0
    
}

// CLUE: 从 ch4 开始不再对调度算法进行测试~
pub fn sys_set_priority(_prio: isize) -> isize {
    -1
}

// YOUR JOB: 扩展内核以实现 sys_mmap 和 sys_munmap
pub fn sys_mmap(_start: usize, _len: usize, _port: usize) -> isize {
    
   mmap_malloc(_start,_len,_port)
    
}

pub fn sys_munmap(_start: usize, _len: usize) -> isize {
    unmap_unalloc(_start,_len)
}

// YOUR JOB: 引入虚地址后重写 sys_task_info
pub fn sys_task_info(ti: *mut TaskInfo) -> isize {
    
    let page_table = PageTable::from_token(current_user_token());
    let ptr = ti as usize;
    let va = VirtAddr::from(ptr);
    let vpn = va.floor();
    let ppn = page_table.translate(vpn).unwrap().ppn();
    let buffers = ppn.get_bytes_array();
    let offset = va.page_offset();
    let pa:PhysAddr = ppn.into();
    unsafe {
        let task_info = ((pa.0 + offset) as *mut TaskInfo).as_mut().unwrap();
        let tmp = TaskInfo{
            status: TaskStatus::Running,
            syscall_times: get_current_num(),
            time: get_time_us()/1000 - get_current_time(),
        };
        *task_info = tmp;
    }
    
    0
}
