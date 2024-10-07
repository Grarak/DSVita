use crate::core::cpu_regs::InterruptFlag;
use crate::core::emu::{get_cpu_regs_mut, Emu};
use crate::core::CpuType::ARM7;
use crate::utils::HeapMemU8;
use bilge::prelude::*;

#[bitsize(16)]
#[derive(FromBits)]
struct WBBCnt {
    index: u8,
    not_used: u4,
    direction: u4,
}

#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq)]
pub enum PaketType {
    Loc1Frame = 0,
    CmdFrame = 1,
    Loc2Frame = 2,
    Loc3Frame = 3,
    BeaconFrame = 4,
    CmdReply = 5,
    CmdAck = 6,
}

#[derive(Default)]
pub struct Wifi {
    pub w_mode_wep: u16,
    pub w_txstat_cnt: u16,
    pub w_irf: u16,
    pub w_ie: u16,
    pub w_macaddr: [u16; 3],
    pub w_bssid: [u16; 3],
    pub w_aid_full: u16,
    pub w_rxcnt: u16,
    pub w_powerstate: u16,
    pub w_powerforce: u16,
    pub w_rxbuf_begin: u16,
    pub w_rxbuf_end: u16,
    pub w_rxbuf_wrcsr: u16,
    pub w_rxbuf_wr_addr: u16,
    pub w_rxbuf_rd_addr: u16,
    pub w_rxbuf_readcsr: u16,
    pub w_rxbuf_gap: u16,
    pub w_rxbuf_gapdisp: u16,
    pub w_txbuf_loc: [u16; 5],
    pub w_beacon_int: u16,
    pub w_txbuf_reply1: u16,
    pub w_txbuf_reply2: u16,
    pub w_txreq_read: u16,
    pub w_txstat: u16,
    pub w_us_countcnt: u16,
    pub w_us_comparecnt: u16,
    pub w_cmd_countcnt: u16,
    pub w_us_compare: u64,
    pub w_us_count: u64,
    pub w_pre_beacon: u16,
    pub w_cmd_count: u16,
    pub w_beacon_count: u16,
    pub w_rxbuf_count: u16,
    pub w_txbuf_wr_addr: u16,
    pub w_txbuf_count: u16,
    pub w_txbuf_gap: u16,
    pub w_txbuf_gapdisp: u16,
    pub w_post_beacon: u16,
    pub w_bb_write: u16,
    pub w_bb_read: u16,
    pub w_tx_seqno: u16,
    bb_registers: HeapMemU8<0x100>,
    pub w_config: [u16; 15],
}

impl Wifi {
    pub fn new() -> Self {
        let mut instance = Wifi::default();
        instance.w_powerstate = 0x200;
        instance.w_txreq_read = 0x10;
        instance.w_config = [0x0048, 0x4840, 0x0000, 0x0000, 0x0142, 0x8064, 0x0000, 0x2443, 0x0042, 0x0016, 0x0016, 0x0016, 0x162C, 0x0204, 0x0058];
        instance.bb_registers[0x00] = 0x6D;
        instance.bb_registers[0x5D] = 0x01;
        instance.bb_registers[0x64] = 0xFF;
        instance
    }

    pub fn get_w_rxbuf_rd_data(&mut self, emu: &mut Emu) -> u16 {
        let value = emu.mem_read::<{ ARM7 }, u16>(0x4804000 + self.w_rxbuf_rd_addr as u32);

        self.w_rxbuf_rd_addr += 2;
        if self.w_rxbuf_rd_addr == self.w_rxbuf_gap {
            self.w_rxbuf_rd_addr += self.w_rxbuf_gapdisp << 1;
        }

        let buf_size = (self.w_rxbuf_end & 0x1FFE) - (self.w_rxbuf_begin & 0x1FFE);
        if buf_size != 0 {
            self.w_rxbuf_rd_addr = ((self.w_rxbuf_begin & 0x1FFE) + (self.w_rxbuf_rd_addr - (self.w_rxbuf_begin & 0x1FFE)) % buf_size) & 0x1FFE;
        }

        if self.w_rxbuf_count > 0 {
            self.w_rxbuf_count -= 1;
            if self.w_rxbuf_count == 0 {
                todo!()
            }
        }
        value
    }

    pub fn get_w_txbuf_loc(&self, paket_type: PaketType) -> u16 {
        self.w_txbuf_loc[paket_type as usize]
    }

    pub fn get_w_us_compare(&self, index: usize) -> u16 {
        (self.w_us_compare >> (index * 16)) as u16
    }

    pub fn get_w_us_count(&self, index: usize) -> u16 {
        (self.w_us_count >> (index * 16)) as u16
    }

    pub fn set_w_mode_wep(&mut self, mask: u16, value: u16) {
        self.w_mode_wep = (self.w_mode_wep & !mask) | (value & mask);
    }

    pub fn set_w_txstat_cnt(&mut self, mut mask: u16, value: u16) {
        mask &= 0xF000;
        self.w_txstat_cnt = (self.w_txstat_cnt & !mask) | (value & mask);
    }

    pub fn set_w_irf(&mut self, mask: u16, value: u16) {
        self.w_irf &= !(value & mask);
    }

    pub fn set_w_ie(&mut self, mut mask: u16, value: u16, emu: &mut Emu) {
        if self.w_ie & self.w_irf == 0 && value & mask & self.w_irf != 0 {
            get_cpu_regs_mut!(emu, ARM7).send_interrupt(InterruptFlag::Wifi, emu);
        }

        mask &= 0xFBFF;
        self.w_ie = (self.w_ie & !mask) | (value & mask);
    }

    pub fn set_w_macaddr(&mut self, index: usize, mask: u16, value: u16) {
        self.w_macaddr[index] = (self.w_macaddr[index] & !mask) | (value & mask);
    }

    pub fn set_w_bssid(&mut self, index: usize, mask: u16, value: u16) {
        self.w_bssid[index] = (self.w_bssid[index] & !mask) | (value & mask);
    }

    pub fn set_w_aid_full(&mut self, mut mask: u16, value: u16) {
        mask &= 0x07FF;
        self.w_aid_full = (self.w_aid_full & !mask) | (value & mask);
    }

    pub fn set_w_rxcnt(&mut self, mut mask: u16, value: u16) {
        mask &= 0xFF0E;
        self.w_rxcnt = (self.w_rxcnt & !mask) | (value & mask);

        if value & 0x1 != 0 {
            self.w_rxbuf_wrcsr = self.w_rxbuf_wr_addr << 1;
        }
    }

    pub fn set_w_powerstate(&mut self, mut mask: u16, value: u16) {
        mask &= 0x0003;
        self.w_powerstate = (self.w_powerstate & !mask) | (value & mask);

        if self.w_powerstate & 0x2 != 0 {
            self.w_powerstate &= !(1 << 9);
        }
    }

    pub fn set_w_powerforce(&mut self, mut mask: u16, value: u16) {
        mask &= 0x8001;
        self.w_powerforce = (self.w_powerforce & !mask) | (value & mask);

        if self.w_powerforce & (1 << 15) != 0 {
            self.w_powerstate = (self.w_powerstate & !(1 << 9)) | ((self.w_powerforce & 0x1) << 9);
        }
    }

    pub fn set_w_rxbuf_begin(&mut self, mask: u16, value: u16) {
        self.w_rxbuf_begin = (self.w_rxbuf_begin & !mask) | (value & mask);
    }

    pub fn set_w_rxbuf_end(&mut self, mask: u16, value: u16) {
        self.w_rxbuf_end = (self.w_rxbuf_end & !mask) | (value & mask);
    }

    pub fn set_w_rxbuf_wr_addr(&mut self, mut mask: u16, value: u16) {
        mask &= 0x0FFF;
        self.w_rxbuf_wr_addr = (self.w_rxbuf_wr_addr & !mask) | (value & mask);
    }

    pub fn set_w_rxbuf_rd_addr(&mut self, mut mask: u16, value: u16) {
        mask &= 0x1FFE;
        self.w_rxbuf_rd_addr = (self.w_rxbuf_rd_addr & !mask) | (value & mask);
    }

    pub fn set_w_rxbuf_readcsr(&mut self, mut mask: u16, value: u16) {
        mask &= 0x0FFF;
        self.w_rxbuf_readcsr = (self.w_rxbuf_readcsr & !mask) | (value & mask);
    }

    pub fn set_w_rxbuf_count(&mut self, mut mask: u16, value: u16) {
        mask &= 0x0FFF;
        self.w_rxbuf_count = (self.w_rxbuf_count & !mask) | (value & mask);
    }

    pub fn set_w_rxbuf_gap(&mut self, mut mask: u16, value: u16) {
        mask &= 0x1FFE;
        self.w_rxbuf_gap = (self.w_rxbuf_gap & !mask) | (value & mask);
    }

    pub fn set_w_rxbuf_gapdisp(&mut self, mut mask: u16, value: u16) {
        mask &= 0x0FFF;
        self.w_rxbuf_gapdisp = (self.w_rxbuf_gapdisp & !mask) | (value & mask);
    }

    pub fn set_w_txbuf_wr_addr(&mut self, mut mask: u16, value: u16) {
        mask &= 0x1FFE;
        self.w_txbuf_wr_addr = (self.w_txbuf_wr_addr & !mask) | (value & mask);
    }

    pub fn set_w_txbuf_count(&mut self, mut mask: u16, value: u16) {
        mask &= 0x0FFF;
        self.w_txbuf_count = (self.w_txbuf_count & !mask) | (value & mask);
    }

    pub fn set_w_txbuf_wr_data(&mut self, mask: u16, value: u16, emu: &mut Emu) {
        emu.mem_write::<{ ARM7 }, u16>(0x4804000 + self.w_txbuf_wr_addr as u32, value & mask);

        self.w_txbuf_wr_addr += 2;
        if self.w_txbuf_wr_addr == self.w_txbuf_gap {
            self.w_txbuf_wr_addr += self.w_txbuf_gapdisp << 1;
        }
        self.w_txbuf_wr_addr &= 0x1FFF;

        if self.w_txbuf_count > 0 {
            self.w_txbuf_count -= 1;
            if self.w_txbuf_count == 0 {
                todo!()
            }
        }
    }

    pub fn set_w_txbuf_gap(&mut self, mut mask: u16, value: u16) {
        mask &= 0x1FFE;
        self.w_txbuf_gap = (self.w_txbuf_gap & !mask) | (value & mask);
    }

    pub fn set_w_txbuf_gapdisp(&mut self, mut mask: u16, value: u16) {
        mask &= 0x0FFF;
        self.w_txbuf_gapdisp = (self.w_txbuf_gapdisp & !mask) | (value & mask);
    }

    pub fn set_w_txbuf_loc(&mut self, paket_type: PaketType, mask: u16, value: u16) {
        self.w_txbuf_loc[paket_type as usize] = (self.w_txbuf_loc[paket_type as usize] & !mask) | (value & mask);

        if paket_type != PaketType::BeaconFrame && self.w_txbuf_loc[paket_type as usize] & (1 << 15) != 0 && self.w_txreq_read & (1 << paket_type as u8) != 0 {
            todo!()
        }
    }

    pub fn set_w_beacon_int(&mut self, mut mask: u16, value: u16) {
        mask &= 0x03FF;
        self.w_beacon_int = (self.w_beacon_int & !mask) | (value & mask);

        self.w_beacon_count = self.w_beacon_int;
    }

    pub fn set_w_txbuf_reply1(&mut self, mask: u16, value: u16) {
        self.w_txbuf_reply1 = (self.w_txbuf_reply1 & !mask) | (value & mask);
    }

    pub fn set_w_txreq_reset(&mut self, mut mask: u16, value: u16) {
        mask &= 0x000F;
        self.w_txreq_read &= !(value & mask);
    }

    pub fn set_w_txreq_set(&mut self, mut mask: u16, value: u16) {
        mask &= 0x000F;
        self.w_txreq_read |= value & mask;

        for i in 0..4 {
            if self.w_txbuf_loc[i] & (1 << 15) != 0 && self.w_txreq_read & (1 << i) != 0 {
                todo!()
            }
        }
    }

    pub fn set_w_us_countcnt(&mut self, mut mask: u16, value: u16) {
        mask &= 0x0001;
        self.w_us_countcnt = (self.w_us_countcnt & !mask) | (value & mask);
    }

    pub fn set_w_us_comparecnt(&mut self, mut mask: u16, value: u16) {
        mask &= 0x0001;
        self.w_us_comparecnt = (self.w_us_comparecnt & !mask) | (value & mask);

        if value & 0x2 != 0 {
            todo!()
        }
    }

    pub fn set_w_cmd_countcnt(&mut self, mut mask: u16, value: u16) {
        mask &= 0x0001;
        self.w_cmd_countcnt = (self.w_cmd_countcnt & !mask) | (value & mask);
    }

    pub fn set_w_us_compare(&mut self, index: usize, mut mask: u16, value: u16) {
        let shift = 16 * index;
        mask &= if index != 0 { 0xFFFF } else { 0xFC00 };
        self.w_us_compare = (self.w_us_compare & !((mask as u64) << shift)) | (((value & mask) as u64) << shift);
    }

    pub fn set_w_us_count(&mut self, index: usize, mask: u16, value: u16) {
        let shift = 16 * index;
        self.w_us_count = (self.w_us_count & !((mask as u64) << shift)) | (((value & mask) as u64) << shift);
    }

    pub fn set_w_pre_beacon(&mut self, mask: u16, value: u16) {
        self.w_pre_beacon = (self.w_pre_beacon & !mask) | (value & mask);
    }

    pub fn set_w_cmd_count(&mut self, mask: u16, value: u16) {
        self.w_cmd_count = (self.w_cmd_count & !mask) | (value & mask);
    }

    pub fn set_w_beacon_count(&mut self, mask: u16, value: u16) {
        self.w_beacon_count = (self.w_beacon_count & !mask) | (value & mask);
    }

    pub fn set_w_config(&mut self, index: usize, mut mask: u16, value: u16) {
        const MASKS: [u16; 15] = [0x81FF, 0xFFFF, 0xFFFF, 0xFFFF, 0x0FFF, 0x8FFF, 0xFFFF, 0xFFFF, 0x00FF, 0x00FF, 0x00FF, 0x00FF, 0xFFFF, 0xFF3F, 0x7A7F];

        mask &= MASKS[index];
        self.w_config[index] = (self.w_config[index] & !mask) | (value & mask);
    }

    pub fn set_w_post_beacon(&mut self, mask: u16, value: u16) {
        self.w_post_beacon = (self.w_post_beacon & !mask) | (value & mask);
    }

    pub fn set_w_bb_cnt(&mut self, mask: u16, value: u16) {
        let cnt = WBBCnt::from(value & mask);
        let index = cnt.index();
        match u8::from(cnt.direction()) {
            5 => {
                if (index >= 0x01 && index <= 0x0C)
                    || (index >= 0x13 && index <= 0x15)
                    || (index >= 0x1B && index <= 0x26)
                    || (index >= 0x28 && index <= 0x4C)
                    || (index >= 0x4E && index <= 0x5C)
                    || (index >= 0x62 && index <= 0x63)
                    || (index == 0x65)
                    || (index >= 0x67 && index <= 0x68)
                {
                    self.bb_registers[index as usize] = self.w_bb_write as u8;
                }
            }
            6 => self.w_bb_read = self.bb_registers[index as usize] as u16,
            _ => {}
        }
    }

    pub fn set_w_bb_write(&mut self, mask: u16, value: u16) {
        self.w_bb_write = (self.w_bb_write & !mask) | (value & mask);
    }

    pub fn set_w_irf_set(&mut self, mut mask: u16, value: u16, emu: &mut Emu) {
        if self.w_ie & self.w_irf == 0 && self.w_ie & value & mask != 0 {
            get_cpu_regs_mut!(emu, ARM7).send_interrupt(InterruptFlag::Wifi, emu);
        }

        mask &= 0xFBFF;
        self.w_irf |= value & mask;
    }
}
