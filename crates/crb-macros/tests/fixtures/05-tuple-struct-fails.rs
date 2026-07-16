#![allow(unused)]

use crb_macros::Cacheable;
use serde::{Deserialize, Serialize};

/// Applying #[derive(Cacheable)] to a tuple struct should fail to compile
/// because the derive macro only supports named fields.
#[derive(Cacheable, Serialize, Deserialize)]
struct CacheableTuple(String, u64);

fn main() {}
