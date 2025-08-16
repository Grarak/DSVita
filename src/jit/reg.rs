pub type Reg = vixl::Reg;
pub type RegReserve = vixl::RegReserve;

macro_rules! reg_reserve {
    ($($reg:expr),*) => {{
        #[allow(unused_mut)]
        let mut reg_reserve = crate::jit::reg::RegReserve::new();
        $(
            reg_reserve.reserve($reg);
        )*
        reg_reserve
    }};
}

pub(crate) use reg_reserve;
