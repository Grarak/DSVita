use crate::core::emu::{get_mem, get_mmu, get_regs};
use crate::core::memory::mmu;
use crate::core::thread_regs::ThreadRegs;
use crate::core::CpuType;
use crate::jit::assembler::block_asm::BlockAsm;
use crate::jit::assembler::{BlockOperand, BlockReg};
use crate::jit::inst_info::Operand;
use crate::jit::inst_jit_handler::inst_slow_mem_patch;
use crate::jit::inst_mem_handler::{inst_mem_handler, inst_mem_handler_multiple};
use crate::jit::jit_asm::JitAsm;
use crate::jit::op::Op;
use crate::jit::reg::{reg_reserve, Reg, RegReserve};
use crate::jit::{Cond, MemoryAmount, ShiftType};
use crate::IS_DEBUG;

impl<const CPU: CpuType> JitAsm<'_, CPU> {
    fn get_inst_mem_handler_func<const WRITE: bool, const IS_SWP: bool>(signed: bool, amount: MemoryAmount) -> *const () {
        match amount {
            MemoryAmount::Byte => {
                if signed {
                    inst_mem_handler::<CPU, WRITE, { MemoryAmount::Byte }, true, IS_SWP> as *const _
                } else {
                    inst_mem_handler::<CPU, WRITE, { MemoryAmount::Byte }, false, IS_SWP> as *const _
                }
            }
            MemoryAmount::Half => {
                if signed {
                    inst_mem_handler::<CPU, WRITE, { MemoryAmount::Half }, true, IS_SWP> as *const _
                } else {
                    inst_mem_handler::<CPU, WRITE, { MemoryAmount::Half }, false, IS_SWP> as *const _
                }
            }
            MemoryAmount::Word => inst_mem_handler::<CPU, WRITE, { MemoryAmount::Word }, false, IS_SWP> as *const _,
            MemoryAmount::Double => inst_mem_handler::<CPU, WRITE, { MemoryAmount::Double }, false, IS_SWP> as *const _,
        }
    }

    pub fn emit_single_transfer<const WRITE: bool>(&mut self, block_asm: &mut BlockAsm, pre: bool, write_back: bool, amount: MemoryAmount, thumb: bool) {
        if WRITE {
            self.emit_single_transfer_write(block_asm, pre, write_back, amount, thumb)
        } else {
            self.emit_single_transfer_read(block_asm, pre, write_back, amount, thumb)
        }
    }

    fn emit_single_transfer_write(&mut self, block_asm: &mut BlockAsm, pre: bool, write_back: bool, amount: MemoryAmount, thumb: bool) {
        let (op, op0, op1, op2) = {
            let inst_info = self.jit_buf.current_inst();
            let operands = inst_info.operands();
            (inst_info.op, *operands[0].as_reg_no_shift().unwrap(), *operands[1].as_reg_no_shift().unwrap(), &operands[2])
        };

        let post_addr_reg = block_asm.new_reg();
        match op2 {
            Operand::Reg { reg, shift } => {
                if op.mem_transfer_single_sub() {
                    block_asm.sub(post_addr_reg, op1, (*reg, *shift))
                } else {
                    block_asm.add(post_addr_reg, op1, (*reg, *shift))
                }
            }
            Operand::Imm(imm) => {
                if op.mem_transfer_single_sub() {
                    block_asm.sub(post_addr_reg, op1, *imm)
                } else {
                    block_asm.add(post_addr_reg, op1, *imm)
                }
            }
            Operand::None => {}
        }

        let addr_reg = if pre {
            post_addr_reg
        } else {
            let reg = block_asm.new_reg();
            block_asm.mov(reg, op1);
            reg
        };

        let value_reg = block_asm.new_reg();
        block_asm.mov(value_reg, op0);

        if write_back {
            block_asm.mov(op1, post_addr_reg);
        }

        self.emit_write(op0, value_reg, addr_reg, block_asm, op.mem_transfer_single_signed(), amount, thumb, false);

        block_asm.free_reg(value_reg);
        block_asm.free_reg(addr_reg);
        block_asm.free_reg(post_addr_reg);
    }

    fn emit_write(&mut self, op0: Reg, value_reg: BlockReg, addr_reg: BlockReg, block_asm: &mut BlockAsm, signed: bool, amount: MemoryAmount, thumb: bool, is_swp: bool) {
        let slow_write_label = block_asm.new_label();
        let continue_label = block_asm.new_label();

        let cpsr_backup_reg = block_asm.new_reg();
        block_asm.mrs_cpsr(cpsr_backup_reg);

        let mmu_ptr = get_mmu!(self.emu, CPU).get_mmu_write_tcm().as_ptr();

        let fast_write_addr_reg = block_asm.new_reg();
        let fast_mmu_ptr_reg = block_asm.new_reg();
        let fast_mmu_index_reg = block_asm.new_reg();
        let fast_mmu_offset_reg = block_asm.new_reg();

        let size = if amount == MemoryAmount::Double { MemoryAmount::Word.size() } else { amount.size() };
        block_asm.bic(fast_write_addr_reg, addr_reg, 0xF0000000 | (size as u32 - 1));
        block_asm.mov(fast_mmu_index_reg, (fast_write_addr_reg.into(), ShiftType::Lsr, BlockOperand::from(mmu::MMU_PAGE_SHIFT as u32)));
        block_asm.mov(fast_mmu_ptr_reg, mmu_ptr as u32);
        block_asm.transfer_read(
            fast_mmu_offset_reg,
            fast_mmu_ptr_reg,
            (fast_mmu_index_reg.into(), ShiftType::Lsl, BlockOperand::from(2)),
            false,
            MemoryAmount::Word,
        );

        block_asm.cmp(fast_mmu_offset_reg, 0);
        block_asm.branch(slow_write_label, Cond::EQ);

        let shm_ptr = get_mem!(self.emu).shm.as_ptr();

        block_asm.bfc(fast_write_addr_reg, mmu::MMU_PAGE_SHIFT as u8, 32 - mmu::MMU_PAGE_SHIFT as u8);
        block_asm.add(fast_write_addr_reg, fast_mmu_offset_reg, fast_write_addr_reg);
        block_asm.mov(fast_mmu_ptr_reg, shm_ptr as u32);
        block_asm.transfer_write(
            value_reg,
            fast_mmu_ptr_reg,
            fast_write_addr_reg,
            signed,
            if amount == MemoryAmount::Double { MemoryAmount::Word } else { amount },
        );
        if amount == MemoryAmount::Double {
            block_asm.add(fast_write_addr_reg, fast_write_addr_reg, 4);
            block_asm.transfer_write(Reg::from(op0 as u8 + 1), fast_mmu_ptr_reg, fast_write_addr_reg, signed, MemoryAmount::Word);
        }

        block_asm.msr_cpsr(cpsr_backup_reg);

        block_asm.branch(continue_label, Cond::AL);

        block_asm.label_unlikely(slow_write_label);
        block_asm.msr_cpsr(cpsr_backup_reg);
        block_asm.save_context();

        if !thumb && op0 == Reg::PC {
            // When op0 is PC, it's read as PC+12
            // Don't need to restore it, since breakouts only happen when PC was written to
            block_asm.add(value_reg, value_reg, 4);
        }

        let func_addr = if is_swp {
            Self::get_inst_mem_handler_func::<true, true>(signed, amount)
        } else {
            Self::get_inst_mem_handler_func::<true, false>(signed, amount)
        };
        block_asm.call4(
            func_addr,
            addr_reg,
            value_reg,
            self.jit_buf.current_pc | (thumb as u32),
            (self.jit_buf.insts_cycle_counts[self.jit_buf.current_index] as u32) | ((op0 as u32) << 16),
        );
        block_asm.restore_reg(Reg::CPSR);

        block_asm.branch(continue_label, Cond::AL);
        block_asm.label(continue_label);

        block_asm.free_reg(fast_mmu_offset_reg);
        block_asm.free_reg(fast_mmu_index_reg);
        block_asm.free_reg(fast_mmu_ptr_reg);
        block_asm.free_reg(fast_write_addr_reg);
        block_asm.free_reg(cpsr_backup_reg);
    }

    fn emit_single_transfer_read(&mut self, block_asm: &mut BlockAsm, pre: bool, write_back: bool, amount: MemoryAmount, thumb: bool) {
        let (op, op0, op1, op2) = {
            let inst_info = self.jit_buf.current_inst();
            let operands = inst_info.operands();
            (inst_info.op, *operands[0].as_reg_no_shift().unwrap(), *operands[1].as_reg_no_shift().unwrap(), &operands[2])
        };

        let post_addr_reg = block_asm.new_reg();
        match op2 {
            Operand::Reg { reg, shift } => {
                if op.mem_transfer_single_sub() {
                    block_asm.sub(post_addr_reg, op1, (*reg, *shift))
                } else {
                    block_asm.add(post_addr_reg, op1, (*reg, *shift))
                }
            }
            Operand::Imm(imm) => {
                if op.mem_transfer_single_sub() {
                    block_asm.sub(post_addr_reg, op1, *imm)
                } else {
                    block_asm.add(post_addr_reg, op1, *imm)
                }
            }
            Operand::None => {}
        }

        if op == Op::LdrPcT {
            block_asm.bic(post_addr_reg, post_addr_reg, 0x3);
        }

        let addr_reg = if pre {
            post_addr_reg
        } else {
            let reg = block_asm.new_reg();
            block_asm.mov(reg, op1);
            reg
        };

        if write_back && op0 != op1 {
            block_asm.mov(op1, post_addr_reg);
        }

        self.emit_read(op0, addr_reg, block_asm, op.mem_transfer_single_signed(), amount, thumb, false);

        block_asm.free_reg(addr_reg);
        block_asm.free_reg(post_addr_reg);
    }

    fn emit_read(&mut self, op0: Reg, addr_reg: BlockReg, block_asm: &mut BlockAsm, signed: bool, amount: MemoryAmount, thumb: bool, is_swp: bool) {
        let fast_read_value_reg = block_asm.new_reg();
        let fast_read_next_addr_reg = block_asm.new_reg();
        let fast_read_addr_masked_reg = block_asm.new_reg();

        let slow_read_patch_label = block_asm.new_label();
        let slow_read_label = block_asm.new_label();
        let fast_mem_mark_dirty_label = block_asm.new_label();
        let continue_label = block_asm.new_label();

        block_asm.branch(slow_read_label, Cond::NV);
        block_asm.pad_block(slow_read_label, -3);

        let mmu = get_mmu!(self.emu, CPU);
        let base_ptr = mmu.get_base_tcm_ptr();
        let size = if amount == MemoryAmount::Double { MemoryAmount::Word.size() } else { amount.size() };
        block_asm.bic(fast_read_addr_masked_reg, addr_reg, 0xF0000000 | (size as u32 - 1));
        let needs_ror = amount == MemoryAmount::Word || amount == MemoryAmount::Double;
        block_asm.transfer_read(
            if needs_ror { fast_read_value_reg } else { op0.into() },
            fast_read_addr_masked_reg,
            base_ptr as u32,
            signed,
            if amount == MemoryAmount::Double { MemoryAmount::Word } else { amount },
        );
        if needs_ror {
            block_asm.mov(fast_read_addr_masked_reg, (addr_reg.into(), ShiftType::Lsl, BlockOperand::from(3)));
            block_asm.mov(op0, (fast_read_value_reg, ShiftType::Ror, fast_read_addr_masked_reg));
        }
        if amount == MemoryAmount::Double {
            let op0 = Reg::from(op0 as u8 + 1);
            block_asm.add(fast_read_next_addr_reg, addr_reg, 4);
            block_asm.bic(fast_read_addr_masked_reg, fast_read_next_addr_reg, 0xF0000000 | (size as u32 - 1));
            block_asm.transfer_read(fast_read_value_reg, fast_read_addr_masked_reg, base_ptr as u32, false, MemoryAmount::Word);
            block_asm.mov(fast_read_addr_masked_reg, (fast_read_next_addr_reg.into(), ShiftType::Lsl, BlockOperand::from(3)));
            block_asm.mov(op0, (fast_read_value_reg, ShiftType::Ror, fast_read_addr_masked_reg));
        }

        block_asm.mark_reg_dirty(op0, false);
        if amount == MemoryAmount::Double {
            block_asm.mark_reg_dirty(Reg::from(op0 as u8 + 1), false);
        }
        block_asm.branch_fallthrough(fast_mem_mark_dirty_label, Cond::AL);
        block_asm.branch(slow_read_patch_label, Cond::AL);

        block_asm.label_unlikely(slow_read_patch_label);
        let cpsr_backup_reg = block_asm.new_reg();
        block_asm.mrs_cpsr(cpsr_backup_reg);
        if IS_DEBUG {
            block_asm.call1(inst_slow_mem_patch as *const (), self.jit_buf.current_pc);
        } else {
            block_asm.call(inst_slow_mem_patch as *const ());
        }
        block_asm.msr_cpsr(cpsr_backup_reg);
        block_asm.branch(slow_read_label, Cond::AL);

        block_asm.label_unlikely(slow_read_label);

        block_asm.restore_reg(op0);
        if amount == MemoryAmount::Double {
            block_asm.restore_reg(Reg::from(op0 as u8 + 1));
        }

        block_asm.save_context();

        let op0_addr = get_regs!(self.emu, CPU).get_reg(op0) as *const _ as u32;
        let op0_addr_reg = block_asm.new_reg();
        block_asm.mov(op0_addr_reg, op0_addr);

        let func_addr = if is_swp {
            Self::get_inst_mem_handler_func::<false, true>(signed, amount)
        } else {
            Self::get_inst_mem_handler_func::<false, false>(signed, amount)
        };
        block_asm.call4(
            func_addr,
            addr_reg,
            op0_addr_reg,
            self.jit_buf.current_pc | (thumb as u32),
            self.jit_buf.insts_cycle_counts[self.jit_buf.current_index] as u32,
        );
        block_asm.restore_reg(op0);
        if amount == MemoryAmount::Double {
            block_asm.restore_reg(Reg::from(op0 as u8 + 1));
        }
        block_asm.restore_reg(Reg::CPSR);

        block_asm.branch(continue_label, Cond::AL);

        block_asm.label(fast_mem_mark_dirty_label);
        block_asm.mark_reg_dirty(op0, true);
        if amount == MemoryAmount::Double {
            block_asm.mark_reg_dirty(Reg::from(op0 as u8 + 1), true);
        }

        block_asm.label(continue_label);

        block_asm.free_reg(cpsr_backup_reg);
        block_asm.free_reg(fast_read_addr_masked_reg);
        block_asm.free_reg(fast_read_next_addr_reg);
        block_asm.free_reg(fast_read_value_reg);
        block_asm.free_reg(op0_addr_reg);
    }

    pub fn emit_single_write(&mut self, block_asm: &mut BlockAsm) {
        let op = self.jit_buf.current_inst().op;
        self.emit_single_transfer::<true>(block_asm, op.mem_transfer_pre(), op.mem_transfer_write_back(), MemoryAmount::from(op), false);
    }

    pub fn emit_single_read(&mut self, block_asm: &mut BlockAsm) {
        let op = self.jit_buf.current_inst().op;
        self.emit_single_transfer::<false>(block_asm, op.mem_transfer_pre(), op.mem_transfer_write_back(), MemoryAmount::from(op), false);
    }

    pub fn emit_multiple_transfer(&mut self, block_asm: &mut BlockAsm, thumb: bool) {
        let inst_info = self.jit_buf.current_inst();

        let mut rlist = RegReserve::from(inst_info.opcode & if thumb { 0xFF } else { 0xFFFF });
        if inst_info.op == Op::PushLrT {
            rlist += Reg::LR;
        } else if inst_info.op == Op::PopPcT {
            rlist += Reg::PC;
        }

        let mut pre = inst_info.op.mem_transfer_pre();
        let decrement = inst_info.op.mem_transfer_decrement();
        let write_back = inst_info.op.mem_transfer_write_back();

        let op0 = *inst_info.operands()[0].as_reg_no_shift().unwrap();

        let slow_label = block_asm.new_label();
        let slow_patch_label = block_asm.new_label();
        let fast_mem_mark_dirty_label = block_asm.new_label();
        let continue_label = block_asm.new_label();

        let cpsr_backup_reg = block_asm.new_reg();

        let assemble_rlist = |rlist: RegReserve| {
            if rlist.is_empty() {
                return (reg_reserve!(), reg_reserve!(), Vec::new());
            }

            let mut gp_regs = rlist.get_gp_regs();
            let mut free_gp_regs = if gp_regs.is_empty() {
                RegReserve::gp()
            } else {
                let highest_gp_reg = gp_regs.get_highest_reg();
                RegReserve::from(!((1 << (highest_gp_reg as u8 + 1)) - 1)).get_gp_regs()
            };
            let mut non_gp_regs = rlist - gp_regs;

            while free_gp_regs.len() < non_gp_regs.len() {
                let highest_gp_reg = gp_regs.get_highest_reg();
                gp_regs -= highest_gp_reg;
                non_gp_regs += highest_gp_reg;
                free_gp_regs = if gp_regs.is_empty() {
                    RegReserve::gp()
                } else {
                    RegReserve::from(!((1 << (gp_regs.get_highest_reg() as u8 + 1)) - 1)).get_gp_regs()
                };
            }

            let mut non_gp_regs_mappings = Vec::with_capacity(non_gp_regs.len());
            let mut fixed_regs = RegReserve::new();
            while !free_gp_regs.is_empty() && !non_gp_regs.is_empty() {
                let fixed_reg = free_gp_regs.pop().unwrap();
                fixed_regs += fixed_reg;
                non_gp_regs_mappings.push((non_gp_regs.pop().unwrap(), fixed_reg));
            }

            debug_assert!(non_gp_regs.is_empty());

            (gp_regs, fixed_regs, non_gp_regs_mappings)
        };

        const USER_REGS: RegReserve = reg_reserve!(Reg::R8, Reg::R9, Reg::R10, Reg::R11, Reg::R12, Reg::SP, Reg::LR);
        let use_fast_mem = !rlist.is_empty() && (!write_back || !rlist.is_reserved(op0)) && (!inst_info.op.mem_transfer_user() || !rlist.is_reserved(Reg::PC));
        if use_fast_mem {
            let needs_rlist_split = rlist.len() >= (RegReserve::gp() + Reg::LR).len() - 2 && !inst_info.op.mem_transfer_user();

            let mut regs = rlist;

            if inst_info.op.mem_transfer_user() {
                regs -= USER_REGS;
            }

            if needs_rlist_split {
                for _ in 0..regs.len() / 2 {
                    regs.pop_rev();
                }
            }

            let (gp_regs, fixed_regs, non_gp_regs_mappings) = assemble_rlist(regs);

            if inst_info.op.mem_is_write() {
                block_asm.mrs_cpsr(cpsr_backup_reg);

                let mmu_ptr = get_mmu!(self.emu, CPU).get_mmu_write_tcm().as_ptr();

                let base_reg = block_asm.new_reg();
                let base_reg_out = block_asm.new_reg();
                let mmu_index_reg = block_asm.new_reg();
                let mmu_ptr_reg = block_asm.new_reg();
                let mmu_offset_reg = block_asm.new_reg();

                block_asm.bic(base_reg, op0, 0xF0000003);
                block_asm.mov(mmu_index_reg, (base_reg.into(), ShiftType::Lsr, BlockOperand::from(mmu::MMU_PAGE_SHIFT as u32)));
                block_asm.mov(mmu_ptr_reg, mmu_ptr as u32);
                block_asm.transfer_read(mmu_offset_reg, mmu_ptr_reg, (mmu_index_reg.into(), ShiftType::Lsl, BlockOperand::from(2)), false, MemoryAmount::Word);

                block_asm.cmp(mmu_offset_reg, 0);
                block_asm.branch(slow_label, Cond::EQ);

                block_asm.msr_cpsr(cpsr_backup_reg);

                let shm_ptr = get_mem!(self.emu).shm.as_ptr();

                block_asm.bfc(base_reg, mmu::MMU_PAGE_SHIFT as u8, 32 - mmu::MMU_PAGE_SHIFT as u8);
                block_asm.add(base_reg, mmu_offset_reg, base_reg);
                block_asm.add(base_reg, base_reg, shm_ptr as u32);

                if needs_rlist_split || inst_info.op.mem_transfer_user() {
                    if decrement {
                        if inst_info.op.mem_transfer_user() {
                            let user_regs = rlist & USER_REGS;
                            if !user_regs.is_empty() {
                                for (i, user_reg) in user_regs.into_iter().enumerate() {
                                    let fixed_reg = Reg::from(i as u8);
                                    let gp_offset = user_reg as u32 - 8;
                                    block_asm.load_u32(
                                        BlockReg::Fixed(fixed_reg),
                                        block_asm.tmp_regs.thread_regs_addr_reg,
                                        ThreadRegs::get_user_regs_offset() as u32 + gp_offset * 4,
                                    );
                                }
                                let fixed_regs = RegReserve::from((1 << user_regs.len()) - 1);
                                block_asm.guest_transfer_write_multiple(base_reg, base_reg_out, reg_reserve!(), fixed_regs, true, pre, false);
                            }
                        } else {
                            let (gp_regs, fixed_regs, non_gp_regs_mappings) = assemble_rlist(rlist - regs);

                            for (guest_reg, fixed_reg) in non_gp_regs_mappings {
                                block_asm.mov(BlockReg::Fixed(fixed_reg), guest_reg);
                            }
                            block_asm.guest_transfer_write_multiple(base_reg, base_reg_out, gp_regs, fixed_regs, true, pre, false);
                        }

                        if !gp_regs.is_empty() {
                            for (guest_reg, fixed_reg) in non_gp_regs_mappings {
                                block_asm.mov(BlockReg::Fixed(fixed_reg), guest_reg);
                            }
                            block_asm.guest_transfer_write_multiple(base_reg_out, base_reg_out, gp_regs, fixed_regs, write_back, pre, false);
                        }
                    } else {
                        if !gp_regs.is_empty() {
                            for (guest_reg, fixed_reg) in non_gp_regs_mappings {
                                block_asm.mov(BlockReg::Fixed(fixed_reg), guest_reg);
                            }
                            block_asm.guest_transfer_write_multiple(base_reg, base_reg_out, gp_regs, fixed_regs, true, pre, true);
                        }

                        if inst_info.op.mem_transfer_user() {
                            let user_regs = rlist & USER_REGS;
                            if !user_regs.is_empty() {
                                for (i, user_reg) in user_regs.into_iter().enumerate() {
                                    let fixed_reg = Reg::from(i as u8);
                                    let gp_offset = user_reg as u32 - 8;
                                    block_asm.load_u32(
                                        BlockReg::Fixed(fixed_reg),
                                        block_asm.tmp_regs.thread_regs_addr_reg,
                                        ThreadRegs::get_user_regs_offset() as u32 + gp_offset * 4,
                                    );
                                }
                                let fixed_regs = RegReserve::from((1 << user_regs.len()) - 1);
                                block_asm.guest_transfer_write_multiple(
                                    if gp_regs.is_empty() { base_reg } else { base_reg_out },
                                    base_reg_out,
                                    reg_reserve!(),
                                    fixed_regs,
                                    write_back,
                                    pre,
                                    true,
                                );
                            }
                        } else {
                            let (gp_regs, fixed_regs, non_gp_regs_mappings) = assemble_rlist(rlist - regs);

                            for (guest_reg, fixed_reg) in non_gp_regs_mappings {
                                block_asm.mov(BlockReg::Fixed(fixed_reg), guest_reg);
                            }
                            block_asm.guest_transfer_write_multiple(base_reg_out, base_reg_out, gp_regs, fixed_regs, write_back, pre, true);
                        }
                    }
                } else {
                    for (guest_reg, fixed_reg) in non_gp_regs_mappings {
                        block_asm.mov(BlockReg::Fixed(fixed_reg), guest_reg);
                    }
                    block_asm.guest_transfer_write_multiple(base_reg, base_reg_out, gp_regs, fixed_regs, write_back, pre, !decrement);
                }

                if write_back {
                    block_asm.sub(base_reg, base_reg_out, base_reg);
                    block_asm.add(op0, op0, base_reg);
                }

                block_asm.branch(continue_label, Cond::AL);

                block_asm.free_reg(mmu_offset_reg);
                block_asm.free_reg(mmu_ptr_reg);
                block_asm.free_reg(mmu_index_reg);
                block_asm.free_reg(base_reg_out);
                block_asm.free_reg(base_reg);
            } else {
                block_asm.branch(slow_label, Cond::NV);
                block_asm.pad_block(slow_label, -3);

                let base_reg = block_asm.new_reg();
                let base_reg_out = block_asm.new_reg();
                let mmu = get_mmu!(self.emu, CPU);
                let base_ptr = mmu.get_base_tcm_ptr();
                block_asm.bic(base_reg, op0, 0xF0000003);
                block_asm.add(base_reg, base_reg, base_ptr as u32);

                if needs_rlist_split || inst_info.op.mem_transfer_user() {
                    if decrement {
                        if inst_info.op.mem_transfer_user() {
                            let user_regs = rlist & USER_REGS;
                            if !user_regs.is_empty() {
                                let fixed_regs = RegReserve::from((1 << user_regs.len()) - 1);
                                block_asm.guest_transfer_read_multiple(base_reg, base_reg_out, reg_reserve!(), fixed_regs, true, pre, false);
                                for (i, user_reg) in user_regs.into_iter().enumerate() {
                                    let fixed_reg = Reg::from(i as u8);
                                    let gp_offset = user_reg as u32 - 8;
                                    block_asm.store_u32(
                                        BlockReg::Fixed(fixed_reg),
                                        block_asm.tmp_regs.thread_regs_addr_reg,
                                        ThreadRegs::get_user_regs_offset() as u32 + gp_offset * 4,
                                    );
                                }
                            }
                        } else {
                            let (gp_regs, fixed_regs, non_gp_regs_mappings) = assemble_rlist(rlist - regs);

                            block_asm.guest_transfer_read_multiple(base_reg, base_reg_out, gp_regs, fixed_regs, true, pre, false);
                            for (guest_reg, fixed_reg) in non_gp_regs_mappings {
                                block_asm.mov(guest_reg, BlockReg::Fixed(fixed_reg));
                            }
                        }

                        if !gp_regs.is_empty() {
                            block_asm.guest_transfer_read_multiple(base_reg_out, base_reg_out, gp_regs, fixed_regs, write_back, pre, false);
                            for (guest_reg, fixed_reg) in non_gp_regs_mappings {
                                block_asm.mov(guest_reg, BlockReg::Fixed(fixed_reg));
                            }
                        }
                    } else {
                        if !gp_regs.is_empty() {
                            block_asm.guest_transfer_read_multiple(base_reg, base_reg_out, gp_regs, fixed_regs, true, pre, true);
                            for (guest_reg, fixed_reg) in non_gp_regs_mappings {
                                block_asm.mov(guest_reg, BlockReg::Fixed(fixed_reg));
                            }
                        }

                        if inst_info.op.mem_transfer_user() {
                            let user_regs = rlist & USER_REGS;
                            if !user_regs.is_empty() {
                                let fixed_regs = RegReserve::from((1 << user_regs.len()) - 1);
                                block_asm.guest_transfer_read_multiple(
                                    if gp_regs.is_empty() { base_reg } else { base_reg_out },
                                    base_reg_out,
                                    reg_reserve!(),
                                    fixed_regs,
                                    write_back,
                                    pre,
                                    true,
                                );
                                for (i, user_reg) in user_regs.into_iter().enumerate() {
                                    let fixed_reg = Reg::from(i as u8);
                                    let gp_offset = user_reg as u32 - 8;
                                    block_asm.store_u32(
                                        BlockReg::Fixed(fixed_reg),
                                        block_asm.tmp_regs.thread_regs_addr_reg,
                                        ThreadRegs::get_user_regs_offset() as u32 + gp_offset * 4,
                                    );
                                }
                            }
                        } else {
                            let (gp_regs, fixed_regs, non_gp_regs_mappings) = assemble_rlist(rlist - regs);

                            block_asm.guest_transfer_read_multiple(base_reg_out, base_reg_out, gp_regs, fixed_regs, write_back, pre, true);
                            for (guest_reg, fixed_reg) in non_gp_regs_mappings {
                                block_asm.mov(guest_reg, BlockReg::Fixed(fixed_reg));
                            }
                        }
                    }
                } else {
                    block_asm.guest_transfer_read_multiple(base_reg, base_reg_out, gp_regs, fixed_regs, write_back, pre, !decrement);

                    for (guest_reg, fixed_reg) in non_gp_regs_mappings {
                        block_asm.mov(guest_reg, BlockReg::Fixed(fixed_reg));
                    }
                }

                if write_back {
                    block_asm.sub(base_reg, base_reg_out, base_reg);
                    block_asm.add(op0, base_reg, op0);
                }

                if inst_info.op.mem_transfer_user() {
                    for guest_reg in rlist - USER_REGS {
                        block_asm.mark_reg_dirty(guest_reg, false);
                    }
                } else {
                    for guest_reg in rlist {
                        block_asm.mark_reg_dirty(guest_reg, false);
                    }
                }

                block_asm.branch_fallthrough(fast_mem_mark_dirty_label, Cond::AL);
                block_asm.branch(slow_patch_label, Cond::AL);

                block_asm.free_reg(base_reg_out);
                block_asm.free_reg(base_reg);
            }
        }

        if decrement {
            pre = !pre;
        }
        let func_addr: *const () = match (inst_info.op.mem_is_write(), inst_info.op.mem_transfer_user(), pre, write_back, decrement) {
            (false, false, false, false, false) => inst_mem_handler_multiple::<CPU, false, false, false, false, false> as _,
            (true, false, false, false, false) => inst_mem_handler_multiple::<CPU, true, false, false, false, false> as _,
            (false, true, false, false, false) => inst_mem_handler_multiple::<CPU, false, true, false, false, false> as _,
            (true, true, false, false, false) => inst_mem_handler_multiple::<CPU, true, true, false, false, false> as _,
            (false, false, true, false, false) => inst_mem_handler_multiple::<CPU, false, false, true, false, false> as _,
            (true, false, true, false, false) => inst_mem_handler_multiple::<CPU, true, false, true, false, false> as _,
            (false, true, true, false, false) => inst_mem_handler_multiple::<CPU, false, true, true, false, false> as _,
            (true, true, true, false, false) => inst_mem_handler_multiple::<CPU, true, true, true, false, false> as _,
            (false, false, false, true, false) => inst_mem_handler_multiple::<CPU, false, false, false, true, false> as _,
            (true, false, false, true, false) => inst_mem_handler_multiple::<CPU, true, false, false, true, false> as _,
            (false, true, false, true, false) => inst_mem_handler_multiple::<CPU, false, true, false, true, false> as _,
            (true, true, false, true, false) => inst_mem_handler_multiple::<CPU, true, true, false, true, false> as _,
            (false, false, true, true, false) => inst_mem_handler_multiple::<CPU, false, false, true, true, false> as _,
            (true, false, true, true, false) => inst_mem_handler_multiple::<CPU, true, false, true, true, false> as _,
            (false, true, true, true, false) => inst_mem_handler_multiple::<CPU, false, true, true, true, false> as _,
            (true, true, true, true, false) => inst_mem_handler_multiple::<CPU, true, true, true, true, false> as _,
            (false, false, false, false, true) => inst_mem_handler_multiple::<CPU, false, false, false, false, true> as _,
            (true, false, false, false, true) => inst_mem_handler_multiple::<CPU, true, false, false, false, true> as _,
            (false, true, false, false, true) => inst_mem_handler_multiple::<CPU, false, true, false, false, true> as _,
            (true, true, false, false, true) => inst_mem_handler_multiple::<CPU, true, true, false, false, true> as _,
            (false, false, true, false, true) => inst_mem_handler_multiple::<CPU, false, false, true, false, true> as _,
            (true, false, true, false, true) => inst_mem_handler_multiple::<CPU, true, false, true, false, true> as _,
            (false, true, true, false, true) => inst_mem_handler_multiple::<CPU, false, true, true, false, true> as _,
            (true, true, true, false, true) => inst_mem_handler_multiple::<CPU, true, true, true, false, true> as _,
            (false, false, false, true, true) => inst_mem_handler_multiple::<CPU, false, false, false, true, true> as _,
            (true, false, false, true, true) => inst_mem_handler_multiple::<CPU, true, false, false, true, true> as _,
            (false, true, false, true, true) => inst_mem_handler_multiple::<CPU, false, true, false, true, true> as _,
            (true, true, false, true, true) => inst_mem_handler_multiple::<CPU, true, true, false, true, true> as _,
            (false, false, true, true, true) => inst_mem_handler_multiple::<CPU, false, false, true, true, true> as _,
            (true, false, true, true, true) => inst_mem_handler_multiple::<CPU, true, false, true, true, true> as _,
            (false, true, true, true, true) => inst_mem_handler_multiple::<CPU, false, true, true, true, true> as _,
            (true, true, true, true, true) => inst_mem_handler_multiple::<CPU, true, true, true, true, true> as _,
        };

        if use_fast_mem {
            if !inst_info.op.mem_is_write() {
                block_asm.label_unlikely(slow_patch_label);
                block_asm.mrs_cpsr(cpsr_backup_reg);
                if IS_DEBUG {
                    block_asm.call1(inst_slow_mem_patch as *const (), self.jit_buf.current_pc);
                } else {
                    block_asm.call(inst_slow_mem_patch as *const ());
                }
                block_asm.msr_cpsr(cpsr_backup_reg);
                block_asm.branch(slow_label, Cond::AL);
            }

            block_asm.label_unlikely(slow_label);
            if inst_info.op.mem_is_write() {
                block_asm.msr_cpsr(cpsr_backup_reg);
            }
        }
        block_asm.save_context();
        block_asm.call3(
            func_addr,
            rlist.0 | ((op0 as u32) << 16),
            self.jit_buf.current_pc | (thumb as u32),
            self.jit_buf.insts_cycle_counts[self.jit_buf.current_index] as u32,
        );

        let mut restore_regs = RegReserve::new();
        if write_back {
            restore_regs += op0;
        }
        if !inst_info.op.mem_is_write() {
            restore_regs += rlist;
        }
        for reg in restore_regs {
            block_asm.restore_reg(reg);
        }
        block_asm.restore_reg(Reg::CPSR);

        if use_fast_mem {
            block_asm.branch(continue_label, Cond::AL);

            if !inst_info.op.mem_is_write() {
                block_asm.label(fast_mem_mark_dirty_label);
                if inst_info.op.mem_transfer_user() {
                    for guest_reg in rlist - USER_REGS {
                        block_asm.mark_reg_dirty(guest_reg, true);
                    }
                } else {
                    for guest_reg in rlist {
                        block_asm.mark_reg_dirty(guest_reg, true);
                    }
                }
            }

            block_asm.label(continue_label);
        }

        block_asm.free_reg(cpsr_backup_reg);
    }

    pub fn emit_swp(&mut self, block_asm: &mut BlockAsm) {
        let inst_info = self.jit_buf.current_inst();
        let operands = inst_info.operands();
        let op = inst_info.op;

        let op0 = *operands[0].as_reg_no_shift().unwrap();
        let op1 = *operands[1].as_reg_no_shift().unwrap();
        let op2 = *operands[2].as_reg_no_shift().unwrap();

        let amount = match op {
            Op::Swp => MemoryAmount::Word,
            Op::Swpb => MemoryAmount::Byte,
            _ => unreachable!(),
        };

        let read_reg = block_asm.new_reg();
        let value_reg = block_asm.new_reg();
        let addr_reg = block_asm.new_reg();
        block_asm.mov(value_reg, op1);
        block_asm.mov(addr_reg, op2);

        self.emit_read(op0, addr_reg, block_asm, false, amount, false, true);
        block_asm.mov(read_reg, op0);
        block_asm.mov(op1, value_reg);
        self.emit_write(op1, value_reg, addr_reg, block_asm, false, amount, false, true);
        block_asm.mov(op0, read_reg);

        block_asm.free_reg(addr_reg);
        block_asm.free_reg(value_reg);
        block_asm.free_reg(read_reg);
    }
}
