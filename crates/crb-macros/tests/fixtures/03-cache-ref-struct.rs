#![allow(unused)]

use crb_macros::Cacheable;
use crb_cache::traits::CacheKey;
use crb_types::wrappers::Prompt;
use serde::{Deserialize, Serialize};

/// Struct with both #[cache_key] and #[cache_ref] annotated fields.
/// The RefForm converts cache_ref fields to String keys via CacheKey::cache_key().
#[derive(Cacheable, Serialize, Deserialize)]
struct CacheRefStruct {
    #[cache_key]
    id: String,
    #[cache_ref]
    prompt: Prompt,
    metadata: u64,
}

fn main() {}
