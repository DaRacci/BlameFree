pub mod api;
pub mod auth;

/// Registers a function to create an axum Router with the given routes and methods.
#[macro_export]
macro_rules! routes_register {
    (
        $($method:ident $route_const:ident => $func:ident),* $(,)?
    ) => {
        pub(crate) fn register_routes<S>(
            _state: &crate::server::AppState<S>,
        ) -> axum::routing::Router<crate::server::AppState<S>>
        where
            S: riv_stor::traits::Store + Send + Sync + Clone + 'static,
        {
            #[allow(unused_imports)]
            use axum::routing::{delete, get, patch, post, put};

            axum::routing::Router::new()
              $(
                .route($route_const, $method($func::<S>))
              )*

        }
    };
}
