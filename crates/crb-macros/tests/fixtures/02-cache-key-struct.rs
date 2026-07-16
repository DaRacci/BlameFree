#![allow(unused)]

use crb_macros::Cacheable;
use serde::{Deserialize, Serialize};

/// Struct with a #[cache_key] annotated field.
/// The RefForm passes the key field through, and the standalone
/// cache_key() method includes it in the hash.
#[derive(Cacheable, Serialize, Deserialize)]
struct CacheKeyStruct {
    #[cache_key]
    id: String,
    name: String,
}

fn main() {}
