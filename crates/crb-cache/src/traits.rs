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
