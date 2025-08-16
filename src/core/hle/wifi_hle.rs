use crate::core::cycle_manager::EventType;
use crate::core::emu::Emu;
use crate::core::hle::arm7_hle::IpcFifoTag;
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
}

impl Emu {
    pub(super) fn wifi_hle_initialize(&mut self) {
        const CHAN_MASK: u16 = 0x2082;
        self.mem_write::<{ ARM7 }, _>(0x027FFCFA, CHAN_MASK);
    }

    fn wifi_hle_reply(&mut self, cmd: WMApiid, status: u16) {
        let callback = WmCallback { api_id: cmd as u16, err_code: status };
        self.mem_write_struct::<{ ARM7 }, true, _>(self.hle.wifi.fifo_7_to_9_ptr, &callback);

        self.arm7_hle_send_ipc_fifo(IpcFifoTag::WirelessManager, self.hle.wifi.fifo_7_to_9_ptr, false);
    }

    pub(super) fn wifi_hle_ipc_recv(&mut self, data: u32) {
        self.hle.wifi.msg_ptr = data;
        let cmd = self.mem_read::<{ ARM7 }, u16>(self.hle.wifi.msg_ptr) & !0x8000;
        if (cmd as u8) < WMApiid::AsyncKindMax as u8 {
            match WMApiid::from(cmd as u8) {
                WMApiid::Initialize => {
                    let arm7_buf_ptr = self.mem_read::<{ ARM7 }, _>(self.hle.wifi.msg_ptr + 4);
                    self.hle.wifi.status_ptr = self.mem_read::<{ ARM7 }, _>(self.hle.wifi.msg_ptr + 8);
                    self.hle.wifi.fifo_7_to_9_ptr = self.mem_read::<{ ARM7 }, _>(self.hle.wifi.msg_ptr + 12);

                    let arm7_buf = WmArm7Buf {
                        status_ptr: self.hle.wifi.status_ptr,
                        fifo_7_to_9: self.hle.wifi.fifo_7_to_9_ptr,
                        ..Default::default()
                    };
                    self.mem_write_struct::<{ ARM7 }, true, _>(arm7_buf_ptr, &arm7_buf);

                    let status = WmStatus {
                        state: WmState::Idle as u16,
                        busy_app_iid: 0,
                    };
                    self.mem_write_struct::<{ ARM7 }, true, _>(self.hle.wifi.status_ptr, &status);

                    self.wifi_hle_reply(WMApiid::Initialize, 0);
                }
                WMApiid::End => {
                    let status = WmStatus {
                        state: WmState::Ready as u16,
                        busy_app_iid: 0,
                    };
                    self.mem_write_struct::<{ ARM7 }, true, _>(self.hle.wifi.status_ptr, &status);

                    self.wifi_hle_reply(WMApiid::End, 0);
                }
                WMApiid::StartScan => {
                    let req = self.mem_read_struct::<{ ARM7 }, true, WMStartScanReq>(self.hle.wifi.msg_ptr);
                    self.hle.wifi.scan_channel = req.channel;

                    let status = WmStatus {
                        state: WmState::Scan as u16,
                        busy_app_iid: 0,
                    };
                    self.mem_write_struct::<{ ARM7 }, true, _>(self.hle.wifi.status_ptr, &status);

                    const MS_CYCLES: u32 = 34418;
                    self.cm.schedule(req.max_channel_time as u32 * MS_CYCLES * 1024, EventType::WifiScanHle);
                    return;
                }
                WMApiid::EndScan => {
                    let status = WmStatus {
                        state: WmState::Idle as u16,
                        busy_app_iid: 0,
                    };
                    self.mem_write_struct::<{ ARM7 }, true, _>(self.hle.wifi.status_ptr, &status);

                    self.wifi_hle_reply(WMApiid::EndScan, 0);
                }
                _ => {}
            }

            self.mem_write::<{ ARM7 }, u32>(self.hle.wifi.status_ptr + 4, 0);
        }

        self.mem_write::<{ ARM7 }, u16>(self.hle.wifi.msg_ptr, cmd | 0x8000);
    }

    pub fn wifi_hle_on_scan_event(&mut self) {
        let callback = WMStartScanCallback {
            state: WmScanState::ParentNotFound as u16,
            channel: self.hle.wifi.scan_channel,
            link_level: 0,
            ..Default::default()
        };
        self.mem_write_struct::<{ ARM7 }, true, _>(self.hle.wifi.fifo_7_to_9_ptr, &callback);
        self.wifi_hle_reply(WMApiid::StartScan, 0);

        self.mem_write::<{ ARM7 }, u32>(self.hle.wifi.status_ptr + 4, 0);
        self.mem_write::<{ ARM7 }, u16>(self.hle.wifi.msg_ptr, 0x8000 | WMApiid::StartScan as u16);
    }
}
