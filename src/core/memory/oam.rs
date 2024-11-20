use crate::core::memory::regions;
use crate::utils;
use crate::utils::HeapMemU8;
use bilge::prelude::*;
use std::mem;

pub struct Oam {
    pub mem: HeapMemU8<{ regions::OAM_SIZE as usize }>,
    pub dirty: bool,
}

impl Oam {
    pub fn new() -> Self {
        Oam { mem: HeapMemU8::new(), dirty: false }
    }

    pub fn read<T: utils::Convert>(&self, addr_offset: u32) -> T {
        utils::read_from_mem(self.mem.as_slice(), addr_offset & (regions::OAM_SIZE - 1))
    }

    pub fn read_slice<T: utils::Convert>(&self, addr_offset: u32, slice: &mut [T]) {
        utils::read_from_mem_slice(self.mem.as_slice(), addr_offset & (regions::OAM_SIZE - 1), slice);
    }

    pub fn write<T: utils::Convert>(&mut self, addr_offset: u32, value: T) {
        self.dirty = true;
        utils::write_to_mem(self.mem.as_mut_slice(), addr_offset & (regions::OAM_SIZE - 1), value)
    }

    pub fn write_slice<T: utils::Convert>(&mut self, addr_offset: u32, slice: &[T]) {
        self.dirty = true;
        utils::write_to_mem_slice(self.mem.as_mut_slice(), (addr_offset & (regions::OAM_SIZE - 1)) as usize, slice);
    }

    pub fn write_memset<T: utils::Convert>(&mut self, addr_offset: u32, value: T, size: usize) {
        self.dirty = true;
        utils::write_memset(self.mem.as_mut_slice(), (addr_offset & (regions::OAM_SIZE - 1)) as usize, value, size)
    }
}

#[repr(u8)]
#[derive(Debug, Eq, PartialEq)]
pub enum OamObjMode {
    Normal = 0,
    Affine = 1,
    Disabled = 2,
    AffineDouble = 3,
}

impl From<u8> for OamObjMode {
    fn from(value: u8) -> Self {
        debug_assert!(value <= OamObjMode::AffineDouble as u8);
        unsafe { mem::transmute(value) }
    }
}

#[repr(u8)]
#[derive(Debug, Eq, PartialEq)]
pub enum OamGfxMode {
    Normal = 0,
    AlphaBlending = 1,
    Window = 2,
    Bitmap = 3,
}

impl From<u8> for OamGfxMode {
    fn from(value: u8) -> Self {
        debug_assert!(value <= OamGfxMode::Bitmap as u8);
        unsafe { mem::transmute(value) }
    }
}

#[bitsize(16)]
#[derive(FromBits)]
pub struct OamAttrib0 {
    pub y: u8,
    obj_mode: u2,
    gfx_mode: u2,
    pub is_mosaic: bool,
    pub is_8bit: bool,
    pub shape: u2,
}

impl OamAttrib0 {
    pub fn get_obj_mode(&self) -> OamObjMode {
        OamObjMode::from(u8::from(self.obj_mode()))
    }

    pub fn get_gfx_mode(&self) -> OamGfxMode {
        OamGfxMode::from(u8::from(self.gfx_mode()))
    }
}

#[bitsize(16)]
#[derive(FromBits)]
pub struct OamAttrib1 {
    pub x: u9,
    pub affine_index: u3,
    pub flip: u2,
    pub size: u2,
}

#[bitsize(16)]
#[derive(FromBits)]
pub struct OamAttrib2 {
    pub tile_index: u10,
    pub priority: u2,
    pub pal_bank: u4,
}

#[repr(C)]
pub struct OamAttribs {
    pub attr0: u16,
    pub attr1: u16,
    pub attr2: u16,
    fill: i16,
}
