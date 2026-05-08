use std::ffi::{c_char, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::OnceLock;

const SKYDIMO_NATIVE_C_ABI_VERSION: u32 = 3;
const SKYDIMO_PLUGIN_KIND_EFFECT: u32 = 1 << 0;
const COLOR_MODE_RAINBOW: u32 = 0;
const COLOR_MODE_CUSTOM: u32 = 1;
const ANIMATION_DIRECTION_INSIDE: u32 = 0;
const ANIMATION_DIRECTION_OUTSIDE: u32 = 1;
const COLOR_ROTATION_CLOCKWISE: u32 = 0;
const COLOR_ROTATION_COUNTER_CLOCKWISE: u32 = 1;
const GRADIENT_SAMPLES: usize = 100;
const HUE_COUNT: usize = 360;
const VALUE_COUNT: usize = 256;
const PI_DEG: f64 = 180.0 / std::f64::consts::PI;

static RAINBOW_VALUE_LUT: OnceLock<Vec<[SkydimoRgb; VALUE_COUNT]>> = OnceLock::new();

#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct SkydimoRgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

#[repr(C)]
pub struct SkydimoHostApiV1 {
    _private: [u8; 0],
}

#[repr(C)]
pub struct SkydimoHardwareCandidateV1 {
    _private: [u8; 0],
}

#[repr(C)]
pub struct SkydimoDeviceInfoV1 {
    _private: [u8; 0],
}

#[repr(C)]
pub struct SkydimoOutputDefinitionV1 {
    _private: [u8; 0],
}

#[repr(C)]
pub struct SkydimoOutputFrameV1 {
    _private: [u8; 0],
}

#[repr(C)]
pub struct SkydimoLedColorV1 {
    _private: [u8; 0],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkydimoEffectApiV1 {
    pub size: u32,
    pub create: Option<unsafe extern "C" fn(*const SkydimoHostApiV1, *mut *mut c_void) -> i32>,
    pub destroy: Option<unsafe extern "C" fn(*mut c_void)>,
    pub resize: Option<unsafe extern "C" fn(*mut c_void, u32, u32, u32) -> i32>,
    pub update_params_json: Option<unsafe extern "C" fn(*mut c_void, *const c_char, usize) -> i32>,
    pub tick: Option<unsafe extern "C" fn(*mut c_void, f64, *mut SkydimoRgb, usize) -> i32>,
    pub is_ready: Option<unsafe extern "C" fn(*mut c_void) -> i32>,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkydimoControllerApiV1 {
    pub size: u32,
    pub create: Option<
        unsafe extern "C" fn(
            *const SkydimoHostApiV1,
            *const SkydimoHardwareCandidateV1,
            *mut *mut c_void,
        ) -> i32,
    >,
    pub destroy: Option<unsafe extern "C" fn(*mut c_void)>,
    pub validate: Option<unsafe extern "C" fn(*mut c_void) -> i32>,
    pub init: Option<unsafe extern "C" fn(*mut c_void) -> i32>,
    pub get_device_info: Option<unsafe extern "C" fn(*mut c_void, *mut SkydimoDeviceInfoV1) -> i32>,
    pub get_output_count: Option<unsafe extern "C" fn(*mut c_void) -> usize>,
    pub get_output:
        Option<unsafe extern "C" fn(*mut c_void, usize, *mut SkydimoOutputDefinitionV1) -> i32>,
    pub update: Option<unsafe extern "C" fn(*mut c_void, *const SkydimoOutputFrameV1, usize) -> i32>,
    pub set_output_leds_count:
        Option<unsafe extern "C" fn(*mut c_void, *const c_char, usize, usize) -> i32>,
    pub update_output:
        Option<unsafe extern "C" fn(*mut c_void, *const SkydimoOutputDefinitionV1) -> i32>,
    pub disconnect: Option<unsafe extern "C" fn(*mut c_void) -> i32>,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkydimoExtensionApiV1 {
    pub size: u32,
    pub create: Option<unsafe extern "C" fn(*const SkydimoHostApiV1, *mut *mut c_void) -> i32>,
    pub destroy: Option<unsafe extern "C" fn(*mut c_void)>,
    pub start: Option<unsafe extern "C" fn(*mut c_void) -> i32>,
    pub stop: Option<unsafe extern "C" fn(*mut c_void) -> i32>,
    pub on_scan_devices: Option<unsafe extern "C" fn(*mut c_void) -> i32>,
    pub on_event_json:
        Option<unsafe extern "C" fn(*mut c_void, *const c_char, usize, *const c_char, usize) -> i32>,
    pub on_page_message_json: Option<unsafe extern "C" fn(*mut c_void, *const c_char, usize) -> i32>,
    pub on_device_frame: Option<
        unsafe extern "C" fn(
            *mut c_void,
            *const c_char,
            usize,
            *const SkydimoOutputFrameV1,
            usize,
        ) -> i32,
    >,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkydimoPluginApiV1 {
    pub size: u32,
    pub abi_version: u32,
    pub kind_mask: u32,
    pub effect: SkydimoEffectApiV1,
    pub controller: SkydimoControllerApiV1,
    pub extension: SkydimoExtensionApiV1,
    pub shutdown_plugin: Option<unsafe extern "C" fn()>,
}

#[derive(Clone, Copy)]
struct Config {
    speed: f64,
    color_mode: u32,
    animation_speed: u32,
    color_rotation_speed: u32,
    animation_direction: u32,
    color_rotation_direction: u32,
    spacing: u32,
    thickness: u32,
    cx_shift: u32,
    cy_shift: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            speed: 50.0,
            color_mode: COLOR_MODE_RAINBOW,
            animation_speed: 10,
            color_rotation_speed: 10,
            animation_direction: ANIMATION_DIRECTION_INSIDE,
            color_rotation_direction: COLOR_ROTATION_CLOCKWISE,
            spacing: 1,
            thickness: 1,
            cx_shift: 50,
            cy_shift: 50,
        }
    }
}

#[derive(Clone, Copy, Default)]
struct CachedHsv {
    h: f32,
    s255: u8,
    v255: u8,
}

#[derive(Clone, Copy, Default)]
struct SpatialPoint {
    phase_base: f64,
    distance: f64,
}

struct HypnotoadEffect {
    config: Config,
    custom_colors: Vec<SkydimoRgb>,
    gradient_strip: [SkydimoRgb; GRADIENT_SAMPLES],
    gradient_hsv: [CachedHsv; GRADIENT_SAMPLES],
    gradient_value_lut: Vec<[SkydimoRgb; VALUE_COUNT]>,
    width: usize,
    height: usize,
    spatial: Vec<SpatialPoint>,
    spatial_width: usize,
    spatial_height: usize,
    spatial_cx_shift: u32,
    spatial_cy_shift: u32,
}

impl HypnotoadEffect {
    fn new() -> Self {
        let mut effect = Self {
            config: Config::default(),
            custom_colors: vec![SkydimoRgb::default()],
            gradient_strip: [SkydimoRgb::default(); GRADIENT_SAMPLES],
            gradient_hsv: [CachedHsv::default(); GRADIENT_SAMPLES],
            gradient_value_lut: vec![[SkydimoRgb::default(); VALUE_COUNT]; GRADIENT_SAMPLES],
            width: 0,
            height: 1,
            spatial: Vec::new(),
            spatial_width: usize::MAX,
            spatial_height: usize::MAX,
            spatial_cx_shift: u32::MAX,
            spatial_cy_shift: u32::MAX,
        };
        effect.rebuild_gradient();
        effect
    }

    fn resize(&mut self, width: u32, height: u32, led_count: u32) {
        let fallback = (led_count as usize).max(1);
        let width = if width == 0 { fallback } else { width as usize };
        let height = height.max(1) as usize;
        if self.width != width || self.height != height {
            self.width = width;
            self.height = height;
            self.invalidate_spatial();
        }
    }

    fn update_params(&mut self, json: &str) {
        if let Some(value) = json_number(json, "speed") {
            self.config.speed = value.clamp(1.0, 100.0);
        }

        if let Some(value) = json_number(json, "color_mode") {
            let value = c_trunc(value) as u32;
            if matches!(value, COLOR_MODE_RAINBOW | COLOR_MODE_CUSTOM) {
                self.config.color_mode = value;
            }
        }

        if let Some(colors) = json_color_array(json, "colors") {
            self.custom_colors = if colors.is_empty() {
                vec![SkydimoRgb::default()]
            } else {
                colors
            };
            self.rebuild_gradient();
        }

        if let Some(value) = json_number(json, "animation_speed") {
            self.config.animation_speed = c_trunc(value.clamp(10.0, 99.0)) as u32;
        }

        if let Some(value) = json_number(json, "animation_direction") {
            let value = c_trunc(value) as u32;
            if matches!(value, ANIMATION_DIRECTION_INSIDE | ANIMATION_DIRECTION_OUTSIDE) {
                self.config.animation_direction = value;
            }
        }

        if let Some(value) = json_number(json, "color_rotation_speed") {
            self.config.color_rotation_speed = c_trunc(value.clamp(10.0, 99.0)) as u32;
        }

        if let Some(value) = json_number(json, "color_rotation_direction") {
            let value = c_trunc(value) as u32;
            if matches!(
                value,
                COLOR_ROTATION_CLOCKWISE | COLOR_ROTATION_COUNTER_CLOCKWISE
            ) {
                self.config.color_rotation_direction = value;
            }
        }

        if let Some(value) = json_number(json, "spacing") {
            self.config.spacing = c_trunc(value.clamp(1.0, 10.0)) as u32;
        }

        if let Some(value) = json_number(json, "thickness") {
            self.config.thickness = c_trunc(value.clamp(1.0, 10.0)) as u32;
        }

        if let Some(value) = json_number(json, "cx") {
            let next = c_trunc(value.clamp(0.0, 100.0)) as u32;
            if self.config.cx_shift != next {
                self.config.cx_shift = next;
                self.invalidate_spatial();
            }
        }

        if let Some(value) = json_number(json, "cy") {
            let next = c_trunc(value.clamp(0.0, 100.0)) as u32;
            if self.config.cy_shift != next {
                self.config.cy_shift = next;
                self.invalidate_spatial();
            }
        }
    }

    fn tick(&mut self, elapsed_seconds: f64, pixels: &mut [SkydimoRgb]) {
        if pixels.is_empty() {
            return;
        }

        let width = self.width.max(1);
        let height = self.height.max(1);
        self.ensure_spatial(width, height);

        let animation_dir = if self.config.animation_direction == ANIMATION_DIRECTION_INSIDE {
            1.0
        } else {
            -1.0
        };
        let color_dir = if self.config.color_rotation_direction == COLOR_ROTATION_CLOCKWISE {
            -1.0
        } else {
            1.0
        };

        let animation_mult = 0.01 * self.config.animation_speed as f64 * animation_dir;
        let inv_spacing = 1.0 / (0.1 * self.config.spacing as f64);
        let wave_distance_mult = animation_mult * inv_spacing;
        let progress = 1000.0 + 0.1 * self.config.speed * elapsed_seconds.max(0.0);
        let color_mult = 0.01 * self.config.color_rotation_speed as f64 * color_dir;
        let phase_progress =
            progress * color_mult * self.config.color_rotation_speed as f64;
        let exponent = (11 - self.config.thickness) as i32;

        let count = pixels.len().min(self.spatial.len());
        if self.config.color_mode == COLOR_MODE_RAINBOW {
            let rainbow = rainbow_value_lut();
            for (pixel, point) in pixels.iter_mut().zip(self.spatial.iter()).take(count) {
                let wave = (wave_distance_mult.mul_add(point.distance, progress)).cos();
                let value = value_byte(wave, exponent);
                let hue = phase_hue(point.phase_base + phase_progress);
                *pixel = rainbow[hue][value];
            }
        } else {
            for (pixel, point) in pixels.iter_mut().zip(self.spatial.iter()).take(count) {
                let wave = (wave_distance_mult.mul_add(point.distance, progress)).cos();
                let factor = value_factor(wave, exponent);
                let hue = phase_hue(point.phase_base + phase_progress);
                let sample = hue * GRADIENT_SAMPLES / HUE_COUNT;
                let hsv = self.gradient_hsv[sample];
                let scaled_value = ((hsv.v255 as f64 * factor).floor() as usize).min(255);
                *pixel = self.gradient_value_lut[sample][scaled_value];
            }
        }

        if count < pixels.len() {
            pixels[count..].fill(SkydimoRgb::default());
        }
    }

    fn rebuild_gradient(&mut self) {
        match self.custom_colors.len() {
            0 => self.gradient_strip.fill(SkydimoRgb::default()),
            1 => self.gradient_strip.fill(self.custom_colors[0]),
            count => {
                let segment_count = (count - 1) as f64;
                for sample in 0..GRADIENT_SAMPLES {
                    let position = (sample as f64 + 0.5) / GRADIENT_SAMPLES as f64;
                    let scaled = position * segment_count;
                    let left_floor = scaled.floor();
                    let mut left = left_floor as usize;
                    let mut blend = scaled - left_floor;

                    if left >= count - 1 {
                        left = count - 2;
                        blend = 1.0;
                    }

                    self.gradient_strip[sample] =
                        lerp_rgb(self.custom_colors[left], self.custom_colors[left + 1], blend);
                }
            }
        }

        for sample in 0..GRADIENT_SAMPLES {
            let hsv = rgb_to_hsv255(self.gradient_strip[sample]);
            self.gradient_hsv[sample] = hsv;
            let saturation = hsv.s255 as f32 / 255.0;
            for value in 0..VALUE_COUNT {
                self.gradient_value_lut[sample][value] =
                    hsv_to_rgb(hsv.h, saturation, value as f32 / 255.0);
            }
        }
    }

    fn ensure_spatial(&mut self, width: usize, height: usize) {
        if self.spatial_width == width
            && self.spatial_height == height
            && self.spatial_cx_shift == self.config.cx_shift
            && self.spatial_cy_shift == self.config.cy_shift
            && !self.spatial.is_empty()
        {
            return;
        }

        self.spatial_width = width;
        self.spatial_height = height;
        self.spatial_cx_shift = self.config.cx_shift;
        self.spatial_cy_shift = self.config.cy_shift;
        self.spatial.clear();
        self.spatial.reserve(width.saturating_mul(height));

        let cx_mult = 0.01 * self.config.cx_shift as f64;
        let cy_mult = 0.01 * self.config.cy_shift as f64;
        let cx = width.saturating_sub(1) as f64 * cx_mult;
        let cy = if height > 1 {
            height.saturating_sub(1) as f64 * cy_mult
        } else {
            cy_mult
        };

        for y in 0..height {
            let yf = y as f64;
            for x in 0..width {
                let xf = x as f64;
                let angle = (yf - cy).atan2(xf - cx) * PI_DEG;
                let dx = cx - xf;
                let dy = cy - yf;
                let distance = dx.mul_add(dx, dy * dy).sqrt();
                self.spatial.push(SpatialPoint {
                    phase_base: angle + distance,
                    distance,
                });
            }
        }
    }

    fn invalidate_spatial(&mut self) {
        self.spatial_width = usize::MAX;
    }
}

unsafe extern "C" fn hypnotoad_create(
    _host: *const SkydimoHostApiV1,
    out_instance: *mut *mut c_void,
) -> i32 {
    catch_ffi(|| {
        if out_instance.is_null() {
            return -1;
        }
        let effect = Box::new(HypnotoadEffect::new());
        unsafe {
            *out_instance = Box::into_raw(effect).cast::<c_void>();
        }
        0
    })
}

unsafe extern "C" fn hypnotoad_destroy(instance: *mut c_void) {
    let _ = catch_ffi(|| {
        if !instance.is_null() {
            unsafe {
                drop(Box::from_raw(instance.cast::<HypnotoadEffect>()));
            }
        }
        0
    });
}

unsafe extern "C" fn hypnotoad_resize(
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

unsafe extern "C" fn hypnotoad_update_params_json(
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

unsafe extern "C" fn hypnotoad_tick(
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

unsafe extern "C" fn hypnotoad_is_ready(instance: *mut c_void) -> i32 {
    catch_ffi(|| if instance.is_null() { -1 } else { 1 })
}

#[no_mangle]
/// # Safety
///
/// `out_api` must point to writable storage for one `SkydimoPluginApiV1`.
/// `requested_abi_version` must match the Core native-c ABI declared in the manifest.
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
                    create: Some(hypnotoad_create),
                    destroy: Some(hypnotoad_destroy),
                    resize: Some(hypnotoad_resize),
                    update_params_json: Some(hypnotoad_update_params_json),
                    tick: Some(hypnotoad_tick),
                    is_ready: Some(hypnotoad_is_ready),
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

fn effect_mut(instance: *mut c_void) -> Option<&'static mut HypnotoadEffect> {
    if instance.is_null() {
        None
    } else {
        Some(unsafe { &mut *instance.cast::<HypnotoadEffect>() })
    }
}

fn catch_ffi(f: impl FnOnce() -> i32) -> i32 {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(-100)
}

fn rainbow_value_lut() -> &'static [[SkydimoRgb; VALUE_COUNT]] {
    RAINBOW_VALUE_LUT
        .get_or_init(|| {
            let mut lut = Vec::with_capacity(HUE_COUNT);
            for hue in 0..HUE_COUNT {
                let mut row = [SkydimoRgb::default(); VALUE_COUNT];
                for (value, color) in row.iter_mut().enumerate() {
                    *color = hsv_to_rgb(hue as f32, 1.0, value as f32 / 255.0);
                }
                lut.push(row);
            }
            lut
        })
        .as_slice()
}

#[inline]
fn value_factor(wave: f64, exponent: i32) -> f64 {
    (((wave + 1.0) * 0.5).clamp(0.0, 1.0)).powi(exponent)
}

#[inline]
fn value_byte(wave: f64, exponent: i32) -> usize {
    (value_factor(wave, exponent) * 255.0).floor().clamp(0.0, 255.0) as usize
}

#[inline]
fn phase_hue(value: f64) -> usize {
    let raw_phase = value.trunc();
    c_remainder(raw_phase, HUE_COUNT as f64).abs() as usize
}

#[inline]
fn c_trunc(value: f64) -> i64 {
    value.trunc() as i64
}

#[inline]
fn c_remainder(value: f64, divisor: f64) -> f64 {
    let quotient = (value / divisor).trunc();
    value - quotient * divisor
}

#[inline]
fn lerp_rgb(left: SkydimoRgb, right: SkydimoRgb, t: f64) -> SkydimoRgb {
    SkydimoRgb {
        r: to_u8(left.r as f64 + (right.r as f64 - left.r as f64) * t),
        g: to_u8(left.g as f64 + (right.g as f64 - left.g as f64) * t),
        b: to_u8(left.b as f64 + (right.b as f64 - left.b as f64) * t),
    }
}

#[inline]
fn to_u8(value: f64) -> u8 {
    (value + 0.5).floor().clamp(0.0, 255.0) as u8
}

fn rgb_to_hsv255(rgb: SkydimoRgb) -> CachedHsv {
    let r = rgb.r as f64;
    let g = rgb.g as f64;
    let b = rgb.b as f64;
    let maxc = r.max(g).max(b);
    let minc = r.min(g).min(b);
    let delta = maxc - minc;
    let mut hue = 0.0;

    if delta > 0.0 {
        hue = if maxc == r {
            60.0 * ((g - b) / delta).rem_euclid(6.0)
        } else if maxc == g {
            60.0 * (((b - r) / delta) + 2.0)
        } else {
            60.0 * (((r - g) / delta) + 4.0)
        };
    }

    if hue < 0.0 {
        hue += 360.0;
    }

    let saturation = if maxc > 0.0 {
        ((delta / maxc) * 255.0).floor().clamp(0.0, 255.0) as u8
    } else {
        0
    };

    CachedHsv {
        h: hue as f32,
        s255: saturation,
        v255: maxc.clamp(0.0, 255.0) as u8,
    }
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
        r: ((r + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        g: ((g + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        b: ((b + m) * 255.0).round().clamp(0.0, 255.0) as u8,
    }
}

fn json_number(json: &str, key: &str) -> Option<f64> {
    let mut raw = json_value_after_key(json, key)?;
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
    raw[..end].trim().parse::<f64>().ok()
}

fn json_color_array(json: &str, key: &str) -> Option<Vec<SkydimoRgb>> {
    let raw = json_value_after_key(json, key)?;
    let Some(mut raw) = raw.strip_prefix('[') else {
        return Some(vec![SkydimoRgb::default()]);
    };
    let mut colors = Vec::new();

    loop {
        raw = raw.trim_start();
        if raw.starts_with(']') {
            break;
        }
        if let Some(rest) = raw.strip_prefix(',') {
            raw = rest.trim_start();
        }
        let Some(rest) = raw.strip_prefix('"') else {
            break;
        };
        let Some(end) = json_string_end(rest) else {
            break;
        };
        if let Some(color) = parse_hex_color(&rest[..end]) {
            colors.push(color);
            if colors.len() >= GRADIENT_SAMPLES {
                break;
            }
        }
        raw = &rest[end + 1..];
    }

    Some(colors)
}

fn json_value_after_key<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("\"{key}\"");
    let pos = json.find(&needle)?;
    let after_key = &json[pos + needle.len()..];
    let colon_pos = after_key.find(':')?;
    Some(after_key[colon_pos + 1..].trim_start())
}

fn json_string_end(raw: &str) -> Option<usize> {
    let mut escaped = false;
    for (idx, ch) in raw.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' => escaped = true,
            '"' => return Some(idx),
            _ => {}
        }
    }
    None
}

fn parse_hex_color(raw: &str) -> Option<SkydimoRgb> {
    let mut digits = [0u8; 6];
    let mut len = 0usize;

    for byte in raw.bytes() {
        if byte == b'#' && len == 0 {
            continue;
        }
        if byte.is_ascii_whitespace() {
            continue;
        }
        if len >= digits.len() {
            return None;
        }
        digits[len] = byte;
        len += 1;
    }

    if len == 3 {
        return Some(SkydimoRgb {
            r: parse_hex_nibble(digits[0])? * 17,
            g: parse_hex_nibble(digits[1])? * 17,
            b: parse_hex_nibble(digits[2])? * 17,
        });
    }

    if len != 6 {
        return None;
    }

    Some(SkydimoRgb {
        r: parse_hex_byte(digits[0], digits[1])?,
        g: parse_hex_byte(digits[2], digits[3])?,
        b: parse_hex_byte(digits[4], digits[5])?,
    })
}

#[inline]
fn parse_hex_byte(hi: u8, lo: u8) -> Option<u8> {
    Some((parse_hex_nibble(hi)? << 4) | parse_hex_nibble(lo)?)
}

#[inline]
fn parse_hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        c_remainder, json_color_array, json_number, parse_hex_color, phase_hue, HypnotoadEffect,
        SkydimoRgb, COLOR_MODE_CUSTOM,
    };

    #[test]
    fn parses_numbers_and_strings() {
        assert_eq!(json_number(r#"{"speed":75}"#, "speed"), Some(75.0));
        assert_eq!(json_number(r#"{"speed":"25"}"#, "speed"), Some(25.0));
        assert_eq!(
            json_number(r#"{"animation_speed":44,"speed":50}"#, "speed"),
            Some(50.0)
        );
    }

    #[test]
    fn parses_color_arrays() {
        let colors = json_color_array(r##"{"colors":["#0af","#102030"]}"##, "colors").unwrap();
        assert_eq!(colors[0], SkydimoRgb { r: 0, g: 170, b: 255 });
        assert_eq!(colors[1], SkydimoRgb { r: 16, g: 32, b: 48 });
    }

    #[test]
    fn parses_compact_hex_like_lua() {
        assert_eq!(
            parse_hex_color("# f f 0"),
            Some(SkydimoRgb {
                r: 255,
                g: 255,
                b: 0
            })
        );
        assert_eq!(parse_hex_color("#xyz"), None);
    }

    #[test]
    fn c_remainder_matches_reference_shape() {
        assert_eq!(c_remainder(361.0, 360.0).abs() as usize, 1);
        assert_eq!(c_remainder(-1.0, 360.0).abs() as usize, 1);
        assert_eq!(c_remainder(-361.0, 360.0).abs() as usize, 1);
        assert_eq!(phase_hue(-720.0), 0);
    }

    #[test]
    fn custom_black_palette_renders_black() {
        let mut effect = HypnotoadEffect::new();
        effect.resize(4, 1, 4);
        effect.update_params(r##"{"color_mode":1,"colors":["#000000"]}"##);
        let mut pixels = [SkydimoRgb::default(); 4];
        effect.tick(1.0, &mut pixels);
        assert!(pixels.iter().all(|pixel| *pixel == SkydimoRgb::default()));
        assert_eq!(effect.config.color_mode, COLOR_MODE_CUSTOM);
    }
}
