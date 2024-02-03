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

pub(super) use alu_delegations::*;

mod transfer_delegations {
    use crate::jit::disassembler::transfer_instructions::*;
    use crate::jit::inst_info::InstInfo;
    use crate::jit::{Op, ShiftType::*};
    use crate::utils::negative;
    use paste::paste;

    macro_rules! generate_variation {
        ($name:ident, $write:expr, $variation:ident, $processor:ident, mem_transfer_imm) => {
            paste! {
                #[inline]
                pub fn [<$name _ $variation>](opcode: u32, op: Op) -> InstInfo {
                    mem_transfer_imm::<$write>(opcode, op, $processor(opcode))
                }
            }
        };

        ($name:ident, $write:expr, $variation:ident, $prefix:tt, $processor:ident, mem_transfer_imm) => {
            paste! {
                #[inline]
                pub fn [<$name _ $variation>](opcode: u32, op: Op) -> InstInfo {
                    mem_transfer_imm::<$write>(opcode, op, negative($processor(opcode)))
                }
            }
        };

        ($name:ident, $write:expr, $variation:ident, mem_transfer_reg) => {
            paste! {
                #[inline]
                pub fn [<$name _ $variation>](opcode: u32, op: Op) -> InstInfo {
                    mem_transfer_reg::<$write>(opcode, op, reg(opcode))
                }
            }
        };

        ($name:ident, $write:expr, $variation:ident, mem_transfer_reg_shift, $shift_type:expr) => {
            paste! {
                #[inline]
                pub fn [<$name _ $variation>](opcode: u32, op: Op) -> InstInfo {
                    mem_transfer_reg_shift::<$write, { $shift_type }>(opcode, op, reg_imm_shift(opcode))
                }
            }
        };
    }

    macro_rules! generate_variations {
        ($name:ident, $write:expr, $([$variation:ident, $($args:tt)*]),+) => {
            $(
                generate_variation!($name, $write, $variation, $($args)*);
            )*
        };
    }

    macro_rules! generate_op_half {
        ($name:ident, $write:expr) => {
            generate_variations!($name, $write,
                [ofim, -, imm_h, mem_transfer_imm],
                [ofip, imm_h, mem_transfer_imm],
                [prim, -, imm_h, mem_transfer_imm],
                [prip, imm_h, mem_transfer_imm],
                [ptim, -, imm_h, mem_transfer_imm],
                [ptip, imm_h, mem_transfer_imm],
                [ofrm, mem_transfer_reg],
                [ofrp, mem_transfer_reg],
                [prrm, mem_transfer_reg],
                [prrp, mem_transfer_reg],
                [ptrm, mem_transfer_reg],
                [ptrp, mem_transfer_reg]
            );
        };
    }

    macro_rules! generate_op_full {
        ($name:ident, $write:expr) => {
            generate_variations!($name, $write,
                [ofim, -, imm, mem_transfer_imm],
                [ofip, imm, mem_transfer_imm],
                [prim, -, imm, mem_transfer_imm],
                [prip, imm, mem_transfer_imm],
                [ptim, -, imm, mem_transfer_imm],
                [ptip, imm, mem_transfer_imm],
                [ofrmll, mem_transfer_reg_shift, Lsl],
                [ofrmlr, mem_transfer_reg_shift, Lsr],
                [ofrmar, mem_transfer_reg_shift, Asr],
                [ofrmrr, mem_transfer_reg_shift, Ror],
                [ofrpll, mem_transfer_reg_shift, Lsl],
                [ofrplr, mem_transfer_reg_shift, Lsr],
                [ofrpar, mem_transfer_reg_shift, Asr],
                [ofrprr, mem_transfer_reg_shift, Ror],
                [prrmll, mem_transfer_reg_shift, Lsl],
                [prrmlr, mem_transfer_reg_shift, Lsr],
                [prrmar, mem_transfer_reg_shift, Asr],
                [prrmrr, mem_transfer_reg_shift, Ror],
                [prrpll, mem_transfer_reg_shift, Lsl],
                [prrplr, mem_transfer_reg_shift, Lsr],
                [prrpar, mem_transfer_reg_shift, Asr],
                [prrprr, mem_transfer_reg_shift, Ror],
                [ptrmll, mem_transfer_reg_shift, Lsl],
                [ptrmlr, mem_transfer_reg_shift, Lsr],
                [ptrmar, mem_transfer_reg_shift, Asr],
                [ptrmrr, mem_transfer_reg_shift, Ror],
                [ptrpll, mem_transfer_reg_shift, Lsl],
                [ptrplr, mem_transfer_reg_shift, Lsr],
                [ptrpar, mem_transfer_reg_shift, Asr],
                [ptrprr, mem_transfer_reg_shift, Ror]
            );
        };
    }

    generate_op_half!(ldrsb, false);
    generate_op_half!(ldrsh, false);
    generate_op_half!(ldrh, false);
    generate_op_half!(strh, true);
    generate_op_half!(ldrd, false);
    generate_op_half!(strd, true);

    generate_op_full!(ldrb, false);
    generate_op_full!(strb, true);
    generate_op_full!(ldr, false);
    generate_op_full!(str, true);
}

pub(super) use transfer_delegations::*;

mod unknown_delegations {
    use crate::jit::inst_info::{InstInfo, Operands};
    use crate::jit::reg::reg_reserve;
    use crate::jit::Op;

    #[inline]
    pub fn unk_arm(opcode: u32, op: Op) -> InstInfo {
        InstInfo::new(
            opcode,
            op,
            Operands::new_empty(),
            reg_reserve!(),
            reg_reserve!(),
            1,
        )
    }
}

pub(super) use unknown_delegations::*;
