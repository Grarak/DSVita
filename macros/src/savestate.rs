use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Data, DeriveInput, Error, Fields, Index, Path};

// Field handling selected by #[savestate(...)] attributes:
// - (none): recurse via the Savestate trait
// - skip: not serialized; left untouched on load (enum variant fields fall back to Default)
// - bytes: raw memcpy of the field (Copy bound enforced by SavestateContext::bytes_of)
// - with = "path": custom fn(&mut Field, &mut SavestateContext)
enum FieldMode {
    Recurse,
    Skip,
    Bytes,
    With(Path),
}

fn parse_field_mode(attrs: &[syn::Attribute]) -> syn::Result<FieldMode> {
    let mut mode = FieldMode::Recurse;
    for attr in attrs {
        if !attr.path().is_ident("savestate") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("skip") {
                mode = FieldMode::Skip;
            } else if meta.path.is_ident("bytes") {
                mode = FieldMode::Bytes;
            } else if meta.path.is_ident("with") {
                let lit: syn::LitStr = meta.value()?.parse()?;
                mode = FieldMode::With(lit.parse()?);
            } else {
                return Err(meta.error("expected `skip`, `bytes` or `with = \"path\"`"));
            }
            Ok(())
        })?;
    }
    Ok(mode)
}

fn struct_is_bytes(input: &DeriveInput) -> syn::Result<bool> {
    let mut bytes = false;
    for attr in &input.attrs {
        if !attr.path().is_ident("savestate") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("bytes") {
                bytes = true;
                Ok(())
            } else {
                Err(meta.error("only `bytes` is supported at type level"))
            }
        })?;
    }
    Ok(bytes)
}

fn field_call(mode: &FieldMode, target: TokenStream) -> Option<TokenStream> {
    match mode {
        FieldMode::Recurse => Some(quote! { crate::savestate::Savestate::savestate(#target, state); }),
        FieldMode::Skip => None,
        FieldMode::Bytes => Some(quote! { state.bytes_of(#target); }),
        FieldMode::With(path) => Some(quote! { #path(#target, state); }),
    }
}

fn derive_struct(fields: &Fields) -> syn::Result<TokenStream> {
    let mut calls = Vec::new();
    for (i, field) in fields.iter().enumerate() {
        let mode = parse_field_mode(&field.attrs)?;
        let target = match &field.ident {
            Some(ident) => quote! { &mut self.#ident },
            None => {
                let index = Index::from(i);
                quote! { &mut self.#index }
            }
        };
        if let Some(call) = field_call(&mode, target) {
            calls.push(call);
        }
    }
    Ok(quote! { #(#calls)* })
}

fn derive_enum(input: &DeriveInput, data: &syn::DataEnum) -> syn::Result<TokenStream> {
    if data.variants.is_empty() {
        return Err(Error::new_spanned(input, "Savestate cannot be derived for empty enums"));
    }
    if data.variants.len() > 256 {
        return Err(Error::new_spanned(input, "Savestate enums are limited to 256 variants (u8 tag)"));
    }

    let mut save_arms = Vec::new();
    let mut load_arms = Vec::new();
    for (tag, variant) in data.variants.iter().enumerate() {
        let tag = tag as u8;
        let variant_ident = &variant.ident;
        match &variant.fields {
            Fields::Unit => {
                save_arms.push(quote! { Self::#variant_ident => state.put_u8(#tag), });
                load_arms.push(quote! { #tag => *self = Self::#variant_ident, });
            }
            Fields::Unnamed(fields) => {
                let bindings = (0..fields.unnamed.len()).map(|i| format_ident!("f{i}")).collect::<Vec<_>>();
                let modes = fields.unnamed.iter().map(|field| parse_field_mode(&field.attrs)).collect::<syn::Result<Vec<_>>>()?;
                let types = fields.unnamed.iter().map(|field| &field.ty).collect::<Vec<_>>();

                let save_calls = bindings.iter().zip(&modes).filter_map(|(binding, mode)| field_call(mode, quote! { #binding }));
                let save_pats = bindings.iter().zip(&modes).map(|(binding, mode)| match mode {
                    FieldMode::Skip => quote! { _ },
                    _ => quote! { #binding },
                });
                save_arms.push(quote! { Self::#variant_ident(#(#save_pats),*) => { state.put_u8(#tag); #(#save_calls)* } });

                // Load constructs every field from Default first (skipped ones stay Default),
                // then fills the serialized ones in declaration order.
                let load_inits = bindings.iter().zip(&types).map(|(binding, ty)| quote! { let mut #binding: #ty = ::core::default::Default::default(); });
                let load_calls = bindings.iter().zip(&modes).filter_map(|(binding, mode)| field_call(mode, quote! { &mut #binding }));
                load_arms.push(quote! { #tag => { #(#load_inits)* #(#load_calls)* *self = Self::#variant_ident(#(#bindings),*); } });
            }
            Fields::Named(fields) => {
                let idents = fields.named.iter().map(|field| field.ident.as_ref().unwrap()).collect::<Vec<_>>();
                let modes = fields.named.iter().map(|field| parse_field_mode(&field.attrs)).collect::<syn::Result<Vec<_>>>()?;
                let types = fields.named.iter().map(|field| &field.ty).collect::<Vec<_>>();

                let save_calls = idents.iter().zip(&modes).filter_map(|(ident, mode)| field_call(mode, quote! { #ident }));
                let save_pats = idents.iter().zip(&modes).map(|(ident, mode)| match mode {
                    FieldMode::Skip => quote! { #ident: _ },
                    _ => quote! { #ident },
                });
                save_arms.push(quote! { Self::#variant_ident { #(#save_pats),* } => { state.put_u8(#tag); #(#save_calls)* } });

                let load_bindings = idents.iter().map(|ident| format_ident!("f_{ident}")).collect::<Vec<_>>();
                let load_inits = load_bindings
                    .iter()
                    .zip(&types)
                    .map(|(binding, ty)| quote! { let mut #binding: #ty = ::core::default::Default::default(); });
                let load_calls = load_bindings.iter().zip(&modes).filter_map(|(binding, mode)| field_call(mode, quote! { &mut #binding }));
                load_arms.push(quote! { #tag => { #(#load_inits)* #(#load_calls)* *self = Self::#variant_ident { #(#idents: #load_bindings),* }; } });
            }
        }
    }

    Ok(quote! {
        if state.is_save() {
            match self {
                #(#save_arms)*
            }
        } else {
            match state.take_u8() {
                #(#load_arms)*
                _ => state.set_error(),
            }
        }
    })
}

pub fn derive(input: DeriveInput) -> syn::Result<TokenStream> {
    let is_bytes = struct_is_bytes(&input)?;
    let body = if is_bytes {
        quote! { state.bytes_of_raw(self); }
    } else {
        match &input.data {
            Data::Struct(data) => derive_struct(&data.fields)?,
            Data::Enum(data) => derive_enum(&input, data)?,
            Data::Union(_) => return Err(Error::new_spanned(&input, "Savestate cannot be derived for unions, use #[savestate(bytes)] or a manual impl")),
        }
    };

    let mut generics = input.generics.clone();
    let type_params = generics.type_params().map(|param| param.ident.clone()).collect::<Vec<_>>();
    if !is_bytes && !type_params.is_empty() {
        let where_clause = generics.make_where_clause();
        for param in type_params {
            where_clause.predicates.push(syn::parse_quote! { #param: crate::savestate::Savestate });
        }
    }
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let ident = &input.ident;
    Ok(quote! {
        impl #impl_generics crate::savestate::Savestate for #ident #ty_generics #where_clause {
            fn savestate(&mut self, state: &mut crate::savestate::SavestateContext) {
                #body
            }
        }
    })
}
