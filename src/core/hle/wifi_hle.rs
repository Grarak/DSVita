use crate::core::cycle_manager::{CycleManager, EventType};
use crate::core::emu::{get_arm7_hle_mut, get_cm_mut, get_mem_mut, Emu};
use crate::core::hle::arm7_hle::{Arm7Hle, IpcFifoTag};
use crate::core::CpuType::ARM7;

#[repr(u8)]
enum WMApiid {
    Initialize = 0,
    Reset = 1,
    End = 2,
    Enable = 3,
    Disable = 4,
    PowerOn = 5,
    PowerOff = 6,
    SetPParam = 7,
    StartParent = 8,
    EndParent = 9,
    StartScan = 10,
    EndScan = 11,
    AsyncKindMax = 46,
    Unknown = 255,
}

impl From<u8> for WMApiid {
    fn from(value: u8) -> Self {
        debug_assert!(value <= WMApiid::Unknown as u8);
        unsafe { std::mem::transmute(value) }
    }
}

#[repr(u8)]
enum WmState {
    Ready = 0,
    Stop = 1,
    Idle = 2,
    Class1 = 3,
    TestMode = 4,
    Scan = 5,
}

#[derive(Default)]
#[repr(C, align(4))]
struct WmArm7Buf {
    status_ptr: u32,
    _reserved: [u8; 4],
    fifo_7_to_9: u32,
}

#[repr(C, align(4))]
struct WmStatus {
    state: u16,
    busy_app_iid: u16,
}

#[repr(C, align(4))]
struct WmCallback {
    api_id: u16,
    err_code: u16,
}

#[repr(u8)]
enum WmScanState {
    ParentStart = 0,
    BeaconSent = 2,
    ScanStart = 3,
    ParentNotFound = 4,
}

#[repr(C, align(4))]
struct WMStartScanReq {
    api_id: u16,
    channel: u16,
    scan_buf_ptr: u32,
    max_channel_time: u16,
}

#[derive(Default)]
#[repr(C, align(4))]
struct WMStartScanCallback {
    api_id: u16,
    err_code: u16,
    wl_cmd_id: u16,
    wl_result: u16,
    state: u16,
    mac_addr: [u8; 6],
    channel: u16,
    link_level: u16,
}

pub struct WifiHle {
    msg_ptr: u32,
    status_ptr: u32,
    fifo_7_to_9_ptr: u32,
    scan_channel: u16,
}

impl WifiHle {
    pub(super) fn new() -> Self {
        WifiHle {
            msg_ptr: 0,
            status_ptr: 0,
            fifo_7_to_9_ptr: 0,
            scan_channel: 0,
        }
    }

    pub(super) fn initialize(emu: &mut Emu) {
        const CHAN_MASK: u16 = 0x2082;
        emu.mem_write::<{ ARM7 }, _>(0x027FFCFA, CHAN_MASK);
    }

    fn reply(&self, cmd: WMApiid, status: u16, emu: &mut Emu) {
        let callback = WmCallback { api_id: cmd as u16, err_code: status };
        get_mem_mut!(emu).write_struct::<{ ARM7 }, true, _>(self.fifo_7_to_9_ptr, &callback, emu);

        Arm7Hle::send_ipc_fifo(IpcFifoTag::WirelessManager, self.fifo_7_to_9_ptr, false, emu);
    }

    pub(super) fn ipc_recv(&mut self, data: u32, emu: &mut Emu) {
        self.msg_ptr = data;
        let cmd = emu.mem_read::<{ ARM7 }, u16>(self.msg_ptr) & !0x8000;
        if (cmd as u8) < WMApiid::AsyncKindMax as u8 {
            match WMApiid::from(cmd as u8) {
                WMApiid::Initialize => {
                    let arm7_buf_ptr = emu.mem_read::<{ ARM7 }, _>(self.msg_ptr + 4);
                    self.status_ptr = emu.mem_read::<{ ARM7 }, _>(self.msg_ptr + 8);
                    self.fifo_7_to_9_ptr = emu.mem_read::<{ ARM7 }, _>(self.msg_ptr + 12);

                    let arm7_buf = WmArm7Buf {
                        status_ptr: self.status_ptr,
                        fifo_7_to_9: self.fifo_7_to_9_ptr,
                        ..Default::default()
                    };
                    get_mem_mut!(emu).write_struct::<{ ARM7 }, true, _>(arm7_buf_ptr, &arm7_buf, emu);

                    let status = WmStatus {
                        state: WmState::Idle as u16,
                        busy_app_iid: 0,
                    };
                    get_mem_mut!(emu).write_struct::<{ ARM7 }, true, _>(self.status_ptr, &status, emu);

                    self.reply(WMApiid::Initialize, 0, emu);
                }
                WMApiid::End => {
                    let status = WmStatus {
                        state: WmState::Ready as u16,
                        busy_app_iid: 0,
                    };
                    get_mem_mut!(emu).write_struct::<{ ARM7 }, true, _>(self.status_ptr, &status, emu);

                    self.reply(WMApiid::End, 0, emu);
                }
                WMApiid::StartScan => {
                    let req = get_mem_mut!(emu).read_struct::<{ ARM7 }, true, WMStartScanReq>(self.msg_ptr, emu);
                    self.scan_channel = req.channel;

                    let status = WmStatus {
                        state: WmState::Scan as u16,
                        busy_app_iid: 0,
                    };
                    get_mem_mut!(emu).write_struct::<{ ARM7 }, true, _>(self.status_ptr, &status, emu);

                    const MS_CYCLES: u32 = 34418;
                    get_cm_mut!(emu).schedule(req.max_channel_time as u32 * MS_CYCLES * 1024, EventType::WifiScanHle, 0);
                    return;
                }
                WMApiid::EndScan => {
                    let status = WmStatus {
                        state: WmState::Idle as u16,
                        busy_app_iid: 0,
                    };
                    get_mem_mut!(emu).write_struct::<{ ARM7 }, true, _>(self.status_ptr, &status, emu);

                    self.reply(WMApiid::EndScan, 0, emu);
                }
                _ => {}
            }

            emu.mem_write::<{ ARM7 }, u32>(self.status_ptr + 4, 0);
        }

        emu.mem_write::<{ ARM7 }, u16>(self.msg_ptr, cmd | 0x8000);
    }

    pub fn on_scan_event(_: &mut CycleManager, emu: &mut Emu, _: u16) {
        let wifi = &get_arm7_hle_mut!(emu).wifi;
        let callback = WMStartScanCallback {
            state: WmScanState::ParentNotFound as u16,
            channel: wifi.scan_channel,
            link_level: 0,
            ..Default::default()
        };
        get_mem_mut!(emu).write_struct::<{ ARM7 }, true, _>(wifi.fifo_7_to_9_ptr, &callback, emu);
        wifi.reply(WMApiid::StartScan, 0, emu);

        emu.mem_write::<{ ARM7 }, u32>(wifi.status_ptr + 4, 0);
        emu.mem_write::<{ ARM7 }, u16>(wifi.msg_ptr, 0x8000 | WMApiid::StartScan as u16);
    }
}
