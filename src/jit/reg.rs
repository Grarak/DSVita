use crate::utils::StrErr;
use std::fmt::{Debug, Formatter};
use std::{mem, ops};

#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum Reg {
    R0 = 0,
    R1 = 1,
    R2 = 2,
    R3 = 3,
    R4 = 4,
    R5 = 5,
    R6 = 6,
    R7 = 7,
    R8 = 8,
    R9 = 9,
    R10 = 10,
    R11 = 11,
    R12 = 12,
    SP = 13,
    LR = 14,
    PC = 15,
    CPSR = 16,
}

pub const GP_REGS: [Reg; 13] =
    [
        Reg::R0,
        Reg::R1,
        Reg::R2,
        Reg::R3,
        Reg::R4,
        Reg::R5,
        Reg::R6,
        Reg::R7,
        Reg::R8,
        Reg::R9,
        Reg::R10,
        Reg::R11,
        Reg::R12,
    ];

impl<T: Into<u32>> From<T> for Reg {
    #[inline]
    fn from(value: T) -> Self {
        unsafe { mem::transmute(value.into() as u8) }
    }
}

#[derive(Copy, Clone)]
pub struct RegReserve(u32);

impl RegReserve {
    #[inline]
    pub fn new() -> Self {
        RegReserve(0)
    }

    #[inline]
    pub fn reserve(&mut self, reg: Reg) {
        self.0 |= 1 << reg as u8;
    }

    pub fn next_free_gp(&self) -> Result<Reg, StrErr> {
        for i in GP_REGS {
            if (self.0 >> i as u8) & 1 == 0 {
                return Ok(Reg::from(i));
            }
        }
        Err(StrErr::from("No free gp registers left"))
    }
}

impl ops::Add<RegReserve> for RegReserve {
    type Output = Self;

    fn add(self, rhs: RegReserve) -> Self::Output {
        RegReserve(self.0 | rhs.0)
    }
}

impl ops::AddAssign<RegReserve> for RegReserve {
    fn add_assign(&mut self, rhs: RegReserve) {
        self.0 |= rhs.0;
    }
}

impl ops::BitXor<RegReserve> for RegReserve {
    type Output = Self;

    fn bitxor(self, rhs: RegReserve) -> Self::Output {
        RegReserve(self.0 ^ rhs.0)
    }
}

impl ops::BitXorAssign<RegReserve> for RegReserve {
    fn bitxor_assign(&mut self, rhs: RegReserve) {
        self.0 ^= rhs.0;
    }
}

impl Debug for RegReserve {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut str = "".to_owned();
        for i in Reg::R0 as u8..=Reg::CPSR as u8 {
            if (self.0 >> i) & 1 == 1 {
                str += &format!("{:?}, ", Reg::from(i));
            }
        }
        let mut chars = str.chars();
        chars.next_back();
        chars.next_back();
        f.write_str(chars.as_str())
    }
}

pub struct GpRegReserve(RegReserve);

impl From<RegReserve> for GpRegReserve {
    fn from(value: RegReserve) -> Self {
        GpRegReserve(value)
    }
}

pub struct GpRegReserveIter {
    reserve: GpRegReserve,
    current: usize,
}

impl IntoIterator for GpRegReserve {
    type Item = Reg;
    type IntoIter = GpRegReserveIter;

    fn into_iter(self) -> Self::IntoIter {
        GpRegReserveIter {
            reserve: self,
            current: 0,
        }
    }
}

impl Iterator for GpRegReserveIter {
    type Item = <GpRegReserve as IntoIterator>::Item;

    fn next(&mut self) -> Option<Self::Item> {
        for i in self.current..GP_REGS.len() {
            if (self.reserve.0 .0 >> i) == 1 {
                return Some(Reg::from(i as u8));
            }
        }
        None
    }
}

macro_rules! reg_reserve {
    [$($reg:expr),*] => {
        {
            let mut gp_reg_reserve = crate::jit::reg::RegReserve::new();
            $(
                gp_reg_reserve.reserve($reg);
            )*
            gp_reg_reserve
        }
    };
}

pub(crate) use reg_reserve;
