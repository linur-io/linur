// SPDX-License-Identifier: GPL-3.0-only
use super::id::TaskUserRes;
use super::{KernelStack, ProcessControlBlock, TaskContext, kstack_alloc};
use crate::config::{BIG_STRIDE, DEFAULT_PRIORITY};
use crate::trap::TrapContext;
use crate::{mm::PhysPageNum, sync::UPSafeCell};
use alloc::collections::BTreeMap;
use alloc::sync::{Arc, Weak};
use core::cell::RefMut;

pub struct TaskControlBlock {
    // immutable
    pub process: Weak<ProcessControlBlock>,
    pub kstack: KernelStack,
    // mutable
    inner: UPSafeCell<TaskControlBlockInner>,
}

impl TaskControlBlock {
    pub fn inner_exclusive_access(&self) -> RefMut<'_, TaskControlBlockInner> {
        self.inner.exclusive_access()
    }

    pub fn get_user_token(&self) -> usize {
        let process = self.process.upgrade().unwrap();
        let inner = process.inner_exclusive_access();
        inner.memory_set.token()
    }

    /// Charge the stride of one scheduling round: `BIG_STRIDE / priority`, at
    /// least 1, saturating at `isize::MAX`.
    pub fn charge_stride(&self) {
        let mut inner = self.inner_exclusive_access();
        let pass = (BIG_STRIDE / inner.priority).max(1);
        inner.stride = inner.stride.saturating_add(pass);
    }

    /// Count one dispatched syscall against this task.
    pub fn record_syscall(&self, syscall_id: usize) {
        let mut inner = self.inner_exclusive_access();
        *inner.syscall_counts.entry(syscall_id).or_insert(0) += 1;
    }

    /// How many times this task has called `syscall_id`.
    pub fn syscall_count(&self, syscall_id: usize) -> isize {
        let inner = self.inner_exclusive_access();
        inner.syscall_counts.get(&syscall_id).copied().unwrap_or(0)
    }
}

pub struct TaskControlBlockInner {
    pub res: Option<TaskUserRes>,
    pub trap_cx_ppn: PhysPageNum,
    pub task_cx: TaskContext,
    pub task_status: TaskStatus,
    pub exit_code: Option<i32>,
    /// Virtual runtime consumed by the stride scheduler.
    pub stride: isize,
    /// Scheduling weight; one descheduling costs `BIG_STRIDE / priority`.
    pub priority: isize,
    /// Syscall counts of this task, keyed by syscall id.
    pub syscall_counts: BTreeMap<usize, isize>,
}

impl TaskControlBlockInner {
    pub fn get_trap_cx(&self) -> &'static mut TrapContext {
        self.trap_cx_ppn.get_mut()
    }

    #[allow(unused)]
    fn get_status(&self) -> TaskStatus {
        self.task_status
    }
}

impl TaskControlBlock {
    pub fn new(
        process: Arc<ProcessControlBlock>,
        ustack_base: usize,
        alloc_user_res: bool,
    ) -> Self {
        let res = TaskUserRes::new(Arc::clone(&process), ustack_base, alloc_user_res);
        let trap_cx_ppn = res.trap_cx_ppn();
        let kstack = kstack_alloc();
        let kstack_top = kstack.get_top();
        Self {
            process: Arc::downgrade(&process),
            kstack,
            inner: unsafe {
                UPSafeCell::new(TaskControlBlockInner {
                    res: Some(res),
                    trap_cx_ppn,
                    task_cx: TaskContext::goto_trap_return(kstack_top),
                    task_status: TaskStatus::Ready,
                    exit_code: None,
                    stride: 0,
                    priority: DEFAULT_PRIORITY,
                    syscall_counts: BTreeMap::new(),
                })
            },
        }
    }
}

#[derive(Copy, Clone, PartialEq)]
pub enum TaskStatus {
    Ready,
    Running,
    Blocked,
}
