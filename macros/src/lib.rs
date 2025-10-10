#![feature(proc_macro_quote)]

extern crate proc_macro;
use proc_macro2::TokenStream;
use std::collections::HashMap;
use syn::__private::quote::{format_ident, quote};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::{parse_macro_input, Expr, ExprTuple, Ident, Lit, Pat};

fn create_lut(entries: &[(usize, usize, Ident, &Ident)], funcs: &HashMap<usize, (Ident, TokenStream)>) -> Vec<TokenStream> {
    let mut lut_entries = Vec::<TokenStream>::new();
    let mut last_offset = entries.first().unwrap().1;
    let mut last_non_empty_offset = last_offset;
    for (size, offset, _, _) in entries {
        match funcs.get(offset) {
            Some((func_name, _)) => {
                let offset_diff = offset - last_offset;
                if offset_diff > 0 {
                    for i in (1..offset_diff + 1).rev() {
                        lut_entries.push(
                            quote! {
                                (empty, #i),
                            }
                            .into(),
                        );
                    }
                }
                for i in (1..*size + 1).rev() {
                    lut_entries.push(
                        quote! {
                            (#func_name, #i),
                        }
                        .into(),
                    );
                }

                last_offset = *offset + *size;
                last_non_empty_offset = *offset + *size;
            }
            None => last_offset = *offset,
        }
    }

    let end_offset = {
        let (size, offset, _, _) = entries.last().unwrap();
        *size + *offset
    };

    let offset_diff = end_offset - last_non_empty_offset;
    for i in (1..offset_diff + 1).rev() {
        lut_entries.push(
            quote! {
                (empty, #i),
            }
            .into(),
        );
    }

    lut_entries
}

fn create_mod<'a>(expr: &'a ExprTuple, all_entries: &mut Vec<(usize, usize, Ident, &'a Ident)>) -> TokenStream {
    let mut entries = Vec::new();
    let mut read_funcs = HashMap::new();
    let mut write_funcs = HashMap::new();

    let mut last_offset = -1;

    let (mod_name, mod_content) = match expr.elems.first().unwrap() {
        Expr::Macro(m) => (&m.mac.path.segments.first().unwrap().ident, &m.mac.tokens),
        _ => unreachable!(),
    };

    for io in expr.elems.iter().skip(1) {
        let entry = match io {
            Expr::Tuple(tuple) => tuple,
            _ => unreachable!(),
        };

        let (size, offset, typ) = match entry.elems.first().unwrap() {
            Expr::Call(call) => {
                let size = match call.func.as_ref() {
                    Expr::Path(path) => match path.path.segments.first().unwrap().ident.to_string().as_str() {
                        "io8" => 1,
                        "io16" => 2,
                        "io32" => 4,
                        _ => unreachable!(),
                    },
                    _ => unreachable!(),
                } as usize;
                (
                    size,
                    match call.args.first().unwrap() {
                        Expr::Lit(lit) => match &lit.lit {
                            Lit::Int(n) => n.base10_parse::<usize>().unwrap(),
                            _ => unreachable!(),
                        },
                        _ => unreachable!(),
                    },
                    if call.args.len() > 1 {
                        match &call.args[1] {
                            Expr::Path(path) => path.path.segments.first().unwrap().ident.clone(),
                            _ => unreachable!(),
                        }
                    } else {
                        format_ident!("u{}", size * 8)
                    },
                )
            }
            _ => unreachable!(),
        };

        assert!(offset as isize > last_offset);
        last_offset = offset as isize;

        let name = match &entry.elems[1] {
            Expr::Path(path) => &path.path.segments.first().unwrap().ident,
            _ => unreachable!(),
        };

        entries.push((size, offset, typ, name));

        let create_func = |expr: &Expr, prefix: &str| match expr {
            Expr::Closure(closure) => {
                if !closure.inputs.is_empty() {
                    assert_eq!(closure.inputs.len(), 1);

                    let func_arg = match closure.inputs.first().unwrap() {
                        Pat::Ident(ident) => &ident.ident,
                        _ => unreachable!(),
                    };

                    let func = closure.body.as_ref();

                    let func_name = format_ident!("_{prefix}_{offset:x}", span = func.span());
                    let func_arg = format_ident!("emu", span = func_arg.span());
                    let func = quote! {
                        #[allow(unreachable_code)]
                        fn #func_name(#func_arg: &mut crate::core::emu::Emu) {
                            #func;
                        }
                    }
                    .into();
                    Some((func_name, func))
                } else {
                    None
                }
            }
            _ => unreachable!(),
        };

        if entry.elems.len() > 2 {
            if let Some((func_name, func)) = create_func(&entry.elems[2], "read") {
                read_funcs.insert(offset, (func_name, func));
            }
        }

        if entry.elems.len() > 3 {
            if let Some((func_name, func)) = create_func(&entry.elems[3], "write") {
                write_funcs.insert(offset, (func_name, func));
            }
        }
    }

    all_entries.extend_from_slice(&entries);

    let lut_read_entries = create_lut(&entries, &read_funcs);
    let lut_write_entries = create_lut(&entries, &write_funcs);
    let begin_addr = entries.first().unwrap().1;
    let end_addr = entries.last().unwrap().0 + entries.last().unwrap().1;

    let read_funcs = read_funcs.iter().map(|(_, (_, func))| func).collect::<Vec<_>>();
    let write_funcs = write_funcs.iter().map(|(_, (_, func))| func).collect::<Vec<_>>();

    let lut_read_size = lut_read_entries.len();
    let lut_write_size = lut_write_entries.len();
    let tokens = quote! {
        pub mod #mod_name {
            #mod_content

            pub const BEGIN_ADDR: usize = #begin_addr;
            pub const END_ADDR: usize = #end_addr;

            static READ_LUT: [(fn(&mut crate::core::emu::Emu), usize); #lut_read_size] = [
                #(#lut_read_entries)*
            ];

            static WRITE_LUT: [(fn(&mut crate::core::emu::Emu), usize); #lut_write_size] = [
                #(#lut_write_entries)*
            ];

            #(#read_funcs)*

            #(#write_funcs)*

            #[allow(unreachable_code)]
            fn empty(_: &mut crate::core::emu::Emu) {
            }
        }
    };
    tokens
}

#[proc_macro]
pub fn io(item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(item with Punctuated::<Expr, syn::Token![,]>::parse_terminated);
    let mut all_entries = Vec::new();
    let mut mods = Vec::new();

    for entry in &input {
        if let Expr::Tuple(tuple) = entry {
            mods.push(create_mod(tuple, &mut all_entries));
        } else {
            unreachable!();
        }
    }

    let mut memory_struct = Vec::new();
    let mut last_offset = 0;
    for (size, offset, typ, name) in all_entries {
        if offset - last_offset > 0 {
            let padding_name = format_ident!("_padding{}", memory_struct.len());
            let offset = offset - last_offset;
            memory_struct.push(quote! {
                #padding_name: Padding::<#offset>,
            });
        }
        memory_struct.push(quote! {
            pub #name: #typ,
        });
        last_offset = offset + size;
    }

    quote! {
        struct Padding<const SIZE: usize>([u8; SIZE]);

        impl<const SIZE: usize> Default for Padding<SIZE> {
            fn default() -> Self {
                unsafe { std::mem::zeroed() }
            }
        }

        #[derive(Default)]
        #[repr(C)]
        pub struct Memory {
            #(#memory_struct)*
        }

        #(#mods)*
    }
    .into()
}
