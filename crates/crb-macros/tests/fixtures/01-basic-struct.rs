#![allow(unused)]

use crb_macros::Cacheable;
use serde::{Deserialize, Serialize};

/// Simple struct with no cache_key or cache_ref annotations.
/// All fields become plain pass-through fields in the RefForm.
#[derive(Cacheable, Serialize, Deserialize)]
struct BasicStruct {
    name: String,
    value: u64,
}

fn main() {}
