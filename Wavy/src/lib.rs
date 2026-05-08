use std::ffi::{c_char, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::{SystemTime, UNIX_EPOCH};

const SKYDIMO_NATIVE_C_ABI_VERSION: u32 = 3;
const SKYDIMO_PLUGIN_KIND_EFFECT: u32 = 1 << 0;
const LOG_INFO: u32 = 2;
const DEFAULT_WIDTH: usize = 1;
const DEFAULT_HEIGHT: usize = 1;

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

#[derive(Clone, Copy)]
struct NativeHost {
    host_ctx: *mut c_void,
    log: Option<HostLogFn>,
}

impl NativeHost {
    fn from_api(host: *const SkydimoHostApiV1) -> Self {
        if host.is_null() {
            return Self {
                host_ctx: std::ptr::null_mut(),
                log: None,
            };
        }

        let host = unsafe { &*host };
        Self {
            host_ctx: host.host_ctx,
            log: host.log,
        }
    }

    fn info(self, msg: &str) {
        if let Some(log) = self.log {
            unsafe {
                log(
                    self.host_ctx,
                    LOG_INFO,
                    msg.as_ptr().cast::<c_char>(),
                    msg.len(),
                );
            }
        }
    }
}

struct WavyEffect {
    wave_frequency: u32,
    wave_speed: u32,
    oscillation_speed: u32,
    random_enabled: bool,
    dir: bool,
    sine_progress: f64,
    wave_progress: f64,
    last_elapsed: Option<f64>,
    width: usize,
    height: usize,
    user_colors: [SkydimoRgb; 2],
    random_colors: [SkydimoRgb; 2],
    row_cache: Vec<SkydimoRgb>,
    rng: FastRng,
}

impl WavyEffect {
    fn new(host: NativeHost) -> Self {
        let mut rng = FastRng::seeded();
        let random_colors = random_color_pair(&mut rng);
        host.info("Wavy native effect initialized");
        Self {
            wave_frequency: 1,
            wave_speed: 50,
            oscillation_speed: 100,
            random_enabled: false,
            dir: true,
            sine_progress: 0.0,
            wave_progress: 0.0,
            last_elapsed: None,
            width: 0,
            height: DEFAULT_HEIGHT,
            user_colors: [
                SkydimoRgb { r: 255, g: 0, b: 0 },
                SkydimoRgb { r: 0, g: 0, b: 255 },
            ],
            random_colors,
            row_cache: Vec::new(),
            rng,
        }
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.width = (width as usize).max(DEFAULT_WIDTH);
        self.height = (height as usize).max(DEFAULT_HEIGHT);
        self.row_cache.clear();
        self.row_cache.reserve(self.width);
    }

    fn update_params(&mut self, json: &str) {
        if let Some(value) = parse_number_field(json, "wave_frequency") {
            self.wave_frequency = rounded_clamped(value, 1, 20);
        }
        if let Some(value) = parse_number_field(json, "wave_speed") {
            self.wave_speed = rounded_clamped(value, 1, 200);
        }
        if let Some(value) = parse_number_field(json, "oscillation_speed") {
            self.oscillation_speed = rounded_clamped(value, 1, 200);
        }
        if let Some(value) = parse_bool_field(json, "random") {
            let was_random = self.random_enabled;
            self.random_enabled = value;
            if self.random_enabled && !was_random {
                self.refresh_random_colors();
            }
        }
        if let Some(colors) = parse_color_pair_field(json, "colors") {
            self.user_colors = colors;
        }
    }

    fn tick(&mut self, elapsed_seconds: f64, pixels: &mut [SkydimoRgb]) -> i32 {
        if pixels.is_empty() {
            return 0;
        }

        let width = if self.width == 0 {
            pixels.len()
        } else {
            self.width
        }
        .max(DEFAULT_WIDTH);
        let height = self.height.max(DEFAULT_HEIGHT);

        self.render_row(width);
        let mut offset = 0usize;
        for _ in 0..height {
            if offset >= pixels.len() {
                break;
            }
            let take = width.min(pixels.len() - offset);
            pixels[offset..offset + take].copy_from_slice(&self.row_cache[..take]);
            offset += take;
        }
        if offset < pixels.len() {
            pixels[offset..].fill(SkydimoRgb::default());
        }

        self.advance(elapsed_seconds);
        0
    }

    fn render_row(&mut self, width: usize) {
        if self.row_cache.len() != width {
            self.row_cache.resize(width, SkydimoRgb::default());
        }

        let colors = if self.random_enabled {
            self.random_colors
        } else {
            self.user_colors
        };
        let c1 = colors[0];
        let c2 = colors[1];
        let width_f = width as f64;
        let wave_offset = self.wave_progress / 100.0;
        let frequency = self.wave_frequency as f64;
        let amplitude = self.sine_progress;

        for (x, out) in self.row_cache.iter_mut().enumerate() {
            let pos = x as f64 / width_f + wave_offset;
            let wave_height = amplitude * (frequency * std::f64::consts::TAU * pos).sin();
            let t = 0.5 + wave_height * 0.5;
            *out = interpolate_color(c1, c2, t);
        }
    }

    fn advance(&mut self, elapsed_seconds: f64) {
        let delta = if elapsed_seconds.is_finite() && elapsed_seconds >= 0.0 {
            let delta = match self.last_elapsed {
                Some(last) if elapsed_seconds >= last => elapsed_seconds - last,
                _ => elapsed_seconds,
            };
            self.last_elapsed = Some(elapsed_seconds);
            delta
        } else {
            0.0
        };

        let sine_inc = delta * 0.01 * self.oscillation_speed as f64;
        if self.dir {
            if self.sine_progress < 1.0 {
                self.sine_progress += sine_inc;
            } else {
                self.dir = false;
                self.sine_progress -= sine_inc;
            }
        } else if self.sine_progress > -1.0 {
            self.sine_progress -= sine_inc;
        } else {
            self.dir = true;
            self.sine_progress += sine_inc;
        }

        if self.random_enabled && (-0.01..=0.01).contains(&self.sine_progress) {
            self.refresh_random_colors();
        }

        self.sine_progress = self.sine_progress.clamp(-1.0, 1.0);

        let wave_inc = delta * 0.05 * self.wave_speed as f64;
        if self.wave_progress < 100.0 {
            self.wave_progress += wave_inc;
        } else {
            self.wave_progress = 0.0;
        }
    }

    fn refresh_random_colors(&mut self) {
        self.random_colors = random_color_pair(&mut self.rng);
    }
}

unsafe extern "C" fn wavy_create(
    host: *const SkydimoHostApiV1,
    out_instance: *mut *mut c_void,
) -> i32 {
    catch_ffi(|| {
        if out_instance.is_null() {
            return -1;
        }
        if !host.is_null() {
            let host_ref = unsafe { &*host };
            if host_ref.abi_version < SKYDIMO_NATIVE_C_ABI_VERSION {
                return -2;
            }
        }

        let effect = Box::new(WavyEffect::new(NativeHost::from_api(host)));
        unsafe {
            *out_instance = Box::into_raw(effect).cast::<c_void>();
        }
        0
    })
}

unsafe extern "C" fn wavy_destroy(instance: *mut c_void) {
    let _ = catch_ffi(|| {
        if !instance.is_null() {
            unsafe {
                drop(Box::from_raw(instance.cast::<WavyEffect>()));
            }
        }
        0
    });
}

unsafe extern "C" fn wavy_resize(
    instance: *mut c_void,
    width: u32,
    height: u32,
    _led_count: u32,
) -> i32 {
    catch_ffi(|| {
        let Some(effect) = effect_mut(instance) else {
            return -1;
        };
        effect.resize(width, height);
        0
    })
}

unsafe extern "C" fn wavy_update_params_json(
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

unsafe extern "C" fn wavy_tick(
    instance: *mut c_void,
    elapsed_seconds: f64,
    buffer: *mut SkydimoRgb,
    len: usize,
) -> i32 {
    catch_ffi(|| {
        let Some(effect) = effect_mut(instance) else {
            return -1;
        };
        if len == 0 {
            return 0;
        }
        if buffer.is_null() {
            return -2;
        }
        let pixels = unsafe { std::slice::from_raw_parts_mut(buffer, len) };
        effect.tick(elapsed_seconds, pixels)
    })
}

unsafe extern "C" fn wavy_is_ready(instance: *mut c_void) -> i32 {
    catch_ffi(|| if instance.is_null() { -1 } else { 1 })
}

#[no_mangle]
/// # Safety
///
/// `out_api` must be writable for one `SkydimoPluginApiV1`.
/// `requested_abi_version` must match the ABI declared by the manifest.
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
                    create: Some(wavy_create),
                    destroy: Some(wavy_destroy),
                    resize: Some(wavy_resize),
                    update_params_json: Some(wavy_update_params_json),
                    tick: Some(wavy_tick),
                    is_ready: Some(wavy_is_ready),
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

fn effect_mut(instance: *mut c_void) -> Option<&'static mut WavyEffect> {
    if instance.is_null() {
        None
    } else {
        Some(unsafe { &mut *instance.cast::<WavyEffect>() })
    }
}

fn catch_ffi(f: impl FnOnce() -> i32) -> i32 {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(-100)
}

fn rounded_clamped(value: f64, min: u32, max: u32) -> u32 {
    if !value.is_finite() {
        return min;
    }
    (value + 0.5).floor().clamp(min as f64, max as f64) as u32
}

fn interpolate_color(a: SkydimoRgb, b: SkydimoRgb, t: f64) -> SkydimoRgb {
    SkydimoRgb {
        r: interpolate_channel(a.r, b.r, t),
        g: interpolate_channel(a.g, b.g, t),
        b: interpolate_channel(a.b, b.b, t),
    }
}

fn interpolate_channel(a: u8, b: u8, t: f64) -> u8 {
    let value = (b as f64 - a as f64) * t + a as f64;
    value.floor().clamp(0.0, 255.0) as u8
}

fn random_color_pair(rng: &mut FastRng) -> [SkydimoRgb; 2] {
    let color = hsv_to_rgb(rng.next_unit() * 360.0, 1.0, 1.0);
    [
        color,
        SkydimoRgb {
            r: 255 - color.r,
            g: 255 - color.g,
            b: 255 - color.b,
        },
    ]
}

fn hsv_to_rgb(h: f64, s: f64, v: f64) -> SkydimoRgb {
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

fn to_u8(value: f64) -> u8 {
    value.round().clamp(0.0, 255.0) as u8
}

fn parse_number_field(json: &str, key: &str) -> Option<f64> {
    json_value_slice(json, key)?.parse::<f64>().ok()
}

fn parse_bool_field(json: &str, key: &str) -> Option<bool> {
    match json_value_slice(json, key)? {
        "true" | "1" => Some(true),
        "false" | "0" => Some(false),
        _ => None,
    }
}

fn parse_color_pair_field(json: &str, key: &str) -> Option<[SkydimoRgb; 2]> {
    let raw = json_array_slice(json, key)?;
    let mut parsed = [SkydimoRgb::default(); 2];
    let mut count = 0usize;
    let bytes = raw.as_bytes();
    let mut pos = 0usize;

    while pos < bytes.len() {
        while pos < bytes.len() && bytes[pos] != b'"' {
            pos += 1;
        }
        if pos >= bytes.len() {
            break;
        }
        let start = pos + 1;
        pos = start;
        let mut escaped = false;
        while pos < bytes.len() {
            let byte = bytes[pos];
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                break;
            }
            pos += 1;
        }
        if pos >= bytes.len() {
            break;
        }
        if let Some(color) = parse_hex_color(&raw[start..pos]) {
            parsed[count] = color;
            count += 1;
            if count == 2 {
                return Some(parsed);
            }
        }
        pos += 1;
    }

    None
}

fn parse_hex_color(value: &str) -> Option<SkydimoRgb> {
    let trimmed = value.trim();
    let hex = trimmed.strip_prefix('#').unwrap_or(trimmed);
    match hex.len() {
        3 => {
            let mut expanded = [0u8; 6];
            let bytes = hex.as_bytes();
            for i in 0..3 {
                expanded[i * 2] = bytes[i];
                expanded[i * 2 + 1] = bytes[i];
            }
            parse_hex6(std::str::from_utf8(&expanded).ok()?)
        }
        6 => parse_hex6(hex),
        _ => None,
    }
}

fn parse_hex6(hex: &str) -> Option<SkydimoRgb> {
    Some(SkydimoRgb {
        r: u8::from_str_radix(&hex[0..2], 16).ok()?,
        g: u8::from_str_radix(&hex[2..4], 16).ok()?,
        b: u8::from_str_radix(&hex[4..6], 16).ok()?,
    })
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

fn json_array_slice<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("\"{key}\"");
    let pos = json.find(&needle)?;
    let after_key = &json[pos + needle.len()..];
    let colon = after_key.find(':')?;
    let raw = after_key[colon + 1..].trim_start();
    let bytes = raw.as_bytes();
    if bytes.first().copied()? != b'[' {
        return None;
    }

    let mut in_string = false;
    let mut escaped = false;
    let mut depth = 0usize;
    for (idx, byte) in bytes.iter().copied().enumerate() {
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            continue;
        }

        match byte {
            b'"' => in_string = true,
            b'[' => depth += 1,
            b']' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(&raw[1..idx]);
                }
            }
            _ => {}
        }
    }
    None
}

struct FastRng {
    state: u64,
}

impl FastRng {
    fn seeded() -> Self {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos() as u64)
            .unwrap_or(0x9E37_79B9_7F4A_7C15);
        Self {
            state: seed ^ 0xA076_1D64_78BD_642F,
        }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn next_unit(&mut self) -> f64 {
        const INV_2_53: f64 = 1.0 / ((1u64 << 53) as f64);
        ((self.next_u64() >> 11) as f64) * INV_2_53
    }
}
