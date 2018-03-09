// Copyright 2018 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![deny(unused_imports)]

fn f() {}
fn g() {}
fn h() {}

mod basic {
    use super::f; //~ ERROR unused import
}

mod part_of_group {
    use super::{f, g, h}; //~ ERROR unused import
    fn main() {
        h();
    }
}

mod part_of_group_leaving_one {
    use super::{f, g, h}; //~ ERROR unused import

    fn main() {
        g();
        h();
    }
}

mod whole_group {
    use super::{f, g, h}; //~ ERROR unused import
}

mod empty_groups {
    use super::{{}, basic::{}}; //~ ERROR unused import
}

mod nested_group_all_unused {
    use std::{collections::{binary_heap::{BinaryHeap}, HashMap, linked_list::*}, io::*};
    //^ ERROR unused import
}

mod nested_group_leaving_one {
    use std::{collections::{binary_heap::{BinaryHeap}, HashMap, linked_list::*}, io::*};
    //^ ERROR unused import

    fn main() {
        HashMap::<u32, u32>::default();
    }
}

mod maintains_trailing_comma {
    use std::collections::{HashMap, HashSet, LinkedList,}; //~ ERROR unused import

    fn main() {
        HashMap::<u32, u32>::default();
        LinkedList::<u32>::default();
    }
}

mod maintains_unaffected_single_groups {
    // We shouldn't remove the {}s around prelude::*,
    // in case that's the user's preference ¯\_(ツ)_/¯
    use std::{io::{prelude::*}, collections::{BinaryHeap, HashMap}}; //~ ERROR unused import

    fn main() {
        let x: HashMap<Box<Read>, Box<Write>> = unimplemented!();
    }
}

fn main() {}
