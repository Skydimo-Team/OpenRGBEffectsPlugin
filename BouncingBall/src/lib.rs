mod abi;

use std::ffi::{c_char, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::{SystemTime, UNIX_EPOCH};

use abi::{
    SkydimoControllerApiV1, SkydimoEffectApiV1, SkydimoExtensionApiV1, SkydimoHostApiV1,
    SkydimoPluginApiV1, SkydimoRgb, SKYDIMO_NATIVE_C_ABI_VERSION, SKYDIMO_PLUGIN_KIND_EFFECT,
};

const MAX_DT_SECONDS: f32 = 0.5;

#[derive(Clone, Copy)]
struct Config {
    radius: u32,
    gravity_raw: f32,
    horizontal_velocity: f32,
    spectrum_velocity: f32,
    drop_pct: f32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            radius: 15,
            gravity_raw: 10.0,
            horizontal_velocity: 10.0,
            spectrum_velocity: 10.0,
            drop_pct: 90.0,
        }
    }
}

#[derive(Clone, Copy)]
struct BallPoint {
    rx: i16,
    ry: i16,
    brightness: f32,
}

#[derive(Clone, Copy)]
struct LinearPoint {
    ry: i16,
    brightness: f32,
}

struct BouncingBallEffect {
    config: Config,
    ball_points: Vec<BallPoint>,
    linear_points: Vec<LinearPoint>,
    x: f32,
    y: f32,
    dx: f32,
    dy: f32,
    ddy: f32,
    impact_velocity: f32,
    sim_width: usize,
    sim_height: usize,
    is_matrix: bool,
    hue_degrees: f32,
    last_t: Option<f32>,
    rng: XorShift64,
}

impl BouncingBallEffect {
    fn new() -> Self {
        let config = Config::default();
        let mut effect = Self {
            config,
            ball_points: Vec::new(),
            linear_points: Vec::new(),
            x: 0.0,
            y: 0.0,
            dx: config.horizontal_velocity,
            dy: 0.0,
            ddy: get_gravity(config.gravity_raw),
            impact_velocity: 0.0,
            sim_width: 0,
            sim_height: 0,
            is_matrix: false,
            hue_degrees: 0.0,
            last_t: None,
            rng: XorShift64::seeded(),
        };
        effect.rebuild_ball_points();
        effect
    }

    fn resize(&mut self, width: u32, height: u32, led_count: u32) {
        let raw_width = if width == 0 {
            led_count.max(1) as usize
        } else {
            width as usize
        };
        let raw_height = height.max(1) as usize;

        let (sim_width, sim_height, is_matrix) = if raw_height > 1 {
            (raw_width.max(1), raw_height, true)
        } else {
            (1, raw_width.max(led_count as usize).max(1), false)
        };

        if self.sim_width != sim_width
            || self.sim_height != sim_height
            || self.is_matrix != is_matrix
        {
            self.sim_width = sim_width;
            self.sim_height = sim_height;
            self.is_matrix = is_matrix;
            self.init_simulation();
        }
    }

    fn update_params(&mut self, json: &str) {
        if let Some(radius) = json_number(json, "radius") {
            let radius = radius.floor().clamp(1.0, 100.0) as u32;
            if self.config.radius != radius {
                self.config.radius = radius;
                self.rebuild_ball_points();
            }
        }

        let mut needs_reinit = false;
        if let Some(gravity) = json_number(json, "gravity") {
            self.config.gravity_raw = gravity.clamp(1.0, 100.0);
            self.ddy = get_gravity(self.config.gravity_raw);
            needs_reinit = true;
        }
        if let Some(drop_pct) = json_number(json, "dropHeight").or_else(|| json_number(json, "drop_height")) {
            self.config.drop_pct = drop_pct.clamp(0.0, 100.0);
            needs_reinit = true;
        }
        if let Some(horizontal_velocity) =
            json_number(json, "horizontalVelocity").or_else(|| json_number(json, "horizontal_velocity"))
        {
            self.config.horizontal_velocity = horizontal_velocity.clamp(0.0, 100.0);
            let sign = if self.dx < 0.0 { -1.0 } else { 1.0 };
            self.dx = self.config.horizontal_velocity * sign;
        }
        if let Some(spectrum_velocity) =
            json_number(json, "spectrumVelocity").or_else(|| json_number(json, "spectrum_velocity"))
        {
            self.config.spectrum_velocity = spectrum_velocity.clamp(0.0, 100.0);
        }

        if needs_reinit {
            self.init_simulation();
        }
    }

    fn tick(&mut self, elapsed_seconds: f64, pixels: &mut [SkydimoRgb]) {
        if pixels.is_empty() {
            return;
        }
        if self.sim_height == 0 {
            self.resize(pixels.len().min(u32::MAX as usize) as u32, 1, pixels.len().min(u32::MAX as usize) as u32);
        }

        self.render(pixels);

        let Some(dt) = self.tick_delta(elapsed_seconds) else {
            return;
        };
        self.step_horizontal(dt);
        self.step_vertical(dt);
        self.advance_hue(dt);
    }

    fn rebuild_ball_points(&mut self) {
        let radius = self.config.radius.max(1) as i16;
        let radius_f = radius as f32;
        let radius_sq = i32::from(radius) * i32::from(radius);
        let diameter = usize::from(radius as u16) * 2 + 1;

        self.ball_points.clear();
        self.ball_points.reserve(diameter.saturating_mul(diameter));
        self.linear_points.clear();
        self.linear_points.reserve(diameter);

        for ry in -radius..=radius {
            for rx in -radius..=radius {
                let dist_sq = i32::from(rx) * i32::from(rx) + i32::from(ry) * i32::from(ry);
                if dist_sq <= radius_sq {
                    let brightness = 1.0 - (dist_sq as f32).sqrt() / radius_f;
                    self.ball_points.push(BallPoint { rx, ry, brightness });
                    if rx == 0 {
                        self.linear_points.push(LinearPoint { ry, brightness });
                    }
                }
            }
        }
    }

    fn init_simulation(&mut self) {
        if self.sim_height == 0 {
            return;
        }

        if self.sim_height <= 1 {
            self.x = 0.0;
            self.y = 0.0;
            self.dy = 0.0;
            self.impact_velocity = 0.0;
            self.last_t = None;
            return;
        }

        let drop_height = self.config.drop_pct * 0.01 * (self.sim_height - 1) as f32;
        self.impact_velocity = (2.0 * self.ddy * drop_height).max(0.0).sqrt();
        self.y = self.sim_height as f32 - drop_height;
        self.dy = 0.0;

        self.x = if self.sim_width > 1 {
            self.rng.range_usize(self.sim_width) as f32
        } else {
            0.0
        };

        let speed = if self.dx.abs() == 0.0 {
            self.config.horizontal_velocity
        } else {
            self.dx.abs()
        };
        self.dx = speed * if self.rng.next_bool() { -1.0 } else { 1.0 };
        self.last_t = None;
    }

    fn render(&self, pixels: &mut [SkydimoRgb]) {
        clear_pixels(pixels);
        let base_color = hsv_to_rgb(self.hue_degrees);

        if self.is_matrix {
            self.render_matrix_ball(pixels, base_color);
        } else {
            self.render_linear_ball(pixels, base_color);
        }
    }

    fn render_matrix_ball(&self, pixels: &mut [SkydimoRgb], base_color: SkydimoRgb) {
        let width = self.sim_width;
        let height = self.sim_height;
        if width == 0 || height == 0 {
            return;
        }

        let active_len = pixels.len().min(width.saturating_mul(height));
        for point in &self.ball_points {
            if point.brightness <= 0.0 {
                continue;
            }

            let sx = (self.x + f32::from(point.rx)).floor() as isize;
            let sy = (self.y + f32::from(point.ry)).floor() as isize;
            if sx < 0 || sy < 0 {
                continue;
            }

            let sx = sx as usize;
            let sy = sy as usize;
            if sx >= width || sy >= height {
                continue;
            }

            let index = sy.saturating_mul(width).saturating_add(sx);
            if index < active_len {
                pixels[index] = scale_rgb(base_color, point.brightness);
            }
        }
    }

    fn render_linear_ball(&self, pixels: &mut [SkydimoRgb], base_color: SkydimoRgb) {
        let height = self.sim_height.min(pixels.len());
        if height == 0 {
            return;
        }

        for point in &self.linear_points {
            if point.brightness <= 0.0 {
                continue;
            }

            let sy = (self.y + f32::from(point.ry)).floor() as isize;
            if sy < 0 {
                continue;
            }

            let index = sy as usize;
            if index < height {
                pixels[index] = scale_rgb(base_color, point.brightness);
            }
        }
    }

    fn tick_delta(&mut self, elapsed_seconds: f64) -> Option<f32> {
        let time_now = if elapsed_seconds.is_finite() && elapsed_seconds >= 0.0 {
            elapsed_seconds as f32
        } else {
            0.0
        };

        let last = self.last_t.replace(time_now)?;
        let dt = time_now - last;
        if dt <= 0.0 || dt > MAX_DT_SECONDS {
            None
        } else {
            Some(dt)
        }
    }

    fn step_horizontal(&mut self, dt: f32) {
        if !self.is_matrix || self.sim_width <= 1 {
            return;
        }

        let previous_x = self.x;
        let previous_dx = self.dx;
        let next_x = previous_x + previous_dx * dt;

        if next_x < 0.0 {
            let denom = next_x - previous_x;
            let pct = if denom.abs() > f32::EPSILON {
                next_x / denom
            } else {
                0.0
            };
            self.dx = -previous_dx;
            self.x = self.dx * dt * pct;
        } else if next_x >= self.sim_width as f32 {
            let overshoot = next_x - self.sim_width as f32 - 1.0;
            let denom = next_x - previous_x;
            let pct = if denom.abs() > f32::EPSILON {
                overshoot / denom
            } else {
                0.0
            };
            self.dx = -previous_dx;
            self.x = (self.sim_width - 1) as f32 + self.dx * dt * pct;
        } else {
            self.x = next_x;
        }
    }

    fn step_vertical(&mut self, dt: f32) {
        if self.sim_height <= 1 {
            self.y = 0.0;
            self.dy = 0.0;
            return;
        }

        let previous_y = self.y;
        let next_dy = self.dy + self.ddy * dt;
        let next_y = previous_y + next_dy * dt;

        if next_y >= self.sim_height as f32 {
            let overshoot = next_y - self.sim_height as f32 - 1.0;
            let denom = next_y - previous_y;
            let pct = if denom.abs() > f32::EPSILON {
                overshoot / denom
            } else {
                0.0
            };
            self.dy = -self.impact_velocity + self.ddy * dt * pct;
            self.y = (self.sim_height - 1) as f32 + self.dy * dt * pct;
        } else {
            self.dy = next_dy;
            self.y = next_y;
        }
    }

    fn advance_hue(&mut self, dt: f32) {
        self.hue_degrees += self.config.spectrum_velocity * dt;
        if self.hue_degrees >= 360.0 {
            self.hue_degrees = self.hue_degrees.rem_euclid(360.0);
        }
    }
}

unsafe extern "C" fn bouncing_ball_create(
    _host: *const SkydimoHostApiV1,
    out_instance: *mut *mut c_void,
) -> i32 {
    catch_ffi(|| {
        if out_instance.is_null() {
            return -1;
        }

        let effect = Box::new(BouncingBallEffect::new());
        unsafe {
            *out_instance = Box::into_raw(effect).cast::<c_void>();
        }
        0
    })
}

unsafe extern "C" fn bouncing_ball_destroy(instance: *mut c_void) {
    let _ = catch_ffi(|| {
        if !instance.is_null() {
            unsafe {
                drop(Box::from_raw(instance.cast::<BouncingBallEffect>()));
            }
        }
        0
    });
}

unsafe extern "C" fn bouncing_ball_resize(
    instance: *mut c_void,
    width: u32,
    height: u32,
    led_count: u32,
) -> i32 {
    catch_ffi(|| {
        let Some(effect) = effect_mut(instance) else {
            return -1;
        };
        effect.resize(width, height, led_count);
        0
    })
}

unsafe extern "C" fn bouncing_ball_update_params_json(
    instance: *mut c_void,
    ptr: *const c_char,
    len: usize,
) -> i32 {
    catch_ffi(|| {
        let Some(effect) = effect_mut(instance) else {
            return -1;
        };
        if ptr.is_null() || len == 0 {
            return 0;
        }

        let bytes = unsafe { std::slice::from_raw_parts(ptr.cast::<u8>(), len) };
        let Ok(json) = std::str::from_utf8(bytes) else {
            return -2;
        };
        effect.update_params(json);
        0
    })
}

unsafe extern "C" fn bouncing_ball_tick(
    instance: *mut c_void,
    elapsed_seconds: f64,
    buffer: *mut SkydimoRgb,
    len: usize,
) -> i32 {
    catch_ffi(|| {
        let Some(effect) = effect_mut(instance) else {
            return -1;
        };
        if buffer.is_null() && len > 0 {
            return -2;
        }
        if len == 0 {
            return 0;
        }

        let pixels = unsafe { std::slice::from_raw_parts_mut(buffer, len) };
        effect.tick(elapsed_seconds, pixels);
        0
    })
}

unsafe extern "C" fn bouncing_ball_is_ready(instance: *mut c_void) -> i32 {
    catch_ffi(|| if instance.is_null() { -1 } else { 1 })
}

#[no_mangle]
/// # Safety
///
/// `out_api` must point to writable storage for one `SkydimoPluginApiV1`.
/// `requested_abi_version` must match the native-c ABI declared in manifest.json.
pub unsafe extern "C" fn skydimo_plugin_get_api(
    requested_abi_version: u32,
    _host: *const SkydimoHostApiV1,
    out_api: *mut SkydimoPluginApiV1,
) -> i32 {
    catch_ffi(|| {
        if out_api.is_null() || requested_abi_version != SKYDIMO_NATIVE_C_ABI_VERSION {
            return -1;
        }

        unsafe {
            *out_api = SkydimoPluginApiV1 {
                size: std::mem::size_of::<SkydimoPluginApiV1>() as u32,
                abi_version: SKYDIMO_NATIVE_C_ABI_VERSION,
                kind_mask: SKYDIMO_PLUGIN_KIND_EFFECT,
                effect: SkydimoEffectApiV1 {
                    size: std::mem::size_of::<SkydimoEffectApiV1>() as u32,
                    create: Some(bouncing_ball_create),
                    destroy: Some(bouncing_ball_destroy),
                    resize: Some(bouncing_ball_resize),
                    update_params_json: Some(bouncing_ball_update_params_json),
                    tick: Some(bouncing_ball_tick),
                    is_ready: Some(bouncing_ball_is_ready),
                },
                controller: SkydimoControllerApiV1 {
                    size: std::mem::size_of::<SkydimoControllerApiV1>() as u32,
                    ..SkydimoControllerApiV1::default()
                },
                extension: SkydimoExtensionApiV1 {
                    size: std::mem::size_of::<SkydimoExtensionApiV1>() as u32,
                    ..SkydimoExtensionApiV1::default()
                },
                shutdown_plugin: None,
            };
        }
        0
    })
}

fn effect_mut(instance: *mut c_void) -> Option<&'static mut BouncingBallEffect> {
    if instance.is_null() {
        None
    } else {
        Some(unsafe { &mut *instance.cast::<BouncingBallEffect>() })
    }
}

fn catch_ffi(f: impl FnOnce() -> i32) -> i32 {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(-100)
}

fn get_gravity(value: f32) -> f32 {
    if value <= 10.0 {
        value
    } else {
        10.0 + 1.07_f32.powf(value)
    }
}

#[inline]
fn clear_pixels(pixels: &mut [SkydimoRgb]) {
    unsafe {
        std::ptr::write_bytes(pixels.as_mut_ptr(), 0, pixels.len());
    }
}

#[inline]
fn scale_rgb(color: SkydimoRgb, brightness: f32) -> SkydimoRgb {
    SkydimoRgb {
        r: to_u8(color.r as f32 * brightness),
        g: to_u8(color.g as f32 * brightness),
        b: to_u8(color.b as f32 * brightness),
    }
}

fn hsv_to_rgb(hue: f32) -> SkydimoRgb {
    let hue = hue.rem_euclid(360.0) / 60.0;
    let sector = hue.floor() as u32;
    let fraction = hue - sector as f32;
    let inverse = 1.0 - fraction;

    let (r, g, b) = match sector {
        0 => (1.0, fraction, 0.0),
        1 => (inverse, 1.0, 0.0),
        2 => (0.0, 1.0, fraction),
        3 => (0.0, inverse, 1.0),
        4 => (fraction, 0.0, 1.0),
        _ => (1.0, 0.0, inverse),
    };

    SkydimoRgb {
        r: to_u8(r * 255.0),
        g: to_u8(g * 255.0),
        b: to_u8(b * 255.0),
    }
}

#[inline]
fn to_u8(value: f32) -> u8 {
    value.round().clamp(0.0, 255.0) as u8
}

fn json_number(json: &str, key: &str) -> Option<f32> {
    let mut raw = json_value_after_key(json, key)?.trim_start();
    if let Some(rest) = raw.strip_prefix('"') {
        raw = rest;
    }
    let end = raw
        .char_indices()
        .find_map(|(idx, ch)| {
            if ch.is_ascii_digit() || matches!(ch, '-' | '+' | '.' | 'e' | 'E') {
                None
            } else {
                Some(idx)
            }
        })
        .unwrap_or(raw.len());
    raw[..end].trim().parse::<f32>().ok()
}

fn json_value_after_key<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let mut start = 0usize;
    loop {
        let rel = json[start..].find('"')?;
        let quote = start + rel;
        let after_quote = quote + 1;
        let key_end = after_quote.checked_add(key.len())?;
        if json.get(after_quote..key_end) == Some(key)
            && json.as_bytes().get(key_end) == Some(&b'"')
        {
            let after_key = &json[key_end + 1..];
            let colon = after_key.find(':')?;
            return Some(&after_key[colon + 1..]);
        }
        start = after_quote;
    }
}

struct XorShift64 {
    state: u64,
}

impl XorShift64 {
    fn seeded() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos() as u64)
            .unwrap_or(0x9E37_79B9_7F4A_7C15);
        Self::new(nanos ^ 0xA076_1D64_78BD_642F)
    }

    fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 {
                0x9E37_79B9_7F4A_7C15
            } else {
                seed
            },
        }
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x.max(1);
        x
    }

    fn next_bool(&mut self) -> bool {
        self.next_u64() & 1 == 0
    }

    fn range_usize(&mut self, upper: usize) -> usize {
        if upper <= 1 {
            0
        } else {
            (self.next_u64() as usize) % upper
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        get_gravity, hsv_to_rgb, json_number, BouncingBallEffect, SkydimoRgb,
    };

    #[test]
    fn parses_manifest_params() {
        let json = r#"{"radius":7,"spectrumVelocity":25,"horizontalVelocity":0,"gravity":42,"dropHeight":55}"#;
        let mut effect = BouncingBallEffect::new();
        effect.update_params(json);

        assert_eq!(effect.config.radius, 7);
        assert_eq!(effect.config.spectrum_velocity, 25.0);
        assert_eq!(effect.config.horizontal_velocity, 0.0);
        assert_eq!(effect.config.gravity_raw, 42.0);
        assert_eq!(effect.config.drop_pct, 55.0);
        assert_eq!(json_number(json, "dropHeight"), Some(55.0));
    }

    #[test]
    fn gravity_matches_lua_curve() {
        assert_eq!(get_gravity(10.0), 10.0);
        assert!(get_gravity(11.0) > 11.0);
    }

    #[test]
    fn radius_one_builds_matrix_and_linear_masks() {
        let mut effect = BouncingBallEffect::new();
        effect.config.radius = 1;
        effect.rebuild_ball_points();

        assert_eq!(effect.ball_points.len(), 5);
        assert_eq!(effect.linear_points.len(), 3);
        assert!(effect.ball_points.iter().any(|point| point.brightness == 1.0));
    }

    #[test]
    fn resize_normalizes_linear_strip_to_vertical_simulation() {
        let mut effect = BouncingBallEffect::new();
        effect.resize(8, 1, 8);

        assert_eq!(effect.sim_width, 1);
        assert_eq!(effect.sim_height, 8);
        assert!(!effect.is_matrix);
    }

    #[test]
    fn resize_preserves_matrix_dimensions() {
        let mut effect = BouncingBallEffect::new();
        effect.resize(4, 3, 12);

        assert_eq!(effect.sim_width, 4);
        assert_eq!(effect.sim_height, 3);
        assert!(effect.is_matrix);
    }

    #[test]
    fn renders_directly_into_host_buffer() {
        let mut effect = BouncingBallEffect::new();
        effect.resize(8, 1, 8);
        let mut pixels = [SkydimoRgb::default(); 8];

        effect.tick(0.0, &mut pixels);

        assert!(pixels.iter().any(|pixel| *pixel != SkydimoRgb::default()));
    }

    #[test]
    fn hsv_primary_hues_match_full_saturation_rgb() {
        assert_eq!(hsv_to_rgb(0.0), SkydimoRgb { r: 255, g: 0, b: 0 });
        assert_eq!(hsv_to_rgb(120.0), SkydimoRgb { r: 0, g: 255, b: 0 });
        assert_eq!(hsv_to_rgb(240.0), SkydimoRgb { r: 0, g: 0, b: 255 });
    }
}
