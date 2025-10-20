#![feature(proc_macro_quote)]
extern crate proc_macro;
use core::panic;
use proc_macro::TokenStream;
use std::cmp::{self, min};
use std::collections::HashMap;
use syn::__private::quote::{format_ident, quote};
use syn::__private::ToTokens;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::{parse_macro_input, Expr, Ident, Lit, Pat};

#[proc_macro]
pub fn io_read(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item with Punctuated::<Expr, syn::Token![,]>::parse_terminated);

    enum EntryFunc {
        Func(Ident),
        MemOffset(Expr),
    }

    struct Entry {
        size: usize,
        offset: usize,
        func: EntryFunc,
    }

    let mut entries = Vec::new();
    let mut funcs = Vec::new();

    let mut last_offset = -1;
    for expr in &input {
        let tuple = match expr {
            Expr::Tuple(tuple) => tuple,
            _ => unreachable!(),
        };

        let (size, io_ident, offset) = match tuple.elems.first().unwrap() {
            Expr::Call(call) => {
                let ident = match call.func.as_ref() {
                    Expr::Path(path) => &path.path.segments.first().unwrap().ident,
                    _ => unreachable!(),
                };
                let size = match ident.to_string().as_str() {
                    "io8" => 1usize,
                    "io16" => 2,
                    "io32" => 4,
                    _ => unreachable!(),
                };
                let offset: usize = match call.args.first().unwrap() {
                    Expr::Lit(lit) => match &lit.lit {
                        Lit::Int(offset) => offset.base10_parse().unwrap(),
                        _ => unreachable!(),
                    },
                    _ => unreachable!(),
                };
                (size, ident, offset)
            }
            _ => unreachable!(),
        };

        assert!(offset as i32 > last_offset);
        last_offset = offset as i32;

        let func = match tuple.elems.last().unwrap() {
            Expr::Closure(closure) => {
                let func_arg = match closure.inputs.first().unwrap() {
                    Pat::Ident(ident) => &ident.ident,
                    _ => unreachable!(),
                };
                let func_body = closure.body.as_ref();
                let func_name = format_ident!("read_0x{offset:x}_u{}_", size * 8, span = io_ident.span());
                let func_ret_type = format_ident!("u{}", size * 8);
                funcs.push(quote! {
                    unsafe extern "C" fn #func_name(#func_arg: &mut crate::core::emu::Emu) -> #func_ret_type {
                        #func_body
                    }
                });
                EntryFunc::Func(func_name)
            }
            Expr::Call(call) => {
                match call.func.as_ref() {
                    Expr::Path(path) => {
                        assert_eq!(path.path.segments.first().unwrap().ident.to_string(), "addr")
                    }
                    _ => unreachable!(),
                }
                EntryFunc::MemOffset(call.args.first().unwrap().clone())
            }
            _ => unreachable!(),
        };

        entries.push(Entry { size, offset, func });
    }

    let min_addr = entries.first().unwrap().offset;
    let max_addr = entries.last().unwrap().offset + entries.last().unwrap().size;

    let normalize_addr = if min_addr == 0 { "".to_string() } else { format!("subs r0, r0, {min_addr}") };
    let align = 5;
    let align_size = 1 << align;

    let mut asm = format!(
        "
        .align {align}
            push {{{{r4-r7,lr}}}}
            {normalize_addr}
            add r3, pc, {}
            lsls r4, r1, 3
            movs r5, 0
            bic r3, r3, {}
            mov r6, r2
            movs r7, 0
            add r3, r3, r0, lsl #{align}
            mov pc, r3
    ",
        align_size - 1,
        align_size - 1
    );

    let mut prev_offset_end = entries.first().unwrap().offset;
    for (entry_index, entry) in entries.iter().enumerate() {
        if entry.offset > prev_offset_end {
            for i in prev_offset_end..entry.offset {
                let remaining_empty = min(entry.offset - i, 4);
                asm.push_str(&if remaining_empty == 4 {
                    format!(
                        ".align {align}
                        {i}:
                            mov r0, r7
                            pop {{{{r4-r7,pc}}}}
                        "
                    )
                } else {
                    format!(
                        ".align {align}
                        {i}:
                            adds r5, r5, {}
                            cmp r4, r5
                            itt le
                            movle r0, r7
                            pople {{{{r4-r7,pc}}}}
                            b {}f
                        ",
                        remaining_empty * 8,
                        entry.offset
                    )
                })
            }
        }
        let entry_end = entry.offset + entry.size;
        prev_offset_end = entry_end;

        let last_entry = entry_index == entries.len() - 1;

        for i in entry.offset..entry_end {
            let read_func = match &entry.func {
                EntryFunc::Func(_) => {
                    format!(
                        "mov r0, r6
                        bl {{func_{}}}",
                        entry.offset
                    )
                }
                EntryFunc::MemOffset(_) => {
                    let op = match entry.size {
                        1 => "ldrb",
                        2 => "ldrh",
                        4 => "ldr",
                        _ => unreachable!(),
                    };
                    format!(
                        "ldr r1, ={{func_{}}}
                        {op} r0, [r6, r1]",
                        entry.offset
                    )
                }
            };

            let entry_offset = i - entry.offset;
            let shift_correct = if entry_offset == 0 { "".to_string() } else { format!("lsrs r0, r0, {}", entry_offset * 8) };

            let (terminate_cond, always_terminate) = if last_entry {
                ("".to_string(), false)
            } else {
                let bytes_read = entry_end - i;
                let next_entry = &entries[entry_index + 1];
                let bytes_read = min(next_entry.offset - entry_end + bytes_read, 4);

                if bytes_read == 4 || i & 1 == 1 {
                    (
                        format!(
                            "mov r0, r7
                    pop {{{{r4-r7,pc}}}}"
                        ),
                        true,
                    )
                } else {
                    (
                        format!(
                            "adds r5, r5, {}
                            cmp r4, r5
                            itt le
                            movle r0, r7
                            pople {{{{r4-r7,pc}}}}",
                            bytes_read * 8
                        ),
                        false,
                    )
                }
            };

            let next_entry = if always_terminate {
                "".to_string()
            } else if entry.offset + entry.size == max_addr {
                "mov r0, r7
                pop {{r4-r7,pc}}"
                    .to_string()
            } else {
                format!("b {}f", entry.offset + entry.size)
            };

            asm.push_str(&format!(
                ".align {align}
                {i}:
                    {read_func}
                    {shift_correct}
                    lsls r0, r0, r5
                    orrs r7, r7, r0
                    {terminate_cond}
                    {next_entry}
                "
            ));
        }
    }

    let mut asm_args = Vec::new();

    for entry in entries {
        let offset = entry.offset;
        match entry.func {
            EntryFunc::Func(func_name) => {
                let func_arg = format_ident!("func_{offset}");
                asm_args.push(quote! {
                    #func_arg = sym #func_name,
                });
            }
            EntryFunc::MemOffset(field) => {
                let func_arg = format_ident!("func_{offset}");
                let value = match field {
                    Expr::Field(field) => {
                        quote! {
                            std::mem::offset_of!(crate::core::emu::Emu, #field)
                        }
                    }
                    Expr::Call(call) => {
                        quote! {
                            #call
                        }
                    }
                    _ => unreachable!(),
                };
                asm_args.push(quote! {
                    #func_arg = const #value,
                });
            }
        }
    }

    quote! {
        #[unsafe(naked)]
        pub unsafe extern "C" fn read(offset: u32, size: usize, emu: &mut crate::core::emu::Emu) -> u32 {
            std::arch::naked_asm!(
                #asm,
                #(#asm_args)*
            );
        }

        #(#funcs)*

        pub const MIN_ADDR: u32 = #min_addr as u32;
        pub const MAX_ADDR: u32 = #max_addr as u32;

        pub fn is_in_range(offset: u32) -> bool {
            offset >= MIN_ADDR && offset < MAX_ADDR
        }
    }
    .into()
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
                if exprs.contains_key(&j) {
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
            const MAX_ADDR: u32 = #max_addr - 3;

            pub fn is_in_range(addr: u32) -> bool {
                addr < Self::MAX_ADDR
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
