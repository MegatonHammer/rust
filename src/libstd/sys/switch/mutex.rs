// Copyright 2017 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use sync::atomic::{AtomicU32, Ordering};
use cell::UnsafeCell;
use megaton_hammer::kernel::svc::{arbitrate_lock, arbitrate_unlock};
use megaton_hammer::tls::TlsStruct;

const HAS_LISTENERS: u32 = 0x40000000;

// Represented as a single u32. Useful for condvar.
#[repr(C)]
pub struct Mutex { lock: AtomicU32 }

unsafe impl Send for Mutex {}
unsafe impl Sync for Mutex {}

impl Mutex {
    pub const fn new() -> Mutex {
        Mutex { lock: AtomicU32::new(0) }
    }

    #[inline]
    pub unsafe fn init(&mut self) {
    }

    #[inline]
    pub unsafe fn lock(&self) {
        let tag = TlsStruct::get_thread_ctx().handle;
        loop {
            let cur = self.lock.compare_and_swap(0, tag, Ordering::SeqCst);

            if cur == 0 {
                // We won the race!
                return;
            }

            if (cur & !HAS_LISTENERS) == tag {
                // Kernel gave it to us!
                return;
            }

            if cur & HAS_LISTENERS != 0 {
                arbitrate_lock(cur & !HAS_LISTENERS, &self.lock as *const AtomicU32 as *mut u32, tag);
            } else {
                let old = self.lock.compare_and_swap(cur, cur | HAS_LISTENERS, Ordering::SeqCst);
                if old == cur {
                    arbitrate_lock(cur & !HAS_LISTENERS, &self.lock as *const AtomicU32 as *mut u32, tag);
                }
            }
        }
    }

    #[inline]
    pub unsafe fn unlock(&self) {
        let old = self.lock.swap(0, Ordering::SeqCst);
        if old & HAS_LISTENERS != 0 {
            arbitrate_unlock(&self.lock as *const AtomicU32 as *mut u32);
        }
    }

    #[inline]
    pub unsafe fn try_lock(&self) -> bool {
        let tag = TlsStruct::get_thread_ctx().handle;
        let cur = self.lock.compare_and_swap(0, tag, Ordering::SeqCst);

        if cur == 0 {
            // We won the race
            return true;
        }

        if cur & !HAS_LISTENERS == tag {
            // Kernel assigned it to us
            return true;
        }

        return false;
    }

    #[inline]
    pub unsafe fn destroy(&self) {
    }
}

pub struct ReentrantMutex {
    thread_tag: UnsafeCell<u32>,
    lock: Mutex,
    counter: UnsafeCell<usize>
}

unsafe impl Send for ReentrantMutex {}
unsafe impl Sync for ReentrantMutex {}

impl ReentrantMutex {
    pub unsafe fn uninitialized() -> ReentrantMutex {
        ReentrantMutex {
            thread_tag: UnsafeCell::new(0),
            lock: Mutex::new(),
            counter: UnsafeCell::new(0)
        }
    }

    pub unsafe fn init(&mut self) {}

    pub unsafe fn lock(&self) {
        let tag = TlsStruct::get_thread_ctx().handle;
        if *self.thread_tag.get() != tag {
            self.lock.lock();
            *self.thread_tag.get() = tag;
        }
        *self.counter.get() = *self.counter.get() + 1;
    }

    #[inline]
    pub unsafe fn try_lock(&self) -> bool {
        let tag = TlsStruct::get_thread_ctx().handle;
        if *self.thread_tag.get() != tag {
            if !self.lock.try_lock() {
                return false;
            }
            *self.thread_tag.get() = tag;
        }
        *self.counter.get() = *self.counter.get() + 1;
        return true;
    }

    pub unsafe fn unlock(&self) {
        *self.counter.get() = *self.counter.get() - 1;
        if *self.counter.get() == 0 {
            *self.thread_tag.get() = 0;
            self.lock.unlock();
        }
    }

    pub unsafe fn destroy(&self) {}
}
