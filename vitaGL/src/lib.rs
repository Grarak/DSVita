pub const VITA_GL_VERSION: &str = include_str!(concat!(env!("OUT_DIR"), "/vita_gl_version"));

unsafe extern "C" {
    pub fn vglSwapBuffers(has_commondialog: u8);
    pub fn vglSetupRuntimeShaderCompiler(opt_level: i32, use_fastmath: i32, use_fastprecision: i32, use_fastint: i32);
    pub fn vglInitExtended(legacy_pool_size: i32, width: i32, height: i32, ram_threshold: i32, msaa: u32) -> u8;
    pub fn vglGetTexDataPointer(target: u32) -> *mut u8;
    pub fn vglFree(addr: *mut u8);
    pub fn vglTexImageDepthBuffer(target: u32);
    pub fn vglGetProcAddress(name: *const u8) -> *const u8;
    pub fn vglRemapTexPtr() -> *mut u8;
    pub fn glTexImage2Drgba5(width: i32, height: i32);
    pub fn vglBindFragUbo(index: u32);
}
