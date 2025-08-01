use std::marker::ConstParamTy;

pub mod registers_2d;
pub mod renderer_2d;

#[derive(ConstParamTy, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
pub enum Gpu2DEngine {
    #[default]
    A,
    B,
}
