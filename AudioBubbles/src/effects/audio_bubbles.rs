use crate::abi::SkydimoRgb;
use crate::color::{fill_black, hex_to_rgb, hsv_to_rgb, rgb_to_hsv, screen_blend_channel, to_u8};
use crate::host::NativeHost;
use crate::json;
use crate::rng::FastRng;

const AUDIO_BINS: usize = 256;
const FPS: f32 = 60.0;

const PRESETS: &[&[&str]] = &[
    &[
        "#FF0000", "#FF00E6", "#0000FF", "#00B3FF", "#00FF51", "#EAFF00", "#FFB300",
        "#FF0000",
    ],
    &["#14E81E", "#00EA8D", "#017ED5", "#B53DFF", "#8D00C4", "#14E81E"],
    &["#00007F", "#0000FF", "#00FFFF", "#00AAFF", "#00007F"],
    &["#FE00C5", "#00C5FF", "#00C5FF", "#FE00C5"],
    &["#FEE000", "#FE00FE", "#FE00FE", "#FEE000"],
    &["#FF5500", "#000000", "#000000", "#000000", "#FF5500"],
    &["#FF2100", "#AA00FF", "#AA00FF", "#FF2100", "#FF2100", "#FF2100"],
    &["#03FFFA", "#55007F", "#55007F", "#03FFFA"],
    &["#FF0000", "#0000FF", "#0000FF", "#FF0000", "#FF0000"],
    &["#00FF00", "#0032FF", "#0032FF", "#00FF00", "#00FF00"],
    &[
        "#FF2100", "#AB006D", "#C01C52", "#D53737", "#EA531B", "#FF6E00", "#FF0000",
        "#FF2100",
    ],
    &["#FF71CE", "#B967FF", "#01CDFE", "#05FFA1", "#FFFB96", "#FF71CE"],
];

#[derive(Clone)]
struct AudioBubblesConfig {
    avg_size: usize,
    preset: usize,
    colors: Vec<SkydimoRgb>,
    spawn_mode: u32,
    trigger: f32,
    max_bubbles: usize,
    speed_mult: f32,
    max_expansion: f32,
    bubbles_thickness: f32,
}

impl AudioBubblesConfig {
    fn new() -> Self {
        Self {
            avg_size: 8,
            preset: 0,
            colors: preset_colors(0),
            spawn_mode: 0,
            trigger: 30.0,
            max_bubbles: 8,
            speed_mult: 1.0,
            max_expansion: 100.0,
            bubbles_thickness: 10.0,
        }
    }
}

#[derive(Clone, Copy)]
struct Bubble {
    freq_id: usize,
    amp: f32,
    cx: f32,
    cy: f32,
    progress: f32,
    speed: f32,
}

pub struct AudioBubblesEffect {
    host: NativeHost,
    config: AudioBubblesConfig,
    width: usize,
    height: usize,
    rng: FastRng,
    bubbles: Vec<Bubble>,
    indexed_fft: Vec<(f32, usize)>,
    colors_scratch: Vec<String>,
}

impl AudioBubblesEffect {
    pub fn new(host: NativeHost) -> Self {
        Self {
            host,
            config: AudioBubblesConfig::new(),
            width: 1,
            height: 1,
            rng: FastRng::new(0x0B0B_B1E5),
            bubbles: Vec::with_capacity(32),
            indexed_fft: Vec::with_capacity(AUDIO_BINS),
            colors_scratch: Vec::with_capacity(16),
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
        if let Some(preset) = json::number_field(json, "preset") {
            let preset = (preset.round() as isize).clamp(0, PRESETS.len() as isize - 1) as usize;
            self.config.preset = preset;
            self.config.colors = preset_colors(preset);
        }

        if json::string_array_field(json, "colors", &mut self.colors_scratch) {
            let mut colors = Vec::with_capacity(self.colors_scratch.len().max(1));
            for value in &self.colors_scratch {
                colors.push(hex_to_rgb(value));
            }
            if !colors.is_empty() {
                self.config.colors = colors;
            }
        }

        if let Some(spawn_mode) = json::number_field(json, "spawnMode") {
            self.config.spawn_mode = (spawn_mode.round() as i32).clamp(0, 3) as u32;
        }
        if let Some(trigger) = json::number_field(json, "trigger") {
            self.config.trigger = trigger.clamp(1.0, 100.0);
        }
        if let Some(max_bubbles) = json::number_field(json, "max_bubbles") {
            self.config.max_bubbles = (max_bubbles.round() as usize).clamp(1, 32);
            if self.bubbles.len() > self.config.max_bubbles {
                self.bubbles.truncate(self.config.max_bubbles);
            }
        }
        if let Some(speed_mult) = json::number_field(json, "speed_mult") {
            self.config.speed_mult = speed_mult.clamp(1.0, 1000.0);
        }
        if let Some(max_expansion) = json::number_field(json, "max_expansion") {
            self.config.max_expansion = max_expansion.clamp(1.0, 1000.0);
        }
        if let Some(thickness) = json::number_field(json, "bubbles_thickness") {
            self.config.bubbles_thickness = thickness.clamp(1.0, 200.0);
        }
        if let Some(avg_size) = json::number_field(json, "avgSize") {
            self.config.avg_size = (avg_size.round() as usize).clamp(1, AUDIO_BINS);
        }
    }

    pub fn tick(&mut self, _elapsed_seconds: f64, pixels: &mut [SkydimoRgb]) -> i32 {
        if pixels.is_empty() {
            return 0;
        }

        let mut bins_buf = [0.0f32; AUDIO_BINS];
        let bin_count = self
            .host
            .capture_audio_into(self.config.avg_size, &mut bins_buf)
            .unwrap_or(AUDIO_BINS)
            .min(AUDIO_BINS);
        let bins = &bins_buf[..bin_count];

        let width = self.width.max(1);
        let height = self.height.max(1);
        if height == 1 || width == 1 {
            self.render_linear(pixels);
        } else {
            self.render_matrix(pixels, width, height);
        }

        self.expand_bubbles();
        self.trigger_bubbles(bins);
        self.cleanup_bubbles();
        0
    }

    fn render_linear(&self, pixels: &mut [SkydimoRgb]) {
        let width = pixels.len().max(1) as f32;
        for (idx, pixel) in pixels.iter_mut().enumerate() {
            *pixel = self.get_color(idx as f32, 0.0, width, 1.0);
        }
    }

    fn render_matrix(&self, pixels: &mut [SkydimoRgb], width: usize, height: usize) {
        let mut led = 0usize;
        for y in 0..height {
            for x in 0..width {
                if led >= pixels.len() {
                    return;
                }
                pixels[led] = self.get_color(x as f32, y as f32, width as f32, height as f32);
                led += 1;
            }
        }
        if led < pixels.len() {
            fill_black(&mut pixels[led..]);
        }
    }

    fn get_color(&self, x: f32, y: f32, width: f32, height: f32) -> SkydimoRgb {
        let mut r = 0u8;
        let mut g = 0u8;
        let mut b = 0u8;

        for bubble in &self.bubbles {
            let dx = width * bubble.cx - x;
            let dy = height * bubble.cy - y;
            let distance = (dx * dx + dy * dy).sqrt();
            let denom = 0.1 * self.config.bubbles_thickness * bubble.amp;
            if denom <= 0.0 {
                continue;
            }

            let shallow = (distance - bubble.progress).abs() / denom;
            let value = if shallow <= 1e-9 {
                255.0
            } else {
                (255.0 / (shallow * shallow * shallow)).min(255.0)
            };
            let max_progress = self.config.max_expansion * bubble.amp;
            let progress_norm = if max_progress > 0.0 {
                (bubble.progress / max_progress).min(1.0)
            } else {
                1.0
            };

            if value <= 0.0 || progress_norm <= 0.0 {
                continue;
            }

            let gradient = self.sample_gradient(bubble.freq_id);
            let (h, s, _) = rgb_to_hsv(gradient);
            let v = (value / 255.0) * (1.0 - progress_norm).sqrt();
            let bubble_color = hsv_to_rgb(h, s, v);

            r = screen_blend_channel(r, bubble_color.r);
            g = screen_blend_channel(g, bubble_color.g);
            b = screen_blend_channel(b, bubble_color.b);
        }

        SkydimoRgb { r, g, b }
    }

    fn sample_gradient(&self, freq_id: usize) -> SkydimoRgb {
        let colors = &self.config.colors;
        if colors.is_empty() {
            return SkydimoRgb {
                r: 255,
                g: 255,
                b: 255,
            };
        }
        if colors.len() == 1 {
            return colors[0];
        }

        let t = (freq_id as f32 / 255.0).clamp(0.0, 1.0);
        let segment = t * (colors.len() - 1) as f32;
        let i0 = segment.floor() as usize;
        if i0 >= colors.len() - 1 {
            return colors[colors.len() - 1];
        }

        let local_t = segment - i0 as f32;
        let c0 = colors[i0];
        let c1 = colors[i0 + 1];
        let inv = 1.0 - local_t;
        SkydimoRgb {
            r: to_u8(c0.r as f32 * inv + c1.r as f32 * local_t),
            g: to_u8(c0.g as f32 * inv + c1.g as f32 * local_t),
            b: to_u8(c0.b as f32 * inv + c1.b as f32 * local_t),
        }
    }

    fn expand_bubbles(&mut self) {
        let speed_mult = self.config.speed_mult;
        for bubble in &mut self.bubbles {
            bubble.progress += 0.1 * speed_mult * bubble.speed / FPS;
        }
    }

    fn trigger_bubbles(&mut self, bins: &[f32]) {
        if self.bubbles.len() >= self.config.max_bubbles {
            return;
        }

        let trigger_value = 0.01 * self.config.trigger;
        let avg_size = self.config.avg_size.clamp(1, AUDIO_BINS);
        self.indexed_fft.clear();

        let mut idx = 0usize;
        while idx < AUDIO_BINS {
            let amp = bins.get(idx).copied().unwrap_or(0.0);
            if amp >= trigger_value {
                self.indexed_fft.push((amp, idx));
            }
            idx += avg_size;
        }

        self.indexed_fft
            .sort_by(|left, right| right.0.total_cmp(&left.0));

        for idx in 0..self.indexed_fft.len() {
            if self.bubbles.len() >= self.config.max_bubbles {
                break;
            }
            let (amp, freq_id) = self.indexed_fft[idx];
            if self.freq_occupied(freq_id) {
                continue;
            }
            self.init_bubble(freq_id, amp);
        }
    }

    fn init_bubble(&mut self, freq_id: usize, amp: f32) {
        let amp = amp.clamp(0.2, 0.8);
        let freq_pos = freq_id as f32 / AUDIO_BINS as f32;
        let (cx, cy) = match self.config.spawn_mode {
            1 => (self.rng.next_unit(), 1.0 - freq_pos),
            2 => (freq_pos, self.rng.next_unit()),
            3 => (0.5, 0.5),
            _ => (self.rng.next_unit(), self.rng.next_unit()),
        };

        self.bubbles.push(Bubble {
            freq_id,
            amp,
            cx,
            cy,
            progress: 0.0,
            speed: 1.0 / amp,
        });
    }

    fn cleanup_bubbles(&mut self) {
        let max_expansion = self.config.max_expansion;
        self.bubbles
            .retain(|bubble| bubble.progress < max_expansion * bubble.amp);
    }

    fn freq_occupied(&self, freq_id: usize) -> bool {
        self.bubbles.iter().any(|bubble| bubble.freq_id == freq_id)
    }
}

fn preset_colors(index: usize) -> Vec<SkydimoRgb> {
    let preset = PRESETS.get(index).copied().unwrap_or(PRESETS[0]);
    preset.iter().map(|hex| hex_to_rgb(hex)).collect()
}
