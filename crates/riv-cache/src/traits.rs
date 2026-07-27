use std::pin::Pin;

use tracing::info;

pub trait CacheKey {
    fn cache_key(&self) -> String;
}

pub trait Cacheable: Sized + serde::Serialize + serde::de::DeserializeOwned {
    type RefForm: serde::Serialize + serde::de::DeserializeOwned + CacheKey;

    fn into_ref(self, backend: &dyn CacheBackend) -> Self::RefForm;
    fn from_ref(form: Self::RefForm, backend: &dyn CacheBackend) -> Self;
}

pub trait CacheBackend: Send + Sync {
    fn store_raw(&self, key: &str, value: &str);
    fn load_raw(&self, key: &str) -> String;
}

/// Inherent method on the trait object so callers with `Arc<dyn CacheBackend>`
/// (or any `&dyn CacheBackend`) can invoke `get_or_compute` directly.
///
/// TODO: Migrate this to a generic `B: CacheBackend` parameter on every consumer
///       so the method can live directly on the trait without the `dyn` workaround.
///       Drop `Arc<dyn CacheBackend>` in favour of `Arc<B>` where `B: CacheBackend`.
impl dyn CacheBackend {
    pub fn get_or_compute<'a, K, T, F, Fut>(
        &'a self,
        key: &'a K,
        compute: F,
    ) -> Pin<Box<dyn std::future::Future<Output = T> + 'a>>
    where
        K: CacheKey + ?Sized + 'a,
        T: serde::Serialize + serde::de::DeserializeOwned + 'a,
        F: FnOnce() -> Fut + 'a,
        Fut: std::future::Future<Output = T> + 'a,
    {
        Box::pin(get_or_compute_impl(self, key, compute))
    }
}

async fn get_or_compute_impl<K, T, F, Fut>(backend: &dyn CacheBackend, key: &K, compute: F) -> T
where
    K: CacheKey + ?Sized,
    T: serde::Serialize + serde::de::DeserializeOwned,
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = T>,
{
    let cache_key = key.cache_key();
    let cached = backend.load_raw(&cache_key);
    if !cached.is_empty() {
        if let Ok(result) = serde_json::from_str::<T>(&cached) {
            info!("CACHE HIT (key={})", cache_key_prefix(&cache_key));
            return result;
        }
    }
    info!("CACHE MISS (key={})", cache_key_prefix(&cache_key));
    let result = compute().await;
    backend.store_raw(
        &cache_key,
        &serde_json::to_string(&result).unwrap_or_default(),
    );
    result
}

fn cache_key_prefix(key: &str) -> &str {
    if key.len() > 12 { &key[..12] } else { key }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    /// A simple in-memory backend for testing.
    struct MockBackend {
        store: Mutex<std::collections::HashMap<String, String>>,
    }

    impl MockBackend {
        fn new() -> Self {
            Self {
                store: Mutex::new(std::collections::HashMap::new()),
            }
        }
    }

    impl CacheBackend for MockBackend {
        fn store_raw(&self, key: &str, value: &str) {
            if let Ok(mut store) = self.store.lock() {
                store.insert(key.to_string(), value.to_string());
            }
        }

        fn load_raw(&self, key: &str) -> String {
            if let Ok(store) = self.store.lock() {
                store.get(key).cloned().unwrap_or_default()
            } else {
                String::new()
            }
        }
    }

    /// A simple key type for testing.
    struct TestKey(String);

    impl CacheKey for TestKey {
        fn cache_key(&self) -> String {
            self.0.clone()
        }
    }

    #[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
    struct TestValue {
        message: String,
        count: u32,
    }

    #[test]
    fn test_cache_key_prefix_short() {
        let key = "short";
        assert_eq!(cache_key_prefix(key), "short");
    }

    #[test]
    fn test_cache_key_prefix_exact_12() {
        let key = "123456789012"; // exactly 12 chars
        assert_eq!(cache_key_prefix(key), "123456789012");
    }

    #[test]
    fn test_cache_key_prefix_long() {
        let key = "1234567890123"; // 13 chars
        let prefix = cache_key_prefix(key);
        assert_eq!(prefix.len(), 12);
        assert_eq!(prefix, "123456789012");
    }

    #[test]
    fn test_mock_backend_store_and_load() {
        let backend = MockBackend::new();
        backend.store_raw("test-key", "test-value");
        let loaded = backend.load_raw("test-key");
        assert_eq!(loaded, "test-value");
    }

    #[test]
    fn test_mock_backend_load_missing() {
        let backend = MockBackend::new();
        let loaded = backend.load_raw("nonexistent");
        assert!(loaded.is_empty());
    }

    #[test]
    fn test_get_or_compute_miss() {
        let backend = MockBackend::new();
        let key = TestKey("my-key".into());
        let mut call_count = 0u32;

        // Cast to trait object to call get_or_compute (defined on dyn CacheBackend)
        let dyn_backend: &dyn CacheBackend = &backend;
        if let Ok(rt) = tokio::runtime::Runtime::new() {
            let result = rt.block_on(dyn_backend.get_or_compute(&key, async || {
                call_count += 1;
                TestValue {
                    message: "computed".into(),
                    count: 42,
                }
            }));
            assert_eq!(result.message, "computed");
            assert_eq!(result.count, 42);
            assert_eq!(call_count, 1, "compute function should have been called once");
        }
    }

    #[test]
    fn test_get_or_compute_hit() {
        let backend = MockBackend::new();
        let key = TestKey("hit-key".into());
        let initial = TestValue {
            message: "cached".into(),
            count: 99,
        };
        // Pre-populate the cache
        if let Ok(json) = serde_json::to_string(&initial) {
            backend.store_raw(&key.cache_key(), &json);
        }

        let dyn_backend: &dyn CacheBackend = &backend;
        let mut call_count = 0u32;
        if let Ok(rt) = tokio::runtime::Runtime::new() {
            let result = rt.block_on(dyn_backend.get_or_compute(&key, async || {
                call_count += 1;
                TestValue {
                    message: "should-not-be-called".into(),
                    count: 0,
                }
            }));
            assert_eq!(result.message, "cached");
            assert_eq!(result.count, 99);
            assert_eq!(call_count, 0, "compute function should NOT have been called on cache hit");
        }
    }

    #[test]
    fn test_get_or_compute_overwrites_on_miss() {
        let backend = MockBackend::new();
        let key = TestKey("overwrite-key".into());
        let mut compute_count = 0u32;
        let dyn_backend: &dyn CacheBackend = &backend;

        // First call: cache miss, compute
        if let Ok(rt) = tokio::runtime::Runtime::new() {
            let r1 = rt.block_on(dyn_backend.get_or_compute(&key, async || {
                compute_count += 1;
                TestValue {
                    message: "first".into(),
                    count: 1,
                }
            }));
            assert_eq!(r1.message, "first");
        }

        // Second call: cache hit, should return cached value
        if let Ok(rt) = tokio::runtime::Runtime::new() {
            let r2 = rt.block_on(dyn_backend.get_or_compute(&key, async || {
                compute_count += 1;
                TestValue {
                    message: "second".into(),
                    count: 2,
                }
            }));
            assert_eq!(r2.message, "first", "should return cached 'first' not recomputed 'second'");
            assert_eq!(compute_count, 1, "compute should have been called exactly once");
        }
    }
}
