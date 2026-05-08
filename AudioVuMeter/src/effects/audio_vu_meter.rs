use crate::abi::SkydimoRgb;
use crate::color::{fill_black, hsv_to_rgb, rgb_to_hsv, to_u8};
use crate::host::NativeHost;
use crate::json;

const AUDIO_BINS: usize = 256;
const ALBUM_ART_SIZE: usize = 64;
const TARGET_FRAME_SECONDS: f32 = 1.0 / 60.0;
const PEAK_HALF_WIDTH: f32 = 0.02;

#[derive(Clone, Copy)]
struct AudioVuMeterConfig {
    speed: f32,
    avg_size: usize,
    color_offset: f32,
    color_spread: f32,
    saturation: f32,
    invert_hue: bool,
    use_album_art: bool,
}

impl Default for AudioVuMeterConfig {
    fn default() -> Self {
        Self {
            speed: 50.0,
            avg_size: 8,
            color_offset: 180.0,
            color_spread: 50.0,
            saturation: 100.0,
            invert_hue: false,
            use_album_art: false,
        }
    }
}

#[derive(Clone, Copy)]
struct AudioVuMeterState {
    peak_height: f32,
    palette_time: f32,
}

impl Default for AudioVuMeterState {
    fn default() -> Self {
        Self {
            peak_height: 0.0,
            palette_time: 0.0,
        }
    }
}

#[derive(Clone, Copy, Default)]
struct WeightedRgb {
    r: u8,
    g: u8,
    b: u8,
    weight: f32,
}

#[derive(Default)]
struct AlbumPalette {
    checksum: Option<u64>,
    colors: Vec<WeightedRgb>,
    blur_temp: Vec<SkydimoRgb>,
    blur_out: Vec<SkydimoRgb>,
    kernel: Vec<f32>,
}

pub struct AudioVuMeterEffect {
    host: NativeHost,
    config: AudioVuMeterConfig,
    state: AudioVuMeterState,
    width: usize,
    height: usize,
    palette: AlbumPalette,
}

impl AudioVuMeterEffect {
    pub fn new(host: NativeHost) -> Self {
        Self {
            host,
            config: AudioVuMeterConfig::default(),
            state: AudioVuMeterState::default(),
            width: 1,
            height: 1,
            palette: AlbumPalette::default(),
        }
    }

    pub fn resize(&mut self, width: u32, height: u32, led_count: u32) {
        if width == 0 || height == 0 {
            self.width = led_count.max(1) as usize;
            self.height = 1;
            return;
        }
        self.width = width.max(1) as usize;
        self.height = height.max(1) as usize;
    }

    pub fn update_params(&mut self, json: &str) {
        if let Some(speed) = json::number_field(json, "speed") {
            self.config.speed = speed.clamp(1.0, 100.0);
        }
        if let Some(avg_size) = json::number_field(json, "avgSize") {
            self.config.avg_size = (avg_size.floor() as usize).clamp(1, AUDIO_BINS);
        }
        if let Some(offset) = json::number_field(json, "colorOffset") {
            self.config.color_offset = offset.rem_euclid(360.0);
        }
        if let Some(spread) = json::number_field(json, "colorSpread") {
            self.config.color_spread = spread.clamp(0.0, 100.0);
        }
        if let Some(saturation) = json::number_field(json, "saturation") {
            self.config.saturation = saturation.clamp(0.0, 100.0);
        }
        if let Some(invert_hue) = json::bool_field(json, "invertHue") {
            self.config.invert_hue = invert_hue;
        }
        if let Some(use_album_art) = json::bool_field(json, "useAlbumArt") {
            self.config.use_album_art = use_album_art;
        }
    }

    pub fn tick(&mut self, elapsed_seconds: f64, pixels: &mut [SkydimoRgb]) -> i32 {
        if pixels.is_empty() {
            return 0;
        }

        let Some(amplitude) = self.host.capture_audio_amplitude(self.config.avg_size) else {
            fill_black(pixels);
            return 0;
        };

        let amp = if amplitude.is_finite() {
            amplitude.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let dt = frame_delta(elapsed_seconds);
        self.update_peak(amp, dt);

        let use_palette = self.config.use_album_art
            && self
                .host
                .with_album_art(ALBUM_ART_SIZE, ALBUM_ART_SIZE, |width, height, art| {
                    self.palette.refresh(width, height, art)
                })
                .unwrap_or(false);

        if use_palette {
            self.state.palette_time = (self.state.palette_time + self.config.speed * dt) % 360.0;
        }

        let width = self.width.max(1);
        let height = self.height.max(1);
        if height == 1 || width == 1 {
            self.render_linear(pixels, amp, use_palette);
        } else {
            self.render_matrix(pixels, width, height, amp, use_palette);
        }
        0
    }

    fn update_peak(&mut self, amp: f32, dt: f32) {
        if self.state.peak_height > amp {
            let decay_rate = 0.05 * self.config.speed * dt;
            self.state.peak_height = (self.state.peak_height - decay_rate).max(0.0);
        } else {
            self.state.peak_height = amp;
        }
    }

    fn render_linear(&self, pixels: &mut [SkydimoRgb], amp: f32, use_palette: bool) {
        let denom = pixels.len().saturating_sub(1).max(1) as f32;
        for (idx, pixel) in pixels.iter_mut().enumerate() {
            let t = idx as f32 / denom;
            let pos = 1.0 - ((t - 0.5).abs() * 2.0);
            *pixel = self.color_for(amp, pos, use_palette);
        }
    }

    fn render_matrix(
        &self,
        pixels: &mut [SkydimoRgb],
        width: usize,
        height: usize,
        amp: f32,
        use_palette: bool,
    ) {
        let y_denom = height.saturating_sub(1).max(1) as f32;
        let mut led = 0usize;
        for y in 0..height {
            let pos = if height > 1 {
                (height - 1 - y) as f32 / y_denom
            } else {
                1.0
            };
            for _x in 0..width {
                if led >= pixels.len() {
                    return;
                }
                pixels[led] = self.color_for(amp, pos, use_palette);
                led += 1;
            }
        }
        if led < pixels.len() {
            fill_black(&mut pixels[led..]);
        }
    }

    fn color_for(&self, amp: f32, pos: f32, use_palette: bool) -> SkydimoRgb {
        let peak = self.state.peak_height;
        let brightness = if pos <= amp {
            1.0
        } else if peak > 0.01 && (pos - peak).abs() < PEAK_HALF_WIDTH {
            1.0 - ((pos - peak).abs() / PEAK_HALF_WIDTH)
        } else {
            return SkydimoRgb::default();
        };

        let saturation = (self.config.saturation * 0.01).clamp(0.0, 1.0);
        if use_palette {
            let flow_phase = (self.state.palette_time / 360.0).rem_euclid(1.0);
            let rgb = self.palette.sample(pos, flow_phase);
            let (hue, _, _) = rgb_to_hsv(rgb);
            return hsv_to_rgb(hue, saturation, brightness);
        }

        let spread = self.config.color_spread * 0.01;
        let mut hue = (self.config.color_offset + pos * 360.0 * spread).rem_euclid(360.0);
        if self.config.invert_hue {
            hue = (360.0 - hue).rem_euclid(360.0);
        }
        hsv_to_rgb(hue, saturation, brightness)
    }
}

impl AlbumPalette {
    fn refresh(&mut self, width: usize, height: usize, pixels: &[SkydimoRgb]) -> bool {
        let total = width.saturating_mul(height).min(pixels.len());
        if total == 0 {
            self.colors.clear();
            self.checksum = None;
            return false;
        }

        let pixels = &pixels[..total];
        let checksum = art_fingerprint(pixels);
        if checksum.is_some() && checksum == self.checksum && !self.colors.is_empty() {
            return true;
        }

        self.extract(pixels, width.max(1), height.max(1), checksum, 8);
        !self.colors.is_empty()
    }

    fn extract(
        &mut self,
        pixels: &[SkydimoRgb],
        width: usize,
        height: usize,
        checksum: Option<u64>,
        max_colors: usize,
    ) {
        self.colors.clear();
        self.checksum = checksum;
        if pixels.is_empty() || width == 0 || height == 0 {
            return;
        }

        gaussian_blur(
            pixels,
            width,
            height,
            8,
            &mut self.kernel,
            &mut self.blur_temp,
            &mut self.blur_out,
        );
        let blurred = if self.blur_out.is_empty() {
            pixels
        } else {
            self.blur_out.as_slice()
        };

        let grid_size = 4usize;
        let min_dist_sq = 30i32 * 30i32;
        let max_colors = max_colors.clamp(1, 16);
        for gy in 0..grid_size {
            for gx in 0..grid_size {
                let mut sx = (gx * width) / grid_size + width / (grid_size * 2);
                let mut sy = (gy * height) / grid_size + height / (grid_size * 2);
                sx = sx.min(width - 1);
                sy = sy.min(height - 1);

                let idx = sy.saturating_mul(width).saturating_add(sx);
                let Some(&rgb) = blurred.get(idx) else {
                    continue;
                };
                if rgb.r.max(rgb.g).max(rgb.b) < 24 {
                    continue;
                }

                let mut closest_idx = None;
                let mut closest_dist = i32::MAX;
                for (idx, color) in self.colors.iter().enumerate() {
                    let dr = color.r as i32 - rgb.r as i32;
                    let dg = color.g as i32 - rgb.g as i32;
                    let db = color.b as i32 - rgb.b as i32;
                    let dist = dr * dr + dg * dg + db * db;
                    if dist < closest_dist {
                        closest_dist = dist;
                        closest_idx = Some(idx);
                    }
                }

                if let Some(idx) = closest_idx.filter(|_| closest_dist < min_dist_sq) {
                    self.colors[idx].weight += 1.0;
                } else if self.colors.len() < max_colors {
                    self.colors.push(WeightedRgb {
                        r: rgb.r,
                        g: rgb.g,
                        b: rgb.b,
                        weight: 1.0,
                    });
                }
            }
        }

        let total_weight = self
            .colors
            .iter()
            .fold(0.0f32, |sum, color| sum + color.weight);
        if total_weight <= 0.0 {
            let uniform = if self.colors.is_empty() {
                0.0
            } else {
                1.0 / self.colors.len() as f32
            };
            for color in &mut self.colors {
                color.weight = uniform;
            }
        } else {
            for color in &mut self.colors {
                color.weight /= total_weight;
            }
        }
        self.colors
            .sort_by(|left, right| right.weight.total_cmp(&left.weight));
    }

    fn sample(&self, pos_01: f32, flow_phase_01: f32) -> SkydimoRgb {
        if self.colors.is_empty() {
            return SkydimoRgb::default();
        }
        if self.colors.len() == 1 {
            let color = self.colors[0];
            return SkydimoRgb {
                r: color.r,
                g: color.g,
                b: color.b,
            };
        }

        let mut pos = pos_01.clamp(0.0, 1.0) + flow_phase_01.clamp(0.0, 1.0);
        if pos > 1.0 {
            pos -= 1.0;
        }

        let mut cumulative = 0.0f32;
        let mut index0 = 0usize;
        let mut index1 = 1usize;
        let mut local_t = 0.0f32;
        for (idx, color) in self.colors.iter().enumerate() {
            if color.weight <= 0.0 {
                continue;
            }
            let next_cum = cumulative + color.weight;
            if pos <= next_cum || idx == self.colors.len() - 1 {
                index0 = idx;
                index1 = if idx + 1 < self.colors.len() {
                    idx + 1
                } else {
                    0
                };
                local_t = ((pos - cumulative) / color.weight.max(0.0001)).clamp(0.0, 1.0);
                break;
            }
            cumulative = next_cum;
        }

        lerp_weighted(self.colors[index0], self.colors[index1], local_t)
    }
}

fn frame_delta(elapsed_seconds: f64) -> f32 {
    if elapsed_seconds.is_finite() && elapsed_seconds > 0.0 {
        (elapsed_seconds as f32).clamp(0.0, 0.25)
    } else {
        TARGET_FRAME_SECONDS
    }
}

fn lerp_weighted(a: WeightedRgb, b: WeightedRgb, t: f32) -> SkydimoRgb {
    let inv = 1.0 - t;
    SkydimoRgb {
        r: to_u8(a.r as f32 * inv + b.r as f32 * t),
        g: to_u8(a.g as f32 * inv + b.g as f32 * t),
        b: to_u8(a.b as f32 * inv + b.b as f32 * t),
    }
}

fn art_fingerprint(pixels: &[SkydimoRgb]) -> Option<u64> {
    if pixels.is_empty() {
        return None;
    }
    let mut hash = 0x5555_5555u64;
    let step = (pixels.len() / 16).max(1);
    for idx in (0..pixels.len()).step_by(step) {
        let rgb = pixels[idx];
        let packed = ((rgb.r as u64) << 16) | ((rgb.g as u64) << 8) | rgb.b as u64;
        hash = ((hash * 31) + packed) % 0x7FFF_FFFF;
    }
    Some(hash)
}

fn gaussian_blur(
    src: &[SkydimoRgb],
    width: usize,
    height: usize,
    radius: usize,
    kernel: &mut Vec<f32>,
    temp: &mut Vec<SkydimoRgb>,
    out: &mut Vec<SkydimoRgb>,
) {
    let total = width.saturating_mul(height).min(src.len());
    if radius == 0 || total == 0 {
        out.clear();
        out.extend_from_slice(&src[..total]);
        return;
    }

    let kernel_len = radius * 2 + 1;
    kernel.clear();
    kernel.reserve(kernel_len);
    let sigma = radius as f32 / 3.0;
    let sigma2 = 2.0 * sigma * sigma;
    let mut kernel_sum = 0.0f32;
    for idx in 0..kernel_len {
        let x = idx as isize - radius as isize;
        let value = (-((x * x) as f32) / sigma2).exp();
        kernel.push(value);
        kernel_sum += value;
    }
    for value in kernel.iter_mut() {
        *value /= kernel_sum.max(0.0001);
    }

    temp.resize(total, SkydimoRgb::default());
    out.resize(total, SkydimoRgb::default());

    for y in 0..height {
        for x in 0..width {
            let mut r = 0.0f32;
            let mut g = 0.0f32;
            let mut b = 0.0f32;
            for (k, weight) in kernel.iter().enumerate() {
                let offset = k as isize - radius as isize;
                let sx = (x as isize + offset).clamp(0, width as isize - 1) as usize;
                let idx = y.saturating_mul(width).saturating_add(sx);
                if idx >= total {
                    continue;
                }
                let rgb = src[idx];
                r += rgb.r as f32 * weight;
                g += rgb.g as f32 * weight;
                b += rgb.b as f32 * weight;
            }
            let idx = y.saturating_mul(width).saturating_add(x);
            if idx < total {
                temp[idx] = SkydimoRgb {
                    r: to_u8(r),
                    g: to_u8(g),
                    b: to_u8(b),
                };
            }
        }
    }

    for y in 0..height {
        for x in 0..width {
            let mut r = 0.0f32;
            let mut g = 0.0f32;
            let mut b = 0.0f32;
            for (k, weight) in kernel.iter().enumerate() {
                let offset = k as isize - radius as isize;
                let sy = (y as isize + offset).clamp(0, height as isize - 1) as usize;
                let idx = sy.saturating_mul(width).saturating_add(x);
                if idx >= total {
                    continue;
                }
                let rgb = temp[idx];
                r += rgb.r as f32 * weight;
                g += rgb.g as f32 * weight;
                b += rgb.b as f32 * weight;
            }
            let idx = y.saturating_mul(width).saturating_add(x);
            if idx < total {
                out[idx] = SkydimoRgb {
                    r: to_u8(r),
                    g: to_u8(g),
                    b: to_u8(b),
                };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{art_fingerprint, frame_delta};
    use crate::abi::SkydimoRgb;

    #[test]
    fn frame_delta_uses_fallback_for_invalid_elapsed() {
        assert!((frame_delta(f64::NAN) - (1.0 / 60.0)).abs() < 0.0001);
        assert!((frame_delta(0.0) - (1.0 / 60.0)).abs() < 0.0001);
    }

    #[test]
    fn album_art_fingerprint_changes_with_color() {
        let red = [SkydimoRgb { r: 255, g: 0, b: 0 }; 16];
        let blue = [SkydimoRgb { r: 0, g: 0, b: 255 }; 16];
        assert_ne!(art_fingerprint(&red), art_fingerprint(&blue));
    }
}
