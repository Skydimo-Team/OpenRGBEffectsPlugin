use crate::abi::SkydimoRgb;
use crate::color::{
    fill_black, hsv_to_rgb, interpolate_black, rgb_to_hsv, screen_blend, to_u8, white,
};
use crate::host::NativeHost;
use crate::json;
use crate::rng::FastRng;

const AUDIO_BINS: usize = 256;
const ALBUM_ART_SIZE: usize = 64;

#[derive(Clone, Copy)]
struct AudioPartyConfig {
    speed: f32,
    color_speed: f32,
    divisions: f32,
    avg_size: usize,
    effect_threshold: f32,
    motion_zone_end: usize,
    color_zone_end: usize,
    use_album_art: bool,
}

impl Default for AudioPartyConfig {
    fn default() -> Self {
        Self {
            speed: 50.0,
            color_speed: 25.0,
            divisions: 4.0,
            avg_size: 8,
            effect_threshold: 20.0,
            motion_zone_end: 64,
            color_zone_end: 192,
            use_album_art: false,
        }
    }
}

#[derive(Clone, Copy)]
struct AudioPartyState {
    x_shift: f32,
    color_shift: f32,
    palette_time: f32,
    effect_progress: f32,
    effect_idx: u32,
}

impl Default for AudioPartyState {
    fn default() -> Self {
        Self {
            x_shift: 0.0,
            color_shift: 0.0,
            palette_time: 0.0,
            effect_progress: 1.0,
            effect_idx: 0,
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

pub struct AudioPartyEffect {
    host: NativeHost,
    config: AudioPartyConfig,
    state: AudioPartyState,
    width: usize,
    height: usize,
    rng: FastRng,
    palette_checksum: Option<u64>,
    palette: Vec<WeightedRgb>,
    album_pixels: Vec<SkydimoRgb>,
    blur_src: Vec<SkydimoRgb>,
    blur_temp: Vec<SkydimoRgb>,
    blur_out: Vec<SkydimoRgb>,
}

impl AudioPartyEffect {
    pub fn new(host: NativeHost) -> Self {
        Self {
            host,
            config: AudioPartyConfig::default(),
            state: AudioPartyState::default(),
            width: 1,
            height: 1,
            rng: FastRng::new(0xA9D1_0F33),
            palette_checksum: None,
            palette: Vec::with_capacity(8),
            album_pixels: Vec::new(),
            blur_src: Vec::new(),
            blur_temp: Vec::new(),
            blur_out: Vec::new(),
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
        if let Some(color_speed) = json::number_field(json, "colorSpeed") {
            self.config.color_speed = color_speed.clamp(1.0, 50.0);
        }
        if let Some(divisions) = json::number_field(json, "divisions") {
            self.config.divisions = divisions.clamp(2.0, 16.0);
        }
        if let Some(avg_size) = json::number_field(json, "avgSize") {
            self.config.avg_size = (avg_size.floor() as usize).clamp(1, AUDIO_BINS);
        }
        if let Some(threshold) = json::number_field(json, "effectThreshold") {
            self.config.effect_threshold = threshold.clamp(5.0, 80.0);
        }
        if let Some(end) = json::number_field(json, "motionZoneEnd") {
            self.config.motion_zone_end = (end.floor() as usize).clamp(1, AUDIO_BINS);
        }
        if let Some(end) = json::number_field(json, "colorZoneEnd") {
            self.config.color_zone_end = (end.floor() as usize).clamp(1, AUDIO_BINS);
        }
        if let Some(use_album_art) = json::bool_field(json, "useAlbumArt") {
            if self.config.use_album_art != use_album_art {
                self.palette_checksum = None;
                self.palette.clear();
            }
            self.config.use_album_art = use_album_art;
        }
    }

    pub fn tick(&mut self, _elapsed_seconds: f64, pixels: &mut [SkydimoRgb]) -> i32 {
        if pixels.is_empty() {
            return 0;
        }

        let mut bins_buf = [0.0f32; AUDIO_BINS];
        let Some(bin_count) = self
            .host
            .capture_audio_into(self.config.avg_size, &mut bins_buf)
        else {
            fill_black(pixels);
            return 0;
        };
        let bins = &bins_buf[..bin_count.min(AUDIO_BINS)];

        let delta = self.config.speed / 60.0;
        let color_delta = self.config.color_speed / 60.0;
        self.process_zones(bins, delta, color_delta);

        if self.config.use_album_art {
            self.refresh_album_palette();
            self.state.palette_time += self.config.speed / 60.0;
        }

        let width = self.width.max(1);
        let height = self.height.max(1);
        if height == 1 || width == 1 {
            self.render_linear(pixels);
        } else {
            self.render_matrix(pixels, width, height);
        }
        0
    }

    fn process_zones(&mut self, bins: &[f32], delta: f32, color_delta: f32) {
        let avg = self.config.avg_size.clamp(1, AUDIO_BINS);
        let motion_end = self.config.motion_zone_end.min(AUDIO_BINS);
        let color_end = self.config.color_zone_end.min(AUDIO_BINS);
        let threshold = self.config.effect_threshold / 100.0;

        let mut c = 0usize;
        let mut i = 1usize;
        while i <= motion_end {
            let cur = bin_1_based(bins, i);
            let next = bin_1_based(bins, i + avg);
            if cur > next {
                let mult = if c.is_multiple_of(2) { 1.0 } else { -1.0 };
                self.state.x_shift += cur * delta * mult;
                break;
            }
            c += 1;
            i += avg;
        }

        c = 0;
        i = motion_end.saturating_add(1);
        while i <= color_end {
            let cur = bin_1_based(bins, i);
            let next = bin_1_based(bins, i + avg);
            if cur > next {
                let mult = if c.is_multiple_of(2) { 1.0 } else { -1.0 };
                self.state.color_shift += cur * color_delta * mult;
                break;
            }
            c += 1;
            i += avg;
        }

        i = color_end.saturating_add(1);
        while i <= AUDIO_BINS {
            let cur = bin_1_based(bins, i);
            let next = bin_1_based(bins, i + avg);
            if cur > threshold && cur > next && self.state.effect_progress >= 1.0 {
                self.state.effect_idx = self.rng.next_range(7);
                self.state.effect_progress = 0.0;
                break;
            }
            i += avg;
        }

        if self.state.effect_progress < 1.0 {
            self.state.effect_progress += 0.1 * delta;
        }
    }

    fn refresh_album_palette(&mut self) {
        let mut pixels = std::mem::take(&mut self.album_pixels);
        let captured =
            self.host
                .capture_album_art_into(ALBUM_ART_SIZE, ALBUM_ART_SIZE, &mut pixels);
        let Some((width, height)) = captured else {
            self.album_pixels = pixels;
            return;
        };

        let total = width.saturating_mul(height).min(pixels.len());
        if total == 0 {
            self.album_pixels = pixels;
            return;
        }

        let checksum = art_fingerprint(&pixels[..total]);
        if checksum.is_some() && checksum == self.palette_checksum && !self.palette.is_empty() {
            self.album_pixels = pixels;
            return;
        }

        self.extract_palette(&pixels[..total], width.max(1), height.max(1), checksum, 8);
        self.album_pixels = pixels;
    }

    fn extract_palette(
        &mut self,
        pixels: &[SkydimoRgb],
        width: usize,
        height: usize,
        checksum: Option<u64>,
        max_colors: usize,
    ) {
        self.palette.clear();
        if pixels.is_empty() || width == 0 || height == 0 {
            self.palette_checksum = checksum;
            return;
        }

        let total = width.saturating_mul(height).min(pixels.len());
        self.blur_src.clear();
        self.blur_src.extend_from_slice(&pixels[..total]);
        gaussian_blur(
            &self.blur_src,
            width,
            height,
            8,
            &mut self.blur_temp,
            &mut self.blur_out,
        );

        let blurred = if self.blur_out.is_empty() {
            self.blur_src.as_slice()
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
                if idx >= blurred.len() {
                    continue;
                }

                let rgb = blurred[idx];
                if rgb.r.max(rgb.g).max(rgb.b) < 24 {
                    continue;
                }

                let mut closest_idx = None;
                let mut closest_dist = i32::MAX;
                for (idx, color) in self.palette.iter().enumerate() {
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
                    self.palette[idx].weight += 1.0;
                } else if self.palette.len() < max_colors {
                    self.palette.push(WeightedRgb {
                        r: rgb.r,
                        g: rgb.g,
                        b: rgb.b,
                        weight: 1.0,
                    });
                }
            }
        }

        let total_weight = self
            .palette
            .iter()
            .fold(0.0f32, |acc, color| acc + color.weight);
        if !self.palette.is_empty() {
            if total_weight <= 0.0 {
                let uniform = 1.0 / self.palette.len() as f32;
                for color in &mut self.palette {
                    color.weight = uniform;
                }
            } else {
                for color in &mut self.palette {
                    color.weight /= total_weight;
                }
            }
            self.palette
                .sort_by(|a, b| b.weight.total_cmp(&a.weight));
        }
        self.palette_checksum = checksum;
    }

    fn render_linear(&mut self, pixels: &mut [SkydimoRgb]) {
        let width = pixels.len().max(1) as f32;
        for (idx, pixel) in pixels.iter_mut().enumerate() {
            *pixel = self.get_color(idx as f32, 0.0, width, 1.0);
        }
    }

    fn render_matrix(&mut self, pixels: &mut [SkydimoRgb], width: usize, height: usize) {
        let mut idx = 0usize;
        for y in 0..height {
            for x in 0..width {
                if idx >= pixels.len() {
                    return;
                }
                pixels[idx] = self.get_color(x as f32, y as f32, width as f32, height as f32);
                idx += 1;
            }
        }
        if idx < pixels.len() {
            fill_black(&mut pixels[idx..]);
        }
    }

    fn get_color(&mut self, pos_x: f32, pos_y: f32, width: f32, height: f32) -> SkydimoRgb {
        let nx = if width > 0.0 { pos_x / width } else { 0.0 };
        let wave = 0.5
            * (1.0
                + (nx * self.config.divisions * std::f32::consts::PI + self.state.x_shift).sin());

        let mut base = if self.config.use_album_art && !self.palette.is_empty() {
            let mut pos = if height > 0.0 { pos_y / height } else { 0.0 };
            let shift = (self.state.color_shift / 360.0).rem_euclid(1.0);
            pos = (pos + shift).rem_euclid(1.0);
            let flow_phase = (self.state.palette_time / 360.0).rem_euclid(1.0);
            let rgb = self.sample_palette(pos, flow_phase);
            let (h, s, _) = rgb_to_hsv(rgb);
            hsv_to_rgb(h, s, 1.0)
        } else {
            let ny = if height > 0.0 { pos_y / height } else { 0.0 };
            let hue = (180.0 + (ny + self.state.color_shift).sin() * 180.0).rem_euclid(360.0);
            hsv_to_rgb(hue, 1.0, 1.0)
        };

        let fx = self.effect_color(pos_x, pos_y, width, height);
        base = screen_blend(base, fx);
        interpolate_black(base, wave)
    }

    fn sample_palette(&self, pos_01: f32, flow_phase_01: f32) -> SkydimoRgb {
        if self.palette.is_empty() {
            return SkydimoRgb::default();
        }
        if self.palette.len() == 1 {
            let color = self.palette[0];
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
        for (idx, color) in self.palette.iter().enumerate() {
            if color.weight <= 0.0 {
                continue;
            }
            let next_cum = cumulative + color.weight;
            if pos <= next_cum || idx == self.palette.len() - 1 {
                index0 = idx;
                index1 = if idx + 1 < self.palette.len() { idx + 1 } else { 0 };
                local_t = ((pos - cumulative) / color.weight.max(0.0001)).clamp(0.0, 1.0);
                break;
            }
            cumulative = next_cum;
        }

        lerp_color(self.palette[index0], self.palette[index1], local_t)
    }

    fn effect_color(&mut self, pos_x: f32, pos_y: f32, width: f32, height: f32) -> SkydimoRgb {
        if self.state.effect_progress >= 1.0 {
            return SkydimoRgb::default();
        }

        match self.state.effect_idx {
            0 if (pos_x - self.state.effect_progress * width).abs() <= 1.0 => white(),
            1 if (pos_x - (width - self.state.effect_progress * width)).abs() <= 1.0 => white(),
            2 if (pos_y - self.state.effect_progress * height).abs() <= 1.0 => white(),
            3 if (pos_y - (height - self.state.effect_progress * height)).abs() <= 1.0 => white(),
            4 => SkydimoRgb {
                r: self.rng.next_u8(),
                g: self.rng.next_u8(),
                b: self.rng.next_u8(),
            },
            5 => {
                let v = ((self.rng.next_unit() * 255.0) + 0.5).floor() as u8;
                SkydimoRgb { r: v, g: v, b: v }
            }
            6 => {
                let v = ((1.0 - self.state.effect_progress) * 255.0)
                    .round()
                    .clamp(0.0, 255.0) as u8;
                SkydimoRgb { r: v, g: v, b: v }
            }
            _ => SkydimoRgb::default(),
        }
    }
}

fn bin_1_based(bins: &[f32], index: usize) -> f32 {
    if index == 0 {
        0.0
    } else {
        bins.get(index - 1).copied().unwrap_or(0.0)
    }
}

fn lerp_color(a: WeightedRgb, b: WeightedRgb, t: f32) -> SkydimoRgb {
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
    temp: &mut Vec<SkydimoRgb>,
    out: &mut Vec<SkydimoRgb>,
) {
    let total = width.saturating_mul(height).min(src.len());
    if radius == 0 || total == 0 {
        out.clear();
        out.extend_from_slice(&src[..total]);
        return;
    }

    let sigma = radius as f32 / 3.0;
    let sigma2 = 2.0 * sigma * sigma;
    let kernel_len = radius * 2 + 1;
    let mut kernel = Vec::with_capacity(kernel_len);
    let mut kernel_sum = 0.0f32;
    for idx in 0..kernel_len {
        let x = idx as isize - radius as isize;
        let value = (-((x * x) as f32) / sigma2).exp();
        kernel.push(value);
        kernel_sum += value;
    }
    for value in &mut kernel {
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
                let idx = y * width + sx;
                if idx >= total {
                    continue;
                }
                let rgb = src[idx];
                r += rgb.r as f32 * weight;
                g += rgb.g as f32 * weight;
                b += rgb.b as f32 * weight;
            }
            let idx = y * width + x;
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
                let idx = sy * width + x;
                if idx >= total {
                    continue;
                }
                let rgb = temp[idx];
                r += rgb.r as f32 * weight;
                g += rgb.g as f32 * weight;
                b += rgb.b as f32 * weight;
            }
            let idx = y * width + x;
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
