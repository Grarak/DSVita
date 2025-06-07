mod soundtouch {
    #![allow(warnings, unused)]
    include!(concat!(env!("OUT_DIR"), "/soundtouch_bindings.rs"));
}

pub struct SoundTouch(soundtouch::root::soundtouch::SoundTouch);

impl SoundTouch {
    pub fn new() -> Self {
        SoundTouch(unsafe { soundtouch::root::soundtouch::SoundTouch::new() })
    }

    pub fn set_channels(&mut self, channels: usize) {
        unsafe { self.0.setChannels(channels as _) };
    }

    pub fn set_sample_rate(&mut self, sample_rate: usize) {
        unsafe { self.0.setSampleRate(sample_rate as _) };
    }

    pub fn set_pitch(&mut self, pitch: f64) {
        unsafe { self.0.setPitch(pitch) };
    }

    pub fn set_tempo(&mut self, tempo: f64) {
        unsafe { self.0.setTempo(tempo) };
    }

    pub fn flush(&mut self) {
        unsafe { self.0.flush() };
    }

    pub fn clear(&mut self) {
        unsafe { soundtouch::root::soundtouch::SoundTouch_clear(&mut self.0 as *mut _ as _) };
    }

    pub fn num_of_samples(&self) -> usize {
        unsafe { soundtouch::root::soundtouch::FIFOProcessor_numSamples(&self.0 as *const _ as _) as usize }
    }

    pub fn put_samples(&mut self, samples: &[i16], num_samples: usize) {
        unsafe { soundtouch::root::soundtouch::SoundTouch_putSamples(&mut self.0 as *const _ as _, samples.as_ptr(), num_samples as _) }
    }

    pub fn receive_samples(&mut self, samples: &mut [i16], max_num_samples: usize) -> usize {
        unsafe { soundtouch::root::soundtouch::SoundTouch_receiveSamples(&mut self.0 as *mut _ as _, samples.as_mut_ptr(), max_num_samples as _) as _ }
    }
}

impl Drop for SoundTouch {
    fn drop(&mut self) {
        unsafe { soundtouch::root::soundtouch::SoundTouch_SoundTouch_destructor(&mut self.0) };
    }
}
