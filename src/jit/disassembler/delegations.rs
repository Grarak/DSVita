mod alu_delegations {
    use crate::jit::disassembler::alu_instructions::*;
    use crate::jit::inst_info::InstInfo;
    use crate::jit::Op;
    use paste::paste;

    macro_rules! generate_variations {
        ($name:ident, $([$variation:ident, $processor:ident]),+) => {
            paste! {
                $(
                    #[inline]
                    pub fn [<$name _ $variation>](opcode: u32, op: Op) -> InstInfo {
                        paste! {
                            [<$name _ $variation _ impl>](opcode, op, $processor(opcode))
                        }
                    }
                )*
            }
        };
    }

    macro_rules! generate_op {
        ($name:ident) => {
            generate_variations!(
                $name,
                [lli, imm_shift],
                [llr, reg_shift],
                [lri, imm_shift],
                [lrr, reg_shift],
                [ari, imm_shift],
                [arr, reg_shift],
                [rri, imm_shift],
                [rrr, reg_shift],
                [imm, imm]
            );
        };
    }

    generate_op!(_and);
    generate_op!(ands);
    generate_op!(eor);
    generate_op!(eors);
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
    generate_op!(tst);
    generate_op!(teq);
    generate_op!(cmp);
    generate_op!(cmn);
    generate_op!(orr);
    generate_op!(orrs);
    generate_op!(mov);
    generate_op!(movs);
    generate_op!(bic);
    generate_op!(bics);
    generate_op!(mvn);
    generate_op!(mvns);
}

pub use alu_delegations::*;

mod transfer_delegations {
    use crate::jit::disassembler::transfer_instructions::*;
    use crate::jit::inst_info::InstInfo;
    use crate::jit::Op;
    use crate::utils::negative;
    use paste::paste;

    macro_rules! generate_variation {
        ($name:ident, $variation:ident, $processor:ident) => {
            paste! {
                #[inline]
                pub fn [<$name _ $variation>](opcode: u32, op: Op) -> InstInfo {
                    [<$name _ $variation _ impl>](opcode, op, $processor(opcode))
                }
            }
        };

        ($name:ident, $suffix:ident, $variation:ident, $processor:ident) => {
            paste! {
                #[inline]
                pub fn [<$name _ $variation>](opcode: u32, op: Op) -> InstInfo {
                    [<$name _ $suffix>](opcode, op, $processor(opcode))
                }
            }
        };

        ($name:ident, $suffix:ident, $variation:ident, $prefix:tt, $processor:ident) => {
            paste! {
                #[inline]
                pub fn [<$name _ $variation>](opcode: u32, op: Op) -> InstInfo {
                    [<$name _ $suffix>](opcode, op, negative($processor(opcode)))
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
            generate_variations!($name, [of, ofim, -, ip_h], [of, ofip, ip_h], [pr, prim, -, ip_h], [pr, prip, ip_h], [pt, ptim, -, ip_h], [pt, ptip, ip_h], [ofrm, rp], [ofrp, rp], [prrm, rp], [prrp, rp], [ptrm, rp], [ptrp, rp]);
        };
    }

    macro_rules! generate_op_full {
        ($name:ident) => {
            generate_variations!($name, [of, ofim, -, ip], [of, ofip, ip], [pr, prim, -, ip], [pr, prip, ip], [pt, ptim, -, ip], [pt, ptip, ip], [ofrmll, imm_shift], [ofrmlr, imm_shift], [ofrmar, imm_shift], [ofrmrr, imm_shift], [ofrpll, imm_shift], [ofrplr, imm_shift], [ofrpar, imm_shift], [ofrprr, imm_shift], [prrmll, imm_shift], [prrmlr, imm_shift], [prrmar, imm_shift], [prrmrr, imm_shift], [prrpll, imm_shift], [prrplr, imm_shift], [prrpar, imm_shift], [prrprr, imm_shift], [ptrmll, imm_shift], [ptrmlr, imm_shift], [ptrmar, imm_shift], [ptrmrr, imm_shift], [ptrpll, imm_shift], [ptrplr, imm_shift], [ptrpar, imm_shift], [ptrprr, imm_shift]);
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
    use crate::jit::inst_info::InstInfo;
    use crate::jit::Op;

    #[inline]
    pub fn unk_arm(opcode: u32, op: Op) -> InstInfo {
        todo!()
    }
}

pub use unknown_delegations::*;
