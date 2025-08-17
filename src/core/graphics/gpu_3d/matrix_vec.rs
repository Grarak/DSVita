use crate::math::Matrix;
use std::arch::arm::vst1q_s32_x4;
use std::intrinsics::unlikely;
use std::mem;
use std::ops::{Index, IndexMut};

#[derive(Default)]
pub struct MatrixVec(Vec<Matrix>);

impl MatrixVec {
    pub fn new() -> Self {
        MatrixVec(Vec::new())
    }

    pub fn push(&mut self, mat: &Matrix) {
        if unlikely(self.0.len() == self.0.capacity()) {
            self.0.reserve(1);
        }
        unsafe {
            let ptr = self.0.as_mut_ptr().add(self.0.len());
            vst1q_s32_x4(ptr as _, mem::transmute(mat.vld()));
            self.0.set_len(self.0.len() + 1);
        }
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }
}

impl Index<usize> for MatrixVec {
    type Output = Matrix;

    fn index(&self, index: usize) -> &Self::Output {
        unsafe { self.0.get_unchecked(index) }
    }
}

impl IndexMut<usize> for MatrixVec {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        unsafe { self.0.get_unchecked_mut(index) }
    }
}
