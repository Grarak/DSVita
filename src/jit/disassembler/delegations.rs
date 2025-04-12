mod alu_delegations {
    use crate::jit::disassembler::alu_instructions::*;
    use crate::jit::inst_info::InstInfo;
    use crate::jit::{Op, ShiftType::*};
    use paste::paste;

    macro_rules! generate_variation {
        ($name:ident, $cpsr:expr, $variation:ident, alu3_imm_shift, $shift_type:expr) => {
            paste! {
                #[inline]
                pub fn [<$name _ $variation>](opcode: u32, op: Op) -> InstInfo {
                    alu3_imm_shift::<{ $shift_type }, $cpsr>(opcode, op, imm_shift(opcode))
                }
            }
        };

        ($name:ident, $cpsr:expr, $variation:ident, alu3_reg_shift, $shift_type:expr) => {
            paste! {
                #[inline]
                pub fn [<$name _ $variation>](opcode: u32, op: Op) -> InstInfo {
                    alu3_reg_shift::<{ $shift_type }, $cpsr>(opcode, op, reg_shift(opcode))
                }
            }
        };

        ($name:ident, $cpsr:expr, $variation:ident, alu3_imm) => {
            paste! {
                #[inline]
                pub fn [<$name _ $variation>](opcode: u32, op: Op) -> InstInfo {
                    alu3_imm::<$cpsr>(opcode, op, imm(opcode))
                }
            }
        };

        ($name:ident, $variation:ident, alu2_op1_imm_shift, $shift_type:expr) => {
            paste! {
                #[inline]
                pub fn [<$name _ $variation>](opcode: u32, op: Op) -> InstInfo {
                    alu2_op1_imm_shift::<{ $shift_type }>(opcode, op, imm_shift(opcode))
                }
            }
        };

        ($name:ident, $variation:ident, alu2_op1_reg_shift, $shift_type:expr) => {
            paste! {
                #[inline]
                pub fn [<$name _ $variation>](opcode: u32, op: Op) -> InstInfo {
                    alu2_op1_reg_shift::<{ $shift_type }>(opcode, op, reg_shift(opcode))
                }
            }
        };

        ($name:ident, $variation:ident, alu2_op1_imm) => {
            paste! {
                #[inline]
                pub fn [<$name _ $variation>](opcode: u32, op: Op) -> InstInfo {
                    alu2_op1_imm(opcode, op, imm(opcode))
                }
            }
        };

        ($name:ident, $cpsr:expr, $variation:ident, alu2_op0_imm_shift, $shift_type:expr) => {
            paste! {
                #[inline]
                pub fn [<$name _ $variation>](opcode: u32, op: Op) -> InstInfo {
                    alu2_op0_imm_shift::<{ $shift_type }, $cpsr>(opcode, op, imm_shift(opcode))
                }
            }
        };

        ($name:ident, $cpsr:expr, $variation:ident, alu2_op0_reg_shift, $shift_type:expr) => {
            paste! {
                #[inline]
                pub fn [<$name _ $variation>](opcode: u32, op: Op) -> InstInfo {
                    alu2_op0_reg_shift::<{ $shift_type }, $cpsr>(opcode, op, reg_shift(opcode))
                }
            }
        };

        ($name:ident, $cpsr:expr, $variation:ident, alu2_op0_imm) => {
            paste! {
                #[inline]
                pub fn [<$name _ $variation>](opcode: u32, op: Op) -> InstInfo {
                    alu2_op0_imm::<$cpsr>(opcode, op, imm(opcode))
                }
            }
        };
    }

    macro_rules! generate_variations {
        ($name:ident, $([$($args:tt)*]),+) => {
            $(
                generate_variation!($name, $($args)*);
            )*
        };

        ($name:ident, $cpsr:expr, $([$($args:tt)*]),+) => {
            $(
                generate_variation!($name, $cpsr, $($args)*);
            )*
        };
    }

    macro_rules! generate_alu3 {
        ($name:ident, $cpsr:expr) => {
            generate_variations!(
                $name,
                $cpsr,
                [lli, alu3_imm_shift, Lsl],
                [llr, alu3_reg_shift, Lsl],
                [lri, alu3_imm_shift, Lsr],
                [lrr, alu3_reg_shift, Lsr],
                [ari, alu3_imm_shift, Asr],
                [arr, alu3_reg_shift, Asr],
                [rri, alu3_imm_shift, Ror],
                [rrr, alu3_reg_shift, Ror],
                [imm, alu3_imm]
            );
        };
    }

    macro_rules! generate_alu2_op1 {
        ($name:ident) => {
            generate_variations!(
                $name,
                [lli, alu2_op1_imm_shift, Lsl],
                [llr, alu2_op1_reg_shift, Lsl],
                [lri, alu2_op1_imm_shift, Lsr],
                [lrr, alu2_op1_reg_shift, Lsr],
                [ari, alu2_op1_imm_shift, Asr],
                [arr, alu2_op1_reg_shift, Asr],
                [rri, alu2_op1_imm_shift, Ror],
                [rrr, alu2_op1_reg_shift, Ror],
                [imm, alu2_op1_imm]
            );
        };
    }

    macro_rules! generate_alu2_op0 {
        ($name:ident, $cpsr:expr) => {
            generate_variations!(
                $name,
                $cpsr,
                [lli, alu2_op0_imm_shift, Lsl],
                [llr, alu2_op0_reg_shift, Lsl],
                [lri, alu2_op0_imm_shift, Lsr],
                [lrr, alu2_op0_reg_shift, Lsr],
                [ari, alu2_op0_imm_shift, Asr],
                [arr, alu2_op0_reg_shift, Asr],
                [rri, alu2_op0_imm_shift, Ror],
                [rrr, alu2_op0_reg_shift, Ror],
                [imm, alu2_op0_imm]
            );
        };
    }

    generate_alu3!(and, false);
    generate_alu3!(ands, true);
    generate_alu3!(eor, false);
    generate_alu3!(eors, true);
    generate_alu3!(sub, false);
    generate_alu3!(subs, true);
    generate_alu3!(rsb, false);
    generate_alu3!(rsbs, true);
    generate_alu3!(add, false);
    generate_alu3!(adds, true);
    generate_alu3!(adc, false);
    generate_alu3!(adcs, true);
    generate_alu3!(sbc, false);
    generate_alu3!(sbcs, true);
    generate_alu3!(rsc, false);
    generate_alu3!(rscs, true);
    generate_alu3!(orr, false);
    generate_alu3!(orrs, true);
    generate_alu3!(bic, false);
    generate_alu3!(bics, true);

    generate_alu2_op1!(tst);
    generate_alu2_op1!(teq);
    generate_alu2_op1!(cmp);
    generate_alu2_op1!(cmn);

    generate_alu2_op0!(mov, false);
    generate_alu2_op0!(movs, true);
    generate_alu2_op0!(mvn, false);
    generate_alu2_op0!(mvns, true);
}

pub(super) use alu_delegations::*;

mod transfer_delegations {
    use crate::jit::disassembler::transfer_instructions::*;
    use crate::jit::inst_info::InstInfo;
    use crate::jit::{Op, ShiftType::*};
    use paste::paste;

    macro_rules! generate_variation {
        ($name:ident, $write:expr, $write_back:expr, $variation:ident, $processor:ident, mem_transfer_imm) => {
            paste! {
                #[inline]
                pub fn [<$name _ $variation>](opcode: u32, op: Op) -> InstInfo {
                    mem_transfer_imm::<$write, $write_back>(opcode, op, $processor(opcode))
                }
            }
        };

        ($name:ident, $write:expr, $write_back:expr, $variation:ident, mem_transfer_reg) => {
            paste! {
                #[inline]
                pub fn [<$name _ $variation>](opcode: u32, op: Op) -> InstInfo {
                    mem_transfer_reg::<$write, $write_back>(opcode, op, reg(opcode))
                }
            }
        };

        ($name:ident, $write:expr, $write_back:expr, $variation:ident, mem_transfer_reg_shift, $shift_type:expr) => {
            paste! {
                #[inline]
                pub fn [<$name _ $variation>](opcode: u32, op: Op) -> InstInfo {
                    mem_transfer_reg_shift::<$write, $write_back, { $shift_type }>(opcode, op, reg_imm_shift(opcode))
                }
            }
        };
    }

    macro_rules! generate_variations {
        ($name:ident, $write:expr, $([$variation:ident, $write_back:expr, $($args:tt)*]),+) => {
            $(
                generate_variation!($name, $write, $write_back, $variation, $($args)*);
            )*
        };
    }

    macro_rules! generate_op_half {
        ($name:ident, $write:expr) => {
            generate_variations!(
                $name,
                $write,
                [ofim, false, imm_h, mem_transfer_imm],
                [ofip, false, imm_h, mem_transfer_imm],
                [prim, true, imm_h, mem_transfer_imm],
                [prip, true, imm_h, mem_transfer_imm],
                [ptim, true, imm_h, mem_transfer_imm],
                [ptip, true, imm_h, mem_transfer_imm],
                [ofrm, false, mem_transfer_reg],
                [ofrp, false, mem_transfer_reg],
                [prrm, true, mem_transfer_reg],
                [prrp, true, mem_transfer_reg],
                [ptrm, true, mem_transfer_reg],
                [ptrp, true, mem_transfer_reg]
            );
        };
    }

    macro_rules! generate_op_full {
        ($name:ident, $write:expr) => {
            generate_variations!(
                $name,
                $write,
                [ofim, false, imm, mem_transfer_imm],
                [ofip, false, imm, mem_transfer_imm],
                [prim, true, imm, mem_transfer_imm],
                [prip, true, imm, mem_transfer_imm],
                [ptim, true, imm, mem_transfer_imm],
                [ptip, true, imm, mem_transfer_imm],
                [ofrmll, false, mem_transfer_reg_shift, Lsl],
                [ofrmlr, false, mem_transfer_reg_shift, Lsr],
                [ofrmar, false, mem_transfer_reg_shift, Asr],
                [ofrmrr, false, mem_transfer_reg_shift, Ror],
                [ofrpll, false, mem_transfer_reg_shift, Lsl],
                [ofrplr, false, mem_transfer_reg_shift, Lsr],
                [ofrpar, false, mem_transfer_reg_shift, Asr],
                [ofrprr, false, mem_transfer_reg_shift, Ror],
                [prrmll, true, mem_transfer_reg_shift, Lsl],
                [prrmlr, true, mem_transfer_reg_shift, Lsr],
                [prrmar, true, mem_transfer_reg_shift, Asr],
                [prrmrr, true, mem_transfer_reg_shift, Ror],
                [prrpll, true, mem_transfer_reg_shift, Lsl],
                [prrplr, true, mem_transfer_reg_shift, Lsr],
                [prrpar, true, mem_transfer_reg_shift, Asr],
                [prrprr, true, mem_transfer_reg_shift, Ror],
                [ptrmll, true, mem_transfer_reg_shift, Lsl],
                [ptrmlr, true, mem_transfer_reg_shift, Lsr],
                [ptrmar, true, mem_transfer_reg_shift, Asr],
                [ptrmrr, true, mem_transfer_reg_shift, Ror],
                [ptrpll, true, mem_transfer_reg_shift, Lsl],
                [ptrplr, true, mem_transfer_reg_shift, Lsr],
                [ptrpar, true, mem_transfer_reg_shift, Asr],
                [ptrprr, true, mem_transfer_reg_shift, Ror]
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
        InstInfo::new(opcode, op, Operands::new_empty(), reg_reserve!(), reg_reserve!(), 1)
    }
}

pub(super) use unknown_delegations::*;
