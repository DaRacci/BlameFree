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
