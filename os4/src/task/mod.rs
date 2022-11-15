//! Task management implementation
//!
//! Everything about task management, like starting and switching tasks is
//! implemented here.
//!
//! A single global instance of [`TaskManager`] called `TASK_MANAGER` controls
//! all the tasks in the operating system.
//!
//! Be careful when you see [`__switch`]. Control flow around this function
//! might not be what you expect.

mod context;
mod switch;
#[allow(clippy::module_inception)]
mod task;

use crate::loader::{get_app_data, get_num_app};
use crate::sync::UPSafeCell;
use crate::trap::TrapContext;
use alloc::vec::Vec;
use lazy_static::*;
pub use switch::__switch;
pub use task::{TaskControlBlock, TaskStatus};
use crate::config::MAX_SYSCALL_NUM;
pub use context::TaskContext;
use crate::timer::get_time_us;
use crate::mm::{MapPermission,VirtAddr,VirtPageNum};
use crate::mm::address::StepByOne;
use crate::mm::address::VPNRange;
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
    /// a `TaskManager` instance through lazy_static!
    pub static ref TASK_MANAGER: TaskManager = {
        info!("init TASK_MANAGER");
        let num_app = get_num_app();
        info!("num_app = {}", num_app);
        let mut tasks: Vec<TaskControlBlock> = Vec::new();
        //println!("the number is {}",num_app);
        for i in 0..num_app {
            tasks.push(TaskControlBlock::new(get_app_data(i), i));
            //println!("the task number is {}",i);
        }
        //println!("come here mod!");
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
        next_task.call_time = get_time_us()/1000;
        let next_task_cx_ptr = &next_task.task_cx as *const TaskContext;
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
        let current = inner.current_task;
        inner.tasks[current].task_status = TaskStatus::Ready;
    }

    /// Change the status of current `Running` task into `Exited`.
    fn mark_current_exited(&self) {
        let mut inner = self.inner.exclusive_access();
        let current = inner.current_task;
        inner.tasks[current].task_status = TaskStatus::Exited;
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

    #[allow(clippy::mut_from_ref)]
    /// Get the current 'Running' task's trap contexts.
    fn get_current_trap_cx(&self) -> &mut TrapContext {
        let inner = self.inner.exclusive_access();
        inner.tasks[inner.current_task].get_trap_cx()
    }

    /// Switch current `Running` task to the task we have found,
    /// or there is no `Ready` task and we can exit with all applications completed
    fn run_next_task(&self) {
        if let Some(next) = self.find_next_task() {
            let mut inner = self.inner.exclusive_access();
            let current = inner.current_task;
            inner.tasks[next].task_status = TaskStatus::Running;
            if inner.tasks[next].call_time ==0{  //lab2
                inner.tasks[next].call_time = get_time_us()/1000; 
            }
            inner.current_task = next;
            let current_task_cx_ptr = &mut inner.tasks[current].task_cx as *mut TaskContext;
            let next_task_cx_ptr = &inner.tasks[next].task_cx as *const TaskContext;
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

    fn get_current_time(&self) -> usize{
        let mut inner = self.inner.exclusive_access();
        let current = inner.current_task;
        inner.tasks[current].call_time
    }
    fn get_current_num(&self)->[u32;MAX_SYSCALL_NUM]{
        let mut inner = self.inner.exclusive_access();
        let current = inner.current_task;
        inner.tasks[current].call_num
    }
    fn add_current_num(&self,syscall_id:usize){
        let mut inner = self.inner.exclusive_access();
        let current = inner.current_task;
        inner.tasks[current].call_num[syscall_id]+=1;
    }
    fn mmap_malloc(&self,_start: usize, _len: usize, _port: usize) -> isize{
        if _len ==0{
            return 0;
        }
        if _start%4096 !=0{
            return -1;
        }
        if _port & (!0x7) != 0{
            return -1;
        }
        if _port & 0x7 ==0{
            return -1;
        }
        let mut inner = self.inner.exclusive_access();
        let current = inner.current_task;
        let memory_set = &mut inner.tasks[current].memory_set;
        let start: VirtAddr  = VirtAddr(_start).floor().into(); 
        let end_vpn:VirtAddr  = VirtAddr::from(_start+_len).ceil().into();
        if memory_set.check_va_overlap(start.into(), end_vpn.into()){
            return -1;
        }
        let mut permission = MapPermission::from_bits((_port as u8) << 1).unwrap();
        permission.set(MapPermission::U, true);
        memory_set.insert_framed_area(start.into(),end_vpn.into(),permission);
        0

    }
    fn unmap_unalloc(&self,_start: usize, _len: usize) -> isize{
        if _len ==0{
            return 0;
        }
        if _start%4096 !=0{
            return -1;
        }
        let mut inner = self.inner.exclusive_access();
        let current = inner.current_task;
        let memory_set = &mut inner.tasks[current].memory_set;
        let mut start = _start; 
        let end = start+_len;
        while start <end{
            let start_va = VirtAddr::from(start);
            let mut vpn = VirtAddr::from(start).floor();
            vpn.step();
            let mut end_va: VirtAddr = vpn.into();
            end_va = end_va.min(VirtAddr::from(end));
            let pte = memory_set.translate(vpn);
            match Some(pte){
                None => return -1,
                _ => (),
            }
            
            let mut index =0;
            let success =  memory_set.unmap(start_va,end_va);
            if success ==false{
                return -1;
            }
            start = end_va.into();
        }
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

pub fn get_current_time()->usize{
    TASK_MANAGER.get_current_time()
}

pub fn get_current_num() -> [u32;MAX_SYSCALL_NUM]{
    TASK_MANAGER.get_current_num()
}
pub fn add_current_num(syscall_id:usize){
    TASK_MANAGER.add_current_num(syscall_id);
}
pub fn mmap_malloc(_start: usize, _len: usize, _port: usize) -> isize{
    TASK_MANAGER.mmap_malloc(_start,_len,_port)
}
pub fn unmap_unalloc(_start:usize,_len:usize) -> isize{
    TASK_MANAGER.unmap_unalloc(_start,_len)
}