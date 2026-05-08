use crate::abi::SkydimoRgb;
use crate::color::fill_black;
use crate::host::NativeHost;
use crate::json;

const AUDIO_BINS: usize = 256;
const VIS_W: usize = 256;
const VIS_H: usize = 64;
const TOTAL_PX: usize = VIS_W * VIS_H;
const FPS: f32 = 60.0;
const PHASE_THIRD: f32 = std::f32::consts::TAU / 3.0;

const ROW_BAR_GRAPH: usize = 0;
const ROW_SINGLE_COLOR: usize = 1;
const ROW_SPECTRO_TOP: usize = 2;

const PAT_SOLID_BLACK: i32 = 0;
const PAT_SOLID_WHITE: i32 = 1;
const PAT_SOLID_RED: i32 = 2;
const PAT_SOLID_ORANGE: i32 = 3;
const PAT_SOLID_YELLOW: i32 = 4;
const PAT_SOLID_GREEN: i32 = 5;
const PAT_SOLID_CYAN: i32 = 6;
const PAT_SOLID_BLUE: i32 = 7;
const PAT_SOLID_PURPLE: i32 = 8;
const PAT_SOLID_ELECTRIC_AQUAMARINE: i32 = 9;
const PAT_STATIC_RED_BLUE: i32 = 10;
const PAT_STATIC_CYAN_ORANGE: i32 = 11;
const PAT_STATIC_CYAN_PURPLE: i32 = 12;
const PAT_STATIC_CYAN_ELECTRIC_AQUAMARINE: i32 = 13;
const PAT_STATIC_GREEN_YELLOW_RED: i32 = 14;
const PAT_STATIC_GREEN_WHITE_RED: i32 = 15;
const PAT_STATIC_BLUE_CYAN_WHITE: i32 = 16;
const PAT_STATIC_RED_WHITE_BLUE: i32 = 17;
const PAT_STATIC_RAINBOW: i32 = 18;
const PAT_STATIC_RAINBOW_INVERSE: i32 = 19;
const PAT_ANIM_RAINBOW_SINUSOIDAL: i32 = 20;
const PAT_ANIM_RAINBOW_HSV: i32 = 21;
const PAT_ANIM_COLOR_WHEEL: i32 = 22;
const PAT_ANIM_COLOR_WHEEL_2: i32 = 23;
const PAT_ANIM_SPECTRUM_CYCLE: i32 = 24;
const PAT_ANIM_SINUSOIDAL_CYCLE: i32 = 25;

const SC_BLACK: i32 = 0;
const SC_WHITE: i32 = 1;
const SC_RED: i32 = 2;
const SC_ORANGE: i32 = 3;
const SC_YELLOW: i32 = 4;
const SC_GREEN: i32 = 5;
const SC_CYAN: i32 = 6;
const SC_BLUE: i32 = 7;
const SC_PURPLE: i32 = 8;
const SC_ELECTRIC_AQUAMARINE: i32 = 9;
const SC_BACKGROUND: i32 = 10;
const SC_FOLLOW_BACKGROUND: i32 = 11;
const SC_FOLLOW_FOREGROUND: i32 = 12;

const C_BLACK: u32 = 0x000000;
const C_WHITE: u32 = 0xFFFFFF;
const C_RED: u32 = 0xFF0000;
const C_ORANGE: u32 = 0xFFA500;
const C_YELLOW: u32 = 0xFFFF00;
const C_LIME: u32 = 0x00FF00;
const C_CYAN: u32 = 0x00FFFF;
const C_BLUE: u32 = 0x0000FF;
const C_PURPLE: u32 = 0x800080;
const C_ELEC_UL: u32 = 0x4000FF;

const RED_BLUE: [u32; 2] = [C_RED, C_BLUE];
const CYAN_ORANGE: [u32; 2] = [C_CYAN, C_ORANGE];
const CYAN_PURPLE: [u32; 2] = [C_CYAN, C_PURPLE];
const CYAN_ELEC: [u32; 2] = [C_CYAN, C_ELEC_UL];
const GREEN_YELLOW_RED: [u32; 3] = [C_LIME, C_YELLOW, C_RED];
const GREEN_WHITE_RED: [u32; 3] = [C_LIME, C_WHITE, C_RED];
const BLUE_CYAN_WHITE: [u32; 3] = [C_BLUE, C_CYAN, C_WHITE];
const RED_WHITE_BLUE: [u32; 3] = [C_RED, C_WHITE, C_BLUE];
const RAINBOW: [u32; 6] = [C_RED, C_YELLOW, C_LIME, C_CYAN, C_BLUE, C_PURPLE];
const RAINBOW_INV: [u32; 6] = [C_PURPLE, C_BLUE, C_CYAN, C_LIME, C_YELLOW, C_RED];

#[derive(Clone, Copy)]
struct AudioVisualizerConfig {
    bg_mode: i32,
    fg_mode: i32,
    single_color_mode: i32,
    bg_brightness: f32,
    anim_speed: f32,
    bg_timeout: f32,
    reactive_bg: bool,
    silent_bg: bool,
    avg_size: usize,
}

impl Default for AudioVisualizerConfig {
    fn default() -> Self {
        Self {
            bg_mode: PAT_ANIM_RAINBOW_SINUSOIDAL,
            fg_mode: PAT_STATIC_GREEN_YELLOW_RED,
            single_color_mode: SC_FOLLOW_FOREGROUND,
            bg_brightness: 100.0,
            anim_speed: 100.0,
            bg_timeout: 120.0,
            reactive_bg: false,
            silent_bg: false,
            avg_size: 8,
        }
    }
}

#[derive(Clone, Copy)]
enum BackgroundCompose {
    Static,
    Scaled(f32),
    Black,
}

pub struct AudioVisualizerEffect {
    host: NativeHost,
    config: AudioVisualizerConfig,
    width: usize,
    height: usize,
    bkgd_step: f32,
    background_timer: f32,
    bg: Vec<SkydimoRgb>,
    fg: Vec<SkydimoRgb>,
    out: Vec<SkydimoRgb>,
    linear_map: Vec<usize>,
    linear_len: usize,
    matrix_x: Vec<usize>,
    matrix_x_len: usize,
    matrix_y: Vec<usize>,
    matrix_y_len: usize,
    wheel_32: Vec<f32>,
    wheel_64: Vec<f32>,
}

impl AudioVisualizerEffect {
    pub fn new(host: NativeHost) -> Self {
        Self {
            host,
            config: AudioVisualizerConfig::default(),
            width: 1,
            height: 1,
            bkgd_step: 0.0,
            background_timer: 0.0,
            bg: vec![SkydimoRgb::default(); TOTAL_PX],
            fg: vec![SkydimoRgb::default(); TOTAL_PX],
            out: vec![SkydimoRgb::default(); TOTAL_PX],
            linear_map: Vec::new(),
            linear_len: 0,
            matrix_x: Vec::new(),
            matrix_x_len: 0,
            matrix_y: Vec::new(),
            matrix_y_len: 0,
            wheel_32: build_wheel_angles(32.0),
            wheel_64: build_wheel_angles(64.0),
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
        if let Some(value) = json::number_field(json, "bg_mode") {
            self.config.bg_mode = clamp_mode(value);
        }
        if let Some(value) = json::number_field(json, "fg_mode") {
            self.config.fg_mode = clamp_mode(value);
        }
        if let Some(value) = json::number_field(json, "single_color_mode") {
            self.config.single_color_mode =
                (value.round() as i32).clamp(SC_BLACK, SC_FOLLOW_FOREGROUND);
        }
        if let Some(value) = json::number_field(json, "bg_brightness") {
            self.config.bg_brightness = value.clamp(0.0, 100.0);
        }
        if let Some(value) = json::number_field(json, "anim_speed") {
            self.config.anim_speed = value.clamp(0.0, 500.0);
        }
        if let Some(value) = json::number_field(json, "bg_timeout") {
            self.config.bg_timeout = value.clamp(0.0, 600.0);
        }
        if let Some(value) = json::bool_field(json, "reactive_bg") {
            self.config.reactive_bg = value;
        }
        if let Some(value) = json::bool_field(json, "silent_bg") {
            self.config.silent_bg = value;
        }
        if let Some(value) = json::number_field(json, "avg_size") {
            self.config.avg_size = (value.floor() as usize).clamp(1, AUDIO_BINS);
        }

        if self.config.reactive_bg && self.config.silent_bg {
            self.config.silent_bg = false;
        }
    }

    pub fn tick(&mut self, _elapsed_seconds: f64, pixels: &mut [SkydimoRgb]) -> i32 {
        if pixels.is_empty() {
            return 0;
        }

        let mut bins_buf = [0.0f32; AUDIO_BINS];
        let Some(bin_count) = self.host.capture_audio_into(self.config.avg_size, &mut bins_buf)
        else {
            fill_black(pixels);
            return 0;
        };
        let bins = &bins_buf[..bin_count.min(AUDIO_BINS)];

        if self.bkgd_step >= 360.0 {
            self.bkgd_step = 0.0;
        } else if self.bkgd_step < 0.0 {
            self.bkgd_step = 360.0;
        }

        self.draw_pattern(
            self.config.bg_mode,
            self.config.bg_brightness,
            self.bkgd_step,
            Target::Background,
        );
        self.draw_pattern(
            self.config.fg_mode,
            100.0,
            self.bkgd_step,
            Target::Foreground,
        );

        let mut brightness = bin(bins, 5);
        self.background_timer += 60.0 / FPS;

        let bg_timeout = self.config.bg_timeout;
        if bg_timeout > 0.0 {
            for i in 0..128 {
                if bin(bins, 2 * i) >= 0.0001 {
                    self.background_timer = 0.0;
                    break;
                }
            }
            if self.background_timer >= bg_timeout {
                let max_timer = 3.0 * bg_timeout;
                if self.background_timer >= max_timer {
                    self.background_timer = max_timer;
                }
                brightness = (self.background_timer - bg_timeout) / (2.0 * bg_timeout);
            }
        }

        self.compose_spectrograph(bins, brightness);
        self.draw_single_color_row(brightness);
        self.bkgd_step += self.config.anim_speed / FPS;
        self.map_to_output(pixels);
        0
    }

    fn draw_pattern(&mut self, pattern: i32, brightness: f32, step: f32, target: Target) {
        let target = match target {
            Target::Background => self.bg.as_mut_slice(),
            Target::Foreground => self.fg.as_mut_slice(),
        };

        match pattern {
            PAT_SOLID_BLACK => draw_solid_color(brightness, C_BLACK, target),
            PAT_SOLID_WHITE => draw_solid_color(brightness, C_WHITE, target),
            PAT_SOLID_RED => draw_solid_color(brightness, C_RED, target),
            PAT_SOLID_ORANGE => draw_solid_color(brightness, C_ORANGE, target),
            PAT_SOLID_YELLOW => draw_solid_color(brightness, C_YELLOW, target),
            PAT_SOLID_GREEN => draw_solid_color(brightness, C_LIME, target),
            PAT_SOLID_CYAN => draw_solid_color(brightness, C_CYAN, target),
            PAT_SOLID_BLUE => draw_solid_color(brightness, C_BLUE, target),
            PAT_SOLID_PURPLE => draw_solid_color(brightness, C_PURPLE, target),
            PAT_SOLID_ELECTRIC_AQUAMARINE => draw_solid_color(brightness, C_ELEC_UL, target),
            PAT_STATIC_RED_BLUE => draw_horizontal_bars(brightness, &RED_BLUE, target),
            PAT_STATIC_CYAN_ORANGE => draw_horizontal_bars(brightness, &CYAN_ORANGE, target),
            PAT_STATIC_CYAN_PURPLE => draw_horizontal_bars(brightness, &CYAN_PURPLE, target),
            PAT_STATIC_CYAN_ELECTRIC_AQUAMARINE => {
                draw_horizontal_bars(brightness, &CYAN_ELEC, target)
            }
            PAT_STATIC_GREEN_YELLOW_RED => {
                draw_horizontal_bars(brightness, &GREEN_YELLOW_RED, target)
            }
            PAT_STATIC_GREEN_WHITE_RED => {
                draw_horizontal_bars(brightness, &GREEN_WHITE_RED, target)
            }
            PAT_STATIC_BLUE_CYAN_WHITE => {
                draw_horizontal_bars(brightness, &BLUE_CYAN_WHITE, target)
            }
            PAT_STATIC_RED_WHITE_BLUE => draw_horizontal_bars(brightness, &RED_WHITE_BLUE, target),
            PAT_STATIC_RAINBOW => draw_horizontal_bars(brightness, &RAINBOW, target),
            PAT_STATIC_RAINBOW_INVERSE => draw_horizontal_bars(brightness, &RAINBOW_INV, target),
            PAT_ANIM_RAINBOW_SINUSOIDAL => draw_rainbow_sinusoidal(brightness, step, target),
            PAT_ANIM_RAINBOW_HSV => draw_rainbow(brightness, step, target),
            PAT_ANIM_COLOR_WHEEL => draw_color_wheel(brightness, step, &self.wheel_32, target),
            PAT_ANIM_COLOR_WHEEL_2 => draw_color_wheel(brightness, step, &self.wheel_64, target),
            PAT_ANIM_SPECTRUM_CYCLE => draw_spectrum_cycle(brightness, step, target),
            PAT_ANIM_SINUSOIDAL_CYCLE => draw_sinusoidal_cycle(brightness, step, target),
            _ => draw_solid_color(brightness, C_BLACK, target),
        }
    }

    fn compose_spectrograph(&mut self, bins: &[f32], brightness: f32) {
        let mode = if self.config.reactive_bg || self.config.silent_bg {
            if !self.config.silent_bg
                || (self.config.bg_timeout > 0.0
                    && self.background_timer >= self.config.bg_timeout)
            {
                BackgroundCompose::Scaled(brightness)
            } else {
                BackgroundCompose::Black
            }
        } else {
            BackgroundCompose::Static
        };

        for x in 0..VIS_W {
            let bin_val = bin(bins, x);
            for y in 0..VIS_H {
                let idx = px_idx(x, y);
                let active = bin_val > ((VIS_H - y) as f32 / VIS_H as f32);
                self.out[idx] = if active {
                    self.fg[idx]
                } else {
                    self.background_pixel(idx, mode)
                };
            }

            let amp5 = bin(bins, 5) - 0.05;
            let bar_active = if x < 128 {
                amp5 > ((127 - x) as f32 / 128.0)
            } else {
                amp5 > ((x - 128) as f32 / 128.0)
            };
            let idx = px_idx(x, ROW_BAR_GRAPH);
            self.out[idx] = if bar_active {
                self.fg[idx]
            } else {
                self.background_pixel(idx, mode)
            };
        }
    }

    fn background_pixel(&self, idx: usize, mode: BackgroundCompose) -> SkydimoRgb {
        match mode {
            BackgroundCompose::Static => self.bg[idx],
            BackgroundCompose::Scaled(scale) => scale_color_f(self.bg[idx], scale),
            BackgroundCompose::Black => SkydimoRgb::default(),
        }
    }

    fn draw_single_color_row(&mut self, brightness: f32) {
        let mut sc_brightness = brightness;
        if self.config.single_color_mode == SC_FOLLOW_BACKGROUND {
            sc_brightness *= self.config.bg_brightness / 100.0;
        }

        if !(self.config.bg_timeout <= 0.0 || self.background_timer < self.config.bg_timeout) {
            return;
        }

        if let Some(color) = single_color(self.config.single_color_mode) {
            self.draw_single_color_static(sc_brightness, color);
            return;
        }

        match self.config.single_color_mode {
            SC_BACKGROUND => {}
            SC_FOLLOW_BACKGROUND => self.draw_single_color_background(sc_brightness),
            SC_FOLLOW_FOREGROUND => self.draw_single_color_foreground(sc_brightness),
            _ => {}
        }
    }

    fn draw_single_color_static(&mut self, amplitude: f32, color: u32) {
        let color = unpack_rgb(color);
        let value = scale_color_f(color, amplitude.clamp(0.0, 1.0));
        for x in 0..VIS_W {
            self.out[px_idx(x, ROW_SINGLE_COLOR)] = value;
        }
    }

    fn draw_single_color_background(&mut self, amplitude: f32) {
        let amplitude = amplitude.clamp(0.0, 1.0);
        for x in 0..VIS_W {
            let idx = px_idx(x, ROW_SINGLE_COLOR);
            self.out[idx] = scale_color_f(self.bg[idx], amplitude);
        }
    }

    fn draw_single_color_foreground(&mut self, amplitude: f32) {
        let amplitude = amplitude.clamp(0.0, 1.0);
        let y_idx = ((64.0 - amplitude * 62.0).floor() as isize).clamp(0, VIS_H as isize - 1)
            as usize;
        let base = scale_color_f(self.fg[px_idx(0, y_idx)], amplitude);

        for x in 0..VIS_W {
            let idx = px_idx(x, ROW_SINGLE_COLOR);
            self.out[idx] = if self.config.fg_mode >= PAT_ANIM_RAINBOW_SINUSOIDAL {
                scale_color_f(self.fg[idx], amplitude)
            } else {
                base
            };
        }
    }

    fn map_to_output(&mut self, pixels: &mut [SkydimoRgb]) {
        let width = self.width.max(1);
        let height = self.height.max(1);
        let is_matrix = height > 1 && width > 1;
        let is_single = (width == 1 && height == 1) || pixels.len() == 1;

        if is_matrix {
            self.map_matrix(width, height, pixels);
        } else if is_single {
            let px = self.out[px_idx(0, ROW_SINGLE_COLOR)];
            pixels.fill(px);
        } else {
            self.map_linear(pixels);
        }
    }

    fn map_linear(&mut self, pixels: &mut [SkydimoRgb]) {
        if self.linear_len != pixels.len() {
            self.linear_map = setup_linear_grid(pixels.len());
            self.linear_len = pixels.len();
        }
        for (idx, pixel) in pixels.iter_mut().enumerate() {
            let x = self.linear_map.get(idx).copied().unwrap_or(0);
            *pixel = self.out[px_idx(x, ROW_BAR_GRAPH)];
        }
    }

    fn map_matrix(&mut self, width: usize, height: usize, pixels: &mut [SkydimoRgb]) {
        if self.matrix_x_len != width {
            self.matrix_x = setup_matrix_x_grid(width);
            self.matrix_x_len = width;
        }
        if self.matrix_y_len != height {
            self.matrix_y = setup_matrix_y_grid(height);
            self.matrix_y_len = height;
        }

        let mut led = 0usize;
        for y in 0..height {
            for x in 0..width {
                if led >= pixels.len() {
                    return;
                }
                let px = px_idx(self.matrix_x[x], self.matrix_y[y]);
                pixels[led] = self.out[px];
                led += 1;
            }
        }
        if led < pixels.len() {
            fill_black(&mut pixels[led..]);
        }
    }
}

#[derive(Clone, Copy)]
enum Target {
    Background,
    Foreground,
}

fn draw_solid_color(brightness: f32, color: u32, target: &mut [SkydimoRgb]) {
    target.fill(scale_color_256(unpack_rgb(color), brightness_to_255(brightness)));
}

fn draw_spectrum_cycle(brightness: f32, step: f32, target: &mut [SkydimoRgb]) {
    let hue = floor_mod_360(step);
    let color = hsv_to_rgb_255(hue as i32, 255, brightness_to_255(brightness));
    target.fill(color);
}

fn draw_sinusoidal_cycle(brightness: f32, step: f32, target: &mut [SkydimoRgb]) {
    let bright = brightness_to_255(brightness);
    let base = ((360.0 / 255.0 - step).floor()).rem_euclid(360.0) / 360.0
        * std::f32::consts::TAU;
    let red = (127.0 * (base.sin() + 1.0)).floor() as i32;
    let grn = (127.0 * ((base - PHASE_THIRD).sin() + 1.0)).floor() as i32;
    let blu = (127.0 * ((base + PHASE_THIRD).sin() + 1.0)).floor() as i32;
    target.fill(scale_color_256(
        SkydimoRgb {
            r: red.clamp(0, 255) as u8,
            g: grn.clamp(0, 255) as u8,
            b: blu.clamp(0, 255) as u8,
        },
        bright,
    ));
}

fn draw_rainbow(brightness: f32, step: f32, target: &mut [SkydimoRgb]) {
    let table = build_hsv_lut(brightness_to_255(brightness));
    for x in 0..VIS_W {
        let h = floor_mod_360(step + (VIS_W - x) as f32);
        let color = table[h];
        for y in 0..VIS_H {
            target[px_idx(x, y)] = color;
        }
    }
}

fn draw_rainbow_sinusoidal(brightness: f32, step: f32, target: &mut [SkydimoRgb]) {
    let bright = brightness_to_255(brightness);
    for x in 0..VIS_W {
        let base = ((x as f32 * (360.0 / 255.0) - step).floor()).rem_euclid(360.0)
            / 360.0
            * std::f32::consts::TAU;
        let red = (127.0 * (base.sin() + 1.0)).floor() as i32;
        let grn = (127.0 * ((base - PHASE_THIRD).sin() + 1.0)).floor() as i32;
        let blu = (127.0 * ((base + PHASE_THIRD).sin() + 1.0)).floor() as i32;
        let color = scale_color_256(
            SkydimoRgb {
                r: red.clamp(0, 255) as u8,
                g: grn.clamp(0, 255) as u8,
                b: blu.clamp(0, 255) as u8,
            },
            bright,
        );
        for y in 0..VIS_H {
            target[px_idx(x, y)] = color;
        }
    }
}

fn draw_color_wheel(brightness: f32, step: f32, angles: &[f32], target: &mut [SkydimoRgb]) {
    let table = build_hsv_lut(brightness_to_255(brightness));
    for idx in 0..TOTAL_PX {
        target[idx] = table[floor_mod_360(step + angles[idx])];
    }
}

fn draw_horizontal_bars(brightness: f32, colors: &[u32], target: &mut [SkydimoRgb]) {
    let bright = brightness_to_255(brightness);
    let mut scaled = [SkydimoRgb::default(); 6];
    for (idx, color) in colors.iter().copied().enumerate() {
        scaled[idx] = scale_color_256(unpack_rgb(color), bright);
    }
    let num_colors = colors.len().max(1);

    for x in 0..VIS_W {
        for y in 0..VIS_H {
            let ci = if y == ROW_BAR_GRAPH {
                if x < 128 {
                    let raw = num_colors as isize
                        - (x as f32 * (num_colors as f32 / 128.0)).floor() as isize;
                    raw.clamp(0, num_colors as isize - 1) as usize
                } else {
                    let raw = ((x - 128) as f32 * (num_colors as f32 / 128.0)).floor() as isize;
                    raw.clamp(0, num_colors as isize - 1) as usize
                }
            } else {
                let raw =
                    num_colors as isize - (y as f32 * (num_colors as f32 / 63.0)).floor() as isize;
                raw.clamp(0, num_colors as isize - 1) as usize
            };
            target[px_idx(x, y)] = scaled[ci];
        }
    }
}

fn setup_linear_grid(x_count: usize) -> Vec<usize> {
    let mut x_idx = vec![0; x_count];
    if x_count == 0 {
        return x_idx;
    }

    if x_count.is_multiple_of(2) {
        for (x, value) in x_idx.iter_mut().enumerate() {
            *value = (x as f32 * (256.0 / x_count as f32) + (128.0 / x_count as f32))
                .floor()
                .clamp(0.0, (VIS_W - 1) as f32) as usize;
        }
    } else {
        let half = x_count / 2;
        let spacing = (256 / (x_count + 1)).max(1);
        for (x, value) in x_idx.iter_mut().enumerate() {
            *value = if x == half {
                128
            } else if x < half + 1 {
                half + (x + 1) * spacing
            } else {
                half + 1 + x * spacing
            }
            .min(VIS_W - 1);
        }
    }
    x_idx
}

fn setup_matrix_x_grid(x_count: usize) -> Vec<usize> {
    let mut x_idx = vec![0; x_count];
    if x_count == 0 {
        return x_idx;
    }
    for (x, value) in x_idx.iter_mut().enumerate() {
        let raw = if x_count < 10 {
            x as f32 * (VIS_W as f32 / x_count as f32) + 0.5 * (VIS_W as f32 / x_count as f32)
        } else if x < x_count / 2 {
            x as f32 * (VIS_W as f32 / (x_count - 1) as f32)
                + 0.5 * (VIS_W as f32 / (x_count - 1) as f32)
        } else {
            x as f32 * (VIS_W as f32 / (x_count - 1) as f32)
                - 0.5 * (VIS_W as f32 / (x_count - 1) as f32)
        };
        *value = raw.floor().clamp(0.0, (VIS_W - 1) as f32) as usize;
    }
    x_idx
}

fn setup_matrix_y_grid(y_count: usize) -> Vec<usize> {
    let mut y_idx = vec![0; y_count];
    if y_count == 0 {
        return y_idx;
    }
    let spectro_rows = VIS_H - ROW_SPECTRO_TOP;
    for (y, value) in y_idx.iter_mut().enumerate() {
        *value = (ROW_SPECTRO_TOP as f32
            + y as f32 * (spectro_rows as f32 / y_count as f32)
            + 0.5 * (spectro_rows as f32 / y_count as f32))
            .floor()
            .clamp(0.0, (VIS_H - 1) as f32) as usize;
    }
    y_idx
}

fn build_wheel_angles(cy: f32) -> Vec<f32> {
    let mut angles = vec![0.0; TOTAL_PX];
    for x in 0..VIS_W {
        for y in 0..VIS_H {
            angles[px_idx(x, y)] =
                (180.0 + ((y as f32 - cy).atan2(x as f32 - 128.0) * 180.0 / std::f32::consts::PI))
                    .rem_euclid(360.0);
        }
    }
    angles
}

fn build_hsv_lut(value: i32) -> [SkydimoRgb; 360] {
    let mut table = [SkydimoRgb::default(); 360];
    for (hue, color) in table.iter_mut().enumerate() {
        *color = hsv_to_rgb_255(hue as i32, 255, value);
    }
    table
}

fn hsv_to_rgb_255(hue: i32, saturation: i32, value: i32) -> SkydimoRgb {
    let h = hue.rem_euclid(360);
    let s = saturation.clamp(0, 255);
    let v = value.clamp(0, 255);
    if s == 0 {
        return SkydimoRgb {
            r: v as u8,
            g: v as u8,
            b: v as u8,
        };
    }

    let region = h / 60;
    let remainder = (h - region * 60) * 6;
    let p = (v * (255 - s)) / 255;
    let q = (v * (255 - (s * remainder) / 360)) / 255;
    let t = (v * (255 - (s * (360 - remainder)) / 360)) / 255;

    let (r, g, b) = match region {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    SkydimoRgb {
        r: r.clamp(0, 255) as u8,
        g: g.clamp(0, 255) as u8,
        b: b.clamp(0, 255) as u8,
    }
}

fn single_color(mode: i32) -> Option<u32> {
    match mode {
        SC_BLACK => Some(C_BLACK),
        SC_WHITE => Some(C_WHITE),
        SC_RED => Some(C_RED),
        SC_ORANGE => Some(C_ORANGE),
        SC_YELLOW => Some(C_YELLOW),
        SC_GREEN => Some(C_LIME),
        SC_CYAN => Some(C_CYAN),
        SC_BLUE => Some(C_BLUE),
        SC_PURPLE => Some(C_PURPLE),
        SC_ELECTRIC_AQUAMARINE => Some(C_ELEC_UL),
        _ => None,
    }
}

fn unpack_rgb(color: u32) -> SkydimoRgb {
    SkydimoRgb {
        r: ((color >> 16) & 0xFF) as u8,
        g: ((color >> 8) & 0xFF) as u8,
        b: (color & 0xFF) as u8,
    }
}

fn scale_color_256(color: SkydimoRgb, brightness: i32) -> SkydimoRgb {
    let brightness = brightness.clamp(0, 255);
    SkydimoRgb {
        r: ((brightness * color.r as i32) / 256).clamp(0, 255) as u8,
        g: ((brightness * color.g as i32) / 256).clamp(0, 255) as u8,
        b: ((brightness * color.b as i32) / 256).clamp(0, 255) as u8,
    }
}

fn scale_color_f(color: SkydimoRgb, factor: f32) -> SkydimoRgb {
    let factor = factor.clamp(0.0, 1.0);
    SkydimoRgb {
        r: (factor * color.r as f32).floor().clamp(0.0, 255.0) as u8,
        g: (factor * color.g as f32).floor().clamp(0.0, 255.0) as u8,
        b: (factor * color.b as f32).floor().clamp(0.0, 255.0) as u8,
    }
}

fn brightness_to_255(brightness: f32) -> i32 {
    (brightness.clamp(0.0, 100.0) * (255.0 / 100.0)).floor() as i32
}

fn floor_mod_360(value: f32) -> usize {
    value.floor().rem_euclid(360.0) as usize
}

fn clamp_mode(value: f32) -> i32 {
    (value.round() as i32).clamp(PAT_SOLID_BLACK, PAT_ANIM_SINUSOIDAL_CYCLE)
}

fn px_idx(x: usize, y: usize) -> usize {
    y * VIS_W + x
}

fn bin(bins: &[f32], index: usize) -> f32 {
    bins.get(index).copied().unwrap_or(0.0)
}
