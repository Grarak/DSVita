use crate::emu::emu::{get_mmu, Emu};
use crate::emu::CpuType::ARM9;
use crate::logging::debug_println;
use bilge::prelude::*;
use std::{cmp, mem};

#[bitsize(32)]
#[derive(FromBits)]
struct Cp15ControlReg {
    mmu_pu_enable: u1,
    alignment_fault_check: u1,
    data_unified_cache: u1,
    write_buffer: u1,
    exception_handling: u1,
    address26bit_faults: u1,
    abort_model: u1,
    big_endian: u1,
    system_protection_bit: u1,
    rom_protection_bit: u1,
    implementation_defined: u1,
    branch_prediction: u1,
    instruction_cache: u1,
    exception_vectors: u1,
    cache_replacement: u1,
    pre_armv5_mode: u1,
    dtcm_enable: u1,
    dtcm_load_mode: u1,
    itcm_enable: u1,
    itcm_load_mode: u1,
    reserved: u2,
    unaligned_access: u1,
    extended_page_table: u1,
    reserved1: u1,
    cpsr_e_on_exceptions: u1,
    reserved2: u1,
    fiq_behaviour: u1,
    tex_remap_bit: u1,
    force_ap: u1,
    reserved3: u2,
}

#[bitsize(32)]
#[derive(FromBits)]
struct TcmReg {
    reserved: u1,
    virtual_size: u5,
    reserved1: u6,
    region_base: u20,
}

const CONTROL_RW_BITS_MASK: u32 = 0x000FF085;
const TCM_MIN_SIZE: u32 = 4 * 1024;

pub struct Cp15 {
    control: u32,
    pub exception_addr: u32,
    dtcm: u32,
    pub dtcm_state: TcmState,
    pub dtcm_addr: u32,
    pub dtcm_size: u32,
    itcm: u32,
    pub itcm_state: TcmState,
    pub itcm_size: u32,
}

#[derive(Eq, PartialEq)]
#[repr(u8)]
pub enum TcmState {
    Disabled = 0,
    RW = 1,
    W = 2,
}

impl From<u8> for TcmState {
    fn from(value: u8) -> Self {
        debug_assert!(value <= TcmState::W as u8);
        unsafe { mem::transmute(value) }
    }
}

impl Cp15 {
    pub fn new() -> Self {
        let mut control_default = Cp15ControlReg::from(0);
        control_default.set_write_buffer(u1::new(1));
        control_default.set_exception_handling(u1::new(1));
        control_default.set_address26bit_faults(u1::new(1));
        control_default.set_abort_model(u1::new(1));

        Cp15 {
            control: u32::from(control_default),
            exception_addr: 0,
            dtcm: 0,
            dtcm_state: TcmState::Disabled,
            dtcm_addr: 0,
            dtcm_size: 0,
            itcm: 0,
            itcm_state: TcmState::Disabled,
            itcm_size: 0,
        }
    }

    fn set_control_reg(&mut self, value: u32, emu: &Emu) {
        self.control = (self.control & (!CONTROL_RW_BITS_MASK)) | (value & CONTROL_RW_BITS_MASK);
        let control_reg = Cp15ControlReg::from(self.control);

        self.exception_addr = if bool::from(control_reg.exception_vectors()) { 0xFFFF0000 } else { 0x00000000 };
        self.dtcm_state = TcmState::from(u8::from(control_reg.dtcm_enable()) + u8::from(control_reg.dtcm_load_mode()));
        self.itcm_state = TcmState::from(u8::from(control_reg.itcm_enable()) + u8::from(control_reg.itcm_load_mode()));

        get_mmu!(emu, ARM9).update_itcm(emu);
        get_mmu!(emu, ARM9).update_dtcm(emu);
    }

    fn set_dtcm(&mut self, value: u32, emu: &Emu) {
        let tcm_reg = TcmReg::from(value);

        self.dtcm = value;
        self.dtcm_addr = u32::from(tcm_reg.region_base()) << 12;
        self.dtcm_size = cmp::max(512 << u8::from(tcm_reg.virtual_size()), TCM_MIN_SIZE);

        get_mmu!(emu, ARM9).update_dtcm(emu);

        debug_println!("{:?} Set dtcm to addr {:x} with size {:x}", ARM9, self.dtcm_addr, self.dtcm_size);
    }

    fn set_itcm(&mut self, value: u32, emu: &Emu) {
        let tcm_reg = TcmReg::from(value);

        self.itcm = value;
        self.itcm_size = cmp::max(512 << u8::from(tcm_reg.virtual_size()), TCM_MIN_SIZE);

        get_mmu!(emu, ARM9).update_itcm(emu);

        debug_println!("Set itcm with size {:x}", self.itcm_size);
    }

    pub fn write(&mut self, reg: u32, value: u32, emu: &Emu) {
        debug_println!("Writing to cp15 reg {:x} {:x}", reg, value);

        match reg {
            0x010000 => self.set_control_reg(value, emu),
            0x090100 => self.set_dtcm(value, emu),
            0x090101 => self.set_itcm(value, emu),
            _ => debug_println!("Unknown cp15 reg write {:x}", reg),
        }
    }

    pub fn read(&self, reg: u32, value: &mut u32) {
        debug_println!("Reading from cp15 reg {:x}", reg);

        *value = match reg {
            0x000000 => 0x41059461, // Main ID
            0x000001 => 0x0F0D2112, // Cache type
            0x010000 => self.control,
            0x090100 => self.dtcm,
            0x090101 => self.itcm,
            _ => {
                debug_println!("Unknown cp15 reg read {:x}", reg);
                0
            }
        }
    }
}
