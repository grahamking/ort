//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King
//!
//! A queue, multi-producer and multi-consumer, without the standard library. Is Send and Sync.
//! Linux x86_64 only.
//!
//! Replaces `std::sync::channel` and friends.
//!
//! Built around a circular buffer. The consumers must never fall behind by more than the buffer
//! length, otherwise they miss items.
//!
//! If a producer tries to get an item but there aren't any yet, it is parked on a `futex`.
//!
//! The modulus operation to access the circular buffer is done right at the end, meaning the
//! consumer and producer positions do not wrap. This makes the math a lot simpler, but means it
//! won't work past u32::MAX items. That's fine for `ort`.
//!
//! A Queue is always wrapped in an Arc - `new` returns `Arc<Queue>`, so it is freely cloneable.
//! The consumers also clone a copy to keep internally.
//!
//! When the queue will not be sent any more items close it, which unblocks any waiting producers.
//!
//! Usage:
//!
//! // A buffer with space for 32 items
//! let producer_1 = Queue::<32>::new();
//! let producer_2 = producer_1.clone();
//!
//! // Two consumers. It doesn't matter which producer.
//! // The consumer will start reading from the current producer position, so if you create it
//! after adding to the queue, those items will not be read.
//! let consumer1 = producer_1.consumer();
//! let consumer2 = producer_1.consumer();
//!
//! // Add an item
//! producer_1.add(item);
//! // Receive it
//! let x = consumer_1.get_next().unwrap();
//! let y = consumer_2.get_next().unwrap();
//!
//! // No more items. Doesn't matter which producer.
//! producer_1.close();
//!
//! assert!(consumer1.get_next().is_none());

use core::ffi::{c_int, c_long};
use core::ptr::null;
use core::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};

extern crate alloc;
use alloc::fmt::Debug;
use alloc::sync::Arc;
use alloc::vec::Vec;

const FUTEX_WAIT: c_int = 0;
const FUTEX_WAKE: c_int = 1;
const SYS_FUTEX: c_long = 202; // asm/unistd_64.h __NR_futex

unsafe extern "C" {
    fn syscall(num: c_long, ...) -> c_long;
}

pub struct Queue<T: Clone + Default + Debug, const N: usize> {
    data: [T; N],
    // The next empty position
    insert_pos: AtomicU32,
    // The read_end is one past the last visible item, it's the full stop for reads.
    read_end: AtomicU32,
    wait: AtomicI32,
    is_closed: AtomicBool,
}

pub struct Consumer<T: Clone + Default + Debug, const N: usize> {
    queue: Arc<Queue<T, N>>,
    current: usize,
}

impl<T: Clone + Default + Debug, const N: usize> Consumer<T, N> {
    pub fn get_next(&mut self) -> Option<T> {
        let item = self.queue.get(self.current);
        self.current += 1;
        item
    }
}

impl<T: Clone + Default + Debug, const N: usize> Queue<T, N> {
    pub fn new() -> Arc<Self> {
        let data: [T; N] = (0..N)
            .map(|_| T::default())
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();
        Arc::new(Queue {
            data,
            insert_pos: AtomicU32::new(0),
            read_end: AtomicU32::new(0),
            wait: AtomicI32::new(0),
            is_closed: AtomicBool::new(false),
        })
    }

    pub fn last(&self) -> usize {
        self.read_end.load(Ordering::Relaxed) as usize
    }

    pub fn consumer(self: &Arc<Self>) -> Consumer<T, N> {
        Consumer {
            queue: Arc::clone(self),
            current: self.read_end.load(Ordering::Relaxed) as usize,
        }
    }

    // Why the two phase commit?
    // We move the insert_pos indicator forward before inserting the item.
    // That "reserves" the slot for us, no other thread will write to it.
    // But it isn't written yet, so we shouldn't read from it.
    pub fn add(&self, item: T) {
        let insert_at = self.insert_pos.fetch_add(1, Ordering::Relaxed);
        unsafe {
            let ptr = self.data.as_ptr().add(insert_at as usize % N) as *mut T;
            *ptr = item;
        }

        // If the current read end is at our position, we can commit our item by
        // moving the read end forward one, which exposes this item.
        // If not, there must be other threads before us that haven't commited, so
        // wait a bit.
        loop {
            let new_commit_pos = self.read_end.compare_exchange(
                insert_at,
                insert_at + 1,
                Ordering::Relaxed,
                Ordering::Relaxed,
            );
            if new_commit_pos == Ok(insert_at) {
                // Success, our item is visible
                break;
            }
            // Othewise wait for other items to commit and retry
            core::hint::spin_loop();
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
        Some(self.data[idx % N].clone())
    }

    pub fn close(&self) {
        self.is_closed.store(true, Ordering::Relaxed);
    }

    /*
    pub fn dump(&self) {
        for i in 0..N {
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

impl<T: Clone + Default + Debug, const N: usize> Drop for Queue<T, N> {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const NUM_ITEMS: usize = 40;

    #[derive(Default, Debug, Clone)]
    pub struct Item {
        pub val: usize,
        #[allow(dead_code)]
        pub s: &'static str,
    }

    impl Item {
        pub fn new(val: usize, s: &'static str) -> Self {
            Item { val, s }
        }
    }

    #[test]
    fn test_queue() {
        let q = Queue::<_, NUM_ITEMS>::new();
        let mut c1 = q.consumer();
        let mut c2 = q.consumer();

        // Ideally we would do all this in threads, but no_std

        // Producer
        for i in 0..NUM_ITEMS {
            let i1 = Item::new(i, "x");
            q.add(i1);
        }
        q.close();

        // Consumer 1
        for i in 0..10 {
            let got_c1 = c1.get_next().unwrap();
            assert_eq!(i, got_c1.val);
        }

        // Consumer 1
        for i in 0..15 {
            let got_c2 = c2.get_next().unwrap();
            assert_eq!(i, got_c2.val);
        }
    }
}
