use crate::jit::assembler::block_asm::{BlockAsm, BLOCK_LOG};
use crate::jit::assembler::block_inst::{BlockAluOp, BlockAluSetCond, BlockSystemRegOp, BlockTransferOp};
use crate::jit::assembler::block_inst_list::{BlockInstList, BlockInstListEntry};
use crate::jit::assembler::block_reg_set::BlockRegSet;
use crate::jit::assembler::{BlockAsmBuf, BlockInstKind};
use crate::jit::reg::{Reg, RegReserve};
use crate::jit::{MemoryAmount, ShiftType};
use crate::IS_DEBUG;
use std::fmt::{Debug, Formatter};

pub struct BasicBlock {
    block_asm_buf_ptr: *const BlockAsmBuf,

    pub block_entry_start: *mut BlockInstListEntry,
    pub block_entry_end: *mut BlockInstListEntry,

    pub guest_regs_resolved: bool,
    pub guest_regs_input_dirty: RegReserve,
    pub guest_regs_output_dirty: RegReserve,

    pub io_resolved: bool,
    pub regs_live_ranges: Vec<BlockRegSet>,

    pub enter_blocks: Vec<usize>,
    pub exit_blocks: Vec<usize>,

    pub insts_link: BlockInstList,

    pub start_pc: u32,
}

impl BasicBlock {
    pub fn new(asm: &mut BlockAsm, block_entry_start: *mut BlockInstListEntry, block_entry_end: *mut BlockInstListEntry) -> Self {
        BasicBlock {
            block_asm_buf_ptr: asm.buf as *mut _,

            block_entry_start,
            block_entry_end,

            guest_regs_resolved: false,
            guest_regs_input_dirty: RegReserve::new(),
            guest_regs_output_dirty: RegReserve::new(),

            io_resolved: false,
            regs_live_ranges: Vec::new(),

            enter_blocks: Vec::new(),
            exit_blocks: Vec::with_capacity(2),

            insts_link: BlockInstList::new(),

            start_pc: 0,
        }
    }

    pub fn init_guest_regs(&mut self, asm: &mut BlockAsm) {
        self.guest_regs_output_dirty = self.guest_regs_input_dirty;

        let mut current_node = self.block_entry_start;
        loop {
            let i = BlockInstList::deref(current_node).value;

            match &mut asm.buf.insts[i].kind {
                BlockInstKind::SaveContext { guest_regs, .. } => {
                    *guest_regs = self.guest_regs_output_dirty;
                    self.guest_regs_output_dirty.clear();
                }
                BlockInstKind::SaveReg { guest_reg, .. } | BlockInstKind::RestoreReg { guest_reg, .. } | BlockInstKind::MarkRegDirty { guest_reg, dirty: false } => {
                    self.guest_regs_output_dirty -= *guest_reg
                }
                BlockInstKind::MarkRegDirty { guest_reg, dirty: true } => self.guest_regs_output_dirty += *guest_reg,
                _ => {
                    let inst = &asm.buf.insts[i];
                    let (_, outputs) = inst.get_io();
                    self.guest_regs_output_dirty += outputs.get_guests();
                }
            }

            if current_node == self.block_entry_end {
                break;
            }
            current_node = BlockInstList::deref(current_node).next;
        }
    }

    pub fn init_insts(&mut self, asm: &mut BlockAsm, basic_block_start_pc: u32, thumb: bool) {
        self.start_pc = basic_block_start_pc;

        let mut last_pc = basic_block_start_pc;

        let mut current_node = self.block_entry_start;
        loop {
            let mut add_inst = true;
            let i = BlockInstList::deref(current_node).value;
            match &mut asm.buf.insts[i].kind {
                BlockInstKind::Label { guest_pc: Some(pc), .. } => last_pc = *pc,
                BlockInstKind::GuestPc(pc) => last_pc = *pc,
                _ => match &asm.buf.insts[i].kind {
                    BlockInstKind::SaveContext { guest_regs, thread_regs_addr_reg } => {
                        let thread_regs_addr_reg = *thread_regs_addr_reg;
                        // Unroll regs to save into individual save regs, easier on reg allocator later on
                        for guest_reg in *guest_regs {
                            self.insts_link.insert_end(asm.buf.insts.len());
                            asm.buf.insts.push(
                                BlockInstKind::SaveReg {
                                    guest_reg,
                                    reg_mapped: guest_reg.into(),
                                    thread_regs_addr_reg,
                                }
                                .into(),
                            );
                        }
                        add_inst = false;
                    }
                    BlockInstKind::SaveReg { .. } | BlockInstKind::MarkRegDirty { .. } => {}
                    _ => {
                        let (inputs, _) = asm.buf.insts[i].get_io();
                        for guest_reg in inputs.get_guests() {
                            match guest_reg {
                                Reg::PC => {
                                    let mut last_pc = last_pc + if thumb { 4 } else { 8 };
                                    if thumb {
                                        match &asm.buf.insts[i].kind {
                                            BlockInstKind::Alu3 { thumb_pc_aligned, .. }
                                            | BlockInstKind::Alu2Op1 { thumb_pc_aligned, .. }
                                            | BlockInstKind::Alu2Op0 { thumb_pc_aligned, .. }
                                            | BlockInstKind::Mul { thumb_pc_aligned, .. } => {
                                                if *thumb_pc_aligned {
                                                    last_pc &= !0x3;
                                                }
                                            }
                                            _ => {}
                                        }
                                    } else if let BlockInstKind::GenericGuestInst { inst, .. } = &asm.buf.insts[i].kind {
                                        // PC + 12 when ALU shift by register
                                        if inst.op.is_alu_reg_shift() && *inst.operands().last().unwrap().as_reg().unwrap().0 == Reg::PC {
                                            last_pc += 4;
                                        }
                                    }

                                    self.insts_link.insert_end(asm.buf.insts.len());
                                    asm.buf.insts.push(
                                        BlockInstKind::Alu2Op0 {
                                            op: BlockAluOp::Mov,
                                            operands: [Reg::PC.into(), last_pc.into()],
                                            set_cond: BlockAluSetCond::None,
                                            thumb_pc_aligned: false,
                                        }
                                        .into(),
                                    );
                                }
                                Reg::CPSR => {
                                    self.insts_link.insert_end(asm.buf.insts.len());
                                    asm.buf.insts.push(
                                        BlockInstKind::SystemReg {
                                            op: BlockSystemRegOp::Mrs,
                                            operand: Reg::CPSR.into(),
                                        }
                                        .into(),
                                    );

                                    self.insts_link.insert_end(asm.buf.insts.len());
                                    asm.buf.insts.push(
                                        BlockInstKind::Transfer {
                                            op: BlockTransferOp::Read,
                                            operands: [asm.tmp_guest_cpsr_reg.into(), asm.thread_regs_addr_reg.into(), (Reg::CPSR as u32 * 4).into()],
                                            signed: false,
                                            amount: MemoryAmount::Half,
                                            add_to_base: true,
                                        }
                                        .into(),
                                    );

                                    self.insts_link.insert_end(asm.buf.insts.len());
                                    asm.buf.insts.push(
                                        BlockInstKind::Alu3 {
                                            op: BlockAluOp::And,
                                            operands: [Reg::CPSR.into(), Reg::CPSR.into(), (0xF8, ShiftType::Ror, 4).into()],
                                            set_cond: BlockAluSetCond::None,
                                            thumb_pc_aligned: false,
                                        }
                                        .into(),
                                    );

                                    self.insts_link.insert_end(asm.buf.insts.len());
                                    asm.buf.insts.push(
                                        BlockInstKind::Alu3 {
                                            op: BlockAluOp::Orr,
                                            operands: [Reg::CPSR.into(), Reg::CPSR.into(), asm.tmp_guest_cpsr_reg.into()],
                                            set_cond: BlockAluSetCond::None,
                                            thumb_pc_aligned: false,
                                        }
                                        .into(),
                                    );
                                }
                                Reg::SPSR => {
                                    self.insts_link.insert_end(asm.buf.insts.len());
                                    asm.buf.insts.push(
                                        BlockInstKind::Transfer {
                                            op: BlockTransferOp::Read,
                                            operands: [Reg::SPSR.into(), asm.thread_regs_addr_reg.into(), (Reg::SPSR as u32 * 4).into()],
                                            signed: false,
                                            amount: MemoryAmount::Word,
                                            add_to_base: true,
                                        }
                                        .into(),
                                    );
                                }
                                _ => {}
                            }
                        }
                    }
                },
            }

            if add_inst {
                self.insts_link.insert_end(i);
            }

            if current_node == self.block_entry_end {
                break;
            }
            current_node = BlockInstList::deref(current_node).next;
        }

        self.regs_live_ranges.resize(self.insts_link.len() + 1, BlockRegSet::new());
    }

    pub fn init_resolve_io(&mut self, asm: &BlockAsm) {
        let mut i = self.insts_link.len() - 1;
        for entry in self.insts_link.iter_rev() {
            let (inputs, outputs) = asm.buf.insts[entry.value].get_io();
            let mut previous_ranges = self.regs_live_ranges[i + 1];
            previous_ranges -= outputs;
            self.regs_live_ranges[i] = previous_ranges + inputs;
            i = i.wrapping_sub(1);
        }
    }

    pub fn remove_dead_code(&mut self, asm: &mut BlockAsm) {
        for (i, entry) in self.insts_link.iter().enumerate() {
            let inst = &mut asm.buf.insts[entry.value];
            if let BlockInstKind::RestoreReg { guest_reg, .. } = &inst.kind {
                if *guest_reg != Reg::CPSR {
                    let (_, outputs) = inst.get_io();
                    if (self.regs_live_ranges[i + 1] - outputs) == self.regs_live_ranges[i + 1] {
                        inst.skip = true;
                    }
                }
            }
        }
    }

    fn flush_reg_io_consolidation(&mut self, asm: &mut BlockAsm, from_reg: Reg, to_reg: Reg, save: bool, start_i: usize, end_i: usize, end_node: *mut BlockInstListEntry) {
        let mut current_node = end_node;
        let mut live_range_accumulated = BlockRegSet::new();
        for i in start_i..=end_i {
            let entry = BlockInstList::deref(current_node);
            let inst = &mut asm.buf.insts[entry.value];
            inst.skip = true;
            current_node = entry.previous;
            live_range_accumulated += self.regs_live_ranges[i];
        }

        for i in start_i..=end_i {
            self.regs_live_ranges[i] = live_range_accumulated;
        }

        let end_entry = BlockInstList::deref(end_node);

        let mut thread_regs_addr_reg = asm.thread_regs_addr_reg;
        if from_reg as u8 > 0 {
            thread_regs_addr_reg = asm.tmp_operand_imm_reg;
            let previous_entry = BlockInstList::deref(end_entry.previous);
            let previous_inst = &mut asm.buf.insts[previous_entry.value];
            *previous_inst = BlockInstKind::Alu3 {
                op: BlockAluOp::Add,
                operands: [thread_regs_addr_reg.into(), asm.thread_regs_addr_reg.into(), (from_reg as u32 * 4).into()],
                set_cond: BlockAluSetCond::None,
                thumb_pc_aligned: false,
            }
            .into();
            self.regs_live_ranges[end_i] += thread_regs_addr_reg;
        }

        let end_inst = &mut asm.buf.insts[end_entry.value];
        let op = if save { BlockTransferOp::Write } else { BlockTransferOp::Read };

        let mut guest_regs = RegReserve::new();
        for reg in from_reg as u8..=to_reg as u8 {
            guest_regs += Reg::from(reg);
        }
        *end_inst = BlockInstKind::GuestTransferMultiple {
            op,
            addr_reg: thread_regs_addr_reg,
            addr_out_reg: thread_regs_addr_reg,
            gp_regs: guest_regs,
            fixed_regs: RegReserve::new(),
            write_back: false,
            pre: false,
            add_to_base: true,
        }
        .into();
    }

    pub fn consolidate_reg_io(&mut self, asm: &mut BlockAsm) {
        let mut count = 0;
        let mut target_reg = Reg::None;
        let mut target_save = false;
        let mut last_reg = Reg::None;
        let mut was_save = None;
        let mut start_i = 0;
        let mut i = 0;

        let mut current_node = self.insts_link.root;
        while !current_node.is_null() {
            let entry = BlockInstList::deref(current_node);
            let inst = &asm.buf.insts[entry.value];
            if !inst.skip {
                let mut flush = true;

                match &inst.kind {
                    BlockInstKind::SaveReg { guest_reg, .. } => {
                        if was_save == Some(true) && *guest_reg <= Reg::R12 && last_reg as u8 + 1 == *guest_reg as u8 {
                            count += 1;
                            flush = false;
                            target_reg = *guest_reg;
                            target_save = true;
                        }
                        last_reg = *guest_reg;
                        was_save = Some(true);
                    }
                    BlockInstKind::RestoreReg { guest_reg, .. } => {
                        if was_save == Some(false) && *guest_reg <= Reg::R12 && last_reg as u8 + 1 == *guest_reg as u8 {
                            count += 1;
                            flush = false;
                            target_reg = *guest_reg;
                            target_save = false;
                        }
                        last_reg = *guest_reg;
                        was_save = Some(false);
                    }
                    _ => {
                        last_reg = Reg::None;
                        was_save = None;
                    }
                }

                if flush && count > 0 {
                    self.flush_reg_io_consolidation(asm, Reg::from(target_reg as u8 - count), target_reg, target_save, start_i, i - 1, entry.previous);
                    count = 0;
                }

                if count == 0 {
                    start_i = i;
                }
            }

            current_node = entry.next;
            i += 1;
        }
    }

    pub fn add_required_outputs(&mut self, required_outputs: BlockRegSet) {
        *self.regs_live_ranges.last_mut().unwrap() += required_outputs;
    }

    pub fn set_required_outputs(&mut self, required_outputs: BlockRegSet) {
        *self.regs_live_ranges.last_mut().unwrap() = required_outputs;
    }

    pub fn get_required_inputs(&self) -> &BlockRegSet {
        self.regs_live_ranges.first().unwrap()
    }

    pub fn get_required_outputs(&self) -> &BlockRegSet {
        self.regs_live_ranges.last().unwrap()
    }

    pub fn allocate_regs(&mut self, asm: &mut BlockAsm) {
        let mut i = 0;
        let mut current_node = self.insts_link.root;
        while !current_node.is_null() {
            let inst_i = BlockInstList::deref(current_node).value;
            let inst = &mut asm.buf.insts[inst_i];
            if !inst.skip {
                asm.buf.reg_allocator.inst_allocate(inst, &self.regs_live_ranges[i..]);
                if !asm.buf.reg_allocator.pre_allocate_insts.is_empty() {
                    for i in asm.buf.insts.len()..asm.buf.insts.len() + asm.buf.reg_allocator.pre_allocate_insts.len() {
                        self.insts_link.insert_entry_begin(current_node, i);
                    }
                    asm.buf.insts.extend_from_slice(&asm.buf.reg_allocator.pre_allocate_insts);
                }
            }
            i += 1;
            current_node = BlockInstList::deref(current_node).next;
        }

        if !self.exit_blocks.is_empty() {
            asm.buf.reg_allocator.ensure_global_mappings(*self.get_required_outputs());

            let end_entry = self.insts_link.end;
            // Make sure to restore mapping before a branch
            if let BlockInstKind::Branch { .. } = &asm.buf.insts[BlockInstList::deref(end_entry).value].kind {
                for i in asm.buf.insts.len()..asm.buf.insts.len() + asm.buf.reg_allocator.pre_allocate_insts.len() {
                    self.insts_link.insert_entry_begin(end_entry, i);
                }
            } else {
                for i in asm.buf.insts.len()..asm.buf.insts.len() + asm.buf.reg_allocator.pre_allocate_insts.len() {
                    self.insts_link.insert_end(i);
                }
            }
            asm.buf.insts.extend_from_slice(&asm.buf.reg_allocator.pre_allocate_insts);
        }
    }

    pub fn emit_opcodes(&self, asm: &mut BlockAsm, opcodes_offset: usize, used_host_regs: RegReserve) -> Vec<u32> {
        let mut opcodes = Vec::new();
        let mut inst_opcodes = Vec::new();
        for entry in self.insts_link.iter() {
            let inst = &mut asm.buf.insts[entry.value];
            if inst.skip {
                continue;
            }

            if IS_DEBUG && unsafe { BLOCK_LOG } {
                match &inst.kind {
                    BlockInstKind::GuestPc(pc) => {
                        println!("(0x{:x}, 0x{pc:x}),", opcodes.len() + opcodes_offset);
                    }
                    BlockInstKind::Label { guest_pc: Some(pc), .. } => {
                        println!("(0x{:x}, 0x{pc:x}),", opcodes.len() + opcodes_offset);
                    }
                    _ => {}
                }
            }

            inst_opcodes.clear();
            inst.kind
                .emit_opcode(&mut inst_opcodes, opcodes.len(), &mut asm.buf.branch_placeholders, opcodes_offset, used_host_regs);
            for opcode in &mut inst_opcodes {
                *opcode = (*opcode & !(0xF << 28)) | ((inst.cond as u32) << 28);
            }
            opcodes.extend(&inst_opcodes);
        }
        opcodes
    }
}

impl Debug for BasicBlock {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.insts_link.is_empty() {
            writeln!(f, "BasicBlock: uninitialized enter blocks: {:?}", self.enter_blocks)?;
            let mut current_node = self.block_entry_start;
            loop {
                let inst = unsafe { &self.block_asm_buf_ptr.as_ref().unwrap().insts[BlockInstList::deref(current_node).value] };
                writeln!(f, "\t{inst:?}")?;
                let (inputs, outputs) = inst.get_io();
                writeln!(f, "\t\tinputs: {inputs:?}, outputs: {outputs:?}")?;
                if current_node == self.block_entry_end {
                    break;
                }
                current_node = BlockInstList::deref(current_node).next;
            }
            write!(f, "BasicBlock end exit blocks: {:?}", self.exit_blocks)
        } else {
            writeln!(f, "BasicBlock: inputs: {:?} enter blocks: {:?}", self.regs_live_ranges.first().unwrap(), self.enter_blocks,)?;
            for (i, entry) in self.insts_link.iter().enumerate() {
                let inst = unsafe { &self.block_asm_buf_ptr.as_ref().unwrap().insts[entry.value] };
                writeln!(f, "\t{inst:?}")?;
                let (inputs, outputs) = inst.get_io();
                writeln!(f, "\t\tinputs: {inputs:?}, outputs: {outputs:?}")?;
                writeln!(f, "\t\tlive range: {:?}", self.regs_live_ranges[i])?;
            }
            write!(f, "BasicBlock end: outputs: {:?} exit blocks: {:?}", self.regs_live_ranges.last().unwrap(), self.exit_blocks,)
        }
    }
}
