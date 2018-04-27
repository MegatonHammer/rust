// Copyright 2017 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use sys::mutex::Mutex;
use time::Duration;
use megaton_hammer::kernel::svc::{wait_process_wide_key_atomic, signal_process_wide_key};
use megaton_hammer::tls::TlsStruct;
use cell::UnsafeCell;

pub struct Condvar {
    tag: UnsafeCell<u32>
}

impl Condvar {
    pub const fn new() -> Condvar {
        Condvar { tag: UnsafeCell::new(0) }
    }

    #[inline]
    pub unsafe fn init(&mut self) {}

    #[inline]
    pub unsafe fn notify_one(&self) {
        let rc = signal_process_wide_key(self.tag.get(), 1);
        assert!(rc == 0);
    }

    #[inline]
    pub unsafe fn notify_all(&self) {
        let rc = signal_process_wide_key(self.tag.get(), u32::max_value());
        assert!(rc == 0);
    }

    pub unsafe fn wait(&self, mutex: &Mutex) {
        self.wait_ns(mutex, u64::max_value());
    }

    pub unsafe fn wait_timeout(&self, mutex: &Mutex, dur: Duration) -> bool {
        self.wait_ns(mutex, dur.as_secs() * 1_000_000_000 + dur.subsec_nanos() as u64)
    }

    unsafe fn wait_ns(&self, mutex: &Mutex, dur: u64) -> bool {
        // A Mutex has the same layout as a u32
        let tag = TlsStruct::get_thread_ctx().handle;
        let rc = wait_process_wide_key_atomic(mutex as *const Mutex as *mut Mutex as *mut u32,
            self.tag.get(), tag, dur);

        assert!(rc == 0xEA01 || rc == 0, "wait_process_wide_key_atomic failed with {:x}", rc);
        if rc == 0xEA01 {
            // On timeout, we need to reacquire the mutex manually
            mutex.lock();
        }

        rc == 0
    }

    #[inline]
    pub unsafe fn destroy(&self) {
    }
}
