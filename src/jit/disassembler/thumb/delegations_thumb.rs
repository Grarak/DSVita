mod unknown_delegations {
    use crate::jit::inst_info::Operands;
    use crate::jit::inst_info_thumb::InstInfoThumb;
    use crate::jit::reg::reg_reserve;
    use crate::jit::Op;

    #[inline]
    pub fn unk_t(opcode: u16, op: Op) -> InstInfoThumb {
        InstInfoThumb::new(opcode, op, Operands::new_empty(), reg_reserve!(), reg_reserve!(), 1)
    }
}

pub(super) use unknown_delegations::*;
