mod cache;
mod routes;
mod stor;

use proc_macro::TokenStream;
use syn::{DeriveInput, parse_macro_input};

use crate::routes::RouteInput;

#[proc_macro_derive(Cacheable, attributes(cache_key, cache_ref))]
pub fn derive_cacheable(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    cache::derive_cachable_impl(&input)
}

#[proc_macro]
pub fn define_routes(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as RouteInput);
    routes::define_routes_impl(&input)
}

/// Derive a SeaORM `Model` struct from a domain struct.
///
/// Generates a hidden entity module with `DeriveEntityModel`, `Relation` enum,
/// `RelationTrait`, and `Related<T>` impls. Supports `#[sea_orm(has_many)],
/// `#[sea_orm(has_one)]`, and `#[sea_orm(belongs_to)]` for relation fields,
/// plus pass-through of unknown `#[sea_orm(...)]` attributes.
///
/// # Usage
///
/// ```ignore
/// #[cfg_attr(feature = "seaorm-storage", derive(crb_macros::EntityModel))]
/// #[cfg_attr(feature = "seaorm-storage", sea_orm(table_name = "pr_results"))]
/// pub struct PrResult {
///     pub id: MagicTypeId,
///     #[cfg_attr(feature = "seaorm-storage",
///         sea_orm(has_many, entity = "GoldenCommentEntity")
///     )]
///     pub golden_comments: Vec<GoldenComment>,
/// }
/// ```
#[proc_macro_derive(EntityModel, attributes(sea_orm))]
pub fn derive_entity_model(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    stor::derive_entity_model_impl(&input)
}
