use proc_macro::TokenStream;
use proc_macro2::{Punct, Spacing};
use quote::quote;
use syn::{
    Ident, LitStr, Token,
    parse::{Parse, ParseStream},
};

pub fn define_routes_impl(input: &RouteInput) -> TokenStream {
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

pub struct RouteInput {
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
