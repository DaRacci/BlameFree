use mti::prelude::MagicTypeId;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::error::Error;
use crb_types::stor::Save;

/// Marker trait for domain types that can be persisted.
///
/// Types implementating this trait declare:
/// - Their unique identifier via [`item_id`](Storable::item_id)
/// - Per-type query options via the [`Options`](Storable::Options) associated type
pub trait Storable: Sized + Serialize + DeserializeOwned + Send + Sync + 'static {
    /// Per-type filter and list options (e.g. pagination, status filter).
    ///
    /// Use [`Default`] when no options are needed.
    type Options: Default + Sync;

    /// Returns a reference to this item's unique identifier.
    fn item_id(&self) -> &MagicTypeId;
}

/// Generic storage interface
pub trait Store: Send + Sync + Clone {
    /// Persist an item.
    ///
    /// Inserts if new, updates if existing (upsert).
    fn save<T: Storable + crb_types::stor::Save>(
        &self,
        item: &T,
    ) -> impl Future<Output = Result<(), Error>>;

    /// Load a single item by its `MagicTypeId`.
    ///
    /// Returns `None` if not found.
    fn load<T: Storable>(&self, id: &MagicTypeId)
    -> impl Future<Output = Result<Option<T>, Error>>;

    /// List items matching the type-specific options.
    fn list<T: Storable>(
        &self,
        options: &T::Options,
    ) -> impl Future<Output = Result<Vec<T>, Error>>;

    /// Delete an item by its `MagicTypeId`.
    ///
    /// Returns `true` if something was deleted.
    fn delete<T: Storable>(&self, id: &MagicTypeId) -> impl Future<Output = Result<bool, Error>>;

    /// Run schema migrations.
    /// Called once at startup.
    ///
    /// This function is idempotent and safe to call multiple times.
    fn migrate(&self) -> impl Future<Output = Result<(), Error>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Compile-time check: a minimal Storable implementor.
    #[derive(serde::Serialize, serde::Deserialize)]
    struct DummyItem {
        id: MagicTypeId,
    }

    impl Storable for DummyItem {
        type Options = ();
        fn item_id(&self) -> &MagicTypeId {
            &self.id
        }
    }

    /// Compile-time check: a minimal Store implementor.
    #[derive(Clone)]
    struct DummyStore;

    impl Store for DummyStore {
        async fn save<T: Storable + crb_types::stor::Save>(&self, _item: &T) -> Result<(), Error> {
            Ok(())
        }

        async fn load<T: Storable>(&self, _id: &MagicTypeId) -> Result<Option<T>, Error> {
            Ok(None)
        }

        async fn list<T: Storable>(&self, _options: &T::Options) -> Result<Vec<T>, Error> {
            Ok(Vec::new())
        }

        async fn delete<T: Storable>(&self, _id: &MagicTypeId) -> Result<bool, Error> {
            Ok(false)
        }

        async fn migrate(&self) -> Result<(), Error> {
            Ok(())
        }
    }

    #[test]
    fn test_dummy_store_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<DummyStore>();
    }

    #[test]
    fn test_storable_trait_object_safety() {
        // Storable is NOT object-safe (generic), but it should be usable as a bound.
        fn requires_storable<T: Storable>() {}
        requires_storable::<DummyItem>();
    }
}
