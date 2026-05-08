mod abi;

use std::ffi::{c_char, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};

use abi::{
    SkydimoControllerApiV1, SkydimoEffectApiV1, SkydimoExtensionApiV1, SkydimoHostApiV1,
    SkydimoPluginApiV1, SkydimoRgb, SKYDIMO_NATIVE_C_ABI_VERSION, SKYDIMO_PLUGIN_KIND_EFFECT,
};

const HUE_TABLE_SIZE: usize = 1536;
const DEGREES_PER_SECOND_AT_SPEED_ONE: f32 = 1.5;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Direction {
    Clockwise,
    CounterClockwise,
}

#[derive(Clone, Copy)]
struct Config {
    speed: f32,
    direction: Direction,
    cx_percent: f32,
    cy_percent: f32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            speed: 50.0,
            direction: Direction::Clockwise,
            cx_percent: 50.0,
            cy_percent: 50.0,
        }
    }
}

struct ColorWheelEffect {
    config: Config,
    width: usize,
    height: usize,
    cached_width: usize,
    cached_height: usize,
    cached_len: usize,
    cached_cx_percent: f32,
    cached_cy_percent: f32,
    base_hues: Vec<u16>,
    rgb_table: [SkydimoRgb; HUE_TABLE_SIZE],
}

impl ColorWheelEffect {
    fn new() -> Self {
        Self {
            config: Config::default(),
            width: 0,
            height: 1,
            cached_width: 0,
            cached_height: 0,
            cached_len: 0,
            cached_cx_percent: f32::NAN,
            cached_cy_percent: f32::NAN,
            base_hues: Vec::new(),
            rgb_table: build_rgb_table(),
        }
    }

    fn resize(&mut self, width: u32, height: u32, led_count: u32) {
        if width == 0 || height == 0 {
            self.width = led_count.max(1) as usize;
            self.height = 1;
        } else {
            self.width = width as usize;
            self.height = height as usize;
        }
        self.invalidate_geometry();
    }

    fn update_params(&mut self, json: &str) {
        if let Some(speed) = json_number(json, "speed") {
            self.config.speed = speed.clamp(0.0, 100.0);
        }

        if let Some(direction) = json_number(json, "direction") {
            match round_to_i32(direction) {
                0 => self.config.direction = Direction::Clockwise,
                1 => self.config.direction = Direction::CounterClockwise,
                _ => {}
            }
        }

        let mut geometry_changed = false;
        if let Some(cx) = json_number(json, "cx") {
            let next = cx.clamp(0.0, 100.0);
            geometry_changed |= (self.config.cx_percent - next).abs() > f32::EPSILON;
            self.config.cx_percent = next;
        }
        if let Some(cy) = json_number(json, "cy") {
            let next = cy.clamp(0.0, 100.0);
            geometry_changed |= (self.config.cy_percent - next).abs() > f32::EPSILON;
            self.config.cy_percent = next;
        }

        if geometry_changed {
            self.invalidate_geometry();
        }
    }

    fn tick(&mut self, elapsed_seconds: f64, pixels: &mut [SkydimoRgb]) {
        if pixels.is_empty() {
            return;
        }

        self.ensure_geometry(pixels.len());
        let progress = (elapsed_seconds as f32)
            .mul_add(self.config.speed * DEGREES_PER_SECOND_AT_SPEED_ONE, 0.0)
            .rem_euclid(360.0);
        let shift = hue_index(progress);

        for (pixel, base_hue) in pixels.iter_mut().zip(self.base_hues.iter().copied()) {
            let index = match self.config.direction {
                Direction::Clockwise => subtract_wrapped(base_hue as usize, shift),
                Direction::CounterClockwise => add_wrapped(base_hue as usize, shift),
            };
            *pixel = self.rgb_table[index];
        }
    }

    fn ensure_geometry(&mut self, led_count: usize) {
        let (width, height) = self.effective_dimensions(led_count);
        if self.cached_width == width
            && self.cached_height == height
            && self.cached_len == led_count
            && self.cached_cx_percent == self.config.cx_percent
            && self.cached_cy_percent == self.config.cy_percent
        {
            return;
        }

        self.rebuild_geometry(width, height, led_count);
    }

    fn effective_dimensions(&self, led_count: usize) -> (usize, usize) {
        let width = self.width.max(1);
        let height = self.height.max(1);
        if width.saturating_mul(height) < led_count {
            (led_count.max(1), 1)
        } else {
            (width, height)
        }
    }

    fn rebuild_geometry(&mut self, width: usize, height: usize, led_count: usize) {
        self.base_hues.clear();
        self.base_hues.reserve(led_count);

        let cx = (width.saturating_sub(1) as f32) * (self.config.cx_percent * 0.01);
        let cy = (height.saturating_sub(1) as f32) * (self.config.cy_percent * 0.01);
        let degrees_per_radian = 180.0f32 / std::f32::consts::PI;

        let mut written = 0usize;
        for y in 0..height {
            if written >= led_count {
                break;
            }
            let dy = y as f32 - cy;
            for x in 0..width {
                if written >= led_count {
                    break;
                }
                let angle = dy.atan2(x as f32 - cx);
                let hue = 180.0 + angle * degrees_per_radian;
                self.base_hues.push(hue_index(hue) as u16);
                written += 1;
            }
        }

        self.cached_width = width;
        self.cached_height = height;
        self.cached_len = led_count;
        self.cached_cx_percent = self.config.cx_percent;
        self.cached_cy_percent = self.config.cy_percent;
    }

    fn invalidate_geometry(&mut self) {
        self.cached_len = 0;
    }
}

unsafe extern "C" fn color_wheel_create(
    _host: *const SkydimoHostApiV1,
    out_instance: *mut *mut c_void,
) -> i32 {
    ffi_status(|| {
        if out_instance.is_null() {
            return -1;
        }

        let effect = Box::new(ColorWheelEffect::new());
        unsafe {
            *out_instance = Box::into_raw(effect).cast::<c_void>();
        }
        0
    })
}

unsafe extern "C" fn color_wheel_destroy(instance: *mut c_void) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if !instance.is_null() {
            unsafe {
                drop(Box::from_raw(instance.cast::<ColorWheelEffect>()));
            }
        }
    }));
}

unsafe extern "C" fn color_wheel_resize(
    instance: *mut c_void,
    width: u32,
    height: u32,
    led_count: u32,
) -> i32 {
    ffi_status(|| {
        let Some(effect) = effect_mut(instance) else {
            return -1;
        };
        effect.resize(width, height, led_count);
        0
    })
}

unsafe extern "C" fn color_wheel_update_params_json(
    instance: *mut c_void,
    ptr: *const c_char,
    len: usize,
) -> i32 {
    ffi_status(|| {
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

unsafe extern "C" fn color_wheel_tick(
    instance: *mut c_void,
    elapsed_seconds: f64,
    buffer: *mut SkydimoRgb,
    len: usize,
) -> i32 {
    ffi_status(|| {
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

unsafe extern "C" fn color_wheel_is_ready(instance: *mut c_void) -> i32 {
    if instance.is_null() {
        -1
    } else {
        1
    }
}

#[no_mangle]
/// # Safety
///
/// `out_api` must be a valid, writable pointer to `SkydimoPluginApiV1`.
/// The host must pass the ABI version declared by this plugin manifest.
pub unsafe extern "C" fn skydimo_plugin_get_api(
    requested_abi_version: u32,
    _host: *const SkydimoHostApiV1,
    out_api: *mut SkydimoPluginApiV1,
) -> i32 {
    ffi_status(|| {
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
                    create: Some(color_wheel_create),
                    destroy: Some(color_wheel_destroy),
                    resize: Some(color_wheel_resize),
                    update_params_json: Some(color_wheel_update_params_json),
                    tick: Some(color_wheel_tick),
                    is_ready: Some(color_wheel_is_ready),
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

fn build_rgb_table() -> [SkydimoRgb; HUE_TABLE_SIZE] {
    std::array::from_fn(|index| {
        let hue = index as f32 * 360.0 / HUE_TABLE_SIZE as f32;
        hsv_to_rgb(hue, 1.0, 1.0)
    })
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> SkydimoRgb {
    let h = h.rem_euclid(360.0);
    let s = s.clamp(0.0, 1.0);
    let v = v.clamp(0.0, 1.0);
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;

    let (r, g, b) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };

    SkydimoRgb {
        r: to_u8((r + m) * 255.0),
        g: to_u8((g + m) * 255.0),
        b: to_u8((b + m) * 255.0),
    }
}

fn hue_index(hue: f32) -> usize {
    ((hue.rem_euclid(360.0) * HUE_TABLE_SIZE as f32 / 360.0) + 0.5).floor() as usize
        % HUE_TABLE_SIZE
}

fn add_wrapped(base: usize, shift: usize) -> usize {
    let next = base + shift;
    if next >= HUE_TABLE_SIZE {
        next - HUE_TABLE_SIZE
    } else {
        next
    }
}

fn subtract_wrapped(base: usize, shift: usize) -> usize {
    if base >= shift {
        base - shift
    } else {
        base + HUE_TABLE_SIZE - shift
    }
}

fn json_number(json: &str, key: &str) -> Option<f32> {
    let raw = json_value_start(json, key)?;
    let value = if let Some(rest) = raw.strip_prefix('"') {
        let end = rest.find('"')?;
        &rest[..end]
    } else {
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
        &raw[..end]
    };
    value.trim().parse::<f32>().ok()
}

fn json_value_start<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("\"{key}\"");
    let pos = json.find(needle.as_str())?;
    let after_key = &json[pos + needle.len()..];
    let colon = after_key.find(':')?;
    Some(after_key[colon + 1..].trim_start())
}

fn round_to_i32(value: f32) -> i32 {
    (value + 0.5).floor() as i32
}

fn to_u8(value: f32) -> u8 {
    value.round().clamp(0.0, 255.0) as u8
}

fn ffi_status(f: impl FnOnce() -> i32) -> i32 {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(-255)
}

fn effect_mut(instance: *mut c_void) -> Option<&'static mut ColorWheelEffect> {
    if instance.is_null() {
        None
    } else {
        Some(unsafe { &mut *instance.cast::<ColorWheelEffect>() })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        color_wheel_tick, color_wheel_update_params_json, hue_index, skydimo_plugin_get_api,
        ColorWheelEffect, Direction, SkydimoPluginApiV1, SkydimoRgb, HUE_TABLE_SIZE,
        SKYDIMO_NATIVE_C_ABI_VERSION, SKYDIMO_PLUGIN_KIND_EFFECT,
    };

    #[test]
    fn hue_indices_wrap_around_table() {
        assert_eq!(hue_index(0.0), 0);
        assert_eq!(hue_index(360.0), 0);
        assert_eq!(hue_index(-360.0), 0);
        assert!(hue_index(359.9) < HUE_TABLE_SIZE);
    }

    #[test]
    fn updates_numeric_params_without_json_allocation() {
        let mut effect = ColorWheelEffect::new();
        effect.update_params(r#"{"speed":100,"direction":1,"cx":25,"cy":"75"}"#);

        assert_eq!(effect.config.speed, 100.0);
        assert!(matches!(effect.config.direction, Direction::CounterClockwise));
        assert_eq!(effect.config.cx_percent, 25.0);
        assert_eq!(effect.config.cy_percent, 75.0);
    }

    #[test]
    fn renders_every_pixel_after_resize() {
        let mut effect = ColorWheelEffect::new();
        effect.resize(4, 3, 12);
        let mut pixels = [SkydimoRgb::default(); 12];

        effect.tick(0.0, &mut pixels);

        assert_eq!(effect.base_hues.len(), pixels.len());
        assert!(pixels.iter().any(|pixel| pixel.r != 0 || pixel.g != 0 || pixel.b != 0));
    }

    #[test]
    fn direction_changes_rotation_phase() {
        let mut clockwise = ColorWheelEffect::new();
        clockwise.resize(6, 1, 6);
        clockwise.update_params(r#"{"speed":50,"direction":0}"#);
        let mut clockwise_pixels = [SkydimoRgb::default(); 6];
        clockwise.tick(1.0, &mut clockwise_pixels);

        let mut counter = ColorWheelEffect::new();
        counter.resize(6, 1, 6);
        counter.update_params(r#"{"speed":50,"direction":1}"#);
        let mut counter_pixels = [SkydimoRgb::default(); 6];
        counter.tick(1.0, &mut counter_pixels);

        assert_ne!(clockwise_pixels, counter_pixels);
    }

    #[test]
    fn ffi_tick_rejects_null_buffer_with_len() {
        let mut effect = Box::new(ColorWheelEffect::new());
        let ptr = (&mut *effect as *mut ColorWheelEffect).cast();
        let status = unsafe { color_wheel_tick(ptr, 0.0, std::ptr::null_mut(), 1) };
        assert_eq!(status, -2);
    }

    #[test]
    fn ffi_params_reject_invalid_utf8() {
        let mut effect = Box::new(ColorWheelEffect::new());
        let ptr = (&mut *effect as *mut ColorWheelEffect).cast();
        let bytes = [0xffu8];
        let status = unsafe {
            color_wheel_update_params_json(ptr, bytes.as_ptr().cast(), bytes.len())
        };
        assert_eq!(status, -2);
    }

    #[test]
    fn exported_api_declares_effect_v3() {
        let mut api = SkydimoPluginApiV1::default();
        let status = unsafe {
            skydimo_plugin_get_api(SKYDIMO_NATIVE_C_ABI_VERSION, std::ptr::null(), &mut api)
        };

        assert_eq!(status, 0);
        assert_eq!(api.abi_version, SKYDIMO_NATIVE_C_ABI_VERSION);
        assert_eq!(api.kind_mask & SKYDIMO_PLUGIN_KIND_EFFECT, SKYDIMO_PLUGIN_KIND_EFFECT);
        assert!(api.effect.create.is_some());
        assert!(api.effect.tick.is_some());
    }
}
