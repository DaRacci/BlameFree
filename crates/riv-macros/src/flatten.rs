use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{
    Data, DeriveInput, Field, Fields, GenericArgument, Ident, LitStr, PathArguments, Token, Type,
    parse::{Parse, ParseStream},
};

/// Unwrap `Option<T>` to get the inner type `T`.
///
/// Returns `None` if the type is not `Option`.
fn unwrap_option_type(ty: &Type) -> Option<Type> {
    if let Type::Path(type_path) = ty
        && let Some(segment) = type_path.path.segments.last()
        && segment.ident == "Option"
        && let PathArguments::AngleBracketed(args) = &segment.arguments
        && let Some(GenericArgument::Type(inner_ty)) = args.args.first()
    {
        return Some(inner_ty.clone());
    }

    None
}

/// A set of well-known primitive/std types that should always be treated as leaf fields.
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

    if let Some(inner) = unwrap_option_type(ty) {
        return is_known_leaf_type(&inner);
    }

    if let Type::Path(type_path) = ty
        && let Some(segment) = type_path.path.segments.last()
    {
        return KNOWN_LEAVES.contains(&segment.ident.to_string().as_str());
    }

    false
}

/// Extract the last path segment name from a type
fn type_last_segment_name(ty: &Type) -> String {
    if let Type::Path(type_path) = ty
        && let Some(segment) = type_path.path.segments.last()
    {
        return segment.ident.to_string();
    }

    String::new()
}

/// Check if a field has `#[flattened(prefix = "...")]` and return the override prefix.
fn get_flattened_prefix(field: &Field) -> Option<String> {
    for attr in &field.attrs {
        if !attr.path().is_ident("flattened") {
            continue;
        }

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
    None
}

/// Check if a field has `#[flattened(...)]` attribute.
fn has_flattened_attr(field: &Field) -> bool {
    field.attrs.iter().any(|a| a.path().is_ident("flattened"))
}

/// For a field, generate the `leaf_fields` entry for a LEAF type.
fn gen_leaf_field_leaf_fields(field_ident: &Ident, ty_name: &str) -> TokenStream2 {
    let field_name = field_ident.to_string();
    let type_str = match ty_name {
        "PathBuf" | "bool" => "String",
        _ => ty_name,
    };

    quote! {
        (Box::leak(format!("{}{}", prefix, #field_name).into_boxed_str()), #type_str)
    }
}

/// For a field, generate the `leaf_fields` entry for a NESTED struct.
fn gen_nested_field_leaf_fields(nested_prefix: &str, field_ty: &Type) -> TokenStream2 {
    let nested_prefix_val = nested_prefix;

    quote! {
        v.extend(<#field_ty as ::riv_types::flatten::FlattenedStruct>::leaf_fields(
            &format!("{}{}", prefix, #nested_prefix_val)
        ));
    }
}

/// For a leaf field, generate the `flatten` entry.
fn gen_leaf_field_flatten(field_ident: &Ident, ty_name: &str, is_option: bool) -> TokenStream2 {
    let field_name = field_ident.to_string();
    let value = match ty_name {
        "String" if is_option => quote! {
            self.#field_ident.clone()
        },
        "String" => quote! {
            Some(self.#field_ident.clone())
        },
        "PathBuf" if is_option => quote! {
            self.#field_ident.as_ref().map(|v| v.to_string_lossy().to_string())
        },
        "PathBuf" => quote! {
            Some(self.#field_ident.to_string_lossy().to_string())
        },
        _ if is_option => quote! {
            self.#field_ident.as_ref().map(|v| v.to_string())
        },
        _ => quote! {
            Some(self.#field_ident.to_string())
        },
    };

    quote! {
      (format!("{}{}", prefix, #field_name), #value)
    }
}

/// For a nested struct field, generate the `flatten` code.
fn gen_nested_field_flatten(field_ident: &Ident, nested_prefix: &str) -> TokenStream2 {
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
    let ext_chain = match ty_name {
        "String" if is_option => quote! { and_then(|v| v.clone()) },
        "String" => quote! { and_then(|v| v.clone()).unwrap_or_default() },
        "PathBuf" if is_option => {
            quote! { and_then(|v| v.clone()).map(::std::path::PathBuf::from) }
        }
        "PathBuf" => {
            quote! { and_then(|v| v.clone()).map(::std::path::PathBuf::from).unwrap_or_default() }
        }
        "bool" if is_option => quote! {
            and_then(|v| v.clone()).and_then(|v| v.parse().ok())
        },
        "bool" => quote! {
            and_then(|v| v.clone()).and_then(|v| v.parse().ok()).unwrap_or_default()
        },
        _ if is_option => quote! {
            and_then(|v| v.clone()).and_then(|v| v.parse().ok())
        },
        _ => quote! {
            and_then(|v| v.clone()).and_then(|v| v.parse().ok()).unwrap_or_default()
        },
    };

    quote! {
      #field_ident: cols.get(&format!("{}{}", prefix, #field_name)).#ext_chain,
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
        #field_ident: <#field_ty as ::riv_types::flatten::FlattenedStruct>::unflatten(
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
        /// The prefix to use when this field is nested
        nested_prefix: String,
    }

    let classified: Vec<ClassifiedField> = fields
        .iter()
        .map(|f| {
            let ident = f.ident.clone().expect("named field");
            let ty = f.ty.clone();
            let is_option = unwrap_option_type(&ty).is_some();

            let ty_name = if is_option {
                match unwrap_option_type(&ty) {
                    Some(inner) => type_last_segment_name(&inner),
                    None => type_last_segment_name(&ty),
                }
            } else {
                type_last_segment_name(&ty)
            };

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

    let leaf_fields_code: Vec<TokenStream2> = classified
        .iter()
        .map(|cf| {
            if cf.is_leaf {
                gen_leaf_field_leaf_fields(&cf.ident, &cf.ty_name)
            } else {
                gen_nested_field_leaf_fields(&cf.nested_prefix, &cf.ty)
            }
        })
        .collect();

    let leaf_fields_fn = if classified.iter().any(|cf| !cf.is_leaf) {
        let leaf_entries: Vec<TokenStream2> = leaf_fields_code;
        quote! {
            fn leaf_fields(prefix: &str) -> Vec<(&'static str, &'static str)> {
                let mut v = Vec::new();
                #( #leaf_entries; )*
                v
            }
        }
    } else {
        quote! {
            fn leaf_fields(prefix: &str) -> Vec<(&'static str, &'static str)> {
                vec![
                    #(#leaf_fields_code,)*
                ]
            }
        }
    };

    let flatten_code: Vec<TokenStream2> = classified
        .iter()
        .map(|cf| {
            if cf.is_leaf {
                gen_leaf_field_flatten(&cf.ident, &cf.ty_name, cf.is_option)
            } else {
                gen_nested_field_flatten(&cf.ident, &cf.nested_prefix)
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
        impl ::riv_types::flatten::FlattenedStruct for #struct_name {
            #leaf_fields_fn
            #flatten_fn
            #unflatten_fn
        }
    };

    TokenStream::from(expanded)
}

/// Parsed `#[variant(...)]` attribute on a single enum variant.
struct VariantAttr {
    /// The tag value for this variant.
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

            match key.to_string().as_str() {
                "tag" => {
                    let _: Token![=] = input.parse()?;
                    let lit: LitStr = input.parse()?;
                    tag = lit.value();
                }
                "fields" => {
                    let content;
                    syn::parenthesized!(content in input);
                    while !content.is_empty() {
                        let col_name: Ident = content.parse()?;
                        let _: Token![:] = content.parse()?;
                        let _: Type = content.parse()?;
                        field_defs.push(FieldDef {
                            col_name: col_name.to_string(),
                        });
                        let _ = content.parse::<Token![,]>();
                    }
                }
                _ => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!("expected `tag` or `fields`, got `{key}`"),
                    ));
                }
            };

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

        let variant_attr: VariantAttr = attr.parse_args().expect("Failed to parse #[variant(...)]");

        // Extract the inner type from the variant's data
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

    let bridge_fields: Vec<TokenStream2> = all_col_names
        .iter()
        .map(|col_name| {
            let field_ident = Ident::new(col_name, proc_macro2::Span::call_site());
            quote! { pub #field_ident: Option<String> }
        })
        .collect();

    let bridge_default_fields: Vec<TokenStream2> = all_col_names
        .iter()
        .map(|col_name| {
            let field_ident = Ident::new(col_name, proc_macro2::Span::call_site());
            quote! { #field_ident: None }
        })
        .collect();

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
                        let __pairs = <#inner_ty as ::riv_types::flatten::FlattenedStruct>::flatten(&inner, "");
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
                            <#inner_ty as ::riv_types::flatten::FlattenedStruct>::unflatten("", &__map)
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
