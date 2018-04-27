// Copyright 2017 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use alloc::boxed::FnBox;
use ffi::CStr;
use io;
use time::Duration;
use megaton_hammer::kernel::svc::*;

pub struct Thread(u32);

pub const DEFAULT_MIN_STACK_SIZE: usize = 4096;

impl Thread {
    pub unsafe fn new<'a>(stack: usize, p: Box<FnBox() + 'a>)
        -> io::Result<Thread>
    {
        unsafe extern "C" fn start_fn(arg1: *mut ()) {
            let p : Box<Box<FnBox()>> = Box::from_raw(arg1 as *mut Box<FnBox()>);
            p();
            exit_thread();
        }

        let (_res, thread) = create_thread(Some(start_fn as _), Box::into_raw(Box::new(p)) as usize as u64, stack as *mut _, 10, -2);
        // TODO:
        //if res != 0 {
        //    return Err(io::Error::);
        //}
        let _res = start_thread(thread);
        // TODO: handle res.
        Ok(Thread(thread))
    }

    pub fn yield_now() {
        Self::sleep(Duration::new(0, 0));
    }

    pub fn set_name(_name: &CStr) {
        // nope
    }

    pub fn sleep(dur: Duration) {
        let rc = unsafe { sleep_thread(dur.as_secs() * 1_000_000_000 + dur.subsec_nanos() as u64) };
        assert!(rc == 0);
    }

    pub fn join(self) {
        // TODO: WaitSync on Thread ? Or do we need actual convars and shit.
        panic!("Can't join");
    }
}

pub mod guard {
    pub type Guard = !;
    pub unsafe fn current() -> Option<Guard> { None }
    pub unsafe fn init() -> Option<Guard> { None }
}
