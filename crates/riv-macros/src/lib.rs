mod cache;
mod flatten;
mod helpers;
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

/// Derive `FlattenedStruct` — generates a runtime `FlattenedStruct` trait
/// implementation for struct flattening to/from flat column maps.
///
/// Supports `#[flattened(prefix = "...")]` attribute on fields that are
/// themselves `FlattenedStruct` types, to control the column name prefix.
///
/// # Usage
///
/// ```ignore
/// #[derive(FlattenedStruct)]
/// pub struct MyStruct {
///     pub name: String,
///     pub value: i32,
///     #[flattened(prefix = "child")]
///     pub child: ChildStruct,
/// }
/// ```
#[proc_macro_derive(FlattenedStruct, attributes(flattened))]
pub fn derive_flattened_struct(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    flatten::derive_flattened_struct_impl(&input)
}

/// Derive `FlattenedEnum` — generates a bridge struct and `From` impls
/// that convert between the enum and a flat `__<Enum>Columns` struct.
///
/// Each variant must carry `#[variant(tag = "...", fields(col1: Type1, col2: Type2))]`.
/// Field paths are NOT supported — use `FlattenedStruct` derive on inner types instead.
///
/// # Usage
///
/// ```ignore
/// #[derive(FlattenedEnum)]
/// pub enum MyEnum {
///     #[variant(tag = "foo", fields(
///         name: String,
///         count: i32,
///     ))]
///     Foo(SomeStruct),
///
///     #[variant(tag = "bar", fields(
///         value: String,
///     ))]
///     Bar(AnotherStruct),
///
///     #[variant(tag = "none")]
///     None,
/// }
/// ```
#[proc_macro_derive(FlattenedEnum, attributes(variant))]
pub fn derive_flattened_enum(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    flatten::derive_flattened_enum_impl(&input)
}

/// Derive a SeaORM `Model` struct from a domain struct.
///
/// Generates a hidden entity module with `DeriveEntityModel`, `Relation` enum,
/// `RelationTrait`, and `Related<T>` impls. Supports `#[sea_orm(has_many)],
/// `#[sea_orm(has_one)]`, and `#[sea_orm(belongs_to)]` for relation fields,
/// plus pass-through of unknown `#[sea_orm(...)]` attributes.
///
/// Also supports `#[flatten(tag = "...", variants = { ... })]` on fields to
/// generate Model columns from a flattened enum.
///
/// # Usage
///
/// ```ignore
/// #[cfg_attr(feature = "seaorm-storage", derive(riv_macros::EntityModel))]
/// #[cfg_attr(feature = "seaorm-storage", sea_orm(table_name = "pr_results"))]
/// pub struct PrResult {
///     pub id: MagicTypeId,
///     #[cfg_attr(feature = "seaorm-storage",
///         sea_orm(has_many, entity = "GoldenCommentEntity")
///     )]
///     pub golden_comments: Vec<GoldenComment>,
/// }
/// ```
#[proc_macro_derive(EntityModel, attributes(sea_orm, flatten))]
pub fn derive_entity_model(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    stor::derive_entity_model_impl(&input)
}
