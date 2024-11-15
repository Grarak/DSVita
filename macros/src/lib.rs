#![feature(proc_macro_quote)]
extern crate proc_macro;
use core::panic;
use proc_macro::TokenStream;
use std::cmp;
use std::collections::HashMap;
use syn::__private::quote::{format_ident, quote};
use syn::__private::ToTokens;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::{parse_macro_input, Expr, Lit, Pat};

#[proc_macro]
pub fn io_read(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item with Punctuated::<Expr, syn::Token![,]>::parse_terminated);
    let name = input.first().unwrap().to_token_stream().to_string();
    let io_ports = match input.last().unwrap() {
        Expr::Array(array) => array,
        _ => panic!(),
    };

    let mut min_addr = u32::MAX;
    let mut max_addr = 0;
    let mut exprs = HashMap::new();
    let mut funcs = Vec::new();

    for elem in &io_ports.elems {
        let tuple = match elem {
            Expr::Tuple(tuple) => tuple,
            _ => panic!(),
        };

        let io_call = match &tuple.elems[0] {
            Expr::Call(io_call) => io_call,
            _ => panic!(),
        };

        let closure = match &tuple.elems[1] {
            Expr::Closure(closure) => closure,
            _ => panic!(),
        };

        let closure_arg = match closure.inputs.first().unwrap() {
            Pat::Ident(ident) => ident,
            _ => panic!(),
        };

        let io_call_path = match &io_call.func.as_ref() {
            Expr::Path(path) => path,
            _ => panic!(),
        };

        let io_call_lit = match &io_call.args[0] {
            Expr::Lit(lit) => match &lit.lit {
                Lit::Int(lit) => lit,
                _ => panic!(),
            },
            _ => panic!(),
        };

        let segment = io_call_path.path.segments.first().unwrap();

        let size = match segment.ident.to_string().as_str() {
            "io8" => 1u8,
            "io16" => 2u8,
            "io32" => 4u8,
            _ => panic!(),
        };

        let addr_str = io_call_lit.to_string();
        let addr = u32::from_str_radix(addr_str.trim_start_matches("0x"), 16).unwrap();
        min_addr = cmp::min(min_addr, addr);
        max_addr = cmp::max(max_addr, addr + size as u32);

        let func_name = format_ident!("_read_{addr_str}");
        let func_arg = format_ident!("emu", span = closure_arg.span());
        let body = closure.body.as_ref();
        let func = quote!(
            #[allow(unreachable_code)]
            fn #func_name(#func_arg: &mut crate::core::emu::Emu) -> u32 {
                #body as u32
            }
        );
        exprs.insert(addr, (size, func_name));
        funcs.push(func);
    }

    funcs.push(quote!(
        fn _read_empty(_: &mut crate::core::emu::Emu) -> u32 {
            0
        }
    ));

    let mut lut_entries = Vec::new();

    if min_addr > 0 {
        min_addr -= cmp::min(3, min_addr);
    }
    max_addr += 3;

    let mut i = min_addr;
    while i < max_addr {
        if let Some((size, func_name)) = exprs.get(&i) {
            for j in 0..*size {
                let remaining = *size - j;
                let offset = j << 3;
                lut_entries.push(quote!(
                    (Self::#func_name, #remaining, #offset),
                ));
            }
            i += *size as u32;
        } else {
            let mut j = i + 1;
            while j < max_addr {
                if exprs.get(&j).is_some() {
                    break;
                }
                j += 1;
            }
            let size = j - i;
            for k in 0..size {
                let remaining = cmp::min(4, size - k) as u8;
                lut_entries.push(quote!(
                    (Self::_read_empty, #remaining, 0),
                ));
            }
            i = j;
        }
    }

    let size = (max_addr - min_addr) as usize;
    assert_eq!(lut_entries.len(), size);

    let lut_tokens = quote!(
        const _LUT: [(fn(&mut crate::core::emu::Emu) -> u32, u8, u8); #size] = [
            #(#lut_entries)*
        ];
    );

    let name = format_ident!("{name}");
    let tokens = quote!(
        pub struct #name;

        impl #name {
            #(#funcs)*

            #lut_tokens

            const MIN_ADDR: u32 = #min_addr;
            const MAX_ADDR: u32 = #max_addr;

            pub fn is_in_range(addr: u32) -> bool {
                (Self::MIN_ADDR..Self::MAX_ADDR).contains(&addr)
            }

            pub fn read(addr: u32, size: u8, emu: &mut crate::core::emu::Emu) -> u32 {
                let addr = addr - Self::MIN_ADDR;
                let mut ret = 0;
                let mut read = 0;
                while read < size {
                    let (func, read_size, offset) = unsafe { Self::_LUT.get_unchecked(addr as usize + read as usize) };
                    let value = func(emu) >> *offset;
                    ret |= value << (read << 3);
                    read += *read_size;
                }
                ret
            }
        }
    );
    tokens.into()
}

#[proc_macro]
pub fn io_write(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item with Punctuated::<Expr, syn::Token![,]>::parse_terminated);
    let name = input.first().unwrap().to_token_stream().to_string();
    let io_ports = match input.last().unwrap() {
        Expr::Array(array) => array,
        _ => panic!(),
    };

    let mut min_addr = u32::MAX;
    let mut max_addr = 0;
    let mut exprs = HashMap::new();
    let mut funcs = Vec::new();

    for elem in &io_ports.elems {
        let tuple = match elem {
            Expr::Tuple(tuple) => tuple,
            _ => panic!(),
        };

        let io_call = match &tuple.elems[0] {
            Expr::Call(io_call) => io_call,
            _ => panic!(),
        };

        let closure = match &tuple.elems[1] {
            Expr::Closure(closure) => closure,
            _ => panic!(),
        };

        let io_call_path = match &io_call.func.as_ref() {
            Expr::Path(path) => path,
            _ => panic!(),
        };

        let io_call_lit = match &io_call.args[0] {
            Expr::Lit(lit) => match &lit.lit {
                Lit::Int(lit) => lit,
                _ => panic!(),
            },
            _ => panic!(),
        };

        let segment = io_call_path.path.segments.first().unwrap();

        let size = match segment.ident.to_string().as_str() {
            "io8" => 1u8,
            "io16" => 2u8,
            "io32" => 4u8,
            _ => panic!(),
        };

        let (closure_mask_arg, closure_value_arg, closure_emu_arg) = if size == 1 {
            let closure_value_arg = match &closure.inputs[0] {
                Pat::Ident(ident) => ident,
                _ => panic!(),
            };

            let closure_emu_arg = match &closure.inputs[1] {
                Pat::Ident(ident) => ident,
                _ => panic!(),
            };

            (None, closure_value_arg, closure_emu_arg)
        } else {
            let closure_mask_arg = match &closure.inputs[0] {
                Pat::Ident(ident) => ident,
                _ => panic!(),
            };

            let closure_value_arg = match &closure.inputs[1] {
                Pat::Ident(ident) => ident,
                _ => panic!(),
            };

            let closure_emu_arg = match &closure.inputs[2] {
                Pat::Ident(ident) => ident,
                _ => panic!(),
            };

            (Some(closure_mask_arg), closure_value_arg, closure_emu_arg)
        };

        let addr_str = io_call_lit.to_string();
        let addr = u32::from_str_radix(addr_str.trim_start_matches("0x"), 16).unwrap();
        min_addr = cmp::min(min_addr, addr);
        max_addr = cmp::max(max_addr, addr + size as u32);

        let func_name = format_ident!("_write_{addr_str}");
        let func_value_arg = format_ident!("value", span = closure_value_arg.span());
        let func_emu_arg = format_ident!("emu", span = closure_emu_arg.span());
        let body = closure.body.as_ref();
        let u_type = format_ident!("u{}", size << 3);

        let func = if size == 1 {
            quote!(
                #[allow(unreachable_code)]
                fn #func_name(_: u32, value: u32, #func_emu_arg: &mut crate::core::emu::Emu) {
                    let #func_value_arg = value as #u_type;
                    #body
                }
            )
        } else {
            let func_mask_arg = format_ident!("mask", span = closure_mask_arg.unwrap().span());
            quote!(
                #[allow(unreachable_code)]
                fn #func_name(mask: u32, value: u32, #func_emu_arg: &mut crate::core::emu::Emu) {
                    let #func_mask_arg = mask as #u_type;
                    let #func_value_arg = value as #u_type;
                    #body
                }
            )
        };
        exprs.insert(addr, (size, func_name));
        funcs.push(func);
    }

    funcs.push(quote!(
        fn _write_empty(_: u32, _: u32, _: &mut crate::core::emu::Emu) {}
    ));

    let mut lut_entries = Vec::new();

    if min_addr > 0 {
        min_addr -= cmp::min(3, min_addr);
    }
    max_addr += 3;

    let mut i = min_addr;
    while i < max_addr {
        if let Some((size, func_name)) = exprs.get(&i) {
            for j in 0..*size {
                let remaining = *size - j;
                let offset = j << 3;
                lut_entries.push(quote!(
                    (Self::#func_name, #remaining, #offset),
                ));
            }
            i += *size as u32;
        } else {
            let mut j = i + 1;
            while j < max_addr {
                if exprs.get(&j).is_some() {
                    break;
                }
                j += 1;
            }
            let size = j - i;
            for k in 0..size {
                let remaining = cmp::min(4, size - k) as u8;
                lut_entries.push(quote!(
                    (Self::_write_empty, #remaining, 0),
                ));
            }
            i = j;
        }
    }

    let size = (max_addr - min_addr) as usize;
    assert_eq!(lut_entries.len(), size);

    let lut_tokens = quote!(
        const _LUT: [(fn(mask: u32, value: u32, &mut crate::core::emu::Emu), u8, u8); #size] = [
            #(#lut_entries)*
        ];
    );

    let name = format_ident!("{name}");
    let tokens = quote!(
        pub struct #name;

        impl #name {
            #(#funcs)*

            #lut_tokens

            const MIN_ADDR: u32 = #min_addr;
            const MAX_ADDR: u32 = #max_addr;

            pub fn is_in_range(addr: u32) -> bool {
                (Self::MIN_ADDR..Self::MAX_ADDR).contains(&addr)
            }

            pub fn write(value: u32, addr: u32, size: u8, emu: &mut crate::core::emu::Emu) {
                let addr = addr - Self::MIN_ADDR;
                let mut written = 0;
                let mask = 0xFFFFFFFF >> ((4 - size) << 3);
                while written < size {
                    let (func, write_size, offset) = unsafe { Self::_LUT.get_unchecked(addr as usize + written as usize) };
                    let value = value >> (written << 3);
                    let value = value << *offset;
                    let mask = mask >> (written << 3);
                    let mask = mask << *offset;
                    func(mask, value, emu);
                    written += *write_size;
                }
            }
        }
    );
    tokens.into()
}
