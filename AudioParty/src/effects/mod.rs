mod audio_party;

use crate::abi::SkydimoRgb;
use crate::host::NativeHost;

pub struct EffectInstance(Box<audio_party::AudioPartyEffect>);

impl EffectInstance {
    pub fn create(host: NativeHost) -> Self {
        Self(Box::new(audio_party::AudioPartyEffect::new(host)))
    }

    pub fn resize(&mut self, width: u32, height: u32, led_count: u32) {
        self.0.resize(width, height, led_count);
    }

    pub fn update_params(&mut self, json: &str) {
        self.0.update_params(json);
    }

    pub fn tick(&mut self, elapsed_seconds: f64, pixels: &mut [SkydimoRgb]) -> i32 {
        self.0.tick(elapsed_seconds, pixels)
    }

    pub fn is_ready(&self) -> bool {
        true
    }
}
