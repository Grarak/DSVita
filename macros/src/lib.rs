#![feature(proc_macro_quote)]
extern crate proc_macro;
use proc_macro2::TokenStream;
use syn::__private::quote::{format_ident, quote};
use syn::punctuated::Punctuated;
use syn::token::Comma;
use syn::{parse_macro_input, Expr, ExprClosure, Ident, Lit, Pat};

struct Entry {
    size: usize,
    offset: usize,
    func_name: Ident,
    func: TokenStream,
}

impl Entry {
    fn offset_end(&self) -> usize {
        self.offset + self.size
    }
}

fn assemble_entries_funcs<F: Fn(&ExprClosure, usize, usize, &Ident) -> (Ident, TokenStream)>(input: Punctuated<Expr, Comma>, entries: &mut Vec<Entry>, build_func: F) {
    let mut last_offset = -1;
    for expr in input {
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

        let (func_name, func) = match tuple.elems.last().unwrap() {
            Expr::Closure(closure) => build_func(closure, size, offset, io_ident),
            _ => unreachable!(),
        };

        entries.push(Entry { size, offset, func_name, func });
    }
}

struct LutEntry<'a> {
    offset: usize,
    entries: Vec<&'a Entry>,
}

fn assemble_lut_entries<'a>(entries: &'a [Entry], align: usize) -> Vec<LutEntry<'a>> {
    let min_addr = entries.first().unwrap().offset;
    let max_addr = entries.last().unwrap().offset_end();
    let align_min_addr = min_addr & !(align - 1);
    let align_max_addr = (max_addr + (align - 1)) & !(align - 1);

    let mut lut_entries = Vec::new();
    let mut entry_index = 0;
    for addr in (align_min_addr..align_max_addr).step_by(align) {
        let addr_end = addr + align;
        let mut entries_within_addr_range = Vec::new();
        for i in entry_index..entries.len() {
            let entry = &entries[i];
            if (addr..addr_end).contains(&entry.offset) || (entry.offset..entry.offset_end()).contains(&addr) {
                entries_within_addr_range.push(entry);
                entry_index = i;
            } else if addr_end <= entry.offset {
                break;
            }
        }
        assert!(entries_within_addr_range.len() <= align);
        lut_entries.push(LutEntry {
            offset: addr,
            entries: entries_within_addr_range,
        });
    }

    lut_entries
}

fn assemble_read_lut(entries: &[LutEntry], align: usize) -> Vec<(Ident, Option<TokenStream>)> {
    let mut entries_funcs = Vec::new();
    for lut_entry in entries {
        let func_ret_type = format_ident!("u{}", align * 8);
        if lut_entry.entries.is_empty() {
            let func_name = format_ident!("read_empty_u{align}_lut");
            entries_funcs.push((func_name, None));
        } else {
            let mut entry_funcs = Vec::new();
            for entry in &lut_entry.entries {
                let func = &entry.func_name;
                entry_funcs.push(if entry.offset >= lut_entry.offset {
                    let offset = entry.offset - lut_entry.offset;
                    let shift = offset * 8;
                    quote! {
                        {
                            (#func(emu) as #func_ret_type) << #shift
                        }
                    }
                } else {
                    let offset = lut_entry.offset - entry.offset;
                    let shift = offset * 8;
                    quote! {
                        {
                            (#func(emu) >> #shift) as #func_ret_type
                        }
                    }
                });
            }
            let func_name = format_ident!("read_0x{:x}_u{align}_lut", lut_entry.offset);
            let read_func = quote! {
                fn #func_name(emu: &mut crate::core::emu::Emu) -> #func_ret_type {
                    0 #( | #entry_funcs)*
                }
            };
            entries_funcs.push((func_name, Some(read_func)));
        }
    }

    entries_funcs
}

#[proc_macro]
pub fn io_read(item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(item with Punctuated::<Expr, syn::Token![,]>::parse_terminated);

    let mut entries = Vec::new();

    assemble_entries_funcs(input, &mut entries, |closure, size, offset, io_ident| {
        let func_arg = match closure.inputs.first().unwrap() {
            Pat::Ident(ident) => &ident.ident,
            _ => unreachable!(),
        };
        let func_body = closure.body.as_ref();
        let func_name = format_ident!("read_0x{offset:x}_u{}_", size * 8, span = io_ident.span());
        let func_ret_type = format_ident!("u{}", size * 8);
        let func = quote! {
            fn #func_name(#func_arg: &mut crate::core::emu::Emu) -> #func_ret_type {
                #func_body
            }
        };
        (func_name, func)
    });

    let mut lut = Vec::new();
    let mut get_read_matches = Vec::new();
    let mut read_matches = Vec::new();
    for align in [1, 2, 4] {
        let lut_entries = assemble_lut_entries(&entries, align);
        let lut_funcs = assemble_read_lut(&lut_entries, align);
        let lut_func_names = lut_funcs.iter().map(|(name, _)| name).collect::<Vec<_>>();
        let lut_funcs = lut_funcs.iter().filter(|(_, func)| func.is_some()).map(|(_, func)| func.as_ref().unwrap()).collect::<Vec<_>>();
        let lut_func_names_len = lut_func_names.len();
        let size = align * 8;
        let lut_name = format_ident!("LUT_{size}");
        let size_type = format_ident!("u{size}");
        let get_read_func = format_ident!("get_read{size}");
        let read_func = format_ident!("read{size}");
        let offset_shift = align >> 1;
        let func_empty_name = format_ident!("read_empty_u{align}_lut");
        lut.push(quote! {
            #(#lut_funcs)*

            static #lut_name: [fn(&mut crate::core::emu::Emu) -> #size_type; #lut_func_names_len] = [
                #(#lut_func_names ,)*
            ];

            pub fn #get_read_func(offset: u32) -> fn(&mut crate::core::emu::Emu) -> #size_type {
                let offset = offset - MIN_ADDR;
                unsafe { *#lut_name.get_unchecked((offset as usize) >> #offset_shift) }
            }

            pub fn #read_func(emu: &mut crate::core::emu::Emu, offset: u32) -> #size_type {
                let func = #get_read_func(offset);
                func(emu)
            }

            pub fn #func_empty_name(_: &mut crate::core::emu::Emu) -> #size_type {
                0
            }
        });
        get_read_matches.push(quote! {
            #align => #get_read_func(offset) as *const (),
        });
        read_matches.push(quote! {
            #align => T::from(#read_func(emu, offset) as u32),
        });
    }

    let funcs = entries.iter().map(|entry| &entry.func);

    let min_addr = entries.first().unwrap().offset;
    let max_addr = entries.last().unwrap().offset_end();
    quote! {
        pub const MIN_ADDR: u32 = #min_addr as u32;
        pub const MAX_ADDR: u32 = #max_addr as u32;

        #(#funcs)*

        #(#lut)*

        pub fn is_in_range(offset: u32) -> bool {
            offset >= MIN_ADDR && offset < MAX_ADDR
        }

        pub fn get_read<T: crate::utils::Convert>(offset: u32) -> fn(&mut crate::core::emu::Emu) -> T {
            unsafe { std::mem::transmute(match size_of::<T>() {
                #(#get_read_matches)*
                _ => unsafe { std::hint::unreachable_unchecked() }
            }) }
        }

        pub fn read<T: crate::utils::Convert>(emu: &mut crate::core::emu::Emu, offset: u32) -> T {
            match size_of::<T>() {
                #(#read_matches)*
                _ => unsafe { std::hint::unreachable_unchecked() }
            }
        }
    }
    .into()
}

fn assemble_write_lut(entries: &[LutEntry], align: usize) -> Vec<(Ident, Option<TokenStream>)> {
    let mut entries_funcs = Vec::new();
    for lut_entry in entries {
        let func_value_type = format_ident!("u{}", align * 8);
        if lut_entry.entries.is_empty() {
            let func_name = format_ident!("write_empty_u{align}_lut");
            entries_funcs.push((func_name, None));
        } else {
            let mut entry_funcs = Vec::new();
            for entry in &lut_entry.entries {
                let func = &entry.func_name;
                let entry_value_type = format_ident!("u{}", entry.size * 8);
                let mask = 0xFFFFFFFFu32 >> ((4 - align) * 8);
                entry_funcs.push(if entry.offset >= lut_entry.offset {
                    let offset = entry.offset - lut_entry.offset;
                    let shift = offset * 8;
                    if entry.size == 1 {
                        quote! {
                            {
                                #func(emu, (value >> #shift) as u8)
                            }
                        }
                    } else {
                        quote! {
                            {
                                #func(emu, (value >> #shift) as #entry_value_type, #mask as #entry_value_type)
                            }
                        }
                    }
                } else {
                    assert_ne!(entry.size, 1);
                    let offset = lut_entry.offset - entry.offset;
                    let shift = offset * 8;
                    let mask = mask << shift;
                    quote! {
                        {
                            #func(emu, (value as #entry_value_type) << #shift, #mask as #entry_value_type)
                        }
                    }
                });
            }
            let func_name = format_ident!("write_0x{:x}_u{align}_lut", lut_entry.offset);
            let write_func = quote! {
                fn #func_name(emu: &mut crate::core::emu::Emu, value: #func_value_type) {
                    #(#entry_funcs ;)*
                }
            };
            entries_funcs.push((func_name, Some(write_func)));
        }
    }

    entries_funcs
}

#[proc_macro]
pub fn io_write(item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(item with Punctuated::<Expr, syn::Token![,]>::parse_terminated);

    let mut entries = Vec::new();

    assemble_entries_funcs(input, &mut entries, |closure, size, offset, io_ident| {
        let func_body = closure.body.as_ref();
        let func_name = format_ident!("write_0x{offset:x}_u{}_", size * 8, span = io_ident.span());
        let func_value_type = format_ident!("u{}", size * 8);
        let func = if size == 1 {
            let func_arg_value = match closure.inputs.first().unwrap() {
                Pat::Ident(ident) => &ident.ident,
                _ => unreachable!(),
            };
            let func_arg_emu = match closure.inputs.last().unwrap() {
                Pat::Ident(ident) => &ident.ident,
                _ => unreachable!(),
            };
            quote! {
                fn #func_name(#func_arg_emu: &mut crate::core::emu::Emu, #func_arg_value: #func_value_type) {
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
                fn #func_name(#func_arg_emu: &mut crate::core::emu::Emu, #func_arg_value: #func_value_type, #func_arg_mask: #func_value_type) {
                    #func_body
                }
            }
        };
        (func_name, func)
    });

    let mut lut = Vec::new();
    let mut get_write_matches = Vec::new();
    let mut write_matches = Vec::new();
    for align in [1, 2, 4] {
        let lut_entries = assemble_lut_entries(&entries, align);
        let lut_funcs = assemble_write_lut(&lut_entries, align);
        let lut_func_names = lut_funcs.iter().map(|(name, _)| name).collect::<Vec<_>>();
        let lut_funcs = lut_funcs.iter().filter(|(_, func)| func.is_some()).map(|(_, func)| func.as_ref().unwrap()).collect::<Vec<_>>();
        let lut_func_names_len = lut_func_names.len();
        let size = align * 8;
        let lut_name = format_ident!("LUT_{size}");
        let size_type = format_ident!("u{size}");
        let get_write_func = format_ident!("get_write{size}");
        let write_func = format_ident!("write{size}");
        let offset_shift = align >> 1;
        let func_empty_name = format_ident!("write_empty_u{align}_lut");
        lut.push(quote! {
            #(#lut_funcs)*

            static #lut_name: [fn(&mut crate::core::emu::Emu, value: #size_type); #lut_func_names_len] = [
                #(#lut_func_names ,)*
            ];

            pub fn #get_write_func(offset: u32) -> fn(&mut crate::core::emu::Emu, value: #size_type) {
                let offset = offset - MIN_ADDR;
                unsafe { *#lut_name.get_unchecked((offset as usize) >> #offset_shift) }
            }

            pub fn #write_func(emu: &mut crate::core::emu::Emu, value: #size_type, offset: u32) {
                let func = #get_write_func(offset);
                func(emu, value);
            }

            pub fn #func_empty_name(_: &mut crate::core::emu::Emu, _: #size_type) {
            }
        });
        get_write_matches.push(quote! {
            #align => #get_write_func(offset) as *const (),
        });
        write_matches.push(quote! {
            #align => #write_func(emu, value.into() as #size_type, offset),
        });
    }

    let funcs = entries.iter().map(|entry| &entry.func);

    let min_addr = entries.first().unwrap().offset;
    let max_addr = entries.last().unwrap().offset_end();
    quote! {
        pub const MIN_ADDR: u32 = #min_addr as u32;
        pub const MAX_ADDR: u32 = #max_addr as u32;

        #(#funcs)*

        #(#lut)*

        pub fn is_in_range(offset: u32) -> bool {
            offset >= MIN_ADDR && offset < MAX_ADDR
        }

        pub fn get_write<T: crate::utils::Convert>(offset: u32) -> fn(&mut crate::core::emu::Emu, T) {
            unsafe { std::mem::transmute(match size_of::<T>() {
                #(#get_write_matches)*
                _ => unsafe { std::hint::unreachable_unchecked() }
            })}
        }

        pub fn write<T: crate::utils::Convert>(emu: &mut crate::core::emu::Emu, value: T, offset: u32) {
            match size_of::<T>() {
                #(#write_matches)*
                _ => unsafe { std::hint::unreachable_unchecked() }
            }
        }
    }
    .into()
}
