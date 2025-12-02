//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King
//!

use core::ffi::{c_int, c_long};
use core::ptr::null;
use core::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};

extern crate alloc;
use alloc::fmt::Debug;
use alloc::sync::Arc;
use alloc::vec::Vec;

const BUF_LEN: usize = 256;

const FUTEX_WAIT: c_int = 0;
const FUTEX_WAKE: c_int = 1;
const SYS_FUTEX: c_long = 202; // asm/unistd_64.h __NR_futex

unsafe extern "C" {
    fn syscall(num: c_long, ...) -> c_long;
}

pub struct Queue<T: Clone + Default + Debug> {
    data: [T; BUF_LEN],
    last: AtomicU32,
    wait: AtomicI32,
    is_closed: AtomicBool,
}

pub struct Consumer<T: Clone + Default + Debug> {
    queue: Arc<Queue<T>>,
    current: usize,
}

impl<T: Clone + Default + Debug> Consumer<T> {
    pub fn get_next(&mut self) -> Option<T> {
        let item = self.queue.get(self.current);
        self.current += 1;
        item
    }
}

impl<T: Clone + Default + Debug> Queue<T> {
    pub fn new() -> Arc<Self> {
        let data: [T; BUF_LEN] = (0..BUF_LEN)
            .map(|_| T::default())
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();
        Arc::new(Queue {
            data,
            last: AtomicU32::new(0),
            wait: AtomicI32::new(0),
            is_closed: AtomicBool::new(false),
        })
    }

    pub fn last(&self) -> usize {
        self.last.load(Ordering::Relaxed) as usize
    }

    pub fn consumer(self: &Arc<Self>) -> Consumer<T> {
        Consumer {
            queue: Arc::clone(self),
            current: self.last.load(Ordering::Relaxed) as usize,
        }
    }

    pub fn add(&self, item: T) {
        let pos = self.last.fetch_add(1, Ordering::Relaxed) as usize % BUF_LEN;
        unsafe {
            let ptr = self.data.as_ptr().add(pos) as *mut T;
            *ptr = item;
        }
        self.wake_threads();
    }

    pub fn get(&self, idx: usize) -> Option<T> {
        while idx == self.last() {
            if self.is_closed.load(Ordering::Relaxed) {
                return None;
            }
            // no values, park until there are
            self.park_thread();
        }
        Some(self.data[idx % BUF_LEN].clone())
    }

    pub fn close(&self) {
        self.is_closed.store(true, Ordering::Relaxed);
    }

    /*
    pub fn dump(&self) {
        for i in 0..BUF_LEN {
            println!("{i}: {:?}", self.data[i]);
        }
    }
    */

    fn park_thread(&self) {
        unsafe {
            syscall(
                SYS_FUTEX,
                self.wait.as_ptr() as *const i32,
                FUTEX_WAIT,
                0,
                null::<c_int>(),
                null::<c_int>(),
                0,
            );
        }
    }

    fn wake_threads(&self) {
        unsafe {
            syscall(
                SYS_FUTEX,
                self.wait.as_ptr() as *const i32,
                FUTEX_WAKE,
                999, // wake all the waiters, could be i32:MAX.
                null::<c_int>(),
                null::<c_int>(),
                0,
            );
        }
    }
}
