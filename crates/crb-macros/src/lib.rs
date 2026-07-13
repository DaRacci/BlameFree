use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_macro_input, Data, DeriveInput, Fields};

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

    // RefForm struct fields: cache_key and plain fields pass through,
    // cache_ref fields become String
    let ref_fields: Vec<_> = cache_key_fields
        .iter()
        .map(|(ident, ty)| quote! { #ident: #ty })
        .chain(cache_ref_fields.iter().map(|(ident, _)| quote! { #ident: String }))
        .chain(plain_fields.iter().map(|(ident, ty)| quote! { #ident: #ty }))
        .collect();

    // CacheKey impl for RefForm: hash contributions from all key fields
    let ref_key_contributions: Vec<_> = cache_key_fields
        .iter()
        .map(|(ident, _)| {
            quote! { ::serde_json::to_string(&self.#ident).unwrap_or_default() }
        })
        .chain(cache_ref_fields.iter().map(|(ident, _)| {
            quote! { self.#ident.clone() }
        }))
        .collect();

    // into_ref: serialize cache_ref fields to JSON, store in backend, replace with key
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
        .chain(plain_fields.iter().map(|(ident, _)| quote! { #ident: self.#ident }))
        .collect();

    // from_ref: load serialized JSON from backend, deserialize
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

    // Standalone cache_key() associated function
    let cache_key_params: Vec<_> = cache_key_fields
        .iter()
        .map(|(ident, ty)| quote! { #ident: &#ty })
        .chain(cache_ref_fields.iter().map(|(ident, ty)| quote! { #ident: &#ty }))
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
