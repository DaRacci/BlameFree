use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{format_ident, quote};
use syn::{
    Data, DeriveInput, Fields, Ident, LitStr, Token, Type,
    parse::{Parse, ParseStream},
};

use crate::helpers::{
    extract_key_value, extract_value, get_option_inner, snake_to_pascal, to_snake_case,
    type_ends_with, type_last_segment, unwrap_option_type,
};

/// Parsed `#[flatten(tag = "...", variants = { ... })]` attribute.
#[derive(Debug, Clone)]
pub struct FlattenSpec {
    /// Discriminator column name.
    pub tag: String,
    /// Each variant's flat column definitions.
    pub variants: Vec<FlattenVariantDef>,
}

/// A single variant's column definitions within a flatten annotation.
#[derive(Debug, Clone)]
pub struct FlattenVariantDef {
    /// The tag VALUE for this variant.
    #[allow(unused)]
    pub name: String,
    /// Column definitions: (column_name, type_string).
    /// Type string is the DB column type.
    pub columns: Vec<(String, String)>,
}

/// Parse `flatten(tag = "...", variants = { name => { col: Ty, ... }, ... })`.
impl Parse for FlattenSpec {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut tag = String::new();
        let mut variants: Vec<FlattenVariantDef> = Vec::new();

        while !input.is_empty() {
            let key: Ident = input.parse()?;

            match key.to_string().as_str() {
                "tag" => {
                    let _: Token![=] = input.parse()?;
                    let lit: LitStr = input.parse()?;
                    tag = lit.value();
                }
                "variants" => {
                    let _: Token![=] = input.parse()?;
                    let variant_braces;
                    syn::braced!(variant_braces in input);
                    while !variant_braces.is_empty() {
                        let variant_name: Ident = variant_braces.parse()?;
                        let _: Token![=>] = variant_braces.parse()?;
                        let col_braces;
                        syn::braced!(col_braces in variant_braces);
                        let mut columns: Vec<(String, String)> = Vec::new();
                        while !col_braces.is_empty() {
                            let col_name: Ident = col_braces.parse()?;
                            let _: Token![:] = col_braces.parse()?;
                            let col_type: Type = col_braces.parse()?;
                            columns.push((col_name.to_string(), quote!(#col_type).to_string()));
                            let _ = col_braces.parse::<Token![,]>();
                        }
                        variants.push(FlattenVariantDef {
                            name: variant_name.to_string(),
                            columns,
                        });
                        let _ = variant_braces.parse::<Token![,]>();
                    }
                }
                _ => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!("expected `tag` or `variants`, got `{key}`"),
                    ));
                }
            }

            let _ = input.parse::<Token![,]>();
        }

        if tag.is_empty() {
            return Err(syn::Error::new(
                input.span(),
                "missing required `tag = \"...\"` in #[flatten(...)]",
            ));
        }

        if variants.is_empty() {
            return Err(syn::Error::new(
                input.span(),
                "missing required `variants = { ... }` in #[flatten(...)]",
            ));
        }

        Ok(FlattenSpec { tag, variants })
    }
}

/// Parsed relation directive for a field's `#[sea_orm(...)]` attributes.
#[derive(Debug, Clone)]
pub enum RelationKind {
    /// Field becomes `HasMany<ChildEntity>`.
    HasMany {
        entity_alias: String,
        /// Optional FK column override: `child_fk = "session_id"`.
        child_fk: Option<String>,
        /// Optional tuple linked child: `tuple(linked_child = "JudgeVerdictEntity")`.
        tuple_linked_child: Option<String>,
    },

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
pub struct ProcessedField {
    pub ident: Ident,
    pub ty: Type,

    /// True if the field has #[sea_orm(ignore)].
    pub is_ignored: bool,

    /// True if the field is the primary key.
    pub is_primary_key: bool,

    /// True if the PK is auto-incremented by the DB.
    pub auto_increment: bool,

    /// `#[sea_orm(...)]` tokens to pass through verbatim.
    pub passthrough: Vec<TokenStream2>,

    /// If non-None, this field is a relation.
    pub relation: Option<RelationKind>,

    /// If non-None, this field uses #[flatten(...)].
    pub flatten: Option<FlattenSpec>,
}

pub fn derive_entity_model_impl(input: &DeriveInput) -> TokenStream {
    let data = match &input.data {
        Data::Struct(s) => s,
        _ => panic!("EntityModel derive macro only supports structs"),
    };

    let fields = match &data.fields {
        Fields::Named(f) => &f.named,
        _ => panic!("EntityModel derive macro requires named fields"),
    };

    let Some(table_name) = extract_key_value(&input.attrs, "sea_orm", "table_name") else {
        panic!("EntityModel: #[sea_orm(table_name = \"...\")] is required on the struct");
    };

    let skip_save = input
        .attrs
        .iter()
        .filter(|a| a.path().is_ident("sea_orm"))
        .any(|a| {
            if let Ok(ts) = a.parse_args::<TokenStream2>() {
                ts.to_string().to_lowercase().contains("skip_save")
            } else {
                false
            }
        });

    let mut processed: Vec<ProcessedField> = fields.iter().map(process_field).collect();

    // Post-fixup: mark implicit `id` fields as primary keys.
    // These are fields named `id` that have no `#[sea_orm(...)]` attribute —
    // the macro synthesizes `#[sea_orm(primary_key, column_type = "Text")]` for them.
    for pf in &mut processed {
        if !pf.is_primary_key && pf.ident == "id" && pf.relation.is_none() && pf.flatten.is_none() {
            pf.is_primary_key = true;
        }
    }

    // Partition into scalar columns, relation fields, and flatten fields
    let mut scalar_fields: Vec<TokenStream2> = Vec::new();
    let mut rel_entries: Vec<(Ident, RelationKind)> = Vec::new();
    let mut has_explicit_id = false;

    let mut flatten_entries: Vec<(Ident, FlattenSpec)> = Vec::new();
    for pf in &processed {
        if pf.is_ignored {
            continue;
        }

        if let Some(ref spec) = pf.flatten {
            flatten_entries.push((pf.ident.clone(), spec.clone()));

            // All flatten columns are Option<String> for universal compatibility.
            // Emit the discriminator column first.
            let tag_col = &spec.tag;
            let tag_col_ident = Ident::new(tag_col, Span::call_site());
            scalar_fields.push(quote! {
                pub #tag_col_ident: Option<String>
            });

            // Emit all variant columns as Option<String>
            let mut seen_cols: Vec<String> = Vec::new();
            for vd in &spec.variants {
                for (col_name, _) in &vd.columns {
                    if seen_cols.contains(col_name) {
                        continue;
                    }
                    seen_cols.push(col_name.clone());
                    let col_ident = Ident::new(col_name, Span::call_site());
                    scalar_fields.push(quote! {
                        pub #col_ident: Option<String>
                    });
                }
            }
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
            if field_ident == "id" {
                has_explicit_id = true;
            }

            let sea_attrs: TokenStream2 = if pf.passthrough.is_empty() {
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
                // auto_increment requires the bare type, not Option.
                let bare_ty = get_option_inner(&pf.ty).unwrap_or(pf.ty.clone());
                scalar_column_type(&bare_ty)
            } else {
                scalar_column_type(&pf.ty)
            };

            scalar_fields.push(quote! {
                #sea_attrs
                pub #field_ident: #out_ty
            });
        }
    }

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

    let relation_variants = build_relation_variants(&rel_entries);
    let relation_trait_impl = build_relation_trait_impl(&rel_entries);
    let related_impls = build_related_impls(rel_entries);
    let (from_domain_impl, from_model_impl) =
        build_from_traits(&processed, flatten_entries, struct_name);
    let entity_id_impl = build_entity_id_impl(&processed, has_explicit_id, struct_name);
    let new_impl = build_new_function(&processed, struct_name);
    let link_impl = build_link_function(&processed, struct_name);
    let save_impl = if skip_save {
        quote! {}
    } else {
        build_save_function(&processed, struct_name, raw_name, &active_name)
    };

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

            #from_model_impl

            #from_domain_impl
        }

        pub use #model_mod::Model as #model_alias;
        pub use #model_mod::Entity as #entity_name;
        pub use #model_mod::Column as #column_name;
        pub use #model_mod::PrimaryKey as #pk_name;
        pub use #model_mod::ActiveModel as #active_name;

        #entity_id_impl
        #new_impl
        #link_impl
        #save_impl
    };

    TokenStream::from(expanded)
}

fn build_relation_variants(rel_entries: &Vec<(Ident, RelationKind)>) -> Vec<TokenStream2> {
    // Build Relation enum variants
    let relation_variants: Vec<TokenStream2> = rel_entries
        .iter()
        .map(|(ident, _)| {
            let v = format_ident!("{}", snake_to_pascal(&ident.to_string()));
            quote! { #v }
        })
        .collect();
    relation_variants
}

// Build RelationTrait::def()
fn build_relation_trait_impl(rel_entries: &Vec<(Ident, RelationKind)>) -> TokenStream2 {
    if rel_entries.is_empty() {
        return quote! {
            impl RelationTrait for Relation {
                fn def(&self) -> sea_orm::RelationDef {
                    match *self {}
                }
            }
        };
    }

    let relation_def_arms: Vec<TokenStream2> = rel_entries
        .iter()
        .map(|(ident, kind)| {
            let v_str = snake_to_pascal(&ident.to_string());
            let v = Ident::new(&v_str, Span::call_site());
            match kind {
                RelationKind::HasMany { entity_alias, .. } => {
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

                RelationKind::BelongsTo {
                    entity_alias,
                    from,
                    to,
                } => {
                    let parent_path = entity_path_from_alias(entity_alias);
                    let from_col_pascal_str = snake_to_pascal(from);
                    let from_col_pascal = Ident::new(&from_col_pascal_str, Span::call_site());
                    let to_col_pascal_str = snake_to_pascal(to);
                    let to_col_pascal = Ident::new(&to_col_pascal_str, Span::call_site());
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
}

// Build impl Related<T> for Entity for BelongsTo relations
fn build_related_impls(rel_entries: Vec<(Ident, RelationKind)>) -> Vec<TokenStream2> {
    rel_entries
        .iter()
        .filter_map(|(ident, kind)| {
            let RelationKind::BelongsTo { entity_alias, .. } = kind else {
                return None;
            };

            let parent_path = entity_path_from_alias(entity_alias);
            let v_str = snake_to_pascal(&ident.to_string());
            let rel_variant = Ident::new(&v_str, Span::call_site());
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
        })
        .collect()
}

// Build From<Model> for DomainType and From<DomainType> for ActiveModel
fn build_from_traits(
    processed: &Vec<ProcessedField>,
    flatten_entries: Vec<(Ident, FlattenSpec)>,
    struct_name: &Ident,
) -> (TokenStream2, TokenStream2) {
    let mut from_model_fields: Vec<TokenStream2> = Vec::new();
    let mut from_domain_fields: Vec<TokenStream2> = Vec::new();

    // Track which columns are set by flatten fields so scalar fields don't conflict
    let mut flatten_col_names: Vec<String> = Vec::new();
    for (_, spec) in &flatten_entries {
        flatten_col_names.push(spec.tag.clone());
        for vd in &spec.variants {
            for (col_name, _) in &vd.columns {
                if !flatten_col_names.contains(col_name) {
                    flatten_col_names.push(col_name.clone());
                }
            }
        }
    }

    for pf in processed {
        let ident = &pf.ident;
        let is_pure_rel = matches!(
            pf.relation,
            Some(RelationKind::HasMany { .. }) | Some(RelationKind::HasOne { .. })
        );
        let is_scalar = !pf.is_ignored && !is_pure_rel && pf.flatten.is_none();
        let has_auto_increment = pf
            .passthrough
            .iter()
            .any(|ts| ts.to_string().to_lowercase().contains("auto_increment"));

        let is_mtid = type_ends_with(&pf.ty, "MagicTypeId");
        let is_opt_mtid =
            get_option_inner(&pf.ty).is_some_and(|inner| type_ends_with(&inner, "MagicTypeId"));

        if is_scalar {
            let model_expr = if has_auto_increment {
                quote! { Default::default() }
            } else if is_mtid {
                quote! { m.#ident.parse::<mti::prelude::MagicTypeId>().unwrap_or_default() }
            } else if is_opt_mtid {
                quote! { m.#ident.map(|s| s.parse::<mti::prelude::MagicTypeId>().unwrap_or_default()) }
            } else {
                quote! { m.#ident }
            };
            from_model_fields.push(quote! { #ident: #model_expr });
        }

        if is_scalar {
            let domain_expr = if has_auto_increment {
                quote! { sea_orm::NotSet }
            } else if is_mtid {
                quote! { sea_orm::Set(d.#ident.to_string()) }
            } else if is_opt_mtid {
                quote! { sea_orm::Set(d.#ident.as_ref().map(|x| x.to_string())) }
            } else {
                quote! { sea_orm::Set(d.#ident) }
            };
            from_domain_fields.push(quote! { #ident: #domain_expr });
        }
    }

    let mut flatten_let_bindings: Vec<TokenStream2> = Vec::new();
    let mut flatten_set_fields: Vec<TokenStream2> = Vec::new();
    for (idx, (field_ident, spec)) in flatten_entries.iter().enumerate() {
        let pf = processed
            .iter()
            .find(|pf| pf.ident == *field_ident)
            .unwrap();
        let field_ty_name = type_last_segment(&pf.ty);

        let bridge_ident = format_ident!("__{}Columns", field_ty_name);
        let cols_ident = format_ident!("__cols_{}", idx);

        let mut all_flatten_cols: Vec<(String, String)> = Vec::new();
        all_flatten_cols.push((spec.tag.clone(), "Option<String>".to_string()));
        for vd in &spec.variants {
            for (col_name, col_type) in &vd.columns {
                if !all_flatten_cols.iter().any(|(c, _)| c == col_name) {
                    all_flatten_cols.push((col_name.clone(), format!("Option<{col_type}")));
                }
            }
        }

        let set_fields: Vec<TokenStream2> = all_flatten_cols
            .iter()
            .map(|(col_name, _)| {
                let col_ident = Ident::new(col_name, Span::call_site());
                quote! { #col_ident: sea_orm::Set(#cols_ident.#col_ident) }
            })
            .collect();

        flatten_let_bindings.push(quote! {
            let #cols_ident: #bridge_ident = d.#field_ident.into();
        });
        flatten_set_fields.extend(set_fields);
    }

    let has_flatten = !flatten_entries.is_empty();
    let has_ignored_fields = processed
        .iter()
        .any(|pf| pf.is_ignored && pf.flatten.is_none());
    let has_pure_relations = processed.iter().any(|pf| {
        matches!(
            pf.relation,
            Some(RelationKind::HasMany { .. }) | Some(RelationKind::HasOne { .. })
        ) && pf.flatten.is_none()
    });
    // Pure relation fields (HasMany/HasOne) are excluded from from_model_fields,
    // so the struct literal needs ..Default::default() to fill them.
    let needs_default_tail = has_ignored_fields || has_pure_relations;

    let from_model_impl = if from_model_fields.is_empty() {
        quote! {}
    } else if has_ignored_fields && !has_flatten {
        // Can't map ignored fields from Model — developer must add #[flatten] or remove ignore.
        quote! {
            compile_error!(
                "EntityModel: struct has #[sea_orm(ignore)] fields but no #[flatten]; \
                 either remove the ignore or add flatten so defaults can be used"
            );
        }
    } else if has_pure_relations && !has_flatten {
        // Structs with HasMany/HasOne but no flatten: skip From<Model>.
        // These structs don't derive Default and aren't loaded via model.into().
        quote! {}
    } else {
        // Either all fields are populated from Model, or there are flatten fields that we populate via the bridge struct.
        // If there are also ignored fields, use ..Default::default()
        let normal_fields: Vec<_> = from_model_fields.iter().cloned().collect();
        let default_tail = if needs_default_tail {
            quote! { ..Default::default() }
        } else {
            quote! {}
        };

        quote! {
            impl From<Model> for super::#struct_name {
                fn from(m: Model) -> Self {
                    Self {
                        #(#normal_fields,)*
                        #default_tail
                    }
                }
            }
        }
    };

    let final_from_domain_fields: Vec<TokenStream2> = from_domain_fields
        .into_iter()
        .chain(flatten_set_fields)
        .collect();

    let from_domain_impl = if final_from_domain_fields.is_empty() && flatten_let_bindings.is_empty()
    {
        quote! {}
    } else {
        let set_fields = &final_from_domain_fields;
        let let_bindings = &flatten_let_bindings;
        quote! {
            impl From<super::#struct_name> for ActiveModel {
                fn from(d: super::#struct_name) -> Self {
                    #(#let_bindings)*
                    Self {
                        #(#set_fields,)*
                    }
                }
            }
        }
    };

    (from_domain_impl, from_model_impl)
}

fn build_entity_id_impl(
    processed: &Vec<ProcessedField>,
    has_explicit_id: bool,
    struct_name: &Ident,
) -> TokenStream2 {
    let return_quote = |entity_id_ty, get_id_expr| {
        quote! {
            impl crate::EntityId for #struct_name {
                type Id = #entity_id_ty;
                fn get_id(&self) -> Option<Self::Id> {
                    #get_id_expr
                }
            }
        }
    };

    if !has_explicit_id {
        return return_quote(quote!(i32), quote! { Some(self.id.clone()) });
    }

    let pk = processed
        .iter()
        .find(|pf| pf.is_primary_key)
        .expect("EntityModel: no primary key found");

    let pk_ident = &pk.ident;

    let (inner_ty, get_id_expr) = get_option_inner(&pk.ty)
        .map(|ty| (ty, quote! { self.#pk_ident.clone() }))
        .unwrap_or_else(|| (pk.ty.clone(), quote! { Some(self.#pk_ident.clone()) }));

    return_quote(quote!(#inner_ty), get_id_expr)
}

fn build_new_function(processed: &Vec<ProcessedField>, struct_name: &Ident) -> TokenStream2 {
    // Skip generation if any field has #[sea_orm(ignore)]
    // we can't guess a reasonable default for arbitrary types.
    if processed.iter().any(|pf| pf.is_ignored) {
        return quote! {};
    }

    // Build complete field list for the struct literal.
    // Each entry: (field_name, expression_token_stream)
    let mut all_fields = Vec::new();
    let mut new_param_decls = Vec::new();

    for pf in processed {
        if pf.flatten.is_some() {
            // flatten fields handled via fallback below
            continue;
        }

        let name = &pf.ident;

        if matches!(
            pf.relation,
            Some(RelationKind::HasMany { .. }) | Some(RelationKind::HasOne { .. })
        ) {
            all_fields.push((name.clone(), quote! { Vec::new() }));
            continue;
        }

        if pf.is_primary_key && pf.auto_increment {
            all_fields.push((name.clone(), quote! { None }));
            continue;
        }

        if matches!(pf.relation, Some(RelationKind::BelongsTo { .. }))
            && get_option_inner(&pf.ty).is_some()
        {
            all_fields.push((name.clone(), quote! { None }));
            continue;
        }

        let ty = pf.ty.clone();
        all_fields.push((name.clone(), quote! { #name }));
        new_param_decls.push(quote! { #name: #ty });
    }

    // If there are flatten fields, use ..Default::default().
    // Otherwise emit a complete literal.
    let has_flatten = processed.iter().any(|pf| pf.flatten.is_some());
    let defaults_fallback = if has_flatten {
        quote! { ..Default::default() }
    } else {
        quote! {}
    };

    if new_param_decls.is_empty() && !has_flatten {
        return quote! {};
    }

    let set_fields = all_fields
        .iter()
        .map(|(name, expr)| quote! { #name: #expr });

    quote! {
        impl #struct_name {
            pub fn new(#(#new_param_decls),*) -> Self {
                Self {
                    #(#set_fields,)*
                    #defaults_fallback
                }
            }
        }
    }
}

fn build_link_function(processed: &Vec<ProcessedField>, struct_name: &Ident) -> TokenStream2 {
    struct LinkParam {
        ident: Ident,
        entity_alias: String,
        ty: TokenStream2,
    }

    let fk_entries: Vec<_> = processed
        .iter()
        .filter_map(|pf| {
            pf.relation.as_ref().and_then(|rel| match rel {
                RelationKind::BelongsTo { entity_alias, .. } => Some(LinkParam {
                    ident: pf.ident.clone(),
                    entity_alias: entity_alias.clone(),
                    ty: unwrap_option_type(&pf.ty),
                }),
                _ => None,
            })
        })
        .collect();

    if fk_entries.is_empty() {
        return quote! {};
    }

    fn get_entity_type(str: &String) -> &str {
        str.strip_suffix("Entity").unwrap_or(str)
    }

    let link_params: Vec<_> = fk_entries
        .iter()
        .map(|link| {
            let entity_type = get_entity_type(&link.entity_alias);
            let param_name = Ident::new(&to_snake_case(entity_type), Span::call_site());
            let inner_ty = &link.ty;
            quote! { #param_name: &dyn crate::EntityId<Id = #inner_ty> }
        })
        .collect();

    let link_sets: Vec<_> = fk_entries
        .iter()
        .map(|link| {
            let entity_type = get_entity_type(&link.entity_alias);
            let param_name = Ident::new(&to_snake_case(entity_type), Span::call_site());
            let fk_ident = &link.ident;

            let fk_pf = processed.iter().find(|pf| pf.ident == link.ident).unwrap();
            let is_opt_fk = type_ends_with(&fk_pf.ty, "Option");
            let get_id_expr = match is_opt_fk {
                true => quote!(#param_name.get_id()),
                false => quote!(#param_name.get_id().expect("link: entity missing id")),
            };

            quote! { self.#fk_ident = #get_id_expr; }
        })
        .collect();

    quote! {
        impl #struct_name {
            pub fn link(mut self, #(#link_params),*) -> Self {
                #(#link_sets)*
                self
            }
        }
    }
}

fn build_save_function(
    processed: &[ProcessedField],
    struct_name: &Ident,
    raw_name: String,
    active_name: &Ident,
) -> TokenStream2 {
    use proc_macro2::Span;

    struct ChildSpec {
        field_ident: Ident,
        fk: String,
        tuple_linked: Option<String>,
    }

    let mut child_specs = Vec::new();
    for pf in processed {
        if let Some(RelationKind::HasMany {
            child_fk,
            tuple_linked_child,
            ..
        }) = &pf.relation
        {
            // Detect tuple Vec<(A, B)> — require annotation or fail loudly
            let ty = &pf.ty;
            let ty_str = quote!(#ty).to_string();
            if ty_str.contains('(') {
                if tuple_linked_child.is_none() {
                    let field_name = pf.ident.to_string();
                    return quote! {
                        compile_error!(concat!(
                            "EntityModel: field ", #field_name,
                            " is a tuple HasMany (Vec<(A, B)>). Add tuple = \"ChildEntity\" to the #[sea_orm(has_many)] attribute."
                        ));
                    };
                };
            }
            let child_fk = child_fk
                .clone()
                .unwrap_or_else(|| format!("{}_id", to_snake_case(&raw_name)));

            child_specs.push(ChildSpec {
                field_ident: pf.ident.clone(),
                fk: child_fk,
                tuple_linked: tuple_linked_child.clone(),
            });
        }
    }

    let auto_inc = processed
        .iter()
        .any(|pf| pf.is_primary_key && pf.auto_increment);

    if child_specs.is_empty() {
        let insert_body = if auto_inc {
            quote! {
                let active = #active_name::from(self.clone());
                let _ = active.insert(db).await?;
                Ok(())
            }
        } else {
            quote! {
                let active = #active_name::from(self.clone());
                match active.clone().insert(db).await {
                    Ok(_) => Ok(()),
                    Err(e) if e.to_string().to_lowercase().contains("unique") => {
                        active.update(db).await
                            .map_err(|e| ::anyhow::anyhow!("update failed: {e}"))?;
                        Ok(())
                    }
                    Err(e) => Err(::anyhow::anyhow!("insert failed: {e}")),
                }
            }
        };
        return quote! {
            impl crate::Save for #struct_name {
                async fn save(&self, db: &::sea_orm::DatabaseConnection) -> Result<(), ::anyhow::Error> {
                    use ::sea_orm::{ActiveModelTrait, IntoActiveModel};
                    #insert_body
                }
            }
        };
    }

    let parent_insert = if auto_inc {
        quote! {
            let active = #active_name::from(self.clone());
            let saved_parent = active.insert(db).await?;
            let __parent_pk = saved_parent.id;
        }
    } else {
        quote! {
            let __parent_pk = self.id.clone();
            let active = #active_name::from(self.clone());
            match active.clone().insert(db).await {
                Ok(_) => {},
                Err(e) if e.to_string().to_lowercase().contains("unique") => {
                    active.update(db).await
                        .map_err(|e| ::anyhow::anyhow!("update failed: {e}"))?;
                }
                Err(e) => return Err(::anyhow::anyhow!("insert failed: {e}")),
            };
        }
    };

    let child_blocks: Vec<TokenStream2> = child_specs
        .iter()
        .map(|child| {
            let fk_ident = Ident::new(&child.fk, Span::call_site());
            let field_ident = &child.field_ident;

            if let Some(tuple) = &child.tuple_linked {
                return quote! {
                    compile_error!(concat!(
                        "EntityModel: field ", stringify!(#field_ident),
                        " is a tuple-linked child (", #tuple, "); this is not yet supported."
                    ));
                };
            }

            quote! {
                for __child in &self.#field_ident {
                    let mut __child_clone = __child.clone();
                    __child_clone.#fk_ident = __parent_pk.clone();
                    __child_clone.save(db).await?;
                }
            }
        })
        .collect();

    quote! {
        impl crate::Save for #struct_name {
            async fn save(&self, db: &::sea_orm::DatabaseConnection) -> Result<(), ::anyhow::Error> {
                use ::sea_orm::{ActiveModelTrait, IntoActiveModel};
                #parent_insert
                #(#child_blocks)*
                Ok(())
            }
        }
    }
}

/// Scan a field's attrs and classify it as scalar / relation / ignored / flattened.
pub fn process_field(field: &syn::Field) -> ProcessedField {
    let ident = field.ident.clone().expect("named field");
    let ty = field.ty.clone();
    let mut is_ignored = false;
    let mut is_primary_key = false;
    let mut auto_increment = false;
    let mut relation: Option<RelationKind> = None;
    let mut passthrough = Vec::new();
    let mut flatten: Option<FlattenSpec> = None;

    for attr in &field.attrs {
        if attr.path().is_ident("flatten") {
            let flatten_spec: FlattenSpec = attr.parse_args().unwrap_or_else(|e| {
                panic!("Failed to parse #[flatten(...)] attribute: {e}");
            });
            flatten = Some(flatten_spec);
            break; // Only one flatten per field
        }
    }

    for attr in &field.attrs {
        if !attr.path().is_ident("sea_orm") {
            continue;
        }

        let inner_ts = match attr.parse_args::<TokenStream2>() {
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
            let child_fk = extract_value(&inner_str, "child_fk");
            let tuple_linked = extract_value(&inner_str, "tuple");
            relation = Some(RelationKind::HasMany {
                entity_alias: extract_value(&inner_str, "entity"),
                child_fk: if child_fk.is_empty() {
                    None
                } else {
                    Some(child_fk)
                },
                tuple_linked_child: if tuple_linked.is_empty() {
                    None
                } else {
                    Some(tuple_linked)
                },
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

        if inner_lower.contains("primary_key") {
            is_primary_key = true;
        }
        if inner_lower.contains("auto_increment") {
            auto_increment = true;
        }

        passthrough.push(quote! { #[sea_orm(#inner_ts)] });
    }

    ProcessedField {
        ident,
        ty,
        is_ignored,
        is_primary_key,
        auto_increment,
        passthrough,
        relation,
        flatten,
    }
}

/// Convert an entity alias like `"GoldenCommentEntity"` to a reference
/// to the re-exported Entity type via `super::`.
pub fn entity_path_from_alias(alias: &str) -> TokenStream2 {
    let alias_ident = Ident::new(alias, Span::call_site());
    quote! { super::#alias_ident }
}

/// Derive the SeaORM column type for a scalar domain field.
pub fn scalar_column_type(ty: &Type) -> TokenStream2 {
    if type_ends_with(ty, "MagicTypeId") {
        return quote! { String };
    }

    let Some(inner_type) = get_option_inner(ty) else {
        return quote! { #ty };
    };

    if type_ends_with(&inner_type, "MagicTypeId") {
        return quote! { Option<String> };
    }

    return quote! { #ty };
}
