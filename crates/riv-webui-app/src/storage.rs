//! Client-side form state persistence via `localStorage`.
//!
//! Keeps user-entered form fields and selections across full page reloads.
//! Values are stored as JSON under a namespaced key.
//! On the server these are no-ops so hydration and static rendering stay deterministic.

use leptos::prelude::*;
use serde::{Serialize, de::DeserializeOwned};

const PREFIX: &str = "blamefree.form.";

/// Restore a previously persisted JSON value for `key`, if any.
pub fn restore<T: DeserializeOwned>(key: &str) -> Option<T> {
    let raw = load(&format!("{PREFIX}{key}"))?;
    serde_json::from_str(&raw).ok()
}

/// Persist `value` as JSON under `key`.
pub fn persist<T: Serialize>(key: &str, value: &T) {
    let full_key = format!("{PREFIX}{key}");
    if let Ok(json) = serde_json::to_string(value) {
        store(&full_key, &json);
    }
}

/// Remove a persisted value for `key`.
pub fn clear(key: &str) {
    remove(&format!("{PREFIX}{key}"));
}

/// Attach a reactive effect that persists `value` to storage whenever it changes.
///
/// Useful for signals that are otherwise initialized from defaults.
pub fn persist_signal<T: Serialize + Clone + Send + Sync + 'static>(
    key: &'static str,
    value: ReadSignal<T>,
) {
    Effect::new(move || {
        let current = value.get();
        persist(key, &current);
    });
}

/// Set `set_value` to a persisted value once, after mount.
///
/// Runs on the next animation frame so the write is a reactive update that notifies dependents.
/// On the server and when no value is stored it is a no-op, so SSR/hydration stay deterministic.
pub fn restore_signal<T: Serialize + DeserializeOwned + Clone + Send + Sync + 'static>(
    key: &'static str,
    set_value: WriteSignal<T>,
) {
    let Some(saved) = restore::<T>(key) else {
        return;
    };

    request_animation_frame(move || {
        set_value.set(saved);
    });
}

fn load(key: &str) -> Option<String> {
    cfg_select! {
      target_arch = "wasm32" => {
        let storage = web_sys::window()?.local_storage().ok().flatten()?;
        storage.get_item(key).ok().flatten()
      }
      _ => None
    }
}

#[allow(unused_variables)]
fn store(key: &str, value: &str) {
    cfg_select! {
      target_arch = "wasm32" => {
        let Some(storage) = web_sys::window()
            .and_then(|w| w.local_storage().ok())
            .flatten()
        else {
            return;
        };
        let _ = storage.set_item(key, value);
      }
      _ => {}
    }
}

#[allow(unused_variables)]
fn remove(key: &str) {
    cfg_select! {
      target_arch = "wasm32" => {
        let Some(storage) = web_sys::window()
            .and_then(|w| w.local_storage().ok())
            .flatten()
        else {
            return;
        };
        let _ = storage.remove_item(key);
      }
      _ => {}
    }
}
