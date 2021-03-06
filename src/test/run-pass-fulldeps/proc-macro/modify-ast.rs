// Copyright 2018 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// aux-build:modify-ast.rs

#![feature(use_extern_macros)]

extern crate modify_ast;

use modify_ast::*;

#[derive(Foo)]
pub struct MyStructc {
    #[cfg_attr(my_cfg, foo)]
    _a: i32,
}

macro_rules! a {
    ($i:item) => ($i)
}

a! {
    #[assert1]
    pub fn foo() {}
}

fn main() {
    let _a = MyStructc { _a: 0 };
    foo();
}
