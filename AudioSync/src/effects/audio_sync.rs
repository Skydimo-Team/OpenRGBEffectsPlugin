use crate::abi::SkydimoRgb;
use crate::color::{fill_black, hsv_to_rgb, rgb_to_hsv, screen_blend, to_u8};
use crate::host::NativeHost;
use crate::json;

const AUDIO_BINS: usize = 256;
const ALBUM_ART_SIZE: usize = 64;
const FPS_REF: f32 = 60.0;
const SILENT_TIMEOUT: u32 = 120;
const MAX_HISTORY: usize = 1024;

#[derive(Clone, Copy)]
struct AudioSyncConfig {
    hue_shift: f32,
    fade_speed: f32,
    saturation_mode: u32,
    roll_mode: u32,
    avg_size: usize,
    bandpass_min: usize,
    bandpass_max: usize,
    use_album_art: bool,
    silent_color: bool,
    silent_color_value: SkydimoRgb,
    edge_beat: bool,
    edge_beat_hue: f32,
    edge_beat_saturation: f32,
    edge_beat_sensitivity: f32,
}

impl Default for AudioSyncConfig {
    fn default() -> Self {
        Self {
            hue_shift: 0.0,
            fade_speed: 50.0,
            saturation_mode: 0,
            roll_mode: 0,
            avg_size: 8,
            bandpass_min: 0,
            bandpass_max: 255,
            use_album_art: false,
            silent_color: false,
            silent_color_value: SkydimoRgb::default(),
            edge_beat: false,
            edge_beat_hue: 0.0,
            edge_beat_saturation: 0.0,
            edge_beat_sensitivity: 100.0,
        }
    }
}

#[derive(Clone, Copy)]
struct AudioSyncState {
    current_hue: f32,
    current_sat: f32,
    current_val: f32,
    palette_time: f32,
    silent_timer: u32,
}

impl Default for AudioSyncState {
    fn default() -> Self {
        Self {
            current_hue: 0.0,
            current_sat: 255.0,
            current_val: 0.0,
            palette_time: 0.0,
            silent_timer: 0,
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

pub struct AudioSyncEffect {
    host: NativeHost,
    config: AudioSyncConfig,
    state: AudioSyncState,
    width: usize,
    height: usize,
    history: [SkydimoRgb; MAX_HISTORY],
    history_head: usize,
    history_len: usize,
    palette_checksum: Option<u64>,
    palette: Vec<WeightedRgb>,
    album_pixels: Vec<SkydimoRgb>,
    blur_src: Vec<SkydimoRgb>,
    blur_temp: Vec<SkydimoRgb>,
    blur_out: Vec<SkydimoRgb>,
    number_scratch: Vec<f32>,
}

impl AudioSyncEffect {
    pub fn new(host: NativeHost) -> Self {
        Self {
            host,
            config: AudioSyncConfig::default(),
            state: AudioSyncState::default(),
            width: 1,
            height: 1,
            history: [SkydimoRgb::default(); MAX_HISTORY],
            history_head: 0,
            history_len: 0,
            palette_checksum: None,
            palette: Vec::with_capacity(8),
            album_pixels: Vec::new(),
            blur_src: Vec::new(),
            blur_temp: Vec::new(),
            blur_out: Vec::new(),
            number_scratch: Vec::with_capacity(4),
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

    pub fn update_params(&mut self, raw: &str) {
        if let Some(value) = json::number_field(raw, "hueShift") {
            self.config.hue_shift = value.floor().clamp(0.0, 360.0);
        }
        if let Some(value) = json::number_field(raw, "fadeSpeed") {
            self.config.fade_speed = value.clamp(1.0, 99.0);
        }
        if let Some(value) = json::number_field(raw, "saturationMode") {
            self.config.saturation_mode = (value.floor() as i32).clamp(0, 2) as u32;
        }
        if let Some(value) = json::number_field(raw, "rollMode") {
            self.config.roll_mode = (value.floor() as i32).clamp(0, 4) as u32;
        }
        if let Some(value) = json::number_field(raw, "avgSize") {
            self.config.avg_size = (value.floor() as usize).clamp(1, AUDIO_BINS);
        }
        if let Some(value) = json::bool_field(raw, "useAlbumArt") {
            if self.config.use_album_art != value {
                self.palette_checksum = None;
                self.palette.clear();
            }
            self.config.use_album_art = value;
        }
        if let Some(value) = json::bool_field(raw, "silentColor") {
            self.config.silent_color = value;
        }
        if let Some(value) = json::string_field(raw, "silentColorValue") {
            self.config.silent_color_value = parse_hex_rgb_or_black(&value);
        }
        if let Some(value) = json::bool_field(raw, "edgeBeat") {
            self.config.edge_beat = value;
        }
        if let Some(value) = json::number_field(raw, "edgeBeatHue") {
            self.config.edge_beat_hue = value.floor().rem_euclid(360.0);
        }
        if let Some(value) = json::number_field(raw, "edgeBeatSaturation") {
            self.config.edge_beat_saturation = value.clamp(0.0, 255.0);
        }
        if let Some(value) = json::number_field(raw, "edgeBeatSensitivity") {
            self.config.edge_beat_sensitivity = value.clamp(1.0, 200.0);
        }

        if json::number_array_field(raw, "bandpassRange", &mut self.number_scratch) {
            let start = self
                .number_scratch
                .first()
                .copied()
                .unwrap_or(self.config.bandpass_min as f32);
            let end = self
                .number_scratch
                .get(1)
                .copied()
                .unwrap_or(self.config.bandpass_max as f32);
            self.set_bandpass(start, end);
        } else {
            let min = json::number_field(raw, "bandpassMin");
            let max = json::number_field(raw, "bandpassMax");
            if min.is_some() || max.is_some() {
                self.set_bandpass(
                    min.unwrap_or(self.config.bandpass_min as f32),
                    max.unwrap_or(self.config.bandpass_max as f32),
                );
            }
        }
    }

    pub fn tick(&mut self, elapsed_seconds: f64, pixels: &mut [SkydimoRgb]) -> i32 {
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
        if bins.is_empty() {
            fill_black(pixels);
            return 0;
        }

        let fps = if elapsed_seconds > 0.001 {
            (1.0 / elapsed_seconds as f32).clamp(10.0, 240.0)
        } else {
            FPS_REF
        };

        if self.config.use_album_art {
            self.refresh_album_palette();
        }

        let max_idx = self.analyze_audio(bins, fps);
        let frame_color = self.compute_frame_color(max_idx);
        self.push_history(frame_color);

        if self.config.use_album_art {
            self.state.palette_time += self.config.fade_speed / fps;
        }

        let width = self.width.max(1);
        let height = self.height.max(1);
        let width = if self.width == 0 { pixels.len() } else { width };
        let height = if self.height == 0 { 1 } else { height };
        self.render_roll(pixels, width, height, bins);
        0
    }

    fn set_bandpass(&mut self, min: f32, max: f32) {
        let mut min = (min.floor() as i32).clamp(0, (AUDIO_BINS - 1) as i32) as usize;
        let mut max = (max.floor() as i32).clamp(0, (AUDIO_BINS - 1) as i32) as usize;
        if min > max {
            std::mem::swap(&mut min, &mut max);
        }
        self.config.bandpass_min = min;
        self.config.bandpass_max = max;
    }

    fn analyze_audio(&mut self, bins: &[f32], fps: f32) -> Option<usize> {
        let bp_min = self.config.bandpass_min.min(AUDIO_BINS - 1);
        let bp_max = self.config.bandpass_max.min(AUDIO_BINS - 1);
        let mut max_idx = None;
        let mut max_value = 0.0f32;

        for idx in bp_min..=bp_max {
            let value = bins.get(idx).copied().unwrap_or(0.0);
            if value > max_value {
                max_value = value;
                max_idx = Some(idx);
            }
        }

        if let Some(idx) = max_idx {
            let shifted = ((idx as i32 + self.config.hue_shift.floor() as i32).rem_euclid(256))
                as usize;
            let immediate_hue = rainbow_hue(shifted);
            let divisor = (1.0 - self.config.fade_speed / 100.0).max(0.01);
            self.state.current_hue =
                (self.state.current_hue + (immediate_hue - self.state.current_hue) / divisor / fps)
                    .rem_euclid(360.0);

            match self.config.saturation_mode {
                1 => {
                    self.state.current_sat = 255.0 - 255.0 * max_value.powi(3);
                }
                2 => {
                    self.state.current_sat = 0.0;
                }
                _ => {
                    self.state.current_sat = 255.0;
                }
            }
            self.state.current_val = max_value * 255.0;
        } else {
            if self.state.current_sat > 1.0 {
                self.state.current_sat -= 1.0 / (self.state.current_sat * fps);
            }
            if self.state.current_val > 1.0 {
                self.state.current_val -= 1.0 / (self.state.current_val * fps);
            }
        }

        self.state.current_sat = self.state.current_sat.clamp(0.0, 255.0);
        self.state.current_val = self.state.current_val.clamp(0.0, 255.0);
        max_idx
    }

    fn compute_frame_color(&mut self, max_idx: Option<usize>) -> SkydimoRgb {
        let saturation = self.state.current_sat / 255.0;
        let value = self.state.current_val / 255.0;
        let mut rgb = if self.config.use_album_art && !self.palette.is_empty() {
            if let Some(idx) = max_idx {
                let pos = idx as f32 / AUDIO_BINS as f32;
                let flow_phase = (self.state.palette_time / 360.0).rem_euclid(1.0);
                let palette_rgb = sample_palette(&self.palette, pos, flow_phase);
                let (h, _, _) = rgb_to_hsv(palette_rgb);
                hsv_to_rgb(h, saturation, value)
            } else {
                hsv_to_rgb(self.state.current_hue, saturation, value)
            }
        } else {
            hsv_to_rgb(self.state.current_hue, saturation, value)
        };

        if rgb.r == 0 && rgb.g == 0 && rgb.b == 0 && self.config.silent_color {
            self.state.silent_timer = self.state.silent_timer.saturating_add(1).min(SILENT_TIMEOUT);
            let t = self.state.silent_timer as f32 / SILENT_TIMEOUT as f32;
            rgb = SkydimoRgb {
                r: to_u8(self.config.silent_color_value.r as f32 * t),
                g: to_u8(self.config.silent_color_value.g as f32 * t),
                b: to_u8(self.config.silent_color_value.b as f32 * t),
            };
        } else {
            self.state.silent_timer = 0;
        }

        rgb
    }

    fn push_history(&mut self, rgb: SkydimoRgb) {
        if self.history_len == 0 {
            self.history_head = 0;
            self.history[0] = rgb;
            self.history_len = 1;
            return;
        }

        self.history_head = (self.history_head + MAX_HISTORY - 1) % MAX_HISTORY;
        self.history[self.history_head] = rgb;
        self.history_len = (self.history_len + 1).min(MAX_HISTORY);
    }

    fn history_color(&self, offset: usize) -> SkydimoRgb {
        if self.history_len == 0 {
            return SkydimoRgb::default();
        }
        let offset = offset.min(self.history_len - 1);
        self.history[(self.history_head + offset) % MAX_HISTORY]
    }

    fn render_roll(
        &self,
        pixels: &mut [SkydimoRgb],
        width: usize,
        height: usize,
        bins: &[f32],
    ) {
        if self.history_len == 0 {
            fill_black(pixels);
            return;
        }

        let roll = self.config.roll_mode;
        let is_matrix = height > 1 && width > 1;
        let edge_color = self
            .config
            .edge_beat
            .then(|| edge_beat_color(bins, self.config));

        if roll == 1 {
            let base = self.history_color(0);
            if is_matrix {
                self.render_single_matrix_color(pixels, width, height, base, edge_color);
            } else {
                self.render_single_linear_color(pixels, base, edge_color);
            }
            return;
        }

        if !is_matrix {
            self.render_linear_roll(pixels, roll, edge_color);
        } else {
            self.render_matrix_roll(pixels, width, height, roll, edge_color);
        }
    }

    fn render_single_linear_color(
        &self,
        pixels: &mut [SkydimoRgb],
        base: SkydimoRgb,
        edge_color: Option<SkydimoRgb>,
    ) {
        let edge_zone = ((pixels.len() as f32 * 0.1).floor() as usize).max(1);
        let edge_start = pixels.len().saturating_sub(edge_zone);
        for (idx, pixel) in pixels.iter_mut().enumerate() {
            let mut rgb = base;
            if let Some(edge) = edge_color {
                if idx < edge_zone || idx >= edge_start {
                    rgb = screen_blend(rgb, edge);
                }
            }
            *pixel = rgb;
        }
    }

    fn render_single_matrix_color(
        &self,
        pixels: &mut [SkydimoRgb],
        width: usize,
        height: usize,
        base: SkydimoRgb,
        edge_color: Option<SkydimoRgb>,
    ) {
        let width_last = width.saturating_sub(1);
        let height_last = height.saturating_sub(1);
        let mut led = 0usize;
        for row in 0..height {
            for col in 0..width {
                if led >= pixels.len() {
                    return;
                }
                let mut rgb = base;
                if let Some(edge) = edge_color {
                    if col == 0 || col == width_last || row == 0 || row == height_last {
                        rgb = screen_blend(rgb, edge);
                    }
                }
                pixels[led] = rgb;
                led += 1;
            }
        }
        if led < pixels.len() {
            fill_black(&mut pixels[led..]);
        }
    }

    fn render_linear_roll(
        &self,
        pixels: &mut [SkydimoRgb],
        roll: u32,
        edge_color: Option<SkydimoRgb>,
    ) {
        let n = pixels.len();
        let edge_zone = ((n as f32 * 0.1).floor() as usize).max(1);
        let edge_start = n.saturating_sub(edge_zone);
        let center = n.saturating_sub(1) as f32 * 0.5;

        for (idx, pixel) in pixels.iter_mut().enumerate() {
            let history_idx = match roll {
                0 => idx,
                4 => n.saturating_sub(1).saturating_sub(idx),
                3 => nonnegative_floor_to_usize(
                    ((n.saturating_sub(1) as f32) * 1.1 - idx as f32).abs() + 0.5,
                ),
                _ => nonnegative_floor_to_usize((center - idx as f32).abs() + 0.5),
            };
            let mut rgb = self.history_color(history_idx);
            if let Some(edge) = edge_color {
                if idx < edge_zone || idx >= edge_start {
                    rgb = screen_blend(rgb, edge);
                }
            }
            *pixel = rgb;
        }
    }

    fn render_matrix_roll(
        &self,
        pixels: &mut [SkydimoRgb],
        width: usize,
        height: usize,
        roll: u32,
        edge_color: Option<SkydimoRgb>,
    ) {
        let width_last = width.saturating_sub(1);
        let height_last = height.saturating_sub(1);
        let cx = width_last as f32 * 0.5;
        let cy = height_last as f32 * 0.5;
        let mut led = 0usize;

        for row in 0..height {
            for col in 0..width {
                if led >= pixels.len() {
                    return;
                }

                let history_idx = match roll {
                    0 => col,
                    4 => nonnegative_floor_to_usize(width_last as f32 - row as f32 + 0.5),
                    2 => {
                        let dx = cx - col as f32;
                        let dy = cy - row as f32;
                        nonnegative_floor_to_usize((dx * dx + dy * dy).sqrt() + 0.5)
                    }
                    3 => {
                        let ox = width_last as f32 * 1.1;
                        let dx = ox - col as f32;
                        let dy = cy - row as f32;
                        nonnegative_floor_to_usize((dx * dx + dy * dy).sqrt() + 0.5)
                    }
                    _ => 0,
                };

                let mut rgb = self.history_color(history_idx);
                if let Some(edge) = edge_color {
                    if col == 0 || col == width_last || row == 0 || row == height_last {
                        rgb = screen_blend(rgb, edge);
                    }
                }
                pixels[led] = rgb;
                led += 1;
            }
        }
        if led < pixels.len() {
            fill_black(&mut pixels[led..]);
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
}

fn rainbow_hue(index: usize) -> f32 {
    (360.0 - (index.min(AUDIO_BINS - 1) as f32 * (360.0 / AUDIO_BINS as f32))).ceil()
}

fn sample_palette(palette: &[WeightedRgb], pos_01: f32, flow_phase_01: f32) -> SkydimoRgb {
    if palette.is_empty() {
        return SkydimoRgb::default();
    }
    if palette.len() == 1 {
        let color = palette[0];
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
    for (idx, color) in palette.iter().enumerate() {
        if color.weight <= 0.0 {
            continue;
        }
        let next_cum = cumulative + color.weight;
        if pos <= next_cum || idx == palette.len() - 1 {
            index0 = idx;
            index1 = if idx + 1 < palette.len() { idx + 1 } else { 0 };
            local_t = ((pos - cumulative) / color.weight.max(0.0001)).clamp(0.0, 1.0);
            break;
        }
        cumulative = next_cum;
    }

    lerp_weighted_rgb(palette[index0], palette[index1], local_t)
}

fn lerp_weighted_rgb(a: WeightedRgb, b: WeightedRgb, t: f32) -> SkydimoRgb {
    let inv = 1.0 - t;
    SkydimoRgb {
        r: to_u8(a.r as f32 * inv + b.r as f32 * t),
        g: to_u8(a.g as f32 * inv + b.g as f32 * t),
        b: to_u8(a.b as f32 * inv + b.b as f32 * t),
    }
}

fn edge_beat_color(bins: &[f32], config: AudioSyncConfig) -> SkydimoRgb {
    let bass_amp = bins.first().copied().unwrap_or(0.0) + bins.get(8).copied().unwrap_or(0.0);
    let edge_value = (0.01 * config.edge_beat_sensitivity * bass_amp).clamp(0.0, 1.0);
    hsv_to_rgb(
        config.edge_beat_hue.rem_euclid(360.0),
        (config.edge_beat_saturation / 255.0).clamp(0.0, 1.0),
        edge_value,
    )
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

fn parse_hex_rgb_or_black(hex: &str) -> SkydimoRgb {
    let raw = hex.trim().trim_start_matches('#');
    if raw.len() != 6 {
        return SkydimoRgb::default();
    }
    let bytes = raw.as_bytes();
    let Some(r) = hex_byte(bytes[0], bytes[1]) else {
        return SkydimoRgb::default();
    };
    let Some(g) = hex_byte(bytes[2], bytes[3]) else {
        return SkydimoRgb::default();
    };
    let Some(b) = hex_byte(bytes[4], bytes[5]) else {
        return SkydimoRgb::default();
    };
    SkydimoRgb { r, g, b }
}

fn hex_byte(high: u8, low: u8) -> Option<u8> {
    Some((hex_nibble(high)? << 4) | hex_nibble(low)?)
}

fn hex_nibble(ch: u8) -> Option<u8> {
    match ch {
        b'0'..=b'9' => Some(ch - b'0'),
        b'a'..=b'f' => Some(ch - b'a' + 10),
        b'A'..=b'F' => Some(ch - b'A' + 10),
        _ => None,
    }
}

fn nonnegative_floor_to_usize(value: f32) -> usize {
    if !value.is_finite() || value <= 0.0 {
        0
    } else {
        value.floor() as usize
    }
}
