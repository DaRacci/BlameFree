use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{Attribute, LitStr, Type};

/// Check if a type's last path segment matches a given name.
pub fn type_ends_with(ty: &Type, name: &str) -> bool {
    let Type::Path(type_path) = ty else {
        return false;
    };

    let Some(segment) = type_path.path.segments.last() else {
        return false;
    };

    segment.ident == name
}

/// Get the last type name segment (e.g., ReviewMetadata from `crate::review::ReviewMetadata`).
pub fn type_last_segment(ty: &Type) -> String {
    let Type::Path(type_path) = ty else {
        return String::new();
    };

    let Some(segment) = type_path.path.segments.last() else {
        return String::new();
    };

    segment.ident.to_string()
}

/// If the type is `Option<T>`, return `T`. Otherwise return None.
pub fn get_option_inner(ty: &Type) -> Option<Type> {
    let Type::Path(type_path) = ty else {
        return None;
    };

    let Some(segment) = type_path.path.segments.last() else {
        return None;
    };

    if segment.ident != "Option" {
        return None;
    }

    let syn::PathArguments::AngleBracketed(args) = &segment.arguments else {
        return None;
    };

    let Some(syn::GenericArgument::Type(inner)) = args.args.first() else {
        return None;
    };

    Some(inner.clone())
}

/// If the type is `Option<T>`, return `T` token stream. Otherwise return the type itself.
pub fn unwrap_option_type(ty: &Type) -> TokenStream2 {
    let Some(inner_type) = get_option_inner(ty) else {
        return quote! { #ty };
    };

    quote! { #inner_type }
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

/// Convert PascalCase or camelCase to snake_case.
pub fn to_snake_case(s: &str) -> String {
    let mut result = String::with_capacity(s.len() + 4);
    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        if !ch.is_uppercase() {
            result.push(ch);
            continue;
        }

        if !result.is_empty()
            && !result.ends_with('_')
            && let Some(&next) = chars.peek()
            && next.is_lowercase()
        {
            result.push('_');
        }

        for lc in ch.to_lowercase() {
            result.push(lc);
        }
    }

    result
}

/// Extract `key = "value"` from a list of attributes for a specific attribute scope.
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
        let Some(eq_pos) = trimmed.find('=') else {
            continue;
        };

        let k = trimmed[..eq_pos].trim();
        if k == key {
            return trimmed[eq_pos + 1..].trim().trim_matches('"').to_string();
        }
    }

    String::new()
}

#[cfg(test)]
mod tests {
    use quote::ToTokens;

    use super::*;

    #[test]
    fn test_snake_to_pascal() {
        assert_eq!(snake_to_pascal("hello_world"), "HelloWorld");
        assert_eq!(snake_to_pascal("foo_bar_baz"), "FooBarBaz");
        assert_eq!(snake_to_pascal("snake_case"), "SnakeCase");
        assert_eq!(snake_to_pascal("alreadyPascal"), "AlreadyPascal");
        assert_eq!(snake_to_pascal("singleword"), "Singleword");
    }

    #[test]
    fn test_camel_to_snake() {
        assert_eq!(to_snake_case("HelloWorld"), "hello_world");
        assert_eq!(to_snake_case("FooBarBaz"), "foo_bar_baz");
        assert_eq!(to_snake_case("snakeCase"), "snake_case");
        assert_eq!(to_snake_case("AlreadySnake"), "already_snake");
        assert_eq!(to_snake_case("SingleWord"), "single_word");
    }

    #[test]
    fn test_to_snake_case() {
        assert_eq!(to_snake_case("HelloWorld"), "hello_world");
        assert_eq!(to_snake_case("FooBarBaz"), "foo_bar_baz");
        assert_eq!(to_snake_case("snakeCase"), "snake_case");
        assert_eq!(to_snake_case("AlreadySnake"), "already_snake");
        assert_eq!(to_snake_case("SingleWord"), "single_word");
    }

    #[test]
    fn test_pascal_to_snake() {
        assert_eq!(to_snake_case("HelloWorld"), "hello_world");
        assert_eq!(to_snake_case("FooBarBaz"), "foo_bar_baz");
        assert_eq!(to_snake_case("snakeCase"), "snake_case");
        assert_eq!(to_snake_case("AlreadySnake"), "already_snake");
        assert_eq!(to_snake_case("SingleWord"), "single_word");
    }

    #[test]
    fn test_get_option() {
        let ty: Type = syn::parse_str("Option<i32>").unwrap();
        let inner = get_option_inner(&ty).unwrap();
        assert_eq!(inner.to_token_stream().to_string(), "i32");

        let ty: Type = syn::parse_str("Option<()>").unwrap();
        let inner = get_option_inner(&ty).unwrap();
        assert_eq!(inner.to_token_stream().to_string(), "()");

        let ty2: Type = syn::parse_str("String").unwrap();
        assert!(get_option_inner(&ty2).is_none());
    }
}
