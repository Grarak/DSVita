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
            nitro: SoundNitro::default(),
        }
    }

    pub fn ipc_recv(&mut self, data: u32, emu: &mut Emu) {
        if self.engine == -1 {
            if data >= 0x02000000 {
                self.engine = 0;
                self.nitro.reset(emu);
            } else {
                self.engine = 1;
            }
        }

        if self.engine == 0 {
            self.nitro.ipc_recv(data, emu);
        }
    }
}
