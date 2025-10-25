#![feature(proc_macro_quote)]
extern crate proc_macro;
use proc_macro::TokenStream;
use std::cmp::min;
use syn::__private::quote::{format_ident, quote};
use syn::punctuated::Punctuated;
use syn::{parse_macro_input, Expr, Ident, Lit, Pat};

#[proc_macro]
pub fn io_read(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item with Punctuated::<Expr, syn::Token![,]>::parse_terminated);

    struct Entry {
        size: usize,
        offset: usize,
        func: Ident,
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
                func_name
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
            let entry_offset = i - entry.offset;
            let shift_correct = if entry_offset == 0 { "".to_string() } else { format!("lsrs r0, r0, {}", entry_offset * 8) };

            let (terminate_cond, always_terminate) = if last_entry {
                ("".to_string(), false)
            } else {
                let bytes_read = entry_end - i;
                let next_entry = &entries[entry_index + 1];
                let bytes_read = min(next_entry.offset - entry_end + bytes_read, 4);

                if bytes_read == 4 || entry_offset & 1 == 1 {
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
                    mov r0, r6
                    bl {{func_{}}}
                    {shift_correct}
                    lsls r0, r0, r5
                    orrs r7, r7, r0
                    {terminate_cond}
                    {next_entry}
                ",
                entry.offset
            ));
        }
    }

    let mut asm_args = Vec::new();

    for entry in entries {
        let offset = entry.offset;
        let func_name = entry.func;
        let func_arg = format_ident!("func_{offset}");
        asm_args.push(quote! {
            #func_arg = sym #func_name,
        });
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

    struct Entry {
        size: usize,
        offset: usize,
        func: Ident,
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
                let func_body = closure.body.as_ref();
                let func_name = format_ident!("write_0x{offset:x}_u{}_", size * 8, span = io_ident.span());
                let func_value_type = format_ident!("u{}", size * 8);
                funcs.push(if size == 1 {
                    let func_arg_value = match closure.inputs.first().unwrap() {
                        Pat::Ident(ident) => &ident.ident,
                        _ => unreachable!(),
                    };
                    let func_arg_emu = match closure.inputs.last().unwrap() {
                        Pat::Ident(ident) => &ident.ident,
                        _ => unreachable!(),
                    };
                    quote! {
                        unsafe extern "C" fn #func_name(#func_arg_emu: &mut crate::core::emu::Emu, #func_arg_value: #func_value_type) {
                            #func_body
                        }
                    }
                } else {
                    let func_arg_mask = match closure.inputs.first().unwrap() {
                        Pat::Ident(ident) => &ident.ident,
                        _ => unreachable!(),
                    };
                    let func_arg_value = match &closure.inputs[1] {
                        Pat::Ident(ident) => &ident.ident,
                        _ => unreachable!(),
                    };
                    let func_arg_emu = match closure.inputs.last().unwrap() {
                        Pat::Ident(ident) => &ident.ident,
                        _ => unreachable!(),
                    };
                    quote! {
                        unsafe extern "C" fn #func_name(#func_arg_emu: &mut crate::core::emu::Emu, #func_arg_value: #func_value_type, #func_arg_mask: #func_value_type) {
                            #func_body
                        }
                    }
                });
                func_name
            }
            _ => unreachable!(),
        };

        entries.push(Entry { size, offset, func });
    }

    let min_addr = entries.first().unwrap().offset;
    let max_addr = entries.last().unwrap().offset + entries.last().unwrap().size;

    let normalize_addr = if min_addr == 0 { "".to_string() } else { format!("subs r1, r1, {min_addr}") };
    let align = 5;
    let align_size = 1 << align;

    let mut asm = format!(
        "
        .align {align}
            push {{{{r4-r6,lr}}}}
            {normalize_addr}
            mov r6, r0
            add r0, pc, {}
            lsls r2, r2, 3
            ldr r4, =0xFFFFFFFF
            bic r0, r0, {}
            lsls r4, r4, r2
            mov r5, r3
            mvns r4, r4
            add r0, r0, r1, lsl #{align}
            mov pc, r0
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
                            pop {{{{r4-r6,pc}}}}
                        "
                    )
                } else {
                    format!(
                        ".align {align}
                        {i}:
                            lsrs r6, r6, {}
                            lsrs r4, r4, {}
                            it eq
                            popeq {{{{r4-r6,pc}}}}
                            b {}f
                        ",
                        remaining_empty * 8,
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
            let entry_offset = i - entry.offset;
            let value_mask = if entry_offset == 0 {
                if entry.size == 1 {
                    "mov r1, r6".to_string()
                } else {
                    "mov r1, r6
                    mov r2, r4"
                        .to_string()
                }
            } else if entry_offset & 1 == 1 {
                format!(
                    "lsls r1, r6, {}
                    ldr r2, ={}",
                    entry_offset * 8,
                    0xFFu32 << (entry_offset * 8)
                )
            } else {
                format!(
                    "lsls r1, r6, {}
                    lsls r2, r4, {}",
                    entry_offset * 8,
                    entry_offset * 8
                )
            };

            let (terminate_cond, always_terminate) = if last_entry {
                ("".to_string(), false)
            } else {
                let bytes_write = entry_end - i;
                let next_entry = &entries[entry_index + 1];
                let bytes_write = min(next_entry.offset - entry_end + bytes_write, 4);

                if bytes_write == 4 || entry_offset & 1 == 1 {
                    (format!("pop {{{{r4-r6,pc}}}}"), true)
                } else {
                    (
                        format!(
                            "lsrs r6, r6, {}
                            lsrs r4, r4, {}
                            it eq
                            popeq {{{{r4-r6,pc}}}}",
                            bytes_write * 8,
                            bytes_write * 8
                        ),
                        false,
                    )
                }
            };

            let next_entry = if always_terminate {
                "".to_string()
            } else if entry.offset + entry.size == max_addr {
                "pop {{r4-r6,pc}}".to_string()
            } else {
                format!("b {}f", entry.offset + entry.size)
            };

            asm.push_str(&format!(
                ".align {align}
                {i}:
                    mov r0, r5
                    {value_mask}
                    bl {{func_{}}}
                    {terminate_cond}
                    {next_entry}
                ",
                entry.offset
            ));
        }
    }

    let mut asm_args = Vec::new();

    for entry in entries {
        let offset = entry.offset;
        let func_name = entry.func;
        let func_arg = format_ident!("func_{offset}");
        asm_args.push(quote! {
            #func_arg = sym #func_name,
        });
    }

    quote! {
        #[unsafe(naked)]
        pub unsafe extern "C" fn write(value: u32, offset: u32, size: usize, emu: &mut crate::core::emu::Emu) -> u32 {
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
