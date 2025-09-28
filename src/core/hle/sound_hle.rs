use crate::core::emu::Emu;
use crate::core::hle::sound_nitro::SoundNitro;

pub struct SoundHle {
    engine: i8,
    pub(super) nitro: SoundNitro,
}

impl SoundHle {
    pub(super) fn new() -> Self {
        SoundHle {
            engine: -1,
            nitro: SoundNitro::new(),
        }
    }
}

impl Emu {
    pub fn sound_hle_ipc_recv(&mut self, data: u32) {
        if self.hle.sound.engine == -1 {
            if data >= 0x02000000 {
                self.hle.sound.engine = 0;
                self.sound_nitro_reset();
            } else {
                self.hle.sound.engine = 1;
            }
        }

        if self.hle.sound.engine == 0 {
            self.sound_nitro_ipc_recv(data);
        }
    }
}
