use leptos::prelude::*;
use serde::Serialize;
use serde::de::DeserializeOwned;

/// Small helper around `Resource` for async loading/data/error flows with manual refresh.
#[derive(Clone)]
pub struct ReloadableResource<T>
where
    T: Clone + Serialize + DeserializeOwned + Send + Sync + 'static,
{
    pub resource: Resource<Result<T, String>>,
    refresh_key: WriteSignal<u64>,
}

impl<T> ReloadableResource<T>
where
    T: Clone + Serialize + DeserializeOwned + Send + Sync + 'static,
{
    pub fn refresh(&self) {
        self.refresh_key.update(|n| *n += 1);
    }

    pub fn data(&self) -> Option<T> {
        self.resource.get().and_then(Result::ok)
    }

    pub fn error(&self) -> Option<String> {
        self.resource.get().and_then(Result::err)
    }

    pub fn loading(&self) -> bool {
        self.resource.get().is_none()
    }
}

pub fn create_reloadable_resource<T, Fetch, Fut>(fetch: Fetch) -> ReloadableResource<T>
where
    T: Clone + Serialize + DeserializeOwned + Send + Sync + 'static,
    Fetch: Fn() -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Result<T, String>> + Send + 'static,
{
    let (refresh_key, set_refresh_key) = signal(0_u64);
    let resource = Resource::new(
        move || refresh_key.get(),
        move |_| {
            let fetch = fetch.clone();
            async move { fetch().await }
        },
    );

    ReloadableResource {
        resource,
        refresh_key: set_refresh_key,
    }
}
