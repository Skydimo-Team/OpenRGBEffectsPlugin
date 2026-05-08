use std::time::{SystemTime, UNIX_EPOCH};

use crate::abi::SkydimoRgb;
use crate::color::hsv_to_rgb;
use crate::host::NativeHost;
use crate::json;
use crate::rng::FastRng;

const HUE_STEPS: usize = 360;
const HUE_MAX: f32 = HUE_STEPS as f32;
const INITIAL_FRAME_DELTA: f64 = 1.0 / 30.0;
const MAX_FRAME_DELTA: f64 = 0.25;

#[derive(Clone, Copy)]
struct BloomConfig {
    speed: f32,
    saturation: f32,
}

impl Default for BloomConfig {
    fn default() -> Self {
        Self {
            speed: 50.0,
            saturation: 1.0,
        }
    }
}

#[derive(Clone, Copy)]
struct Flower {
    hue: f32,
    speed_mult: f32,
}

pub struct BloomEffect {
    config: BloomConfig,
    flowers: Vec<Flower>,
    rng: FastRng,
    rgb_table: [SkydimoRgb; HUE_STEPS],
    last_elapsed: Option<f64>,
}

impl BloomEffect {
    pub fn new(_host: NativeHost) -> Self {
        Self::new_with_seed(seed_from_time())
    }

    fn new_with_seed(seed: u32) -> Self {
        let config = BloomConfig::default();
        Self {
            config,
            flowers: Vec::new(),
            rng: FastRng::new(seed),
            rgb_table: build_rgb_table(config.saturation),
            last_elapsed: None,
        }
    }

    pub fn resize(&mut self, _width: u32, _height: u32, led_count: u32) {
        self.ensure_flower_count(led_count as usize);
    }

    pub fn update_params(&mut self, json: &str) {
        if let Some(speed) = json::number_field(json, "speed") {
            self.config.speed = speed.clamp(1.0, 100.0);
        }

        if let Some(saturation) = json::number_field(json, "saturation") {
            let saturation = (saturation / 100.0).clamp(0.0, 1.0);
            if (self.config.saturation - saturation).abs() > f32::EPSILON {
                self.config.saturation = saturation;
                self.rgb_table = build_rgb_table(saturation);
            }
        }
    }

    pub fn tick(&mut self, elapsed_seconds: f64, pixels: &mut [SkydimoRgb]) -> i32 {
        if pixels.is_empty() {
            self.advance_elapsed(elapsed_seconds);
            return 0;
        }

        self.ensure_flower_count(pixels.len());

        let delta = self.config.speed * self.advance_elapsed(elapsed_seconds) as f32;
        for (flower, pixel) in self.flowers.iter_mut().zip(pixels.iter_mut()) {
            let mut hue = flower.hue + flower.speed_mult * delta;
            if hue >= HUE_MAX {
                hue -= (hue / HUE_MAX).floor() * HUE_MAX;
            }
            flower.hue = hue;
            *pixel = self.rgb_table[hue as usize];
        }

        0
    }

    fn ensure_flower_count(&mut self, count: usize) {
        if self.flowers.len() == count {
            return;
        }

        self.flowers.clear();
        self.flowers.reserve(count);
        for _ in 0..count {
            self.flowers.push(Flower {
                hue: self.rng.next_unit().min(0.999_999) * HUE_STEPS as f32,
                speed_mult: 1.0 + self.rng.next_unit(),
            });
        }
    }

    fn advance_elapsed(&mut self, elapsed_seconds: f64) -> f64 {
        let elapsed = if elapsed_seconds.is_finite() && elapsed_seconds >= 0.0 {
            elapsed_seconds
        } else {
            0.0
        };
        let delta = match self.last_elapsed {
            None => INITIAL_FRAME_DELTA,
            Some(last) if elapsed >= last => elapsed - last,
            Some(_) => INITIAL_FRAME_DELTA,
        };
        self.last_elapsed = Some(elapsed);
        delta.min(MAX_FRAME_DELTA)
    }
}

fn build_rgb_table(saturation: f32) -> [SkydimoRgb; HUE_STEPS] {
    std::array::from_fn(|hue| hsv_to_rgb(hue as f32, saturation, 1.0))
}

fn seed_from_time() -> u32 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(0xA076_1D64_78BD_642F);
    let mixed = nanos ^ (nanos >> 32) ^ 0xB100_2A4D;
    (mixed as u32).max(1)
}

#[cfg(test)]
mod tests {
    use super::BloomEffect;

    #[test]
    fn params_update_speed_and_saturation_table() {
        let mut effect = BloomEffect::new_with_seed(1);

        effect.update_params(r#"{"speed":75,"saturation":0}"#);

        assert_eq!(effect.config.speed, 75.0);
        assert_eq!(effect.config.saturation, 0.0);
        let color = effect.rgb_table[120];
        assert_eq!((color.r, color.g, color.b), (255, 255, 255));
    }

    #[test]
    fn resize_seeds_one_flower_per_led() {
        let mut effect = BloomEffect::new_with_seed(1);

        effect.resize(0, 0, 4);

        assert_eq!(effect.flowers.len(), 4);
        assert!(effect
            .flowers
            .iter()
            .all(|flower| flower.hue >= 0.0 && flower.hue < 360.0));
    }
}
