use proc_macro::TokenStream;
use proc_macro2::{Punct, Spacing};
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::{Attribute, Data, DeriveInput, Fields, Ident, LitStr, Token, Type, parse_macro_input};

#[proc_macro_derive(Cacheable, attributes(cache_key, cache_ref))]
pub fn derive_cacheable(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let ref_name = format_ident!("{}Ref", name);

    let data = match &input.data {
        Data::Struct(s) => s,
        _ => panic!("Cacheable derive macro only supports structs"),
    };

    let fields = match &data.fields {
        Fields::Named(f) => &f.named,
        _ => panic!("Cacheable derive macro requires named fields"),
    };

    let mut cache_key_fields = Vec::new();
    let mut cache_ref_fields = Vec::new();
    let mut plain_fields = Vec::new();

    for field in fields.iter() {
        let ident = field.ident.as_ref().expect("named field");
        let ty = &field.ty;

        let has_cache_key = field.attrs.iter().any(|a| a.path().is_ident("cache_key"));
        let has_cache_ref = field.attrs.iter().any(|a| a.path().is_ident("cache_ref"));

        if has_cache_key {
            cache_key_fields.push((ident, ty));
        } else if has_cache_ref {
            cache_ref_fields.push((ident, ty));
        } else {
            plain_fields.push((ident, ty));
        }
    }

    let ref_fields: Vec<_> = cache_key_fields
        .iter()
        .map(|(ident, ty)| quote! { #ident: #ty })
        .chain(
            cache_ref_fields
                .iter()
                .map(|(ident, _)| quote! { #ident: String }),
        )
        .chain(
            plain_fields
                .iter()
                .map(|(ident, ty)| quote! { #ident: #ty }),
        )
        .collect();

    let ref_key_contributions: Vec<_> = cache_key_fields
        .iter()
        .map(|(ident, _)| {
            quote! { ::serde_json::to_string(&self.#ident).unwrap_or_default() }
        })
        .chain(cache_ref_fields.iter().map(|(ident, _)| {
            quote! { self.#ident.clone() }
        }))
        .collect();

    let into_ref_fields: Vec<_> = cache_key_fields
        .iter()
        .map(|(ident, _)| quote! { #ident: self.#ident })
        .chain(cache_ref_fields.iter().map(|(ident, _)| {
            quote! {
                #ident: {
                    let key = ::crb_cache::traits::CacheKey::cache_key(&self.#ident);
                    let serialized = ::serde_json::to_string(&self.#ident)
                        .expect("Cacheable::into_ref serialization failed");
                    ::crb_cache::traits::CacheBackend::store_raw(backend, &key, &serialized);
                    key
                }
            }
        }))
        .chain(
            plain_fields
                .iter()
                .map(|(ident, _)| quote! { #ident: self.#ident }),
        )
        .collect();

    let from_ref_fields: Vec<_> = cache_key_fields
        .iter()
        .map(|(ident, _)| quote! { #ident: form.#ident })
        .chain(cache_ref_fields.iter().map(|(ident, _)| {
            quote! {
                #ident: {
                    let serialized = ::crb_cache::traits::CacheBackend::load_raw(backend, &form.#ident);
                    ::serde_json::from_str(&serialized)
                        .expect("Cacheable::from_ref deserialization failed")
                }
            }
        }))
        .chain(plain_fields.iter().map(|(ident, _)| quote! { #ident: form.#ident }))
        .collect();

    let cache_key_params: Vec<_> = cache_key_fields
        .iter()
        .map(|(ident, ty)| quote! { #ident: &#ty })
        .chain(
            cache_ref_fields
                .iter()
                .map(|(ident, ty)| quote! { #ident: &#ty }),
        )
        .collect();

    let cache_key_contributions: Vec<_> = cache_key_fields
        .iter()
        .map(|(ident, _)| {
            quote! { ::serde_json::to_string(#ident).unwrap_or_default() }
        })
        .chain(cache_ref_fields.iter().map(|(ident, _)| {
            quote! { ::crb_cache::traits::CacheKey::cache_key(#ident) }
        }))
        .collect();

    let expanded = quote! {
        #[derive(::serde::Serialize, ::serde::Deserialize)]
        pub struct #ref_name {
            #(#ref_fields,)*
        }

        impl ::crb_cache::traits::CacheKey for #ref_name {
            fn cache_key(&self) -> String {
                use ::sha2::{Digest, Sha256};
                let mut hasher = Sha256::new();
                let parts: Vec<String> = vec![
                    #(#ref_key_contributions,)*
                ];
                hasher.update(parts.join(":"));
                format!("{:x}", hasher.finalize())
            }
        }

        impl ::crb_cache::traits::Cacheable for #name {
            type RefForm = #ref_name;

            fn into_ref(self, backend: &dyn ::crb_cache::traits::CacheBackend) -> Self::RefForm {
                #ref_name {
                    #(#into_ref_fields,)*
                }
            }

            fn from_ref(form: Self::RefForm, backend: &dyn ::crb_cache::traits::CacheBackend) -> Self {
                Self {
                    #(#from_ref_fields,)*
                }
            }
        }

        impl #name {
            pub fn cache_key(#(#cache_key_params),*) -> String {
                use ::sha2::{Digest, Sha256};
                let mut hasher = Sha256::new();
                let parts: Vec<String> = vec![
                    #(#cache_key_contributions,)*
                ];
                hasher.update(parts.join(":"));
                format!("{:x}", hasher.finalize())
            }
        }
    };

    TokenStream::from(expanded)
}

struct RouteEntry {
    name: Ident,
    template: LitStr,
}

impl Parse for RouteEntry {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let name = input.parse()?;
        input.parse::<Token![,]>()?;
        let template = input.parse()?;
        input.parse::<Token![;]>()?;
        Ok(RouteEntry { name, template })
    }
}

struct RouteInput {
    entries: Vec<RouteEntry>,
}

impl Parse for RouteInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut entries = Vec::new();
        while !input.is_empty() {
            entries.push(input.parse()?);
        }
        Ok(RouteInput { entries })
    }
}

fn metavar_ref(name: &str, span: proc_macro2::Span) -> proc_macro2::TokenStream {
    let dollar = Punct::new('$', Spacing::Alone);
    let ident = Ident::new(name, span);
    quote! { #dollar #ident }
}

fn metavar_binding(name: &str, span: proc_macro2::Span) -> proc_macro2::TokenStream {
    let dollar = Punct::new('$', Spacing::Alone);
    let ident = Ident::new(name, span);
    let colon = Punct::new(':', Spacing::Alone);
    let kind = Ident::new("ident", span);
    quote! { #dollar #ident #colon #kind }
}

fn parse_placeholders(template: &str) -> (String, Vec<String>) {
    let mut fmt = String::with_capacity(template.len());
    let mut placeholders = Vec::new();
    let mut chars = template.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == ':' {
            let mut name = String::new();
            while let Some(&next) = chars.peek() {
                if next.is_ascii_alphanumeric() || next == '_' {
                    name.push(next);
                    chars.next();
                } else {
                    break;
                }
            }
            if !name.is_empty() {
                placeholders.push(name);
                fmt.push_str("{}");
                continue;
            }
        }
        fmt.push(ch);
    }

    (fmt, placeholders)
}

#[proc_macro]
pub fn define_routes(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as RouteInput);

    let mut consts = Vec::new();
    let mut static_arms = Vec::new();
    let mut param_arms = Vec::new();

    for entry in &input.entries {
        let name = &entry.name;
        let template_lit = &entry.template;
        let template_val = entry.template.value();

        consts.push(quote! {
            pub const #name: &str = #template_lit;
        });

        let (fmt_str, placeholders) = parse_placeholders(&template_val);

        if placeholders.is_empty() {
            static_arms.push(quote! {
                (#name) => { #template_lit.to_string() };
            });
        } else {
            let span = name.span();
            let param_bindings: Vec<proc_macro2::TokenStream> = placeholders
                .iter()
                .map(|p| metavar_binding(p, span))
                .collect();
            let param_refs: Vec<proc_macro2::TokenStream> =
                placeholders.iter().map(|p| metavar_ref(p, span)).collect();
            let fmt_lit = LitStr::new(&fmt_str, entry.template.span());

            param_arms.push(quote! {
                (#name, #(#param_bindings),*) => {
                    format!(#fmt_lit, #(#param_refs),*)
                };
            });
        }
    }

    let expanded = quote! {
        #(#consts)*

        #[macro_export]
        macro_rules! route {
            #(#static_arms)*
            #(#param_arms)*
            ($name:ident $(, $arg:ident)* $(,)?) => {
                compile_error!(concat!(
                    "no matching `route!` arm for `", stringify!($name),
                    "` with the given arguments. Check route name and argument names/count."
                ))
            };
            ($name:ident, $($arg:expr),+ $(,)?) => {
                compile_error!("`route!` only accepts identifier arguments")
            };
        }
    };

    TokenStream::from(expanded)
}

/// Check if a type's last path segment matches a given name.
fn type_ends_with(ty: &Type, name: &str) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            return segment.ident == name;
        }
    }
    false
}

fn extract_key_value(
    attrs: &[Attribute],
    attribute_scope: &str,
    target_key: &str,
) -> Option<String> {
    for attr in attrs {
        if !attr.path().is_ident(attribute_scope) {
            continue;
        }

        let mut result: Option<String> = None;
        let _ = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident(target_key) {
                let value: LitStr = meta.value()?.parse()?;
                result = Some(value.value());
            }
            Ok(())
        });

        if let Some(name) = result {
            return Some(name);
        }
    }

    None
}

/// Parsed relation directive for a field's `#[sea_orm(...)]` attributes.
#[derive(Debug, Clone)]
enum RelationKind {
    /// Field becomes `HasMany<ChildEntity>`.
    HasMany { entity_alias: String },

    /// Field becomes `HasOne<ChildEntity>`.
    HasOne { entity_alias: String },

    /// Field stays as a scalar column but also generates a belongs_to
    /// Relation variant and `Related<ParentEntity>` impl.
    BelongsTo {
        entity_alias: String,
        from: String,
        to: String,
    },
}

/// Parsed & classified struct field.
struct ProcessedField {
    ident: Ident,

    ty: Type,

    /// True if the field has #[sea_orm(ignore)].
    is_ignored: bool,

    /// `#[sea_orm(...)]` tokens to pass through verbatim.
    passthrough: Vec<proc_macro2::TokenStream>,

    /// If non-None, this field is a relation.
    /// For HasMany/HasOne: the field becomes a pure relation (no DB column).
    /// For BelongsTo: the field stays as a scalar column + additional relation metadata is generated.
    relation: Option<RelationKind>,
}

/// Extract `key = "value"` from a raw token string for a specific key.
fn extract_value(tokens: &str, key: &str) -> String {
    for part in tokens.split(',') {
        let trimmed = part.trim();
        if let Some(eq_pos) = trimmed.find('=') {
            let k = trimmed[..eq_pos].trim();
            if k == key {
                return trimmed[eq_pos + 1..].trim().trim_matches('"').to_string();
            }
        }
    }
    String::new()
}

/// Scan a field's attrs and classify it as scalar / relation / ignored.
fn process_field(field: &syn::Field) -> ProcessedField {
    let ident = field.ident.clone().expect("named field");
    let ty = field.ty.clone();
    let mut is_ignored = false;
    let mut relation: Option<RelationKind> = None;
    let mut passthrough = Vec::new();

    for attr in &field.attrs {
        if !attr.path().is_ident("sea_orm") {
            continue;
        }

        let inner_ts = match attr.parse_args::<proc_macro2::TokenStream>() {
            Ok(ts) => ts,
            Err(_) => continue,
        };
        let inner_str = inner_ts.to_string();
        let inner_lower = inner_str.to_lowercase();

        if inner_lower.contains("ignore") {
            is_ignored = true;
            continue;
        }

        if inner_lower.contains("has_many") {
            relation = Some(RelationKind::HasMany {
                entity_alias: extract_value(&inner_str, "entity"),
            });
            continue;
        }

        if inner_lower.contains("has_one") {
            relation = Some(RelationKind::HasOne {
                entity_alias: extract_value(&inner_str, "entity"),
            });
            continue;
        }

        if inner_lower.contains("belongs_to") {
            relation = Some(RelationKind::BelongsTo {
                entity_alias: extract_value(&inner_str, "entity"),
                from: extract_value(&inner_str, "from"),
                to: extract_value(&inner_str, "to"),
            });
            continue;
        }

        // Pass through the original attribute verbatim
        passthrough.push(quote! { #[sea_orm(#inner_ts)] });
    }

    ProcessedField {
        ident,
        ty,
        is_ignored,
        passthrough,
        relation,
    }
}

/// Convert an entity alias like `"GoldenCommentEntity"` to a reference
/// to the re-exported Entity type via `super::`.
/// The user must ensure the entity type is in scope (re-exported at the
/// module level by the child type's own EntityModel derive).
fn entity_path_from_alias(alias: &str) -> proc_macro2::TokenStream {
    let alias_ident = Ident::new(alias, proc_macro2::Span::call_site());
    quote! { super::#alias_ident }
}

/// Convert snake_case to PascalCase (e.g. `"pr_result_id"` → `"PrResultId"`).
fn snake_to_pascal(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut capitalize_next = true;
    for ch in s.chars() {
        if ch == '_' {
            capitalize_next = true;
        } else if capitalize_next {
            result.extend(ch.to_uppercase());
            capitalize_next = false;
        } else {
            result.push(ch);
        }
    }
    result
}

/// Derive the SeaORM column type for a scalar domain field.
fn scalar_column_type(ty: &Type) -> proc_macro2::TokenStream {
    if type_ends_with(ty, "MagicTypeId") {
        return quote! { String };
    }
    // Check for Option<MagicTypeId>
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            if segment.ident == "Option" {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
                        if type_ends_with(inner, "MagicTypeId") {
                            return quote! { Option<String> };
                        }
                    }
                }
            }
        }
    }
    quote! { #ty }
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

    let data = match &input.data {
        Data::Struct(s) => s,
        _ => panic!("EntityModel derive macro only supports structs"),
    };

    let fields = match &data.fields {
        Fields::Named(f) => &f.named,
        _ => panic!("EntityModel derive macro requires named fields"),
    };

    let Some(table_name) = extract_key_value(&input.attrs, "sea_orm", "table_name") else {
        panic!("EntityModel: #[sea_orm(table_name = \"...\")] is required on the struct",);
    };

    let processed: Vec<ProcessedField> = fields.iter().map(process_field).collect();

    // Partition into scalar columns and relation fields
    let mut scalar_fields: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut rel_entries: Vec<(Ident, RelationKind)> = Vec::new(); // for enum + impls
    let mut has_explicit_id = false;

    for pf in &processed {
        if pf.is_ignored {
            continue;
        }

        let field_ident = &pf.ident;

        if let Some(ref kind) = pf.relation {
            match kind {
                RelationKind::HasMany { .. } | RelationKind::HasOne { .. } => {
                    // Pure relation field — no DB column
                    rel_entries.push((pf.ident.clone(), kind.clone()));
                }
                RelationKind::BelongsTo { .. } => {
                    // Field is a scalar column (the FK) AND generates a relation entry
                    let sea_attrs = if pf.passthrough.is_empty() {
                        quote! {}
                    } else {
                        let attrs = &pf.passthrough;
                        quote! { #(#attrs)* }
                    };
                    let out_ty = scalar_column_type(&pf.ty);
                    scalar_fields.push(quote! {
                        #sea_attrs
                        pub #field_ident: #out_ty
                    });
                    rel_entries.push((pf.ident.clone(), kind.clone()));
                }
            }
        } else {
            // Pure scalar (column) field
            if field_ident == "id" {
                has_explicit_id = true;
            }

            let sea_attrs: proc_macro2::TokenStream = if pf.passthrough.is_empty() {
                if field_ident == "id" {
                    quote! { #[sea_orm(primary_key, column_type = "Text")] }
                } else {
                    quote! {}
                }
            } else {
                let attrs = &pf.passthrough;
                quote! { #(#attrs)* }
            };

            // Check if this field has auto_increment — requires integer type
            let has_auto_increment = pf
                .passthrough
                .iter()
                .any(|ts| ts.to_string().to_lowercase().contains("auto_increment"));

            let out_ty = if has_auto_increment {
                quote! { i32 }
            } else {
                scalar_column_type(&pf.ty)
            };

            scalar_fields.push(quote! {
                #sea_attrs
                pub #field_ident: #out_ty
            });
        }
    }

    // Surrogate auto-increment PK when no explicit `id` field
    if !has_explicit_id {
        scalar_fields.insert(
            0,
            quote! {
                #[sea_orm(primary_key, auto_increment = true)]
                pub id: i32
            },
        );
    }

    let struct_name = &input.ident;
    let raw_name = struct_name.to_string();
    let model_mod = format_ident!("__{}_entity", raw_name.to_lowercase());
    let entity_name = format_ident!("{}Entity", raw_name);
    let active_name = format_ident!("{}ActiveModel", raw_name);
    let model_alias = format_ident!("{}Model", raw_name);
    let column_name = format_ident!("{}Column", raw_name);
    let pk_name = format_ident!("{}PrimaryKey", raw_name);

    // Build Relation enum variants
    let relation_variants: Vec<proc_macro2::TokenStream> = rel_entries
        .iter()
        .map(|(ident, _)| {
            let v = format_ident!("{}", Ident::new(&ident.to_string(), ident.span()));
            quote! { #v }
        })
        .collect();

    // Build RelationTrait::def() — handle empty relations gracefully
    let relation_trait_impl: proc_macro2::TokenStream =
        if rel_entries.is_empty() {
            quote! {
                impl RelationTrait for Relation {
                    fn def(&self) -> sea_orm::RelationDef {
                        match *self {}
                    }
                }
            }
        } else {
            let relation_def_arms: Vec<proc_macro2::TokenStream> = rel_entries
            .iter()
            .map(|(ident, kind)| {
                let v = Ident::new(&ident.to_string(), ident.span());
                match kind {
                    RelationKind::HasMany { entity_alias } => {
                        let child_path = entity_path_from_alias(entity_alias);
                        quote! {
                            Relation::#v => Entity::has_many(#child_path).into()
                        }
                    }
                    RelationKind::HasOne { entity_alias } => {
                        let child_path = entity_path_from_alias(entity_alias);
                        quote! {
                            Relation::#v => Entity::has_one(#child_path).into()
                        }
                    }
                    RelationKind::BelongsTo { entity_alias, from, to } => {
                        let parent_path = entity_path_from_alias(entity_alias);
                        let from_col_pascal_str = snake_to_pascal(from);
                        let from_col_pascal =
                            Ident::new(&from_col_pascal_str, proc_macro2::Span::call_site());
                        let to_col_pascal_str = snake_to_pascal(to);
                        let to_col_pascal =
                            Ident::new(&to_col_pascal_str, proc_macro2::Span::call_site());
                        quote! {
                            Relation::#v => Entity::belongs_to(#parent_path)
                                .from(Column::#from_col_pascal)
                                .to(<#parent_path as sea_orm::EntityTrait>::Column::#to_col_pascal)
                                .into()
                        }
                    }
                }
            })
            .collect();

            quote! {
                impl RelationTrait for Relation {
                    fn def(&self) -> sea_orm::RelationDef {
                        match self {
                            #(#relation_def_arms,)*
                        }
                    }
                }
            }
        };

    // Build impl Related<T> for Entity for BelongsTo relations
    let related_impls: Vec<proc_macro2::TokenStream> = rel_entries
        .iter()
        .filter_map(|(ident, kind)| {
            if let RelationKind::BelongsTo { entity_alias, .. } = kind {
                let parent_path = entity_path_from_alias(entity_alias);
                let rel_variant = ident; // field name IS the Relation variant name
                Some(quote! {
                    impl Related<#parent_path> for Entity {
                        fn to() -> sea_orm::RelationDef {
                            Relation::#rel_variant.def()
                        }
                        fn via() -> Option<sea_orm::RelationDef> {
                            None
                        }
                    }
                })
            } else {
                None
            }
        })
        .collect();

    let expanded = quote! {
        /// SeaORM entity model auto-generated by `EntityModel` derive.
        #[doc(hidden)]
        mod #model_mod {
            use sea_orm::entity::prelude::*;
            use super::*;

            #[derive(Debug, Clone, PartialEq, DeriveEntityModel)]
            #[sea_orm(table_name = #table_name)]
            pub struct Model {
                #(#scalar_fields,)*
            }

            #[derive(Debug, EnumIter)]
            pub enum Relation {
                #(#relation_variants,)*
            }

            #relation_trait_impl

            #(#related_impls)*

            impl ActiveModelBehavior for ActiveModel {}
        }

        pub use #model_mod::Model as #model_alias;
        pub use #model_mod::Entity as #entity_name;
        pub use #model_mod::Column as #column_name;
        pub use #model_mod::PrimaryKey as #pk_name;
        pub use #model_mod::ActiveModel as #active_name;
    };

    TokenStream::from(expanded)
}
