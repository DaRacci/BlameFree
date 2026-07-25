use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{
    Attribute, Data, DeriveInput, Fields, Ident, LitStr, Token, Type,
    parse::{Parse, ParseStream},
};

/// Parsed `#[flatten(tag = "...", variants = { ... })]` attribute.
#[derive(Debug, Clone)]
pub struct FlattenSpec {
    /// Discriminator column name (e.g., "review_type").
    pub tag: String,
    /// Each variant's flat column definitions.
    pub variants: Vec<FlattenVariantDef>,
}

/// A single variant's column definitions within a flatten annotation.
#[derive(Debug, Clone)]
pub struct FlattenVariantDef {
    /// The tag VALUE for this variant (e.g., "pull_request").
    #[allow(unused)]
    pub name: String,
    /// Column definitions: (column_name, type_string).
    /// Type string is the DB column type (e.g., "String", "i32").
    pub columns: Vec<(String, String)>,
}

/// Parse `flatten(tag = "...", variants = { name => { col: Ty, ... }, ... })`.
impl Parse for FlattenSpec {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut tag = String::new();
        let mut variants: Vec<FlattenVariantDef> = Vec::new();

        while !input.is_empty() {
            let key: Ident = input.parse()?;

            if key == "tag" {
                let _: Token![=] = input.parse()?;
                let lit: LitStr = input.parse()?;
                tag = lit.value();
            } else if key == "variants" {
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
            } else {
                return Err(syn::Error::new(
                    key.span(),
                    format!("expected `tag` or `variants`, got `{key}`"),
                ));
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

    // Collect: (field_ident, flattened_field_combined_tokens)
    let mut flatten_entries: Vec<(Ident, FlattenSpec)> = Vec::new();

    for pf in &processed {
        if pf.is_ignored {
            continue;
        }

        if let Some(ref spec) = pf.flatten {
            // Flatten field: generate columns from the annotation
            flatten_entries.push((pf.ident.clone(), spec.clone()));

            // All flatten columns are Option<String> for universal compatibility.
            // Emit the discriminator column first.
            let tag_col = &spec.tag;
            let tag_col_ident = Ident::new(tag_col, proc_macro2::Span::call_site());
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
                    let col_ident = Ident::new(col_name, proc_macro2::Span::call_site());
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
    let relation_variants: Vec<TokenStream2> = rel_entries
        .iter()
        .map(|(ident, _)| {
            let v = format_ident!("{}", snake_to_pascal(&ident.to_string()));
            quote! { #v }
        })
        .collect();

    // Build RelationTrait::def() — handle empty relations gracefully
    let relation_trait_impl: TokenStream2 = if rel_entries.is_empty() {
        quote! {
            impl RelationTrait for Relation {
                fn def(&self) -> sea_orm::RelationDef {
                    match *self {}
                }
            }
        }
    } else {
        let relation_def_arms: Vec<TokenStream2> = rel_entries
            .iter()
            .map(|(ident, kind)| {
                let v_str = snake_to_pascal(&ident.to_string());
                let v = Ident::new(&v_str, proc_macro2::Span::call_site());
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
                    RelationKind::BelongsTo {
                        entity_alias,
                        from,
                        to,
                    } => {
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
    let related_impls: Vec<TokenStream2> = rel_entries
        .iter()
        .filter_map(|(ident, kind)| {
            if let RelationKind::BelongsTo { entity_alias, .. } = kind {
                let parent_path = entity_path_from_alias(entity_alias);
                let v_str = snake_to_pascal(&ident.to_string());
                let rel_variant = Ident::new(&v_str, proc_macro2::Span::call_site());
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

    // Build From<Model> for DomainType and From<DomainType> for ActiveModel
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

    for pf in &processed {
        let ident = &pf.ident;
        let is_pure_rel = matches!(
            pf.relation,
            Some(RelationKind::HasMany { .. }) | Some(RelationKind::HasOne { .. })
        );

        // Determine if this field is "scalar" — has a DB column
        let is_scalar = !pf.is_ignored && !is_pure_rel && pf.flatten.is_none();

        // Check auto-increment
        let has_auto_increment = pf
            .passthrough
            .iter()
            .any(|ts| ts.to_string().to_lowercase().contains("auto_increment"));

        // Check if the field's domain type is MagicTypeId or Option<MagicTypeId>
        let is_mtid = type_ends_with(&pf.ty, "MagicTypeId");
        let is_opt_mtid = is_optional_magic_type_id(&pf.ty);

        // --- From<Model> for DomainType (only scalar fields) ---
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

        // --- From<DomainType> for ActiveModel (only scalar fields) ---
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

    // Generate flatten field conversions for From<DomainType> only (not From<Model>)
    let mut flatten_let_bindings: Vec<TokenStream2> = Vec::new();
    let mut flatten_set_fields: Vec<TokenStream2> = Vec::new();

    for (idx, (field_ident, spec)) in flatten_entries.iter().enumerate() {
        // Get the type name for the bridge struct naming convention
        let pf = processed
            .iter()
            .find(|pf| pf.ident == *field_ident)
            .unwrap();
        let field_ty_name = type_last_segment(&pf.ty);

        // Bridge struct name: __<FieldType>Columns (e.g., __ReviewMetadataColumns)
        let bridge_ident = format_ident!("__{}Columns", field_ty_name);
        let cols_ident = format_ident!("__cols_{}", idx);

        // Collect all column names for this flatten
        let mut all_flatten_cols: Vec<(String, String)> = Vec::new();
        all_flatten_cols.push((spec.tag.clone(), "Option<String>".to_string()));
        for vd in &spec.variants {
            for (col_name, col_type) in &vd.columns {
                if !all_flatten_cols.iter().any(|(c, _)| c == col_name) {
                    all_flatten_cols.push((col_name.clone(), format!("Option<{col_type}")));
                }
            }
        }

        // --- From<DomainType> -> convert domain enum to bridge, then Set each column ---
        let set_fields: Vec<TokenStream2> = all_flatten_cols
            .iter()
            .map(|(col_name, _)| {
                let col_ident = Ident::new(col_name, proc_macro2::Span::call_site());
                quote! { #col_ident: sea_orm::Set(#cols_ident.#col_ident) }
            })
            .collect();

        flatten_let_bindings.push(quote! {
            let #cols_ident: #bridge_ident = d.#field_ident.into();
        });
        flatten_set_fields.extend(set_fields);
    }

    // Combine all from-model fields: just scalar fields (flatten fields use ..Default::default())
    let final_from_model_fields: Vec<TokenStream2> = from_model_fields;

    // Determine if there are flatten fields (triggers From<Model> generation)
    let has_flatten = !flatten_entries.is_empty();

    // Determine if there are ignored/pure-relation fields that need Default::default()
    let has_ignored_fields = processed.iter().any(|pf| {
        let is_pure_rel = matches!(
            pf.relation,
            Some(RelationKind::HasMany { .. }) | Some(RelationKind::HasOne { .. })
        );
        (pf.is_ignored || is_pure_rel) && pf.flatten.is_none()
    });

    let from_model_impl = if final_from_model_fields.is_empty() {
        quote! {}
    } else if has_ignored_fields && !has_flatten {
        // Only ignored fields, no flatten — preserve old behavior, skip From<Model>.
        // The user writes their own conversion for this case.
        quote! {}
    } else {
        // Either all fields are populated from Model, or there are flatten fields
        // that we populate via the bridge struct.
        // If there are also ignored fields, use ..Default::default().
        let scalar_fields = &final_from_model_fields;
        if has_ignored_fields {
            quote! {
                impl From<Model> for super::#struct_name {
                    fn from(m: Model) -> Self {
                        Self {
                            #(#scalar_fields,)*
                            ..Default::default()
                        }
                    }
                }
            }
        } else {
            quote! {
                impl From<Model> for super::#struct_name {
                    fn from(m: Model) -> Self {
                        Self {
                            #(#scalar_fields,)*
                        }
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

    // ── EntityId impl ──────────────────────────────────────
    // Find the PK field and derive its Id type.
    let (pk_ident, pk_is_option, entity_id_ty) = if has_explicit_id {
        let pk = processed
            .iter()
            .find(|pf| pf.is_primary_key)
            .expect("EntityModel: no primary key found");
        let is_opt = type_ends_with(&pk.ty, "Option");
        // Unwrap Option<T> → T
        let inner_ty = if is_opt {
            // Parse unwrapped type from Option<T>
            if let Type::Path(tp) = &pk.ty {
                if let Some(seg) = tp.path.segments.last() {
                    if let syn::PathArguments::AngleBracketed(args) = &seg.arguments {
                        if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
                            Some(quote!(#inner))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };
        let id_ty = if let Some(ref inner) = inner_ty {
            quote!(#inner)
        } else {
            let pk_ty = &pk.ty;
            quote!(#pk_ty)
        };
        (pk.ident.clone(), is_opt, id_ty)
    } else {
        // Implicit auto-increment PK
        let pk = Ident::new("id", proc_macro2::Span::call_site());
        (pk, false, quote!(i32))
    };

    let entity_id_impl = {
        let get_id_expr = if pk_is_option {
            quote! { self.#pk_ident.clone() }
        } else {
            quote! { Some(self.#pk_ident.clone()) }
        };
        quote! {
            impl crate::EntityId for #struct_name {
                type Id = #entity_id_ty;
                fn get_id(&self) -> Option<Self::Id> {
                    #get_id_expr
                }
            }
        }
    };

    // ── new() ──────────────────────────────────────────────
    //
    // Skip generation if any field has #[sea_orm(ignore)] — we can't guess
    // a reasonable default for arbitrary types.
    let has_ignored = processed.iter().any(|pf| pf.is_ignored);

    let new_impl = if has_ignored {
        quote! {}
    } else {
        // Build complete field list for the struct literal.
        // Each entry: (field_name, expression_token_stream)
        let mut all_fields: Vec<(Ident, TokenStream2)> = Vec::new();
        let mut new_param_decls: Vec<TokenStream2> = Vec::new();

        for pf in &processed {
            if pf.flatten.is_some() {
                continue; // flatten fields handled via ..Default::default() fallback below
            }

            let name = &pf.ident;

            // HasMany / HasOne — pure relation, always Vec::new()
            if matches!(
                pf.relation,
                Some(RelationKind::HasMany { .. }) | Some(RelationKind::HasOne { .. })
            ) {
                all_fields.push((name.clone(), quote! { Vec::new() }));
                continue;
            }

            // Auto-increment PK → None
            if pf.is_primary_key && pf.auto_increment {
                all_fields.push((name.clone(), quote! { None }));
                continue;
            }

            // Optional FK (BelongsTo on Option<T>) → None
            if matches!(pf.relation, Some(RelationKind::BelongsTo { .. }))
                && type_ends_with(&pf.ty, "Option")
            {
                all_fields.push((name.clone(), quote! { None }));
                continue;
            }

            // All other fields become required params
            let ty = pf.ty.clone();
            all_fields.push((name.clone(), quote! { #name }));
            new_param_decls.push(quote! { #name: #ty });
        }

        // Determine fallback: if there are flatten fields, use ..Default::default().
        // Otherwise emit a complete literal.
        let has_flatten = processed.iter().any(|pf| pf.flatten.is_some());
        let defaults_fallback = if has_flatten {
            quote! { ..Default::default() }
        } else {
            quote! {}
        };

        if new_param_decls.is_empty() && !has_flatten {
            quote! {}
        } else {
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
    };

    // ── link() ─────────────────────────────────────────────
    // Collect FK field info: (fk_field_ident, entity_name, to_column, inner_type)
    // inner_type is the unwrapped type of the FK field (e.g. i32 from Option<i32>).
    let fk_entries: Vec<(Ident, String, String, TokenStream2)> = processed
        .iter()
        .filter_map(|pf| {
            if let Some(RelationKind::BelongsTo {
                entity_alias, to, ..
            }) = &pf.relation
            {
                // Unwrap Option<T> → T for the FK type
                let inner_ty = unwrap_option_type(&pf.ty);
                Some((pf.ident.clone(), entity_alias.clone(), to.clone(), inner_ty))
            } else {
                None
            }
        })
        .collect();

    let link_impl = if fk_entries.is_empty() {
        quote! {}
    } else {
        let link_params: Vec<TokenStream2> = fk_entries
            .iter()
            .map(|(_, entity_alias, _, inner_ty)| {
                // Strip "Entity" suffix, snake_case the entity name
                let entity_type = entity_alias.strip_suffix("Entity").unwrap_or(entity_alias);
                let param_name =
                    Ident::new(&to_snake_case(entity_type), proc_macro2::Span::call_site());
                quote! { #param_name: &dyn crate::EntityId<Id = #inner_ty> }
            })
            .collect();

        let link_sets: Vec<TokenStream2> = fk_entries
            .iter()
            .map(|(fk_ident, entity_alias, _, _inner_ty)| {
                let entity_type = entity_alias.strip_suffix("Entity").unwrap_or(entity_alias);
                let param_name =
                    Ident::new(&to_snake_case(entity_type), proc_macro2::Span::call_site());
                // Find the FK field to check if it's Option<T>
                let fk_pf = processed.iter().find(|pf| pf.ident == *fk_ident).unwrap();
                let is_opt_fk = type_ends_with(&fk_pf.ty, "Option");
                let get_id_expr = if is_opt_fk {
                    quote! { #param_name.get_id() }
                } else {
                    quote! { #param_name.get_id().expect("link: entity missing id") }
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
    };

    TokenStream::from(expanded)
}

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

/// Parsed relation directive for a field's `#[sea_orm(...)]` attributes.
#[derive(Debug, Clone)]
pub enum RelationKind {
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

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Check if a type's last path segment matches a given name.
pub fn type_ends_with(ty: &Type, name: &str) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            return segment.ident == name;
        }
    }
    false
}

/// Get the last type name segment (e.g., ReviewMetadata from `crate::review::ReviewMetadata`).
pub fn type_last_segment(ty: &Type) -> String {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            return segment.ident.to_string();
        }
    }
    String::new()
}

/// Check if type is `Option<MagicTypeId>`.
pub fn is_optional_magic_type_id(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            if segment.ident == "Option" {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
                        return type_ends_with(inner, "MagicTypeId");
                    }
                }
            }
        }
    }
    false
}

/// If the type is `Option<T>`, return `T` token stream. Otherwise return the type itself.
pub fn unwrap_option_type(ty: &Type) -> TokenStream2 {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            if segment.ident == "Option" {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
                        return quote! { #inner };
                    }
                }
            }
        }
    }
    quote! { #ty }
}

pub fn extract_key_value(
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

/// Extract `key = "value"` from a raw token string for a specific key.
pub fn extract_value(tokens: &str, key: &str) -> String {
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

    // First, check for #[flatten(...)] attribute (used independently of sea_orm)
    for attr in &field.attrs {
        if attr.path().is_ident("flatten") {
            let flatten_spec: FlattenSpec = attr.parse_args().unwrap_or_else(|e| {
                panic!("Failed to parse #[flatten(...)] attribute: {e}");
            });
            flatten = Some(flatten_spec);
            break; // Only one flatten per field
        }
    }

    // Then process #[sea_orm(...)] attributes
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

        // Detect primary key & auto_increment
        if inner_lower.contains("primary_key") {
            is_primary_key = true;
        }
        if inner_lower.contains("auto_increment") {
            auto_increment = true;
        }

        // Pass through the original attribute verbatim
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
    let alias_ident = Ident::new(alias, proc_macro2::Span::call_site());
    quote! { super::#alias_ident }
}

/// Convert PascalCase or camelCase to snake_case.
pub fn to_snake_case(s: &str) -> String {
    let mut result = String::with_capacity(s.len() + 4);
    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch.is_uppercase() {
            if !result.is_empty() && !result.ends_with('_') {
                // Insert underscore before uppercase, but not if it's first or
                // the previous char is already uppercase (handles acronyms).
                if let Some(&next) = chars.peek() {
                    if next.is_lowercase() {
                        result.push('_');
                    }
                }
            }
            for lc in ch.to_lowercase() {
                result.push(lc);
            }
        } else {
            result.push(ch);
        }
    }
    result
}

/// Convert snake_case to PascalCase
pub fn snake_to_pascal(s: &str) -> String {
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
pub fn scalar_column_type(ty: &Type) -> TokenStream2 {
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
