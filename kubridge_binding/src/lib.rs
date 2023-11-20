use vitasdk_sys::{c_char, c_uint, SceKernelAllocMemBlockKernelOpt, SceKernelMemBlockType, SceUID};

#[link(name = "kubridge_stub")]
extern "C" {
    pub fn kuKernelAllocMemBlock(
        name: *const c_char,
        mem_block_type: SceKernelMemBlockType,
        size: c_uint,
        opt: *mut SceKernelAllocMemBlockKernelOpt,
    ) -> SceUID;
}
