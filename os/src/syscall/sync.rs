// SPDX-License-Identifier: GPL-3.0-only
use crate::sync::{Condvar, Mutex, MutexBlocking, MutexSpin, Semaphore};
use crate::task::{block_current_and_run_next, current_process, current_task};
use crate::timer::{add_timer, get_time_ms};
use alloc::sync::Arc;

const DEADLOCK: isize = -0xdead;

fn current_tid() -> usize {
    current_task()
        .unwrap()
        .inner_exclusive_access()
        .res
        .as_ref()
        .unwrap()
        .tid
}

pub fn sys_enable_deadlock_detect(is_enable: usize) -> isize {
    if is_enable > 1 {
        return -1;
    }
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    if is_enable == 0 {
        process_inner.deadlock_detect_enabled = false;
        return 0;
    }
    let thread_count = process_inner.tasks.len();
    if !process_inner.mutex_detector.is_safe_for(thread_count)
        || !process_inner.semaphore_detector.is_safe_for(thread_count)
    {
        return -1;
    }
    process_inner.deadlock_detect_enabled = true;
    0
}

pub fn sys_sleep(ms: usize) -> isize {
    let expire_ms = get_time_ms() + ms;
    let task = current_task().unwrap();
    add_timer(expire_ms, task);
    block_current_and_run_next();
    0
}

pub fn sys_mutex_create(blocking: bool) -> isize {
    let process = current_process();
    let mutex: Option<Arc<dyn Mutex>> = if !blocking {
        Some(Arc::new(MutexSpin::new()))
    } else {
        Some(Arc::new(MutexBlocking::new()))
    };
    let mut process_inner = process.inner_exclusive_access();
    let id = if let Some(id) = process_inner
        .mutex_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.mutex_list[id] = mutex;
        id
    } else {
        process_inner.mutex_list.push(mutex);
        process_inner.mutex_list.len() - 1
    };
    process_inner.mutex_detector.add_resource(1);
    id as isize
}

pub fn sys_mutex_lock(mutex_id: usize) -> isize {
    let tid = current_tid();
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
    let thread_count = process_inner.tasks.len();
    let detect = process_inner.deadlock_detect_enabled;
    if !process_inner
        .mutex_detector
        .request(thread_count, tid, mutex_id, detect)
    {
        return DEADLOCK;
    }
    drop(process_inner);
    mutex.lock();
    process
        .inner_exclusive_access()
        .mutex_detector
        .acquire(tid, mutex_id);
    0
}

pub fn sys_mutex_unlock(mutex_id: usize) -> isize {
    let tid = current_tid();
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
    process_inner.mutex_detector.release(tid, mutex_id);
    drop(process_inner);
    mutex.unlock();
    0
}

pub fn sys_semaphore_create(res_count: usize) -> isize {
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let id = if let Some(id) = process_inner
        .semaphore_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.semaphore_list[id] = Some(Arc::new(Semaphore::new(res_count)));
        id
    } else {
        process_inner
            .semaphore_list
            .push(Some(Arc::new(Semaphore::new(res_count))));
        process_inner.semaphore_list.len() - 1
    };
    process_inner.semaphore_detector.add_resource(res_count);
    id as isize
}

pub fn sys_semaphore_up(sem_id: usize) -> isize {
    let tid = current_tid();
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let sem = Arc::clone(process_inner.semaphore_list[sem_id].as_ref().unwrap());
    process_inner.semaphore_detector.release(tid, sem_id);
    drop(process_inner);
    sem.up();
    0
}

pub fn sys_semaphore_down(sem_id: usize) -> isize {
    let tid = current_tid();
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let sem = Arc::clone(process_inner.semaphore_list[sem_id].as_ref().unwrap());
    let thread_count = process_inner.tasks.len();
    let detect = process_inner.deadlock_detect_enabled;
    if !process_inner
        .semaphore_detector
        .request(thread_count, tid, sem_id, detect)
    {
        return DEADLOCK;
    }
    drop(process_inner);
    sem.down();
    process
        .inner_exclusive_access()
        .semaphore_detector
        .acquire(tid, sem_id);
    0
}

pub fn sys_condvar_create() -> isize {
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let id = if let Some(id) = process_inner
        .condvar_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.condvar_list[id] = Some(Arc::new(Condvar::new()));
        id
    } else {
        process_inner
            .condvar_list
            .push(Some(Arc::new(Condvar::new())));
        process_inner.condvar_list.len() - 1
    };
    id as isize
}

pub fn sys_condvar_signal(condvar_id: usize) -> isize {
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let condvar = Arc::clone(process_inner.condvar_list[condvar_id].as_ref().unwrap());
    drop(process_inner);
    condvar.signal();
    0
}

pub fn sys_condvar_wait(condvar_id: usize, mutex_id: usize) -> isize {
    let tid = current_tid();
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let condvar = Arc::clone(process_inner.condvar_list[condvar_id].as_ref().unwrap());
    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
    process_inner.mutex_detector.release(tid, mutex_id);
    drop(process_inner);
    condvar.wait(mutex);
    process
        .inner_exclusive_access()
        .mutex_detector
        .allocate(tid, mutex_id);
    0
}
