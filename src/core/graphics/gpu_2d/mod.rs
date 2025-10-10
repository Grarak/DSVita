use std::{
    marker::ConstParamTy,
    ops::{Index, IndexMut},
};

pub mod registers_2d;
pub mod renderer_2d;
pub mod renderer_soft_2d;

#[derive(ConstParamTy, Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
pub enum Gpu2DEngine {
    #[default]
    A,
    B,
}

impl<T> Index<Gpu2DEngine> for [T; 2] {
    type Output = T;

    fn index(&self, index: Gpu2DEngine) -> &Self::Output {
        &self[index as usize]
    }
}

impl<T> IndexMut<Gpu2DEngine> for [T; 2] {
    fn index_mut(&mut self, index: Gpu2DEngine) -> &mut Self::Output {
        &mut self[index as usize]
    }
}
