use crate::core::CpuType;
use crate::jit::assembler::block_asm::BlockAsm;
use crate::jit::assembler::vixl::vixl::Operand;
use crate::jit::assembler::vixl::{
    MacroAssembler, MasmAdc4, MasmAdcs4, MasmAdd4, MasmAdds4, MasmAnd4, MasmAnds4, MasmBic4, MasmBics4, MasmCmn3, MasmCmp3, MasmEor4, MasmEors4, MasmMov3, MasmMovs3, MasmMvn3, MasmOrr4, MasmOrrs4,
    MasmRsb4, MasmRsbs4, MasmRsc4, MasmRscs4, MasmSbc4, MasmSbcs4, MasmSub4, MasmSubs4, MasmTeq3, MasmTst2, MasmTst3,
};
use crate::jit::jit_asm::JitAsm;
use crate::jit::op::Op;
use crate::jit::reg::Reg;
use crate::jit::Cond;

impl<const CPU: CpuType> JitAsm<'_, CPU> {
    pub fn emit_alu(&mut self, block_asm: &mut BlockAsm) {
        let inst_info = self.jit_buf.current_inst();
        let operands = inst_info.operands();
        match inst_info.operands().len() {
            2 => {
                let func = match inst_info.op {
                    Op::Tst => <MacroAssembler as MasmTst3<Cond, Reg, &Operand>>::tst3,
                    Op::Teq => <MacroAssembler as MasmTeq3<_, _, _>>::teq3,
                    Op::Cmp => <MacroAssembler as MasmCmp3<_, _, _>>::cmp3,
                    Op::Cmn => <MacroAssembler as MasmCmn3<_, _, _>>::cmn3,
                    Op::Mov => <MacroAssembler as MasmMov3<_, _, _>>::mov3,
                    Op::Mvn => <MacroAssembler as MasmMvn3<_, _, _>>::mvn3,
                    Op::Movs => <MacroAssembler as MasmMovs3<_, _, _>>::movs3,
                    Op::Mvns => <MacroAssembler as MasmMvn3<_, _, _>>::mvn3,
                    _ => unreachable!(),
                };
                let op0 = operands[0].as_reg_no_shift().unwrap();
                let op0_mapped = block_asm.map_guest_reg(op0, inst_info.src_regs.is_reserved(op0));
                
                func(block_asm, inst_info.cond, );
            }
            3 => {
                let func = match inst_info.op {
                    Op::And => <MacroAssembler as MasmAnd4<Cond, Reg, Reg, &Operand>>::and4,
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
            }
            _ => unreachable!(),
        }
    }
}
