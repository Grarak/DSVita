use crate::core::graphics::gpu_2d::renderer_2d::{Gpu2DCommon, Gpu2DMem, Gpu2DRenderRegs};
use crate::core::graphics::gpu_2d::Gpu2DEngine;

pub struct Gpu2DSoftRenderer {}

impl Gpu2DSoftRenderer {
    pub fn new() -> Gpu2DSoftRenderer {
        Gpu2DSoftRenderer {}
    }

    pub fn start_render<const ENGINE: Gpu2DEngine>(&mut self, common: &Gpu2DCommon, regs: &Gpu2DRenderRegs, mem: Gpu2DMem) {}

    pub fn wait_for_render() {}
}
