#![feature(proc_macro_quote)]

extern crate proc_macro;

use proc_macro::{Span, TokenStream};
use syn::__private::quote::{format_ident, quote_spanned};
use syn::__private::ToTokens;
use syn::parse::{Parse, ParseStream};
use syn::token::DotDotEq;
use syn::{parse_macro_input, parse_quote_spanned, Arm, Block, Expr, ExprBlock, ExprLit, ExprMatch, ExprRange, Lit, LitInt, Pat, RangeLimits, Stmt};

fn write_block(base: u32, size: usize, span: Span) -> Block {
    if size == 1 {
        parse_quote_spanned! {span.into()=>{
            #[allow(unused_assignments, unused_variables)]
            {
                let value = bytes_window[index];
                addr_offset_tmp += 1;
                #[allow(unused_braces)]
                { block_placeholder() }
            }
        }}
    } else {
        let type_str = format_ident!("u{}", size << 3);
        let (le_bytes_arg, le_mask_arg) = if size == 2 {
            (
                quote_spanned!(span.into()=> [bytes_window[index_start], bytes_window[index_start + 1]]),
                quote_spanned!(span.into()=> [mask_window[index_start], mask_window[index_start + 1]]),
            )
        } else {
            (
                quote_spanned!(span.into()=> [bytes_window[index_start], bytes_window[index_start + 1], bytes_window[index_start + 2], bytes_window[index_start + 3]]),
                quote_spanned!(span.into()=> [mask_window[index_start], mask_window[index_start + 1], mask_window[index_start + 2], mask_window[index_start + 3]]),
            )
        };
        parse_quote_spanned! {span.into()=>{
            #[allow(unused_assignments, unused_variables)]
            {
                let offset = addr_offset_tmp - #base;
                let index_start = index - offset as usize;
                let index_end = index_start + #size;
                let value = #type_str::from_le_bytes(#le_bytes_arg);
                let mask = #type_str::from_le_bytes(#le_mask_arg);
                index = index_end - 1;
                addr_offset_tmp = addr_offset + #size as u32;
                #[allow(unused_braces)]
                { block_placeholder() }
            }
        }}
    }
}

fn read_block(base: u32, size: usize, span: Span) -> Block {
    let type_str = format_ident!("u{}", size << 3);
    if size == 1 {
        parse_quote_spanned! {span.into()=>{
            #[allow(unreachable_code, unused_variables)]
            {
                let ret: #type_str = { block_placeholder() };
                bytes_window[index] = ret;
                addr_offset_tmp += 1;
            }
        }}
    } else {
        parse_quote_spanned! {span.into()=>{
            #[allow(unreachable_code, unused_variables)]
            {
                let ret: #type_str = { block_placeholder() };
                let bytes = ret.to_le_bytes();
                let bytes = bytes.as_slice();
                let offset = addr_offset_tmp - #base;
                let index_start = index - offset as usize;
                let index_end = index_start + #size;
                bytes_window[index_start..index_end].copy_from_slice(bytes);
                index = index_end - 1;
                addr_offset_tmp = addr_offset + #size as u32;
            }
        }}
    }
}

fn place_block(block: &mut Block, replacement: &Expr) {
    for stmt in &mut block.stmts {
        match stmt {
            Stmt::Local(local) => {
                if let Some(local_init) = &mut local.init {
                    match local_init.expr.as_mut() {
                        Expr::Array(array) => {
                            for elem in &mut array.elems {
                                if let Expr::Block(block) = elem {
                                    place_block(&mut block.block, replacement)
                                }
                            }
                        }
                        Expr::Block(block) => {
                            place_block(&mut block.block, replacement);
                        }
                        _ => {}
                    }
                }
            }
            Stmt::Expr(expr, _) => match expr {
                Expr::Block(block) => {
                    place_block(&mut block.block, replacement);
                }
                Expr::Call(call) => {
                    if let Expr::Path(path) = call.func.as_mut() {
                        for segment in &path.path.segments {
                            if segment.ident == "block_placeholder" {
                                *expr = replacement.clone();
                                break;
                            }
                        }
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }
}

fn traverse_match<const WRITE: bool>(expr: &mut ExprMatch) {
    for arm in &mut expr.arms {
        if let Pat::TupleStruct(tuple_struct) = &mut arm.pat {
            if tuple_struct.path.segments.len() == 1 {
                let ident = &tuple_struct.path.segments.first().unwrap().ident;
                let get_addr = || {
                    assert_eq!(tuple_struct.elems.len(), 1);
                    if let Pat::Lit(lit) = tuple_struct.elems.first().unwrap() {
                        if let Lit::Int(lit) = &lit.lit {
                            return Some((u32::from_str_radix(lit.to_string().trim_start_matches("0x"), 16).unwrap(), lit.span()));
                        }
                    }
                    None
                };
                let replace = |arm: &mut Arm, addr: u32, length: u32, span: Span| {
                    arm.pat = if length == 1 {
                        Pat::Lit(ExprLit {
                            attrs: Vec::new(),
                            lit: Lit::Int(LitInt::new(&addr.to_string(), span.into())),
                        })
                    } else {
                        Pat::Range(ExprRange {
                            attrs: Vec::new(),
                            start: Some(Box::new(Expr::Lit(ExprLit {
                                attrs: Vec::new(),
                                lit: Lit::Int(LitInt::new(&addr.to_string(), span.into())),
                            }))),
                            limits: RangeLimits::Closed(DotDotEq { spans: [span.into(); 3] }),
                            end: Some(Box::new(Expr::Lit(ExprLit {
                                attrs: Vec::new(),
                                lit: Lit::Int(LitInt::new(&(addr + length - 1).to_string(), span.into())),
                            }))),
                        })
                    };

                    let mut new_block = if WRITE {
                        write_block(addr, length as usize, span)
                    } else {
                        read_block(addr, length as usize, span)
                    };

                    place_block(&mut new_block, &arm.body);

                    arm.body = Box::new(Expr::Block(ExprBlock {
                        attrs: Vec::new(),
                        label: None,
                        block: new_block,
                    }));
                };
                match ident.to_string().as_str() {
                    "io8" => {
                        if let Some((addr, span)) = get_addr() {
                            replace(arm, addr, 1, span.unwrap());
                        }
                    }
                    "io16" => {
                        if let Some((addr, span)) = get_addr() {
                            replace(arm, addr, 2, span.unwrap());
                        }
                    }
                    "io32" => {
                        if let Some((addr, span)) = get_addr() {
                            replace(arm, addr, 4, span.unwrap());
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

struct IoPortsRead {
    expr_match: ExprMatch,
}

struct IoPortsWrite {
    expr_match: ExprMatch,
}

impl Parse for IoPortsRead {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut expr_match: ExprMatch = input.parse()?;
        traverse_match::<false>(&mut expr_match);
        Ok(IoPortsRead { expr_match })
    }
}

impl Parse for IoPortsWrite {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut expr_match: ExprMatch = input.parse()?;
        traverse_match::<true>(&mut expr_match);
        Ok(IoPortsWrite { expr_match })
    }
}

#[proc_macro]
pub fn io_ports_read(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as IoPortsRead);
    input.expr_match.to_token_stream().into()
}

#[proc_macro]
pub fn io_ports_write(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as IoPortsWrite);
    input.expr_match.to_token_stream().into()
}
