use bilge::prelude::*;

#[bitsize(16)]
#[derive(FromBits)]
struct KeyInput {
    a: u1,
    b: u1,
    select: u1,
    start: u1,
    r: u1,
    l: u1,
    u: u1,
    d: u1,
    trigger_r: u1,
    trigger_l: u1,
    not_used: u6,
}

pub struct InputContext {
    pub key_input: u16,
}

impl InputContext {
    pub fn new() -> Self {
        InputContext { key_input: 0x3FF }
    }
}
