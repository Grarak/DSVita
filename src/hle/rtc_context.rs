pub struct RtcContext {
    rtc: u8,
}

impl RtcContext {
    pub fn new() -> Self {
        RtcContext { rtc: 0 }
    }

    pub fn get_rtc(&self) -> u8 {
        self.rtc
    }

    pub fn set_rtc(&mut self, value: u8) {
        self.rtc = value & 0x7;
    }
}
