use crate::core::CpuType::ARM9;
use crate::jit::assembler::block_asm::BlockAsm;
use crate::jit::jit_asm::JitAsm;
use crate::jit::op::Op;
use crate::jit::reg::Reg;
use crate::jit::Cond;
use vixl::{
    MacroAssembler, MasmAdc4, MasmAdcs4, MasmAdd4, MasmAdds4, MasmAnd4, MasmAnds4, MasmBic4, MasmBics4, MasmClz3, MasmCmn3, MasmCmp3, MasmEor4, MasmEors4, MasmMla5, MasmMlas5, MasmMov3, MasmMovs3,
    MasmMul4, MasmMuls4, MasmMvn3, MasmMvns3, MasmOrr4, MasmOrrs4, MasmQadd4, MasmQdadd4, MasmQdsub4, MasmQsub4, MasmRsb4, MasmRsbs4, MasmRsc4, MasmRscs4, MasmSbc4, MasmSbcs4, MasmSmlabb5,
    MasmSmlabt5, MasmSmlal5, MasmSmlalbb5, MasmSmlalbt5, MasmSmlals5, MasmSmlaltb5, MasmSmlaltt5, MasmSmlatb5, MasmSmlatt5, MasmSmlawb5, MasmSmlawt5, MasmSmulbb4, MasmSmulbt4, MasmSmull5,
    MasmSmulls5, MasmSmultb4, MasmSmultt4, MasmSmulwb4, MasmSmulwt4, MasmSub4, MasmSubs4, MasmTeq3, MasmTst3, MasmUmlal5, MasmUmlals5, MasmUmull5, MasmUmulls5,
};

impl JitAsm<'_> {
    pub fn emit_alu(&mut self, inst_index: usize, block_asm: &mut BlockAsm) {
        let inst = &self.jit_buf.insts[inst_index];
        let mut cond = inst.cond;
        if inst.out_regs.is_reserved(Reg::PC) {
            cond = Cond::AL;
        }

        let operands = inst.operands();
        match operands.len() {
            2 => {
                let func = match inst.op {
                    Op::Tst => <MacroAssembler as MasmTst3<Cond, Reg, &vixl::Operand>>::tst3,
                    Op::Teq => <MacroAssembler as MasmTeq3<_, _, _>>::teq3,
                    Op::Cmp => <MacroAssembler as MasmCmp3<_, _, _>>::cmp3,
                    Op::Cmn => <MacroAssembler as MasmCmn3<_, _, _>>::cmn3,
                    Op::Mov => <MacroAssembler as MasmMov3<_, _, _>>::mov3,
                    Op::Mvn => <MacroAssembler as MasmMvn3<_, _, _>>::mvn3,
                    Op::Movs => <MacroAssembler as MasmMovs3<_, _, _>>::movs3,
                    Op::Mvns => <MacroAssembler as MasmMvns3<_, _, _>>::mvns3,
                    _ => unreachable!(),
                };

                let op0_mapped = block_asm.get_guest_map(operands[0].as_reg_no_shift().unwrap());
                let op1_operand = block_asm.get_guest_operand_map(&operands[1]);

                func(block_asm, cond, op0_mapped, &op1_operand);
            }
            3 => {
                let func = match inst.op {
                    Op::And => <MacroAssembler as MasmAnd4<Cond, Reg, Reg, &vixl::Operand>>::and4,
                    Op::Eor => <MacroAssembler as MasmEor4<_, _, _, _>>::eor4,
                    Op::Sub => <MacroAssembler as MasmSub4<_, _, _, _>>::sub4,
                    Op::Rsb => <MacroAssembler as MasmRsb4<_, _, _, _>>::rsb4,
                    Op::Add => <MacroAssembler as MasmAdd4<_, _, _, _>>::add4,
                    Op::Adc => <MacroAssembler as MasmAdc4<_, _, _, _>>::adc4,
                    Op::Sbc => <MacroAssembler as MasmSbc4<_, _, _, _>>::sbc4,
                    Op::Rsc => <MacroAssembler as MasmRsc4<_, _, _, _>>::rsc4,
                    Op::Orr => <MacroAssembler as MasmOrr4<_, _, _, _>>::orr4,
                    Op::Bic => <MacroAssembler as MasmBic4<_, _, _, _>>::bic4,
                    Op::Ands => <MacroAssembler as MasmAnds4<_, _, _, _>>::ands4,
                    Op::Eors => <MacroAssembler as MasmEors4<_, _, _, _>>::eors4,
                    Op::Subs => <MacroAssembler as MasmSubs4<_, _, _, _>>::subs4,
                    Op::Rsbs => <MacroAssembler as MasmRsbs4<_, _, _, _>>::rsbs4,
                    Op::Adds => <MacroAssembler as MasmAdds4<_, _, _, _>>::adds4,
                    Op::Adcs => <MacroAssembler as MasmAdcs4<_, _, _, _>>::adcs4,
                    Op::Sbcs => <MacroAssembler as MasmSbcs4<_, _, _, _>>::sbcs4,
                    Op::Rscs => <MacroAssembler as MasmRscs4<_, _, _, _>>::rscs4,
                    Op::Orrs => <MacroAssembler as MasmOrrs4<_, _, _, _>>::orrs4,
                    Op::Bics => <MacroAssembler as MasmBics4<_, _, _, _>>::bics4,
                    _ => unreachable!(),
                };

                let op0_mapped = block_asm.get_guest_map(operands[0].as_reg_no_shift().unwrap());
                let op1_mapped = block_asm.get_guest_map(operands[1].as_reg_no_shift().unwrap());
                let op2_operand = block_asm.get_guest_operand_map(&operands[2]);

                func(block_asm, cond, op0_mapped, op1_mapped, &op2_operand);
            }
            _ => unreachable!(),
        }
    }

    pub fn emit_mul(&mut self, inst_index: usize, block_asm: &mut BlockAsm) {
        let inst = &self.jit_buf.insts[inst_index];

        let operands = inst.operands();
        let op0_mapped = block_asm.get_guest_map(operands[0].as_reg_no_shift().unwrap());
        let op1_mapped = block_asm.get_guest_map(operands[1].as_reg_no_shift().unwrap());
        let op2_mapped = block_asm.get_guest_map(operands[2].as_reg_no_shift().unwrap());

        match inst.operands().len() {
            3 => {
                let func = match inst.op {
                    Op::Mul => <MacroAssembler as MasmMul4<Cond, Reg, Reg, Reg>>::mul4,
                    Op::Muls => <MacroAssembler as MasmMuls4<_, _, _, _>>::muls4,
                    Op::Smulwb => <MacroAssembler as MasmSmulwb4<_, _, _, _>>::smulwb4,
                    Op::Smulwt => <MacroAssembler as MasmSmulwt4<_, _, _, _>>::smulwt4,
                    Op::Smulbb => <MacroAssembler as MasmSmulbb4<_, _, _, _>>::smulbb4,
                    Op::Smulbt => <MacroAssembler as MasmSmulbt4<_, _, _, _>>::smulbt4,
                    Op::Smultb => <MacroAssembler as MasmSmultb4<_, _, _, _>>::smultb4,
                    Op::Smultt => <MacroAssembler as MasmSmultt4<_, _, _, _>>::smultt4,
                    _ => unreachable!(),
                };
                func(block_asm, inst.cond, op0_mapped, op1_mapped, op2_mapped);
            }
            4 => {
                let op3_mapped = block_asm.get_guest_map(operands[3].as_reg_no_shift().unwrap());
                let func = match inst.op {
                    Op::Mla => <MacroAssembler as MasmMla5<Cond, Reg, Reg, Reg, Reg>>::mla5,
                    Op::Mlas => <MacroAssembler as MasmMlas5<_, _, _, _, _>>::mlas5,
                    Op::Smull => <MacroAssembler as MasmSmull5<_, _, _, _, _>>::smull5,
                    Op::Smulls => <MacroAssembler as MasmSmulls5<_, _, _, _, _>>::smulls5,
                    Op::Smlal => <MacroAssembler as MasmSmlal5<_, _, _, _, _>>::smlal5,
                    Op::Smlals => <MacroAssembler as MasmSmlals5<_, _, _, _, _>>::smlals5,
                    Op::Smlalbb => <MacroAssembler as MasmSmlalbb5<_, _, _, _, _>>::smlalbb5,
                    Op::Smlalbt => <MacroAssembler as MasmSmlalbt5<_, _, _, _, _>>::smlalbt5,
                    Op::Smlaltb => <MacroAssembler as MasmSmlaltb5<_, _, _, _, _>>::smlaltb5,
                    Op::Smlaltt => <MacroAssembler as MasmSmlaltt5<_, _, _, _, _>>::smlaltt5,
                    Op::Smlabb => <MacroAssembler as MasmSmlabb5<_, _, _, _, _>>::smlabb5,
                    Op::Smlabt => <MacroAssembler as MasmSmlabt5<_, _, _, _, _>>::smlabt5,
                    Op::Smlatb => <MacroAssembler as MasmSmlatb5<_, _, _, _, _>>::smlatb5,
                    Op::Smlatt => <MacroAssembler as MasmSmlatt5<_, _, _, _, _>>::smlatt5,
                    Op::Smlawb => <MacroAssembler as MasmSmlawb5<_, _, _, _, _>>::smlawb5,
                    Op::Smlawt => <MacroAssembler as MasmSmlawt5<_, _, _, _, _>>::smlawt5,
                    Op::Umull => <MacroAssembler as MasmUmull5<_, _, _, _, _>>::umull5,
                    Op::Umulls => <MacroAssembler as MasmUmulls5<_, _, _, _, _>>::umulls5,
                    Op::Umlal => <MacroAssembler as MasmUmlal5<_, _, _, _, _>>::umlal5,
                    Op::Umlals => <MacroAssembler as MasmUmlals5<_, _, _, _, _>>::umlals5,
                    _ => unreachable!(),
                };
                func(block_asm, inst.cond, op0_mapped, op1_mapped, op2_mapped, op3_mapped);
            }
            _ => unreachable!(),
        }
    }

    pub fn emit_clz(&mut self, inst_index: usize, block_asm: &mut BlockAsm) {
        if self.cpu != ARM9 {
            return;
        }

        let inst = &self.jit_buf.insts[inst_index];

        let operands = inst.operands();
        let op0_mapped = block_asm.get_guest_map(operands[0].as_reg_no_shift().unwrap());
        let op1_mapped = block_asm.get_guest_map(operands[1].as_reg_no_shift().unwrap());

        block_asm.clz3(inst.cond, op0_mapped, op1_mapped);
    }

    pub fn emit_q_op(&mut self, inst_index: usize, block_asm: &mut BlockAsm) {
        if self.cpu != ARM9 {
            return;
        }

        let inst = &self.jit_buf.insts[inst_index];

        let operands = inst.operands();
        let op0_mapped = block_asm.get_guest_map(operands[0].as_reg_no_shift().unwrap());
        let op1_mapped = block_asm.get_guest_map(operands[1].as_reg_no_shift().unwrap());
        let op2_mapped = block_asm.get_guest_map(operands[2].as_reg_no_shift().unwrap());

        let func = match inst.op {
            Op::Qadd => <MacroAssembler as MasmQadd4<Cond, Reg, Reg, Reg>>::qadd4,
            Op::Qsub => <MacroAssembler as MasmQsub4<_, _, _, _>>::qsub4,
            Op::Qdadd => <MacroAssembler as MasmQdadd4<_, _, _, _>>::qdadd4,
            Op::Qdsub => <MacroAssembler as MasmQdsub4<_, _, _, _>>::qdsub4,
            _ => unreachable!(),
        };

        func(block_asm, inst.cond, op0_mapped, op1_mapped, op2_mapped);
    }
}
