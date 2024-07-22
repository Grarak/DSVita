use std::marker::ConstParamTy;

pub mod registers_2d;
pub mod renderer_2d;
pub mod renderer_soft_2d;

#[derive(ConstParamTy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Gpu2DEngine {
    A,
    B,
}
