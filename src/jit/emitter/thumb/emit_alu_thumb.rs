use crate::jit::assembler::block_asm::BlockAsm;
use crate::jit::inst_info::Operand;
use crate::jit::jit_asm::JitAsm;
use crate::jit::op::Op;
use crate::jit::reg::Reg;
use vixl::{
    MacroAssembler, MasmAdcs3, MasmAdd3, MasmAdds3, MasmAnds3, MasmAsrs3, MasmBics3, MasmCmp2, MasmEors3, MasmLsls3, MasmLsrs3, MasmMov2, MasmMovs2, MasmMuls3, MasmMvns2, MasmOrrs3, MasmRors3,
    MasmRsbs3, MasmSbcs3, MasmSub3, MasmSubs3, MasmTst2,
};

impl JitAsm<'_> {
    pub fn emit_alu_thumb(&mut self, inst_index: usize, block_asm: &mut BlockAsm) {
        let inst = &self.jit_buf.insts[inst_index];

        let operands = inst.operands();
        match operands.len() {
            2 => match inst.op {
                Op::MulT => {
                    let op0_mapped = block_asm.get_guest_map(operands[0].as_reg_no_shift().unwrap());
                    let op1_mapped = block_asm.get_guest_map(operands[1].as_reg_no_shift().unwrap());
                    block_asm.muls3(op0_mapped, op1_mapped, op0_mapped);
                }
                _ => {
                    let func = match inst.op {
                        Op::CmpT | Op::CmpHT => <MacroAssembler as MasmCmp2<Reg, &vixl::Operand>>::cmp2,
                        Op::MovT => <MacroAssembler as MasmMovs2<_, _>>::movs2,
                        Op::TstT => <MacroAssembler as MasmTst2<_, _>>::tst2,
                        Op::MvnT => <MacroAssembler as MasmMvns2<_, _>>::mvns2,
                        Op::MovHT => <MacroAssembler as MasmMov2<_, _>>::mov2,
                        _ => todo!("{inst:?}"),
                    };

                    let op0_mapped = block_asm.get_guest_map(operands[0].as_reg_no_shift().unwrap());
                    let op1_operand = block_asm.get_guest_operand_map(&operands[1]);

                    func(block_asm, op0_mapped, &op1_operand);
                }
            },
            3 => {
                let func = match inst.op {
                    Op::AddT => <MacroAssembler as MasmAdds3<Reg, Reg, &vixl::Operand>>::adds3,
                    Op::SubT => <MacroAssembler as MasmSubs3<_, _, _>>::subs3,
                    Op::LslT => <MacroAssembler as MasmLsls3<_, _, _>>::lsls3,
                    Op::LsrT => <MacroAssembler as MasmLsrs3<_, _, _>>::lsrs3,
                    Op::AsrT => <MacroAssembler as MasmAsrs3<_, _, _>>::asrs3,
                    Op::RorT => <MacroAssembler as MasmRors3<_, _, _>>::rors3,
                    Op::AndT => <MacroAssembler as MasmAnds3<_, _, _>>::ands3,
                    Op::EorT => <MacroAssembler as MasmEors3<_, _, _>>::eors3,
                    Op::AdcT => <MacroAssembler as MasmAdcs3<_, _, _>>::adcs3,
                    Op::SbcT => <MacroAssembler as MasmSbcs3<_, _, _>>::sbcs3,
                    Op::OrrT => <MacroAssembler as MasmOrrs3<_, _, _>>::orrs3,
                    Op::BicT => <MacroAssembler as MasmBics3<_, _, _>>::bics3,
                    Op::NegT => <MacroAssembler as MasmRsbs3<_, _, _>>::rsbs3,
                    Op::AddHT => <MacroAssembler as MasmAdd3<_, _, _>>::add3,
                    Op::AddPcT | Op::AddSpT => <MacroAssembler as MasmAdd3<_, _, _>>::add3,
                    Op::AddSpImmT => {
                        let sub = inst.opcode & (1 << 7) != 0;
                        if sub {
                            <MacroAssembler as MasmSub3<_, _, _>>::sub3
                        } else {
                            <MacroAssembler as MasmAdd3<_, _, _>>::add3
                        }
                    }
                    _ => todo!("{inst:?}"),
                };

                let op0_mapped = block_asm.get_guest_map(operands[0].as_reg_no_shift().unwrap());
                let op1_mapped = block_asm.get_guest_map(operands[1].as_reg_no_shift().unwrap());
                let mut op2_operand = block_asm.get_guest_operand_map(&operands[2]);

                match inst.op {
                    Op::LslT => {
                        if let Operand::Imm(imm) = operands[2] {
                            if imm == 0 {
                                block_asm.movs2(op0_mapped, &op1_mapped.into());
                                return;
                            }
                        }
                    }
                    Op::LsrT | Op::AsrT => {
                        if let Operand::Imm(imm) = operands[2] {
                            if imm == 0 {
                                op2_operand.imm_ = 32;
                            }
                        }
                    }
                    _ => {}
                }

                func(block_asm, op0_mapped, op1_mapped, &op2_operand);
            }
            _ => unreachable!(),
        }
    }
}
