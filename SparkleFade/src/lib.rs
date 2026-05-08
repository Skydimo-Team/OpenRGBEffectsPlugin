use std::ffi::{c_char, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::{SystemTime, UNIX_EPOCH};

const SKYDIMO_NATIVE_C_ABI_VERSION: u32 = 3;
const SKYDIMO_PLUGIN_KIND_EFFECT: u32 = 1 << 0;

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

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkydimoStr {
    pub ptr: *const c_char,
    pub len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
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
    pub get_plugin_id: Option<unsafe extern "C" fn(*mut c_void, *mut SkydimoStr) -> i32>,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SparkleState {
    Off,
    FadeIn,
    On,
    FadeOut,
}

#[derive(Clone, Copy)]
struct SparkleConfig {
    off_time: f32,
    fade_in_time: f32,
    fade_out_time: f32,
    random_enabled: bool,
    user_color: SkydimoRgb,
}

impl Default for SparkleConfig {
    fn default() -> Self {
        Self {
            off_time: 0.5,
            fade_in_time: 3.0,
            fade_out_time: 10.0,
            random_enabled: false,
            user_color: SkydimoRgb { r: 255, g: 0, b: 0 },
        }
    }
}

struct SparkleFadeEffect {
    config: SparkleConfig,
    state: SparkleState,
    state_start_time: f32,
    base_color: SkydimoRgb,
    led_fade_start: Vec<f32>,
    led_fade_period: Vec<f32>,
    rng: FastRng,
}

impl SparkleFadeEffect {
    fn new(seed: u32) -> Self {
        Self {
            config: SparkleConfig::default(),
            state: SparkleState::Off,
            state_start_time: 0.0,
            base_color: SkydimoRgb { r: 255, g: 0, b: 0 },
            led_fade_start: Vec::new(),
            led_fade_period: Vec::new(),
            rng: FastRng::new(seed),
        }
    }

    fn resize(&mut self, led_count: usize) {
        self.ensure_led_count(led_count);
    }

    fn update_params(&mut self, json: &str) {
        if let Some(off_time) = parse_number_field(json, "off_time") {
            self.config.off_time = off_time.max(0.0);
        }
        if let Some(fade_in_time) = parse_number_field(json, "fade_in_time") {
            self.config.fade_in_time = fade_in_time.max(0.001);
        }
        if let Some(fade_out_time) = parse_number_field(json, "fade_out_time") {
            self.config.fade_out_time = fade_out_time.max(0.001);
        }
        if let Some(random_enabled) = parse_bool_field(json, "random") {
            self.config.random_enabled = random_enabled;
        }
        if let Some(color) = parse_color_field(json, "color") {
            self.config.user_color = color;
        }
    }

    fn tick(&mut self, elapsed_seconds: f32, pixels: &mut [SkydimoRgb]) {
        if pixels.is_empty() {
            return;
        }

        self.ensure_led_count(pixels.len());
        let elapsed_in_state = (elapsed_seconds - self.state_start_time).max(0.0);

        match self.state {
            SparkleState::Off => {
                fill_black(pixels);
                if elapsed_in_state >= self.config.off_time {
                    self.state = SparkleState::FadeIn;
                    self.state_start_time = elapsed_seconds;
                    self.base_color = if self.config.random_enabled {
                        random_full_color(&mut self.rng)
                    } else {
                        self.config.user_color
                    };
                }
            }
            SparkleState::FadeIn => {
                if elapsed_in_state >= self.config.fade_in_time {
                    self.state = SparkleState::On;
                    self.state_start_time = elapsed_seconds;
                } else {
                    let multiplier = elapsed_in_state / self.config.fade_in_time;
                    pixels.fill(scale_color(self.base_color, multiplier));
                }
            }
            SparkleState::On => {
                let max_half = (self.config.fade_out_time * 0.5).max(0.001);
                for (fade_start, fade_period) in self
                    .led_fade_start
                    .iter_mut()
                    .zip(self.led_fade_period.iter_mut())
                    .take(pixels.len())
                {
                    *fade_start = elapsed_seconds + self.rng.next_unit() * max_half;
                    *fade_period = (self.rng.next_unit() * max_half).max(0.001);
                }

                self.state = SparkleState::FadeOut;
                self.state_start_time = elapsed_seconds;
                pixels.fill(self.base_color);
            }
            SparkleState::FadeOut => {
                let mut all_faded = true;

                for (pixel, (&fade_start, &fade_period)) in pixels.iter_mut().zip(
                    self.led_fade_start
                        .iter()
                        .zip(self.led_fade_period.iter()),
                ) {
                    *pixel = if elapsed_seconds >= fade_start + fade_period {
                        SkydimoRgb::default()
                    } else {
                        all_faded = false;
                        if elapsed_seconds >= fade_start {
                            let progress = (elapsed_seconds - fade_start) / fade_period;
                            scale_color(self.base_color, 1.0 - progress)
                        } else {
                            self.base_color
                        }
                    };
                }

                if all_faded {
                    self.state = SparkleState::Off;
                    self.state_start_time = elapsed_seconds;
                }
            }
        }
    }

    fn ensure_led_count(&mut self, led_count: usize) {
        if self.led_fade_start.len() == led_count {
            return;
        }
        self.led_fade_start.clear();
        self.led_fade_period.clear();
        self.led_fade_start.resize(led_count, 0.0);
        self.led_fade_period.resize(led_count, 0.5);
    }
}

#[derive(Clone, Copy)]
struct FastRng {
    state: u32,
}

impl FastRng {
    fn new(seed: u32) -> Self {
        Self { state: seed.max(1) }
    }

    #[inline]
    fn next_u32(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x.max(1);
        x
    }

    #[inline]
    fn next_unit(&mut self) -> f32 {
        self.next_u32() as f32 / u32::MAX as f32
    }
}

unsafe extern "C" fn sparkle_fade_create(
    host: *const SkydimoHostApiV1,
    out_instance: *mut *mut c_void,
) -> i32 {
    catch_ffi(|| {
        if out_instance.is_null() || host.is_null() {
            return -1;
        }
        let host = unsafe { &*host };
        if host.abi_version < SKYDIMO_NATIVE_C_ABI_VERSION {
            return -2;
        }

        let seed = random_seed(host.host_ctx);
        let effect = Box::new(SparkleFadeEffect::new(seed));
        unsafe {
            *out_instance = Box::into_raw(effect).cast::<c_void>();
        }
        0
    })
}

unsafe extern "C" fn sparkle_fade_destroy(instance: *mut c_void) {
    let _ = catch_ffi(|| {
        if !instance.is_null() {
            unsafe {
                drop(Box::from_raw(instance.cast::<SparkleFadeEffect>()));
            }
        }
        0
    });
}

unsafe extern "C" fn sparkle_fade_resize(
    instance: *mut c_void,
    width: u32,
    height: u32,
    led_count: u32,
) -> i32 {
    catch_ffi(|| {
        let Some(effect) = effect_mut(instance) else {
            return -1;
        };
        let total = if led_count == 0 {
            width.max(1).saturating_mul(height.max(1))
        } else {
            led_count
        };
        effect.resize(total as usize);
        0
    })
}

unsafe extern "C" fn sparkle_fade_update_params_json(
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

unsafe extern "C" fn sparkle_fade_tick(
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
        if !elapsed_seconds.is_finite() {
            return -3;
        }

        let pixels = unsafe { std::slice::from_raw_parts_mut(buffer, len) };
        let t = elapsed_seconds.max(0.0).min(f32::MAX as f64) as f32;
        effect.tick(t, pixels);
        0
    })
}

unsafe extern "C" fn sparkle_fade_is_ready(instance: *mut c_void) -> i32 {
    catch_ffi(|| if instance.is_null() { -1 } else { 1 })
}

#[no_mangle]
/// # Safety
///
/// `out_api` must be a valid, writable pointer to a `SkydimoPluginApiV1`.
/// The host must pass the ABI version it expects in `requested_abi_version`.
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
                    create: Some(sparkle_fade_create),
                    destroy: Some(sparkle_fade_destroy),
                    resize: Some(sparkle_fade_resize),
                    update_params_json: Some(sparkle_fade_update_params_json),
                    tick: Some(sparkle_fade_tick),
                    is_ready: Some(sparkle_fade_is_ready),
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

fn effect_mut(instance: *mut c_void) -> Option<&'static mut SparkleFadeEffect> {
    if instance.is_null() {
        None
    } else {
        Some(unsafe { &mut *instance.cast::<SparkleFadeEffect>() })
    }
}

fn catch_ffi(f: impl FnOnce() -> i32) -> i32 {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(-100)
}

fn random_seed(host_ctx: *mut c_void) -> u32 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(0);
    let mixed = nanos ^ ((host_ctx as usize as u64).rotate_left(17));
    let seed = (mixed ^ (mixed >> 32)) as u32;
    seed.max(1)
}

fn random_full_color(rng: &mut FastRng) -> SkydimoRgb {
    hsv_to_rgb(rng.next_unit() * 360.0, 1.0, 1.0)
}

#[inline]
fn fill_black(pixels: &mut [SkydimoRgb]) {
    pixels.fill(SkydimoRgb::default());
}

#[inline]
fn scale_color(color: SkydimoRgb, multiplier: f32) -> SkydimoRgb {
    let multiplier = multiplier.clamp(0.0, 1.0);
    SkydimoRgb {
        r: to_u8(color.r as f32 * multiplier),
        g: to_u8(color.g as f32 * multiplier),
        b: to_u8(color.b as f32 * multiplier),
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
        r: to_u8((r + m) * 255.0),
        g: to_u8((g + m) * 255.0),
        b: to_u8((b + m) * 255.0),
    }
}

#[inline]
fn to_u8(value: f32) -> u8 {
    value.round().clamp(0.0, 255.0) as u8
}

fn parse_number_field(json: &str, key: &str) -> Option<f32> {
    let raw = json_value_slice(json, key)?;
    raw.parse::<f32>().ok()
}

fn parse_bool_field(json: &str, key: &str) -> Option<bool> {
    let raw = json_value_slice(json, key)?;
    match raw {
        "true" | "1" => Some(true),
        "false" | "0" => Some(false),
        _ => None,
    }
}

fn parse_color_field(json: &str, key: &str) -> Option<SkydimoRgb> {
    parse_hex_color(json_value_slice(json, key)?)
}

fn json_value_slice<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("\"{key}\"");
    let pos = json.find(&needle)?;
    let after_key = &json[pos + needle.len()..];
    let colon = after_key.find(':')?;
    let raw = after_key[colon + 1..].trim_start();

    if let Some(rest) = raw.strip_prefix('"') {
        let end = rest.find('"')?;
        return Some(rest[..end].trim());
    }

    let end = raw
        .char_indices()
        .find_map(|(idx, ch)| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '+' | '.') {
                None
            } else {
                Some(idx)
            }
        })
        .unwrap_or(raw.len());
    Some(raw[..end].trim())
}

fn parse_hex_color(value: &str) -> Option<SkydimoRgb> {
    let raw = value.trim();
    let hex = raw.strip_prefix('#').unwrap_or(raw);
    let mut out = [0u8; 6];

    match hex.len() {
        3 => {
            for (idx, byte) in hex.bytes().enumerate() {
                let high = hex_nibble(byte)?;
                out[idx * 2] = high;
                out[idx * 2 + 1] = high;
            }
        }
        6 => {
            for (idx, byte) in hex.bytes().enumerate() {
                out[idx] = hex_nibble(byte)?;
            }
        }
        _ => return None,
    }

    Some(SkydimoRgb {
        r: (out[0] << 4) | out[1],
        g: (out[2] << 4) | out[3],
        b: (out[4] << 4) | out[5],
    })
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_hex_color, SkydimoRgb, SparkleFadeEffect, SparkleState};

    #[test]
    fn parses_short_and_long_hex_colors() {
        assert_eq!(
            parse_hex_color("#f06"),
            Some(SkydimoRgb {
                r: 255,
                g: 0,
                b: 102,
            })
        );
        assert_eq!(
            parse_hex_color("336699"),
            Some(SkydimoRgb {
                r: 51,
                g: 102,
                b: 153,
            })
        );
        assert_eq!(parse_hex_color("#xyz"), None);
    }

    #[test]
    fn advances_through_sparkle_cycle() {
        let mut effect = SparkleFadeEffect::new(1);
        effect.update_params(
            r##"{"off_time":0,"fade_in_time":0.001,"fade_out_time":0.01,"random":false,"color":"#336699"}"##,
        );
        effect.resize(4);

        let mut pixels = vec![SkydimoRgb::default(); 4];
        effect.tick(0.0, &mut pixels);
        assert_eq!(effect.state, SparkleState::FadeIn);
        assert!(pixels.iter().all(|pixel| *pixel == SkydimoRgb::default()));

        effect.tick(0.001, &mut pixels);
        assert_eq!(effect.state, SparkleState::On);

        effect.tick(0.002, &mut pixels);
        assert_eq!(effect.state, SparkleState::FadeOut);
        assert!(pixels.iter().all(|pixel| {
            *pixel
                == SkydimoRgb {
                    r: 51,
                    g: 102,
                    b: 153,
                }
        }));

        effect.tick(1.0, &mut pixels);
        assert_eq!(effect.state, SparkleState::Off);
        assert!(pixels.iter().all(|pixel| *pixel == SkydimoRgb::default()));
    }
}
