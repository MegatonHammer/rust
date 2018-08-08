// Copyright 2017 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// TODO: Properly implement mutex. The switch has multiple syscalls to implement
// mutexes properly.

use megaton_hammer::kernel::sync::Mutex as InternalMutex;
use megaton_hammer::kernel::sync::RMutex as InternalRMutex;

pub struct Mutex {
    internal: InternalMutex
}

unsafe impl Send for Mutex {}
unsafe impl Sync for Mutex {} // no threads on wasm

impl Mutex {
    pub const fn new() -> Mutex {
        Mutex { internal: InternalMutex::new() }
    }

    #[inline]
    pub unsafe fn init(&mut self) {
    }

    #[inline]
    pub unsafe fn lock(&self) {
        self.internal.lock()
    }

    #[inline]
    pub unsafe fn unlock(&self) {
        self.internal.unlock()
    }

    #[inline]
    pub unsafe fn try_lock(&self) -> bool {
        self.internal.try_lock()
    }

    #[inline]
    pub unsafe fn destroy(&self) {
    }
}

pub struct ReentrantMutex {
    internal: InternalRMutex
}

impl ReentrantMutex {
    pub unsafe fn uninitialized() -> ReentrantMutex {
        ReentrantMutex {
            internal: InternalRMutex::new()
        }
    }

    pub unsafe fn init(&mut self) {}

    pub unsafe fn lock(&self) {
        self.internal.lock()
    }

    #[inline]
    pub unsafe fn try_lock(&self) -> bool {
        self.internal.try_lock()
    }

    pub unsafe fn unlock(&self) {
        self.internal.unlock()
    }

    pub unsafe fn destroy(&self) {}
}

unsafe impl Send for ReentrantMutex {}
unsafe impl Sync for ReentrantMutex {}
