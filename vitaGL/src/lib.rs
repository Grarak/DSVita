pub const VITA_GL_VERSION: &str = include_str!(concat!(env!("OUT_DIR"), "/vita_gl_version"));

#[repr(i32)]
pub enum SharkOpt {
    Slow = 0,
    Safe = 1,
    Default = 2,
    Fast = 3,
    Unsafe = 4,
}

#[repr(i32)]
pub enum VglMemType {
    Vram = 0,
    Ram = 1,
    Slow = 2,
    Budget = 3,
    External = 4,
    All = 5,
}

unsafe extern "C" {
    pub static gxm_context: *const u8;

    pub fn vglSwapBuffers(has_commondialog: u8);
    pub fn vglSetupRuntimeShaderCompiler(opt_level: SharkOpt, use_fastmath: i32, use_fastprecision: i32, use_fastint: i32);
    pub fn vglInitExtended(legacy_pool_size: i32, width: i32, height: i32, ram_threshold: i32, msaa: u32) -> u8;
    pub fn vglUseCachedMem(use_cached: bool);
    pub fn vglUseExtraMem(use_extra: bool);
    pub fn vglGetTexDataPointer(target: u32) -> *mut u8;
    pub fn vglFree(addr: *mut u8);
    pub fn vglTexImageDepthBuffer(target: u32);
    pub fn vglGetProcAddress(name: *const u8) -> *const u8;
    pub fn vglRemapTexPtr() -> *mut u8;
    pub fn glTexImage2Drgba5(width: i32, height: i32);
    pub fn vglBindFragUbo(index: u32);
    pub fn vglBufferData(target: u32, data: *const u8);
    pub fn vgl_memalign(alignment: usize, size: usize, vgl_mem_type: VglMemType) -> *mut u8;
}
