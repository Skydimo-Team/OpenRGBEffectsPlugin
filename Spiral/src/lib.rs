use std::ffi::{c_char, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};

const SKYDIMO_NATIVE_C_ABI_VERSION: u32 = 3;
const SKYDIMO_PLUGIN_KIND_EFFECT: u32 = 1 << 0;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkydimoStr {
    pub ptr: *const c_char,
    pub len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkydimoRgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkydimoFloatSliceV1 {
    pub ptr: *const f32,
    pub len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkydimoAudioFrameV1 {
    pub amplitude: f32,
    pub bins: SkydimoFloatSliceV1,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkydimoRgbFrameV1 {
    pub width: usize,
    pub height: usize,
    pub pixels: *const SkydimoRgb,
    pub pixels_len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkydimoMatrixMapV1 {
    pub width: usize,
    pub height: usize,
    pub map: *const i64,
    pub map_len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkydimoLayoutTransformV1 {
    pub flip_horizontal: u8,
    pub flip_vertical: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkydimoOutputCapabilitiesV1 {
    pub editable: u8,
    pub min_total_leds: usize,
    pub max_total_leds: usize,
    pub allowed_total_leds: *const usize,
    pub allowed_total_leds_len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkydimoOutputDefinitionV1 {
    pub id: SkydimoStr,
    pub name: SkydimoStr,
    pub output_type: u32,
    pub leds_count: usize,
    pub matrix: *const SkydimoMatrixMapV1,
    pub transform: SkydimoLayoutTransformV1,
    pub capabilities: SkydimoOutputCapabilitiesV1,
    pub default_effect: SkydimoStr,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkydimoOutputFrameV1 {
    pub output_id: SkydimoStr,
    pub colors: *const SkydimoRgb,
    pub colors_len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkydimoLedColorV1 {
    pub index: usize,
    pub color: SkydimoRgb,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkydimoDeviceInfoV1 {
    pub manufacturer: SkydimoStr,
    pub model: SkydimoStr,
    pub serial_id: SkydimoStr,
    pub description: SkydimoStr,
    pub device_type: u32,
    pub image_url: SkydimoStr,
    pub controller_id: SkydimoStr,
    pub controller_name: SkydimoStr,
    pub device_path: SkydimoStr,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkydimoHardwareCandidateV1 {
    pub candidate_type: u32,
    pub port_key: SkydimoStr,
    pub path: SkydimoStr,
    pub vendor_id: u32,
    pub product_id: u32,
    pub has_vendor_id: u8,
    pub has_product_id: u8,
    pub interface_number: i32,
    pub has_interface_number: u8,
    pub serial_number: SkydimoStr,
    pub manufacturer_string: SkydimoStr,
    pub product_string: SkydimoStr,
}

type HostLogFn = unsafe extern "C" fn(*mut c_void, u32, *const c_char, usize);
type HostCallJsonFn = unsafe extern "C" fn(
    *mut c_void,
    *const c_char,
    usize,
    *const c_char,
    usize,
    *mut u8,
    usize,
    *mut usize,
) -> i32;
type EffectAudioCaptureFn =
    unsafe extern "C" fn(*mut c_void, usize, *mut SkydimoAudioFrameV1) -> i32;
type EffectRgbCaptureFn =
    unsafe extern "C" fn(*mut c_void, usize, usize, *mut SkydimoRgbFrameV1) -> i32;
type HostPluginIdFn = unsafe extern "C" fn(*mut c_void, *mut SkydimoStr) -> i32;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SkydimoHostApiV1 {
    pub size: u32,
    pub abi_version: u32,
    pub host_ctx: *mut c_void,
    pub log: Option<HostLogFn>,
    pub call_json: Option<HostCallJsonFn>,
    pub controller_set_device_info:
        Option<unsafe extern "C" fn(*mut c_void, *const SkydimoDeviceInfoV1) -> i32>,
    pub controller_add_output:
        Option<unsafe extern "C" fn(*mut c_void, *const SkydimoOutputDefinitionV1) -> i32>,
    pub controller_output_led_count:
        Option<unsafe extern "C" fn(*mut c_void, *const c_char, usize) -> usize>,
    pub controller_get_rgb_bytes:
        Option<unsafe extern "C" fn(*mut c_void, *const c_char, usize, *mut u8, usize) -> isize>,
    pub controller_write: Option<unsafe extern "C" fn(*mut c_void, *const u8, usize) -> isize>,
    pub controller_read: Option<unsafe extern "C" fn(*mut c_void, *mut u8, usize, u32) -> isize>,
    pub controller_hid_send_feature_report:
        Option<unsafe extern "C" fn(*mut c_void, *const u8, usize) -> isize>,
    pub controller_hid_get_feature_report:
        Option<unsafe extern "C" fn(*mut c_void, *mut u8, usize, u8) -> isize>,
    pub extension_lock_leds: Option<
        unsafe extern "C" fn(
            *mut c_void,
            *const c_char,
            usize,
            *const c_char,
            usize,
            *const usize,
            usize,
            *mut usize,
            *mut usize,
        ) -> i32,
    >,
    pub extension_unlock_leds: Option<
        unsafe extern "C" fn(
            *mut c_void,
            *const c_char,
            usize,
            *const c_char,
            usize,
            *const usize,
            usize,
        ) -> i32,
    >,
    pub extension_set_leds_rgb: Option<
        unsafe extern "C" fn(
            *mut c_void,
            *const c_char,
            usize,
            *const c_char,
            usize,
            *const SkydimoLedColorV1,
            usize,
        ) -> i32,
    >,
    pub effect_audio_capture: Option<EffectAudioCaptureFn>,
    pub effect_screen_capture: Option<EffectRgbCaptureFn>,
    pub effect_album_art: Option<EffectRgbCaptureFn>,
    pub get_plugin_id: Option<HostPluginIdFn>,
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

const COLOR_MODE_RAINBOW: u32 = 0;
const COLOR_MODE_SINGLE: u32 = 1;
const DIR_DEFAULT: u32 = 0;
const DIR_REVERSED: u32 = 1;

#[derive(Clone, Copy, Default)]
struct Rgb {
    r: u8,
    g: u8,
    b: u8,
}

#[derive(Clone, Copy)]
struct SpiralConfig {
    speed: f64,
    shape: f64,
    direction: u32,
    color_mode: u32,
    color: Rgb,
    user_h: f32,
    user_s: f32,
}

impl Default for SpiralConfig {
    fn default() -> Self {
        let mut config = Self {
            speed: 50.0,
            shape: 10.0,
            direction: DIR_DEFAULT,
            color_mode: COLOR_MODE_RAINBOW,
            color: Rgb { r: 255, g: 0, b: 0 },
            user_h: 0.0,
            user_s: 1.0,
        };
        config.update_user_hsv();
        config
    }
}

impl SpiralConfig {
    fn update_user_hsv(&mut self) {
        let (h, s, _) = rgb_to_hsv(self.color);
        self.user_h = h;
        self.user_s = s;
    }
}

struct SpiralEffect {
    config: SpiralConfig,
    width: usize,
    height: usize,
    time_acc: f64,
    prev_t: f64,
    rainbow_lut: [SkydimoRgb; 360],
    single_lut: [SkydimoRgb; 360],
    spatial: Vec<f64>,
    spatial_width: usize,
    spatial_height: usize,
    spatial_shape: f64,
    spatial_direction: u32,
}

impl Default for SpiralEffect {
    fn default() -> Self {
        let config = SpiralConfig::default();
        let single_lut = build_single_lut(config.user_h, config.user_s);
        Self {
            config,
            width: 0,
            height: 1,
            time_acc: 0.0,
            prev_t: 0.0,
            rainbow_lut: build_rainbow_lut(),
            single_lut,
            spatial: Vec::new(),
            spatial_width: 0,
            spatial_height: 0,
            spatial_shape: f64::NAN,
            spatial_direction: u32::MAX,
        }
    }
}

impl SpiralEffect {
    fn resize(&mut self, width: u32, height: u32, led_count: u32) {
        self.width = if width == 0 {
            led_count.max(1) as usize
        } else {
            width as usize
        };
        self.height = height.max(1) as usize;
        self.invalidate_spatial();
    }

    fn update_params(&mut self, json: &str) {
        if let Some(speed) = parse_number_field(json, "speed") {
            self.config.speed = speed;
        }
        if let Some(shape) = parse_number_field(json, "shape") {
            if self.config.shape != shape {
                self.config.shape = shape;
                self.invalidate_spatial();
            }
        }
        if let Some(direction) = parse_number_field(json, "direction") {
            let value = (direction + 0.5).floor().max(0.0) as u32;
            if matches!(value, DIR_DEFAULT | DIR_REVERSED) && self.config.direction != value {
                self.config.direction = value;
                self.invalidate_spatial();
            }
        }
        if let Some(mode) = parse_number_field(json, "color_mode") {
            let value = (mode + 0.5).floor().max(0.0) as u32;
            if matches!(value, COLOR_MODE_RAINBOW | COLOR_MODE_SINGLE) {
                self.config.color_mode = value;
            }
        }
        if let Some(raw) = parse_string_field(json, "color") {
            if let Some(color) = parse_hex_color(raw) {
                self.config.color = color;
                self.config.update_user_hsv();
                self.single_lut = build_single_lut(self.config.user_h, self.config.user_s);
            }
        }
    }

    fn tick(&mut self, elapsed_seconds: f64, pixels: &mut [SkydimoRgb]) {
        if pixels.is_empty() {
            return;
        }

        let dt = elapsed_seconds - self.prev_t;
        self.prev_t = elapsed_seconds;
        if dt.is_finite() {
            self.time_acc += self.config.speed * 4.0 * dt;
        }

        let (width, height) = self.effective_layout(pixels.len());
        self.ensure_spatial(width, height);

        let total = pixels.len().min(self.spatial.len());
        if total == 0 {
            pixels.fill(SkydimoRgb::default());
            return;
        }

        let time = self.time_acc;
        if self.config.color_mode == COLOR_MODE_RAINBOW {
            for (pixel, base) in pixels.iter_mut().zip(self.spatial.iter()).take(total) {
                let idx = c_abs_int_mod(*base - time, 360) as usize;
                *pixel = self.rainbow_lut[idx];
            }
        } else {
            for (pixel, base) in pixels.iter_mut().zip(self.spatial.iter()).take(total) {
                let idx = c_abs_int_mod(*base - time, 360) as usize;
                *pixel = self.single_lut[idx];
            }
        }

        if total < pixels.len() {
            pixels[total..].fill(SkydimoRgb::default());
        }
    }

    fn effective_layout(&self, len: usize) -> (usize, usize) {
        let width = if self.width == 0 { len } else { self.width }.max(1);
        let height = if self.height == 0 { 1 } else { self.height }.max(1);
        (width, height)
    }

    fn ensure_spatial(&mut self, width: usize, height: usize) {
        if self.spatial_width == width
            && self.spatial_height == height
            && self.spatial_shape == self.config.shape
            && self.spatial_direction == self.config.direction
            && !self.spatial.is_empty()
        {
            return;
        }

        self.spatial_width = width;
        self.spatial_height = height;
        self.spatial_shape = self.config.shape;
        self.spatial_direction = self.config.direction;

        let total = width.saturating_mul(height);
        self.spatial.clear();
        self.spatial.reserve(total);
        if total == 0 {
            return;
        }

        let (cx, cy) = if height <= 1 {
            (width as f64 * 0.5, 0.5)
        } else {
            ((width - 1) as f64 * 0.5, (height - 1) as f64 * 0.5)
        };
        let dir_sign = if self.config.direction == DIR_REVERSED {
            1.0
        } else {
            -1.0
        };
        let shape = self.config.shape;
        let inv_pi = 180.0 / std::f64::consts::PI;

        for y in 0..height {
            let yf = y as f64;
            let dy = cy - yf;
            for x in 0..width {
                let xf = x as f64;
                let dx = cx - xf;
                let angle = dir_sign * (xf - cx).atan2(yf - cy) * inv_pi;
                let distance = dx.mul_add(dx, dy * dy).sqrt();
                self.spatial.push(angle + shape * distance);
            }
        }
    }

    fn invalidate_spatial(&mut self) {
        self.spatial_shape = f64::NAN;
    }
}

unsafe extern "C" fn spiral_create(
    _host: *const SkydimoHostApiV1,
    out_instance: *mut *mut c_void,
) -> i32 {
    catch_ffi(|| {
        if out_instance.is_null() {
            return -1;
        }
        let effect = Box::new(SpiralEffect::default());
        unsafe {
            *out_instance = Box::into_raw(effect).cast::<c_void>();
        }
        0
    })
}

unsafe extern "C" fn spiral_destroy(instance: *mut c_void) {
    let _ = catch_ffi(|| {
        if !instance.is_null() {
            unsafe {
                drop(Box::from_raw(instance.cast::<SpiralEffect>()));
            }
        }
        0
    });
}

unsafe extern "C" fn spiral_resize(
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

unsafe extern "C" fn spiral_update_params_json(
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

unsafe extern "C" fn spiral_tick(
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

        let pixels = unsafe { std::slice::from_raw_parts_mut(buffer, len) };
        effect.tick(elapsed_seconds, pixels);
        0
    })
}

unsafe extern "C" fn spiral_is_ready(instance: *mut c_void) -> i32 {
    catch_ffi(|| if instance.is_null() { -1 } else { 1 })
}

#[no_mangle]
/// # Safety
///
/// `out_api` must be a valid writable pointer. The host must pass the ABI
/// version declared in the plugin manifest.
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
                    create: Some(spiral_create),
                    destroy: Some(spiral_destroy),
                    resize: Some(spiral_resize),
                    update_params_json: Some(spiral_update_params_json),
                    tick: Some(spiral_tick),
                    is_ready: Some(spiral_is_ready),
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

fn effect_mut(instance: *mut c_void) -> Option<&'static mut SpiralEffect> {
    if instance.is_null() {
        None
    } else {
        Some(unsafe { &mut *instance.cast::<SpiralEffect>() })
    }
}

fn catch_ffi(f: impl FnOnce() -> i32) -> i32 {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(-100)
}

fn c_abs_int_mod(value: f64, divisor: i64) -> i64 {
    let int_val = value.trunc() as i64;
    (int_val % divisor).abs()
}

fn build_rainbow_lut() -> [SkydimoRgb; 360] {
    let mut lut = [SkydimoRgb::default(); 360];
    for (hue, color) in lut.iter_mut().enumerate() {
        *color = hsv_to_rgb(hue as f32, 1.0, 1.0);
    }
    lut
}

fn build_single_lut(h: f32, s: f32) -> [SkydimoRgb; 360] {
    let mut lut = [SkydimoRgb::default(); 360];
    for (idx, color) in lut.iter_mut().enumerate() {
        let value = 1.0 - (idx as f32 / 360.0);
        *color = hsv_to_rgb(h, s, value);
    }
    lut
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

fn rgb_to_hsv(rgb: Rgb) -> (f32, f32, f32) {
    let rf = rgb.r as f32 / 255.0;
    let gf = rgb.g as f32 / 255.0;
    let bf = rgb.b as f32 / 255.0;
    let maxc = rf.max(gf).max(bf);
    let minc = rf.min(gf).min(bf);
    let delta = maxc - minc;

    if maxc == 0.0 || delta == 0.0 {
        return (0.0, 0.0, maxc);
    }

    let h = if maxc == rf {
        60.0 * ((gf - bf) / delta).rem_euclid(6.0)
    } else if maxc == gf {
        60.0 * (((bf - rf) / delta) + 2.0)
    } else {
        60.0 * (((rf - gf) / delta) + 4.0)
    };
    let s = delta / maxc;
    (h.rem_euclid(360.0), s, maxc)
}

fn to_u8(value: f32) -> u8 {
    value.round().clamp(0.0, 255.0) as u8
}

fn parse_number_field(json: &str, key: &str) -> Option<f64> {
    let raw = json_value_slice(json, key)?;
    raw.parse::<f64>().ok()
}

fn parse_string_field<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let raw = json_value_slice(json, key)?;
    raw.strip_prefix('"')?.strip_suffix('"')
}

fn json_value_slice<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("\"{key}\"");
    let pos = json.find(&needle)?;
    let after_key = &json[pos + needle.len()..];
    let colon = after_key.find(':')?;
    let raw = after_key[colon + 1..].trim_start();

    if let Some(rest) = raw.strip_prefix('"') {
        let end = rest.find('"')?;
        return Some(raw[..end + 2].trim());
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
    Some(raw[..end].trim())
}

fn parse_hex_color(raw: &str) -> Option<Rgb> {
    let mut hex = raw.trim();
    if let Some(stripped) = hex.strip_prefix('#') {
        hex = stripped;
    }

    if hex.len() == 3 {
        let bytes = hex.as_bytes();
        return Some(Rgb {
            r: parse_hex_nibble(bytes[0])? * 17,
            g: parse_hex_nibble(bytes[1])? * 17,
            b: parse_hex_nibble(bytes[2])? * 17,
        });
    }
    if hex.len() != 6 {
        return None;
    }

    let bytes = hex.as_bytes();
    Some(Rgb {
        r: parse_hex_byte(bytes[0], bytes[1])?,
        g: parse_hex_byte(bytes[2], bytes[3])?,
        b: parse_hex_byte(bytes[4], bytes[5])?,
    })
}

fn parse_hex_byte(hi: u8, lo: u8) -> Option<u8> {
    Some((parse_hex_nibble(hi)? << 4) | parse_hex_nibble(lo)?)
}

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
    use super::{c_abs_int_mod, parse_hex_color, rgb_to_hsv, Rgb, SpiralEffect};

    #[test]
    fn c_abs_int_mod_matches_cpp_signed_remainder_shape() {
        assert_eq!(c_abs_int_mod(361.9, 360), 1);
        assert_eq!(c_abs_int_mod(-1.9, 360), 1);
        assert_eq!(c_abs_int_mod(-361.2, 360), 1);
    }

    #[test]
    fn parses_full_and_short_hex_colors() {
        let full = parse_hex_color("#00B3FF").expect("full hex");
        assert_eq!((full.r, full.g, full.b), (0, 179, 255));

        let short = parse_hex_color("#0af").expect("short hex");
        assert_eq!((short.r, short.g, short.b), (0, 170, 255));
    }

    #[test]
    fn rgb_to_hsv_handles_user_color() {
        let (h, s, v) = rgb_to_hsv(Rgb { r: 255, g: 0, b: 0 });
        assert_eq!(h, 0.0);
        assert_eq!(s, 1.0);
        assert_eq!(v, 1.0);
    }

    #[test]
    fn precomputes_spatial_terms_for_current_layout() {
        let mut effect = SpiralEffect::default();
        effect.resize(4, 1, 4);
        effect.ensure_spatial(4, 1);
        assert_eq!(effect.spatial.len(), 4);
        assert_eq!(effect.spatial_width, 4);
        assert_eq!(effect.spatial_height, 1);
    }
}
