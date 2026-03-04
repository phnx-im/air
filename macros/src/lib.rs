// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Procedural macros for the Air Protocol crates.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{Data, DeriveInput, Fields, FieldsNamed, GenericParam, LitInt, parse_macro_input};

// Helpers

struct FieldInfo {
    ident: syn::Ident,
    ty: syn::Type,
    tag: u32,
    /// `true` when the field type is `Vec<u8>` or `[u8; N]` — serialized via `serde_bytes`.
    is_bytes: bool,
    /// Length expression when `is_bytes` is set for a fixed-size `[u8; N]` field.
    array_len: Option<syn::Expr>,
}

/// Returns the single path segment of a plain (no `qself`) single-segment type path, or `None`.
fn single_path_seg(ty: &syn::Type) -> Option<&syn::PathSegment> {
    if let syn::Type::Path(tp) = ty
        && tp.qself.is_none()
        && tp.path.segments.len() == 1
    {
        tp.path.segments.first()
    } else {
        None
    }
}

/// Returns `true` when `ty` is syntactically `Vec<u8>`.
fn is_vec_u8(ty: &syn::Type) -> bool {
    if let Some(seg) = single_path_seg(ty)
        && seg.ident == "Vec"
        && let syn::PathArguments::AngleBracketed(args) = &seg.arguments
        && args.args.len() == 1
        && let syn::GenericArgument::Type(inner) = &args.args[0]
        && let Some(inner_seg) = single_path_seg(inner)
        && inner_seg.ident == "u8"
        && matches!(inner_seg.arguments, syn::PathArguments::None)
    {
        true
    } else {
        false
    }
}

/// Returns the length expression when `ty` is syntactically `[u8; N]`, otherwise `None`.
fn array_u8_len(ty: &syn::Type) -> Option<syn::Expr> {
    if let syn::Type::Array(arr) = ty
        && let Some(inner_seg) = single_path_seg(&arr.elem)
        && inner_seg.ident == "u8"
        && matches!(inner_seg.arguments, syn::PathArguments::None)
    {
        Some(arr.len.clone())
    } else {
        None
    }
}

/// Returns `true` when `ty` is syntactically `Vec<_>` (any element type).
fn is_vec(ty: &syn::Type) -> bool {
    single_path_seg(ty).is_some_and(|s| s.ident == "Vec")
}

/// Returns `true` when `ty` is syntactically `String`.
fn is_string(ty: &syn::Type) -> bool {
    single_path_seg(ty)
        .is_some_and(|s| s.ident == "String" && matches!(s.arguments, syn::PathArguments::None))
}

/// Returns `true` when `ty` is syntactically `Option<_>`.
fn is_option(ty: &syn::Type) -> bool {
    single_path_seg(ty).is_some_and(|s| s.ident == "Option")
}

/// Returns `true` when `ty` is syntactically `bool`.
fn is_bool(ty: &syn::Type) -> bool {
    single_path_seg(ty)
        .is_some_and(|s| s.ident == "bool" && matches!(s.arguments, syn::PathArguments::None))
}

const INTEGER_TYPES: &[&str] = &[
    "u8", "u16", "u32", "u64", "u128", "i8", "i16", "i32", "i64", "i128", "usize", "isize",
];

/// Returns `true` when `ty` is syntactically a primitive integer type.
fn is_integer(ty: &syn::Type) -> bool {
    single_path_seg(ty).is_some_and(|s| {
        matches!(s.arguments, syn::PathArguments::None)
            && INTEGER_TYPES.contains(&s.ident.to_string().as_str())
    })
}

/// Returns a boolean expression that is `true` when the field value differs from its default,
/// using type-specific checks to avoid allocating a default value where possible.
fn skip_if_default_condition(fi: &FieldInfo) -> TokenStream2 {
    let ident = &fi.ident;
    let ty = &fi.ty;
    if is_vec(ty) || is_string(ty) {
        quote! { !self.#ident.is_empty() }
    } else if is_option(ty) {
        quote! { self.#ident.is_some() }
    } else if is_bool(ty) {
        quote! { self.#ident }
    } else if is_integer(ty) {
        quote! { self.#ident != 0 }
    } else {
        quote! { {
            let __default: #ty = ::core::default::Default::default();
            self.#ident != __default
        } }
    }
}

fn extract_field_infos(fields: &FieldsNamed) -> Vec<FieldInfo> {
    let infos: Vec<FieldInfo> = fields
        .named
        .iter()
        .map(|field| {
            let ident = field.ident.clone().unwrap();
            let ty = field.ty.clone();
            let array_len = array_u8_len(&ty);
            let is_bytes = is_vec_u8(&ty) || array_len.is_some();

            let mut tag: Option<u32> = None;
            for attr in &field.attrs {
                if attr.path().is_ident("tag") {
                    let lit: LitInt = attr
                        .parse_args()
                        .expect("#[tag(N)] expects a single integer literal");
                    let n = lit
                        .base10_parse::<u32>()
                        .expect("#[tag(N)] key must fit in a u32");
                    assert!(n >= 1, "#[tag(N)] tags must start at 1 (got 0)");
                    tag = Some(n);
                    break;
                }
            }

            FieldInfo {
                ident,
                ty,
                tag: tag.expect("every field must carry an #[tag(N)] attribute"),
                is_bytes,
                array_len,
            }
        })
        .collect();

    // Detect duplicate tags across fields.
    let mut seen: std::collections::HashMap<u32, &syn::Ident> = std::collections::HashMap::new();
    for fi in &infos {
        if let Some(prev) = seen.insert(fi.tag, &fi.ident) {
            panic!(
                "#[tag({tag})] is used on both `{prev}` and `{curr}` — tags must be unique within a struct",
                tag = fi.tag,
                prev = prev,
                curr = fi.ident,
            );
        }
    }

    infos
}

/// Derives `serde::Serialize` for a struct as an integer-keyed map.
///
/// Every field must be annotated with `#[tag(N)]` where `N` is a `u32` integer that becomes the
/// map key in the encoded form.
///
/// A field is **omitted** from the output when its value equals `Default::default()`
/// (skip-if-default semantics). `Vec<u8>` and `[u8; N]` fields are automatically serialized as
/// CBOR byte strings via `serde_bytes`.
///
/// # Example
///
/// ```ignore
/// #[derive(Serialize_tagged_map, Deserialize_tagged_map)]
/// pub struct Foo {
///     #[tag(0)]
///     pub id: Uuid,
///     #[tag(1)]
///     pub payload: Vec<u8>,  // encoded as bytes
/// }
/// ```
#[proc_macro_derive(Serialize_tagged_map, attributes(tag))]
pub fn derive_serialize_tagged_map(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let fields = match &input.data {
        Data::Struct(s) => match &s.fields {
            Fields::Named(f) => f,
            _ => panic!("Serialize_tagged_map only supports named structs"),
        },
        _ => panic!("Serialize_tagged_map only supports structs"),
    };

    let infos = extract_field_infos(fields);

    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let entries: Vec<TokenStream2> = infos
        .iter()
        .map(|fi| {
            let ident = &fi.ident;
            let tag = fi.tag;
            let serialize_value: TokenStream2 = if fi.is_bytes {
                quote! { ::serde_bytes::Bytes::new(&self.#ident) }
            } else {
                quote! { &self.#ident }
            };
            let condition = skip_if_default_condition(fi);
            quote! {
                if #condition {
                    _map.serialize_entry(&#tag, #serialize_value)?;
                }
            }
        })
        .collect();

    quote! {
        impl #impl_generics serde::Serialize for #name #ty_generics #where_clause {
            fn serialize<S>(&self, serializer: S) -> ::core::result::Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                use serde::ser::SerializeMap as _;
                let mut _map = serializer.serialize_map(None)?;
                #(#entries)*
                _map.end()
            }
        }
    }
    .into()
}

/// Derives `serde::Deserialize` for a struct from an integer-keyed map.
///
/// Every field must be annotated with `#[tag(N)]` where `N` is a `u32` integer that matches the
/// map key in the encoded form.
///
/// Missing keys deserialize to `Default::default()` (default-if-absent semantics). Unknown keys
/// are silently ignored for forward compatibility. `Vec<u8>` and `[u8; N]` fields are
/// automatically deserialized from CBOR byte strings via `serde_bytes`.
#[proc_macro_derive(Deserialize_tagged_map, attributes(tag))]
pub fn derive_deserialize_tagged_map(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let fields = match &input.data {
        Data::Struct(s) => match &s.fields {
            Fields::Named(f) => f,
            _ => panic!("Deserialize_tagged_map only supports named structs"),
        },
        _ => panic!("Deserialize_tagged_map only supports structs"),
    };

    let infos = extract_field_infos(fields);
    let visitor_name = syn::Ident::new(&format!("{name}Visitor"), name.span());

    // Original generics (without 'de) — used after the type name.
    let orig_generics = &input.generics;
    let (_, ty_generics, orig_where_clause) = orig_generics.split_for_impl();

    // Add 'de as a plain lifetime with no bounds on the struct's own lifetimes.
    // We deliberately do NOT add 'de: 'a because Cow<'a, str> (and similar types)
    // always deserialise as owned values — serde's Deserialize impl for Cow never
    // borrows from the input.  Adding 'de: 'a would make the impl too restrictive
    // (it would no longer satisfy `for<'de> Deserialize<'de>` bounds that callers
    // like PersistenceCodec::from_slice require).
    let de_lt_param: syn::LifetimeParam = syn::parse_quote!('de);

    // Clone the struct's generics and prepend 'de.
    let mut all_generics = orig_generics.clone();
    all_generics
        .params
        .insert(0, GenericParam::Lifetime(de_lt_param));
    let (all_impl_generics, all_ty_generics, all_where_clause) =
        all_generics.split_for_impl();

    let var_decls: Vec<TokenStream2> = infos
        .iter()
        .map(|fi| {
            let ident = &fi.ident;
            let ty = &fi.ty;
            quote! { let mut #ident: #ty = ::core::default::Default::default(); }
        })
        .collect();

    let match_arms: Vec<TokenStream2> = infos
        .iter()
        .map(|fi| {
            let ident = &fi.ident;
            let tag = fi.tag;
            if fi.is_bytes {
                if let Some(n) = &fi.array_len {
                    quote! {
                        #tag => {
                            #ident = _map.next_value::<::serde_bytes::ByteArray<#n>>()?.into_array();
                        }
                    }
                } else {
                    quote! {
                        #tag => {
                            #ident = _map.next_value::<::serde_bytes::ByteBuf>()?.into_vec();
                        }
                    }
                }
            } else {
                quote! {
                    #tag => {
                        #ident = _map.next_value()?;
                    }
                }
            }
        })
        .collect();

    let field_names: Vec<&syn::Ident> = infos.iter().map(|fi| &fi.ident).collect();

    quote! {
        impl #all_impl_generics serde::Deserialize<'de> for #name #ty_generics #orig_where_clause {
            fn deserialize<D>(deserializer: D) -> ::core::result::Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                // Two PhantomData fields: `marker` captures the struct's own generics
                // (e.g. 'a), `lifetime` captures 'de so Rust knows the visitor uses it.
                struct #visitor_name #all_ty_generics {
                    marker: ::core::marker::PhantomData<#name #ty_generics>,
                    lifetime: ::core::marker::PhantomData<&'de ()>,
                }

                impl #all_impl_generics serde::de::Visitor<'de> for #visitor_name #all_ty_generics #all_where_clause {
                    type Value = #name #ty_generics;

                    fn expecting(
                        &self,
                        f: &mut ::core::fmt::Formatter,
                    ) -> ::core::fmt::Result {
                        f.write_str(concat!("a map representing ", stringify!(#name)))
                    }

                    fn visit_map<A>(
                        self,
                        mut _map: A,
                    ) -> ::core::result::Result<Self::Value, A::Error>
                    where
                        A: serde::de::MapAccess<'de>,
                    {
                        #(#var_decls)*
                        while let Some(_key) = _map.next_key::<u32>()? {
                            match _key {
                                #(#match_arms)*
                                _ => {
                                    let _: serde::de::IgnoredAny = _map.next_value()?;
                                }
                            }
                        }
                        ::core::result::Result::Ok(#name {
                            #(#field_names,)*
                        })
                    }
                }

                deserializer.deserialize_map(#visitor_name {
                    marker: ::core::marker::PhantomData,
                    lifetime: ::core::marker::PhantomData,
                })
            }
        }
    }
    .into()
}
