use bilge::prelude::*;
use std::mem;

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
    pub obj_mode: u2,
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
    pub h_flip: bool,
    pub v_flip: bool,
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
