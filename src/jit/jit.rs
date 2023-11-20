pub struct JitAsm {
    write_to_memory: bool,
}

impl JitAsm {
    pub fn new() -> Self {
        JitAsm {
            write_to_memory: false,
        }
    }
}
