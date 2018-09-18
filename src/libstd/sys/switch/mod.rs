// Copyright 2017 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! System bindings for the Nintendo Switch platform
//!
//! This module contains the facade (aka platform-specific) implementations of
//! OS level functionality for the Nintendo Switch.
//!
//! Currently all functions here are basically stubs that immediately return
//! errors.

#![allow(dead_code, missing_docs, bad_style)]

// Link against runwind here to avoid future conflicts
extern crate runwind;

use io::{self, ErrorKind};
use megaton_hammer::error::{Error, Module};

pub mod args;
#[cfg(feature = "backtrace")]
pub mod backtrace;
pub mod cmath;
pub mod condvar;
pub mod env;
pub mod ext;
pub mod fast_thread_local;
pub mod fs;
pub mod memchr;
pub mod mutex;
pub mod net;
pub mod os;
pub mod os_str;
pub mod path;
pub mod pipe;
pub mod process;
pub mod rwlock;
pub mod stack_overflow;
pub mod stdio;
pub mod thread;
pub mod thread_local;
pub mod time;

#[cfg(not(test))]
pub fn init() {}

pub fn unsupported<T>() -> io::Result<T> {
    Err(unsupported_err())
}

pub fn unsupported_err() -> io::Error {
    io::Error::new(io::ErrorKind::Other,
                   "operation not supported on switch yet")
}

pub fn decode_error_kind(errno: i32) -> ErrorKind {
    mod linux {
        pub const ECONNREFUSED: u32 = 111;
        pub const ECONNRESET: u32 = 104;
        pub const EPERM: u32 = 1;
        pub const EACCES: u32 = 13;
        pub const EPIPE: u32 = 32;
        pub const ENOTCONN: u32 = 107;
        pub const ECONNABORTED: u32 = 103;
        pub const EADDRNOTAVAIL: u32 = 99;
        pub const EADDRINUSE: u32 = 98;
        pub const ENOENT: u32 = 2;
        pub const EINTR: u32 = 4;
        pub const EINVAL: u32 = 22;
        pub const ETIMEDOUT: u32 = 110;
        pub const EEXIST: u32 = 17;
        pub const EAGAIN: u32 = 11;
        pub const EWOULDBLOCK: u32 = EAGAIN;
    }

    // Taken from linux, which is what net seems to use.
    let err = Error(errno as u32);
    match (err.module(), err.description_id()) {
        (Ok(Module::MegatonHammerLinux), linux::ECONNREFUSED) => ErrorKind::ConnectionRefused,
        (Ok(Module::MegatonHammerLinux), linux::ECONNRESET) => ErrorKind::ConnectionReset,
        (Ok(Module::MegatonHammerLinux), linux::EPERM) |
        (Ok(Module::MegatonHammerLinux), linux::EACCES) => ErrorKind::PermissionDenied,
        (Ok(Module::MegatonHammerLinux), linux::EPIPE) => ErrorKind::BrokenPipe,
        (Ok(Module::MegatonHammerLinux), linux::ENOTCONN) => ErrorKind::NotConnected,
        (Ok(Module::MegatonHammerLinux), linux::ECONNABORTED) => ErrorKind::ConnectionAborted,
        (Ok(Module::MegatonHammerLinux), linux::EADDRNOTAVAIL) => ErrorKind::AddrNotAvailable,
        (Ok(Module::MegatonHammerLinux), linux::EADDRINUSE) => ErrorKind::AddrInUse,
        (Ok(Module::MegatonHammerLinux), linux::ENOENT) => ErrorKind::NotFound,
        (Ok(Module::MegatonHammerLinux), linux::EINTR) => ErrorKind::Interrupted,
        (Ok(Module::MegatonHammerLinux), linux::EINVAL) => ErrorKind::InvalidInput,
        (Ok(Module::MegatonHammerLinux), linux::ETIMEDOUT) => ErrorKind::TimedOut,
        (Ok(Module::MegatonHammerLinux), linux::EEXIST) => ErrorKind::AlreadyExists,

        // These two constants can have the same value on some systems,
        // but different values on others, so we can't use a match
        // clause
        (Ok(Module::MegatonHammerLinux), x) if x == linux::EAGAIN || x == linux::EWOULDBLOCK =>
            ErrorKind::WouldBlock,

        _ => ErrorKind::Other,
    }
}

/// TODO: Do a proper abort using the exit stuff.
pub unsafe fn abort_internal() -> ! {
    ::core::intrinsics::abort();
}

// This enum is used as the storage for a bunch of types which can't actually
// exist.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub enum Void {}

pub unsafe fn strlen(mut s: *const i8) -> usize {
    let mut n = 0;
    while *s != 0 {
        n += 1;
        s = s.offset(1);
    }
    return n
}

// We don't have randomness yet, but I totally used a random number generator to
// generate these numbers.
//
// More seriously though this is just for DOS protection in hash maps. It's ok
// if we don't do that on switch just yet.
pub fn hashmap_random_keys() -> (u64, u64) {
    (1, 2)
}

#[stable(feature = "rust1", since = "1.0.0")]
impl From<::megaton_hammer::error::Error> for io::Error {
    fn from(err: ::megaton_hammer::error::Error) -> io::Error {
        io::Error::from_raw_os_error(err.0 as i32)
    }
}

// TODO: Figure out a better way to do this.
#[stable(feature = "rust1", since = "1.0.0")]
impl From<::megaton_hammer::error::MegatonHammerDescription> for io::Error {
    fn from(err: ::megaton_hammer::error::MegatonHammerDescription) -> io::Error {
        io::Error::from(::megaton_hammer::error::Error::from(err))
    }
}
