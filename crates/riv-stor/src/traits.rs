use mti::prelude::MagicTypeId;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::error::Error;

pub trait Storable: Sized + Serialize + DeserializeOwned + Send + Sync + 'static {
    type Options: Default + Sync;
    fn item_id(&self) -> &MagicTypeId;
}

/// Generic storage interface.
pub trait Store: Send + Sync + Clone {
    fn save<'a, T: Storable + riv_types::stor::Save>(
        &'a self,
        item: &'a T,
    ) -> impl Future<Output = Result<(), Error>> + Send + 'a;

    fn load<'a, T: Storable + riv_types::stor::EntityLoader + riv_types::stor::LoadChildren>(
        &'a self,
        id: &'a MagicTypeId,
    ) -> impl Future<Output = Result<Option<T>, Error>> + Send + 'a;

    fn list<T: Storable + riv_types::stor::EntityLoader>(
        &self,
        options: &T::Options,
    ) -> impl Future<Output = Result<Vec<T>, Error>> + Send;

    fn delete<T: Storable>(
        &self,
        id: &MagicTypeId,
    ) -> impl Future<Output = Result<bool, Error>> + Send;

    fn migrate(&self) -> impl Future<Output = Result<(), Error>> + Send;
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[derive(Clone)]
    struct DummyStore;

    impl Store for DummyStore {
        async fn save<T: Storable + riv_types::stor::Save>(&self, _item: &T) -> Result<(), Error> {
            Ok(())
        }
        async fn load<
            T: Storable + riv_types::stor::EntityLoader + riv_types::stor::LoadChildren,
        >(
            &self,
            _id: &MagicTypeId,
        ) -> Result<Option<T>, Error> {
            Ok(None)
        }
        async fn list<T: Storable + riv_types::stor::EntityLoader>(
            &self,
            _options: &T::Options,
        ) -> Result<Vec<T>, Error> {
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
}
