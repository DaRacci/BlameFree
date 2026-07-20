use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{
    parse::{Parse, ParseStream},
    Data, DeriveInput, Field, Fields, Ident, LitStr, Token, Type,
};

// ---------------------------------------------------------------------------
// FlattenedStruct derive
// ---------------------------------------------------------------------------

/// Unwrap `Option<T>` to get the inner type `T`. Returns `None` if the type
/// is not `Option`.
fn unwrap_option_type(ty: &Type) -> Option<Type> {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            if segment.ident == "Option" {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first() {
                        return Some(inner_ty.clone());
                    }
                }
            }
        }
    }
    None
}

/// A set of well-known primitive/std types that should always be treated
/// as leaf fields (simple string/number/enum values, not nested structs).
///
/// Handles both direct types and `Option<T>` wrappers.
fn is_known_leaf_type(ty: &Type) -> bool {
    const KNOWN_LEAVES: &[&str] = &[
        "String",
        "i32",
        "i64",
        "u32",
        "u64",
        "f32",
        "f64",
        "bool",
        "PathBuf",
        "usize",
        "isize",
        "char",
        "VCSPlatform",
    ];
    // Unwrap Option<T> → check against T
    if let Some(inner) = unwrap_option_type(ty) {
        return is_known_leaf_type(&inner);
    }
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            return KNOWN_LEAVES.contains(&segment.ident.to_string().as_str());
        }
    }
    false
}

/// Extract the last path segment name from a type (e.g. `RemoteRepositoryMeta`
/// from `crate::vcs::RemoteRepositoryMeta`, or `String` from `String`).
fn type_last_segment_name(ty: &Type) -> String {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            return segment.ident.to_string();
        }
    }
    String::new()
}

/// Check if a field has `#[flattened(prefix = "...")]` and return the override prefix.
fn get_flattened_prefix(field: &Field) -> Option<String> {
    for attr in &field.attrs {
        if attr.path().is_ident("flattened") {
            let mut prefix = None;
            let _ = attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("prefix") {
                    let value: LitStr = meta.value()?.parse()?;
                    prefix = Some(value.value());
                }
                Ok(())
            });
            return prefix;
        }
    }
    None
}

/// Check if a field has `#[flattened(...)]` attribute (any variant).
fn has_flattened_attr(field: &Field) -> bool {
    field.attrs.iter().any(|a| a.path().is_ident("flattened"))
}

/// For a field, generate the `leaf_fields` entry for a LEAF type.
fn gen_leaf_field_leaf_fields(field_ident: &Ident, ty_name: &str, _is_option: bool) -> TokenStream2 {
    let field_name = field_ident.to_string();
    let type_str = if ty_name == "PathBuf" {
        "String"
    } else if ty_name == "bool" {
        "String"
    } else {
        ty_name
    };
    quote! {
        (Box::leak(format!("{}{}", prefix, #field_name).into_boxed_str()), #type_str)
    }
}

/// For a field, generate the `leaf_fields` entry for a NESTED struct.
fn gen_nested_field_leaf_fields(
    nested_prefix: &str,
    field_ty: &Type,
) -> TokenStream2 {
    let nested_prefix_val = nested_prefix;
    quote! {
        v.extend(<#field_ty as ::crb_types::flatten::FlattenedStruct>::leaf_fields(
            &format!("{}{}", prefix, #nested_prefix_val)
        ));
    }
}

/// For a leaf field, generate the `flatten` entry.
/// NOTE: no trailing comma — the caller adds `,` (vec![]) or `;` (mixed).
fn gen_leaf_field_flatten(field_ident: &Ident, ty_name: &str, is_option: bool) -> TokenStream2 {
    let field_name = field_ident.to_string();
    if is_option && ty_name == "String" {
        // Option<String>: self.field.clone() returns Option<String>
        quote! {
            (format!("{}{}", prefix, #field_name), self.#field_ident.clone())
        }
    } else if ty_name == "String" {
        quote! {
            (format!("{}{}", prefix, #field_name), Some(self.#field_ident.clone()))
        }
    } else if is_option && ty_name == "PathBuf" {
        // Option<PathBuf>: map to Option<String>
        quote! {
            (format!("{}{}", prefix, #field_name), self.#field_ident.as_ref().map(|v| v.to_string_lossy().to_string()))
        }
    } else if ty_name == "PathBuf" {
        quote! {
            (format!("{}{}", prefix, #field_name), Some(self.#field_ident.to_string_lossy().to_string()))
        }
    } else if is_option {
        // Option<T> for numeric/bool/types with Display
        quote! {
            (format!("{}{}", prefix, #field_name), self.#field_ident.as_ref().map(|v| v.to_string()))
        }
    } else {
        // Numeric, bool, enum types that implement Display
        quote! {
            (format!("{}{}", prefix, #field_name), Some(self.#field_ident.to_string()))
        }
    }
}

/// For a nested struct field, generate the `flatten` code.
fn gen_nested_field_flatten(
    field_ident: &Ident,
    nested_prefix: &str,
    field_ty: &Type,
) -> TokenStream2 {
    let nested_prefix_val = nested_prefix;
    quote! {
        v.extend(self.#field_ident.flatten(
            &format!("{}{}", prefix, #nested_prefix_val)
        ));
    }
}

/// For a leaf field, generate the `unflatten` entry.
fn gen_leaf_field_unflatten(field_ident: &Ident, ty_name: &str, is_option: bool) -> TokenStream2 {
    let field_name = field_ident.to_string();
    if is_option && ty_name == "String" {
        // Option<String>: cols value is already Option<String>
        quote! {
            #field_ident: cols.get(&format!("{}{}", prefix, #field_name))
                .and_then(|v| v.clone()),
        }
    } else if ty_name == "String" {
        quote! {
            #field_ident: cols.get(&format!("{}{}", prefix, #field_name))
                .and_then(|v| v.clone())
                .unwrap_or_default(),
        }
    } else if is_option && ty_name == "PathBuf" {
        // Option<PathBuf>: from Option<String> → map to PathBuf
        quote! {
            #field_ident: cols.get(&format!("{}{}", prefix, #field_name))
                .and_then(|v| v.clone())
                .map(::std::path::PathBuf::from),
        }
    } else if ty_name == "PathBuf" {
        quote! {
            #field_ident: cols.get(&format!("{}{}", prefix, #field_name))
                .and_then(|v| v.clone())
                .map(::std::path::PathBuf::from)
                .unwrap_or_default(),
        }
    } else if is_option && ty_name == "bool" {
        quote! {
            #field_ident: cols.get(&format!("{}{}", prefix, #field_name))
                .and_then(|v| v.clone())
                .and_then(|v| v.parse().ok()),
        }
    } else if ty_name == "bool" {
        quote! {
            #field_ident: cols.get(&format!("{}{}", prefix, #field_name))
                .and_then(|v| v.clone())
                .and_then(|v| v.parse().ok())
                .unwrap_or_default(),
        }
    } else if is_option {
        // Option<T> for numeric types and enums with Display + FromStr
        quote! {
            #field_ident: cols.get(&format!("{}{}", prefix, #field_name))
                .and_then(|v| v.clone())
                .and_then(|v| v.parse().ok()),
        }
    } else {
        // Numeric types and enum types with Display + FromStr
        quote! {
            #field_ident: cols.get(&format!("{}{}", prefix, #field_name))
                .and_then(|v| v.clone())
                .and_then(|v| v.parse().ok())
                .unwrap_or_default(),
        }
    }
}

/// For a nested struct field, generate the `unflatten` code.
fn gen_nested_field_unflatten(
    field_ident: &Ident,
    nested_prefix: &str,
    field_ty: &Type,
) -> TokenStream2 {
    let nested_prefix_val = nested_prefix;
    quote! {
        #field_ident: <#field_ty as ::crb_types::flatten::FlattenedStruct>::unflatten(
            &format!("{}{}", prefix, #nested_prefix_val), cols
        ),
    }
}

pub fn derive_flattened_struct_impl(input: &DeriveInput) -> TokenStream {
    let struct_name = &input.ident;
    let data = match &input.data {
        Data::Struct(s) => s,
        _ => panic!("FlattenedStruct derive macro only supports structs"),
    };
    let fields = match &data.fields {
        Fields::Named(f) => &f.named,
        _ => panic!("FlattenedStruct derive macro requires named fields"),
    };

    // Classify each field as leaf or nested
    struct ClassifiedField {
        ident: Ident,
        ty: Type,
        ty_name: String,
        is_leaf: bool,
        is_option: bool,
        /// The prefix to use when this field is nested — either from
        /// `#[flattened(prefix = "...")]` or the default `fieldname_`.
        nested_prefix: String,
    }

    let classified: Vec<ClassifiedField> = fields
        .iter()
        .map(|f| {
            let ident = f.ident.clone().expect("named field");
            let ty = f.ty.clone();
            let is_option = unwrap_option_type(&ty).is_some();

            // For Option<T>, get ty_name from the inner type T
            let ty_name = if is_option {
                if let Some(inner) = unwrap_option_type(&ty) {
                    type_last_segment_name(&inner)
                } else {
                    type_last_segment_name(&ty)
                }
            } else {
                type_last_segment_name(&ty)
            };

            // is_known_leaf_type already handles Option<T> unwrapping
            let is_leaf = if has_flattened_attr(f) {
                false
            } else {
                is_known_leaf_type(&ty)
            };

            let override_prefix = get_flattened_prefix(f);
            let nested_prefix = override_prefix.unwrap_or_else(|| format!("{}_", ident));

            ClassifiedField {
                ident,
                ty,
                ty_name,
                is_leaf,
                is_option,
                nested_prefix,
            }
        })
        .collect();

    // --- leaf_fields ---
    let leaf_fields_code: Vec<TokenStream2> = classified
        .iter()
        .map(|cf| {
            if cf.is_leaf {
                gen_leaf_field_leaf_fields(&cf.ident, &cf.ty_name, cf.is_option)
            } else {
                gen_nested_field_leaf_fields(&cf.nested_prefix, &cf.ty)
            }
        })
        .collect();

    let leaf_fields_fn = if classified.iter().any(|cf| !cf.is_leaf) {
        // Mixed or fully nested — use Vec + extend.
        // Add ; after every entry so leaf entries (which are bare tuple expressions)
        // become proper expression statements, while nested entries (which end with
        // their own ;) just get an empty-statement ; that's harmless.
        let leaf_entries: Vec<TokenStream2> = leaf_fields_code;
        quote! {
            fn leaf_fields(prefix: &str) -> Vec<(&'static str, &'static str)> {
                let mut v = Vec::new();
                #( #leaf_entries; )*
                v
            }
        }
    } else {
        // All leaf — can use vec![...] directly
        quote! {
            fn leaf_fields(prefix: &str) -> Vec<(&'static str, &'static str)> {
                vec![
                    #(#leaf_fields_code,)*
                ]
            }
        }
    };

    // --- flatten ---
    let flatten_code: Vec<TokenStream2> = classified
        .iter()
        .map(|cf| {
            if cf.is_leaf {
                gen_leaf_field_flatten(&cf.ident, &cf.ty_name, cf.is_option)
            } else {
                gen_nested_field_flatten(&cf.ident, &cf.nested_prefix, &cf.ty)
            }
        })
        .collect();

    let flatten_fn = if classified.iter().any(|cf| !cf.is_leaf) {
        quote! {
            fn flatten(&self, prefix: &str) -> Vec<(String, Option<String>)> {
                let mut v = Vec::new();
                #(#flatten_code;)*
                v
            }
        }
    } else {
        quote! {
            fn flatten(&self, prefix: &str) -> Vec<(String, Option<String>)> {
                vec![
                    #(#flatten_code,)*
                ]
            }
        }
    };

    // --- unflatten ---
    let unflatten_code: Vec<TokenStream2> = classified
        .iter()
        .map(|cf| {
            if cf.is_leaf {
                gen_leaf_field_unflatten(&cf.ident, &cf.ty_name, cf.is_option)
            } else {
                gen_nested_field_unflatten(&cf.ident, &cf.nested_prefix, &cf.ty)
            }
        })
        .collect();

    let unflatten_fn = quote! {
        fn unflatten(prefix: &str, cols: &std::collections::HashMap<String, Option<String>>) -> Self {
            Self {
                #(#unflatten_code)*
            }
        }
    };

    let expanded = quote! {
        impl ::crb_types::flatten::FlattenedStruct for #struct_name {
            #leaf_fields_fn
            #flatten_fn
            #unflatten_fn
        }
    };

    TokenStream::from(expanded)
}

// ---------------------------------------------------------------------------
// Dotted field path (e.g. `repository.owner`) — no longer used by FlattenedEnum
// after the migration to FlattenedStruct trait, but kept for reference.
// ---------------------------------------------------------------------------

/// A dotted path like `repository.owner` for accessing nested struct fields.
struct DottedPath {
    segments: Vec<Ident>,
}

impl Parse for DottedPath {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut segments = vec![input.parse::<Ident>()?];
        while input.peek(Token![.]) {
            let _: Token![.] = input.parse()?;
            segments.push(input.parse::<Ident>()?);
        }
        Ok(DottedPath { segments })
    }
}

// ---------------------------------------------------------------------------
// Parsing: #[variant(tag = "...", fields(
//     col_name: Type,
// ))]
//
// NOTE: Field-path syntax (e.g. `repository.owner => repository_owner: Type`)
// has been REMOVED. The new syntax is just `col_name: Type`.
// ---------------------------------------------------------------------------

/// Parsed `#[variant(...)]` attribute on a single enum variant.
struct VariantAttr {
    /// The tag value for this variant (e.g., "pull_request").
    tag: String,
    /// Field mappings — just column names and types (no field paths).
    field_defs: Vec<FieldDef>,
}

/// A single field definition: column name + type.
struct FieldDef {
    col_name: String,
}

impl Parse for VariantAttr {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut tag = String::new();
        let mut field_defs: Vec<FieldDef> = Vec::new();

        while !input.is_empty() {
            let key: Ident = input.parse()?;

            if key == "tag" {
                let _: Token![=] = input.parse()?;
                let lit: LitStr = input.parse()?;
                tag = lit.value();
            } else if key == "fields" {
                let content;
                syn::parenthesized!(content in input);
                while !content.is_empty() {
                    // Parse: col_name: Type  (no field path, no `=>`)
                    let col_name: Ident = content.parse()?;
                    let _: Token![:] = content.parse()?;
                    // Discard the type — it's documentation only now;
                    // the actual types come from FlattenedStruct.
                    let _: Type = content.parse()?;
                    field_defs.push(FieldDef {
                        col_name: col_name.to_string(),
                    });
                    // optional trailing comma
                    let _ = content.parse::<Token![,]>();
                }
            } else {
                return Err(syn::Error::new(
                    key.span(),
                    format!("expected `tag` or `fields`, got `{key}`"),
                ));
            }

            // optional trailing comma between items
            let _ = input.parse::<Token![,]>();
        }

        if tag.is_empty() {
            return Err(syn::Error::new(
                input.span(),
                "missing required `tag = \"...\"` in #[variant(...)]",
            ));
        }

        Ok(VariantAttr { tag, field_defs })
    }
}

/// Parsed enum variant.
struct EnumVariant {
    name: String,
    attr: VariantAttr,
    /// The inner type of the variant (e.g., `PullRequestReviewMetadata`),
    /// if it has an unnamed field. `None` for unit variants.
    inner_type: Option<Type>,
}

// ---------------------------------------------------------------------------
// FlattenedEnum derive entry point
// ---------------------------------------------------------------------------

pub fn derive_flattened_enum_impl(input: &DeriveInput) -> TokenStream {
    let enum_name = &input.ident;
    let enum_name_str = enum_name.to_string();
    let bridge_name = format_ident!("__{}Columns", enum_name_str);

    let data = match &input.data {
        Data::Enum(e) => e,
        _ => panic!("FlattenedEnum derive macro only supports enums"),
    };

    // Parse each variant's #[variant(...)] attribute
    let mut variants: Vec<EnumVariant> = Vec::new();

    for variant in &data.variants {
        let variant_name = variant.ident.to_string();

        let attr = variant
            .attrs
            .iter()
            .find(|a| a.path().is_ident("variant"))
            .unwrap_or_else(|| {
                panic!(
                    "Missing #[variant(...)] attribute on enum variant `{}`",
                    variant_name
                )
            });

        let variant_attr: VariantAttr =
            attr.parse_args().expect("Failed to parse #[variant(...)]");

        // Extract the inner type from the variant's data (unnamed field)
        let inner_type = match &variant.fields {
            Fields::Unnamed(u) => {
                if u.unnamed.len() == 1 {
                    Some(u.unnamed.first().unwrap().ty.clone())
                } else {
                    None
                }
            }
            _ => None,
        };

        variants.push(EnumVariant {
            name: variant_name,
            attr: variant_attr,
            inner_type,
        });
    }

    // Collect all unique column names across all variants
    let mut all_col_names: Vec<String> = Vec::new();
    all_col_names.push("review_type".to_string());

    for v in &variants {
        for def in &v.attr.field_defs {
            if !all_col_names.contains(&def.col_name) {
                all_col_names.push(def.col_name.clone());
            }
        }
    }

    // Bridge fields (all Option<String>)
    let bridge_fields: Vec<TokenStream2> = all_col_names
        .iter()
        .map(|col_name| {
            let field_ident = Ident::new(col_name, proc_macro2::Span::call_site());
            quote! { pub #field_ident: Option<String> }
        })
        .collect();

    // Default fields
    let bridge_default_fields: Vec<TokenStream2> = all_col_names
        .iter()
        .map(|col_name| {
            let field_ident = Ident::new(col_name, proc_macro2::Span::call_site());
            quote! { #field_ident: None }
        })
        .collect();

    // -----------------------------------------------------------------------
    // From<Enum> for Bridge — use FlattenedStruct::flatten()
    // -----------------------------------------------------------------------
    let from_enum_arms: Vec<TokenStream2> = variants
        .iter()
        .map(|v| {
            let variant_ident = Ident::new(&v.name, proc_macro2::Span::call_site());
            let tag_value = &v.attr.tag;

            let _col_idents: Vec<Ident> = all_col_names
                .iter()
                .map(|c| Ident::new(c, proc_macro2::Span::call_site()))
                .collect();

            if v.attr.field_defs.is_empty() {
                // Unit variant — all columns None except discriminator
                let none_all: Vec<TokenStream2> = all_col_names
                    .iter()
                    .filter(|col_name| *col_name != "review_type")
                    .map(|col_name| {
                        let col_ident = Ident::new(col_name, proc_macro2::Span::call_site());
                        quote! { #col_ident: None, }
                    })
                    .collect();

                quote! {
                    #enum_name::#variant_ident => #bridge_name {
                        review_type: Some(#tag_value.into()),
                        #( #none_all )*
                    }
                }
            } else if let Some(inner_ty) = &v.inner_type {
                // Variant with inner struct — use FlattenedStruct::flatten()
                // to get (col_name, value) pairs, then match against bridge columns.
                let set_fields: Vec<TokenStream2> = all_col_names
                    .iter()
                    .map(|col_name| {
                        let col_ident = Ident::new(col_name, proc_macro2::Span::call_site());
                        if col_name == "review_type" {
                            quote! { #col_ident: Some(#tag_value.into()), }
                        } else {
                            // Look up the column name in the flattened pairs
                            quote! {
                                #col_ident: __pairs.iter()
                                    .find(|(k, _)| k == #col_name)
                                    .and_then(|(_, v)| v.clone()),
                            }
                        }
                    })
                    .collect();

                quote! {
                    #enum_name::#variant_ident(inner) => {
                        let __pairs = <#inner_ty as ::crb_types::flatten::FlattenedStruct>::flatten(&inner, "");
                        #bridge_name {
                            #(#set_fields)*
                        }
                    }
                }
            } else {
                // Multivariate variant or other — fallback
                let none_all: Vec<TokenStream2> = all_col_names
                    .iter()
                    .filter(|col_name| *col_name != "review_type")
                    .map(|col_name| {
                        let col_ident = Ident::new(col_name, proc_macro2::Span::call_site());
                        quote! { #col_ident: None, }
                    })
                    .collect();

                quote! {
                    #enum_name::#variant_ident { .. } => #bridge_name {
                        review_type: Some(#tag_value.into()),
                        #( #none_all )*
                    }
                }
            }
        })
        .collect();

    // -----------------------------------------------------------------------
    // From<Bridge> for Enum — use FlattenedStruct::unflatten()
    // -----------------------------------------------------------------------
    let from_bridge_arms: Vec<TokenStream2> = variants
        .iter()
        .map(|v| {
            let variant_ident = Ident::new(&v.name, proc_macro2::Span::call_site());
            let tag_value = &v.attr.tag;

            if v.attr.field_defs.is_empty() {
                // Unit variant — just match the tag
                quote! {
                    Some(#tag_value) => #enum_name::#variant_ident,
                }
            } else if let Some(inner_ty) = &v.inner_type {
                // Build a HashMap from bridge columns and call unflatten
                let insert_stmts: Vec<TokenStream2> = all_col_names
                    .iter()
                    .map(|col_name| {
                        let col_ident = Ident::new(col_name, proc_macro2::Span::call_site());
                        quote! {
                            __map.insert(#col_name.to_string(), val.#col_ident.clone());
                        }
                    })
                    .collect();

                quote! {
                    Some(#tag_value) => {
                        let mut __map: std::collections::HashMap<String, Option<String>> = std::collections::HashMap::new();
                        #(#insert_stmts)*
                        #enum_name::#variant_ident(
                            <#inner_ty as ::crb_types::flatten::FlattenedStruct>::unflatten("", &__map)
                        )
                    }
                }
            } else {
                // Fallback
                quote! {
                    Some(#tag_value) => panic!("cannot reconstruct variant `{}` from bridge", #tag_value),
                }
            }
        })
        .collect();

    // FLAT_COLUMNS constant
    let field_list: Vec<TokenStream2> = all_col_names
        .iter()
        .map(|col_name| {
            quote! { (#col_name, "Option<String>") }
        })
        .collect();

    let default_impl = if all_col_names.is_empty() {
        quote! {}
    } else {
        quote! {
            impl Default for #bridge_name {
                fn default() -> Self {
                    #bridge_name {
                        #( #bridge_default_fields, )*
                    }
                }
            }
        }
    };

    // Build From<Bridge> for Enum — match on review_type tag
    let tag_ident = Ident::new("review_type", proc_macro2::Span::call_site());
    let from_bridge_impl = if from_bridge_arms.is_empty() {
        quote! {}
    } else {
        quote! {
            impl From<#bridge_name> for #enum_name {
                fn from(val: #bridge_name) -> Self {
                    match val.#tag_ident.as_deref() {
                        #(#from_bridge_arms)*
                        other => panic!(
                            "unknown {} tag: {:?}",
                            stringify!(#enum_name), other
                        ),
                    }
                }
            }
        }
    };

    let expanded = quote! {
        #[doc(hidden)]
        #[derive(Debug, Clone)]
        pub struct #bridge_name {
            #(#bridge_fields,)*
        }

        #default_impl

        impl From<#enum_name> for #bridge_name {
            fn from(val: #enum_name) -> Self {
                match val {
                    #(#from_enum_arms,)*
                }
            }
        }

        #from_bridge_impl

        impl #enum_name {
            /// List of all flattened column names and their types.
            const FLAT_COLUMNS: &'static [(&'static str, &'static str)] = &[
                #(#field_list,)*
            ];
        }
    };

    TokenStream::from(expanded)
}
