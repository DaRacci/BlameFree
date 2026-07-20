use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{Attribute, Data, DeriveInput, Fields, Ident, LitStr, Type};

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
            let v = format_ident!("{}", snake_to_pascal(&ident.to_string()));
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
    //
    // For From<Model> → DomainType, we ONLY map fields that have a DB column
    // (non-ignored, non-pure-relation scalar fields). Fields that are ignored or
    // purely relational (HasMany/HasOne) cannot be populated from the Model, so
    // they are passed through with the struct-literal spread `..Default::default()`.
    // This means the DomainType struct must implement `Default`. If it doesn't,
    // the From impl simply is not emitted for that entity.
    //
    // For From<DomainType> → ActiveModel, we only Set the scalar fields.
    // Ignored/pure-relation fields remain NotSet (default).

    let mut all_scalar = true;
    let mut from_model_fields: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut from_domain_fields: Vec<proc_macro2::TokenStream> = Vec::new();

    for pf in &processed {
        let ident = &pf.ident;
        let is_pure_rel = matches!(
            pf.relation,
            Some(RelationKind::HasMany { .. }) | Some(RelationKind::HasOne { .. })
        );

        // Determine if this field is "scalar" — has a DB column
        let is_scalar = !pf.is_ignored && !is_pure_rel;

        if !is_scalar {
            all_scalar = false;
        }

        // Check auto-increment
        let has_auto_increment = pf
            .passthrough
            .iter()
            .any(|ts| ts.to_string().to_lowercase().contains("auto_increment"));

        // Check if the field's domain type is MagicTypeId or Option<MagicTypeId>
        let is_mtid = type_ends_with(&pf.ty, "MagicTypeId");
        let is_opt_mtid = if let Type::Path(type_path) = &pf.ty {
            if let Some(segment) = type_path.path.segments.last() {
                if segment.ident == "Option" {
                    if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                        if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
                            type_ends_with(inner, "MagicTypeId")
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            false
        };

        // --- From<Model> for DomainType (only scalar fields) ---
        if is_scalar {
            let model_expr = if has_auto_increment {
                // Auto-increment PK: model type is i32, domain type may differ
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

    let from_model_impl = if from_model_fields.is_empty() || !all_scalar {
        // When there are non-scalar fields that can't be filled from the Model,
        // skip emitting From<Model> — it would require DomainType: Default which
        // we can't guarantee at macro time. The user can write their own conversion.
        quote! {}
    } else {
        quote! {
            impl From<Model> for super::#struct_name {
                fn from(m: Model) -> Self {
                    Self {
                        #(#from_model_fields,)*
                    }
                }
            }
        }
    };

    let from_domain_impl = if from_domain_fields.is_empty() {
        quote! {}
    } else {
        quote! {
            impl From<super::#struct_name> for ActiveModel {
                fn from(d: super::#struct_name) -> Self {
                    Self {
                        #(#from_domain_fields,)*
                    }
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
    };

    TokenStream::from(expanded)
}

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

    /// `#[sea_orm(...)]` tokens to pass through verbatim.
    pub passthrough: Vec<proc_macro2::TokenStream>,

    /// If non-None, this field is a relation.
    /// For HasMany/HasOne: the field becomes a pure relation (no DB column).
    /// For BelongsTo: the field stays as a scalar column + additional relation metadata is generated.
    pub relation: Option<RelationKind>,
}

/// Check if a type's last path segment matches a given name.
pub fn type_ends_with(ty: &Type, name: &str) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            return segment.ident == name;
        }
    }
    false
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

/// Scan a field's attrs and classify it as scalar / relation / ignored.
pub fn process_field(field: &syn::Field) -> ProcessedField {
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
pub fn entity_path_from_alias(alias: &str) -> proc_macro2::TokenStream {
    let alias_ident = Ident::new(alias, proc_macro2::Span::call_site());
    quote! { super::#alias_ident }
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
pub fn scalar_column_type(ty: &Type) -> proc_macro2::TokenStream {
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
