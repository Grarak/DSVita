const CHANNEL_COUNT: usize = 4;

#[derive(Default)]
pub struct TimersContext {
    cnt_l: [u16; CHANNEL_COUNT],
    cnt_h: [u16; CHANNEL_COUNT],
}

impl TimersContext {
    pub fn new() -> Self {
        TimersContext::default()
    }
}
