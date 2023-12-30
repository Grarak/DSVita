mod unknown_delegations {
    use crate::jit::inst_info_thumb::InstInfoThumb;
    use crate::jit::Op;

    #[inline]
    pub fn unk_t(opcode: u16, op: Op) -> InstInfoThumb {
        todo!()
    }
}

pub(super) use unknown_delegations::*;
