mod alu_delegations {
    use crate::jit::disassembler::alu_instructions::*;
    use crate::jit::disassembler::InstInfo;
    use crate::jit::jit::JitAsm;
    use paste::paste;

    macro_rules! generate_variations {
        ($name:ident, $([$variation:ident]),+) => {
            paste! {
                $(
                    #[inline]
                    pub fn [<$name _ $variation>](asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
                        $name(asm, name, opcode, $variation(opcode))
                    }
                )*
            }
        };

        ($name:ident, $([$variation:ident, $suffix:tt]),+) => {
            paste! {
                $(
                    #[inline]
                    pub fn [<$name _ $variation>](asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
                        $name(asm, name, opcode, [<$variation $suffix>](opcode))
                    }
                )*
            }
        };
    }

    macro_rules! generate_op {
        ($name:ident) => {
            generate_variations!($name, [lli], [llr], [lri], [lrr], [ari], [arr], [rri], [rrr], [imm]);
        };

        ($name:ident, $suffix:tt) => {
            paste! {
                generate_variations!(
                    $name, [lli, $suffix], [llr, $suffix], [lri, $suffix], [lrr, $suffix], [ari, $suffix], [arr, $suffix],
                    [rri, $suffix], [rrr, $suffix], [imm, $suffix]
                );
            }
        };
    }

    generate_op!(_and);
    generate_op!(ands, _s);
    generate_op!(eor);
    generate_op!(eors, _s);
    generate_op!(sub);
    generate_op!(subs);
    generate_op!(rsb);
    generate_op!(rsbs);
    generate_op!(add);
    generate_op!(adds);
    generate_op!(adc);
    generate_op!(adcs);
    generate_op!(sbc);
    generate_op!(sbcs);
    generate_op!(rsc);
    generate_op!(rscs);
    generate_op!(tst, _s);
    generate_op!(teq, _s);
    generate_op!(cmp, _s);
    generate_op!(cmn, _s);
    generate_op!(orr);
    generate_op!(orrs, _s);
    generate_op!(mov);
    generate_op!(movs, _s);
    generate_op!(bic);
    generate_op!(bics, _s);
    generate_op!(mvn);
    generate_op!(mvns, _s);
}

pub use alu_delegations::*;

mod transfer_delegations {
    use crate::jit::disassembler::transfer_instructions::*;
    use crate::jit::disassembler::InstInfo;
    use crate::jit::jit::JitAsm;
    use paste::paste;

    macro_rules! generate_variation {
        ($name:ident, $suffix:ident, $variation:ident, $processor:ident) => {
            paste! {
                #[inline]
                pub fn [<$name _ $variation>](asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
                    [<$name _ $suffix>](asm, name, opcode, $processor(opcode))
                }
            }
        };

        ($name:ident, $suffix:ident, $variation:ident, $prefix:tt, $processor:ident) => {
            paste! {
                #[inline]
                pub fn [<$name _ $variation>](asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
                    [<$name _ $suffix>](asm, name, opcode, !($processor(opcode) - 1))
                }
            }
        };
    }

    macro_rules! generate_variations {
        ($name:ident, $([$suffix:ident, $($args:tt)*]),+) => {
            $(
                generate_variation!($name, $suffix, $($args)*);
            )*
        };
    }

    macro_rules! generate_op_half {
        ($name:ident) => {
            generate_variations!($name, [of, ofrm, -, rp], [of, ofim, -, ip_h], [of, ofrp, rp], [of, ofip, ip_h], [pr, prrm, -, rp], [pr, prim, -, ip_h], [pr, prrp, rp], [pr, prip, ip_h], [pt, ptrm, -, rp], [pt, ptim, -, ip_h], [pt, ptrp, rp], [pt, ptip, ip_h]);
        };
    }

    macro_rules! generate_op_full {
        ($name:ident) => {
            generate_variations!($name, [of, ofim, -, ip], [of, ofip, ip], [of, ofrmll, -, rpll], [of, ofrmlr, -, rplr], [of, ofrmar, -, rpar], [of, ofrmrr, -, rprr], [of, ofrpll, rpll], [of, ofrplr, rplr], [of, ofrpar, rpar], [of, ofrprr, rprr], [pr, prim, -, ip], [pr, prip, ip], [pr, prrmll, -, rpll], [pr, prrmlr, -, rplr], [pr, prrmar, -, rpar], [pr, prrmrr, -, rprr], [pr, prrpll, rpll], [pr, prrplr, rplr], [pr, prrpar, rpar], [pr, prrprr, rprr], [pt, ptim, -, ip], [pt, ptip, ip], [pt, ptrmll, -, rpll], [pt, ptrmlr, -, rplr], [pt, ptrmar, -, rpar], [pt, ptrmrr, -, rprr], [pt, ptrpll, rpll], [pt, ptrplr, rplr], [pt, ptrpar, rpar], [pt, ptrprr, rprr]);
        };
    }

    generate_op_half!(ldrsb);
    generate_op_half!(ldrsh);
    generate_op_half!(ldrh);
    generate_op_half!(strh);
    generate_op_half!(ldrd);
    generate_op_half!(strd);

    generate_op_full!(ldrb);
    generate_op_full!(strb);
    generate_op_full!(ldr);
    generate_op_full!(str);
}

pub use transfer_delegations::*;

mod unknown_delegations {
    use crate::jit::disassembler::InstInfo;
    use crate::jit::jit::JitAsm;

    pub fn unk_arm(asm: &mut JitAsm, name: &'static str, opcode: u32) -> InstInfo {
        todo!()
    }
}

pub use unknown_delegations::*;
