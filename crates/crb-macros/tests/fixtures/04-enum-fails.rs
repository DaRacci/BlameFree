#![allow(unused)]

use crb_macros::Cacheable;
use serde::{Deserialize, Serialize};

/// Applying #[derive(Cacheable)] to an enum should fail to compile
/// because the derive macro only supports structs.
#[derive(Cacheable, Serialize, Deserialize)]
enum CacheableEnum {
    VariantA,
    VariantB,
}

fn main() {}
