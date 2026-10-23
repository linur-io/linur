// SPDX-License-Identifier: GPL-3.0-only
use super::{ProcessControlBlock, TaskControlBlock, TaskStatus};
use crate::sync::UPSafeCell;
use alloc::collections::{BTreeMap, BinaryHeap};
use alloc::sync::Arc;
use core::cmp::Ordering;
use lazy_static::*;

/// A ready task together with the scheduling key it had when it was queued.
struct ReadyTask {
    task: Arc<TaskControlBlock>,
    stride: isize,
    pid: usize,
    tid: usize,
}

impl Ord for ReadyTask {
    /// Smallest stride first, then smallest pid, then smallest tid, so the order
    /// is total and does not depend on the heap's internal layout. Reversed for
    /// the max-heap.
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .stride
            .cmp(&self.stride)
            .then_with(|| other.pid.cmp(&self.pid))
            .then_with(|| other.tid.cmp(&self.tid))
    }
}

impl PartialOrd for ReadyTask {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for ReadyTask {
    fn eq(&self, other: &Self) -> bool {
        self.stride == other.stride && self.pid == other.pid && self.tid == other.tid
    }
}

impl Eq for ReadyTask {}

/// The ready set of the stride scheduler.
pub struct TaskManager {
    ready_heap: BinaryHeap<ReadyTask>,
}

impl TaskManager {
    /// Create an empty `TaskManager`.
    pub fn new() -> Self {
        Self {
            ready_heap: BinaryHeap::new(),
        }
    }
    /// Add a task to `TaskManager`, snapshotting its scheduling key. The pid
    /// is read first, before the TCB lock, so the lock order stays process -> task.
    pub fn add(&mut self, task: Arc<TaskControlBlock>) {
        let pid = task.process.upgrade().unwrap().getpid();
        let inner = task.inner_exclusive_access();
        let stride = inner.stride;
        let tid = inner.res.as_ref().map_or(0, |res| res.tid);
        drop(inner);
        self.ready_heap.push(ReadyTask {
            task,
            stride,
            pid,
            tid,
        });
    }
    /// Remove and return the ready task with the smallest stride.
    pub fn fetch(&mut self) -> Option<Arc<TaskControlBlock>> {
        self.ready_heap.pop().map(|ready| ready.task)
    }
    /// Drop `task` from the ready set; a task that is not queued changes nothing.
    pub fn remove(&mut self, task: Arc<TaskControlBlock>) {
        self.ready_heap
            .retain(|ready| !Arc::ptr_eq(&ready.task, &task));
    }
}

lazy_static! {
    pub static ref TASK_MANAGER: UPSafeCell<TaskManager> =
        unsafe { UPSafeCell::new(TaskManager::new()) };
    pub static ref PID2PCB: UPSafeCell<BTreeMap<usize, Arc<ProcessControlBlock>>> =
        unsafe { UPSafeCell::new(BTreeMap::new()) };
}

pub fn add_task(task: Arc<TaskControlBlock>) {
    TASK_MANAGER.exclusive_access().add(task);
}

pub fn wakeup_task(task: Arc<TaskControlBlock>) {
    let mut task_inner = task.inner_exclusive_access();
    task_inner.task_status = TaskStatus::Ready;
    drop(task_inner);
    add_task(task);
}

pub fn remove_task(task: Arc<TaskControlBlock>) {
    TASK_MANAGER.exclusive_access().remove(task);
}

pub fn fetch_task() -> Option<Arc<TaskControlBlock>> {
    TASK_MANAGER.exclusive_access().fetch()
}

pub fn pid2process(pid: usize) -> Option<Arc<ProcessControlBlock>> {
    let map = PID2PCB.exclusive_access();
    map.get(&pid).map(Arc::clone)
}

pub fn insert_into_pid2process(pid: usize, process: Arc<ProcessControlBlock>) {
    PID2PCB.exclusive_access().insert(pid, process);
}

pub fn remove_from_pid2process(pid: usize) {
    let mut map = PID2PCB.exclusive_access();
    if map.remove(&pid).is_none() {
        panic!("cannot find pid {} in pid2task!", pid);
    }
}
