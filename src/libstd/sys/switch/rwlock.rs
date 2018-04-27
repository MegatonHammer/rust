// Copyright 2016 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use super::mutex::Mutex;
use super::condvar::Condvar;

// Based on http://heather.cs.ucdavis.edu/~matloff/158/PLN/RWLock.c
pub struct RWLock {
    mutex: Mutex,
    /// Wait for read
    read: Condvar,
    /// Wait for write
    write: Condvar,
    /// Readers waiting
    r_wait: UnsafeCell<u32>,
    /// Writers waiting
    w_wait: UnsafeCell<u32>,
    /// Readers active
    r_active: UnsafeCell<u32>,
    /// Writers active
    w_active: UnsafeCell<bool>
}

impl RWLock {
    pub const fn new() -> RWLock {
        RWLock {
            mutex: Mutex::new(),
            read: Condvar::new(),
            write: Condvar::new(),
            r_wait: UnsafeCell::new(0),
            w_wait: UnsafeCell::new(0),
            r_active: UnsafeCell::new(0),
            w_active: UnsafeCell::new(false)
        }
    }

    #[inline]
    pub unsafe fn read(&self) {
        self.mutex.lock();
        if *self.w_active.get() {
            *self.r_wait.get() += 1;
            // TODO: Cleanup_push
            while *self.w_active.get() {
                self.read.wait(&self.mutex);
            }
            // TODO: Cleanup pop
            *self.r_wait.get() -= 1;
        }
        *self.r_active.get() += 1;
        self.mutex.unlock();
    }

    #[inline]
    pub unsafe fn try_read(&self) -> bool {
        // This lock is temporary.
        self.mutex.lock();

        if *self.w_active.get() {
            self.mutex.unlock();
            false
        } else {
            *self.r_active.get() += 1;
            self.mutex.unlock();
            true
        }
    }

    #[inline]
    pub unsafe fn write(&self) {
        self.mutex.lock();
        if *self.w_active.get() || *self.r_active.get() > 0 {
            *self.w_wait.get() += 1;
            // TODO: Write cleanup
            while *self.w_active.get() || *self.r_active.get() > 0 {
                self.write.wait(&self.mutex);
            }
            // TODO: Cleanup pop
            *self.w_wait.get() -= 1;
        }
        self.w_active = true;
        self.mutex.unlock();
    }

    #[inline]
    pub unsafe fn try_write(&self) -> bool {
        self.mutex.lock();
        if *self.w_active.get() || *self.r_active.get() > 0 {
            self.mutex.unlock();
            false
        } else {
            *self.w_active.get() = true;
            self.mutex.unlock();
            true
        }
    }

    #[inline]
    pub unsafe fn read_unlock(&self) {
        self.mutex.lock();
        *self.r_active.get() -= 1;
        if *self.r_active.get() == 0 && *self.w_wait.get() > 0 {
            self.write.notify_one();
        }
        self.mutex.unlock();
    }

    #[inline]
    pub unsafe fn write_unlock(&self) {
        self.mutex.lock();
        *self.w_active.get() = false;
        if *self.w_active.get() && *self.r_wait.get() > 0 {
            self.read.notify_one();
        }
        self.mutex.unlock();
    }

    #[inline]
    pub unsafe fn destroy(&self) {
        self.mutex.destroy();
    }
}
