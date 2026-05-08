use std::ffi::{c_char, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::{SystemTime, UNIX_EPOCH};

const SKYDIMO_NATIVE_C_ABI_VERSION: u32 = 3;
const SKYDIMO_PLUGIN_KIND_EFFECT: u32 = 1 << 0;
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
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
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

struct GradientWaveEffect {
    speed: f64,
    random_enabled: bool,
    user_colors: [SkydimoRgb; 2],
    random_colors: [SkydimoRgb; 2],
    progress: f64,
    last_elapsed: Option<f64>,
    width: usize,
    height: usize,
    row_cache: Vec<SkydimoRgb>,
    rng: FastRng,
}

impl Default for GradientWaveEffect {
    fn default() -> Self {
        let mut rng = FastRng::seeded();
        let random_colors = [random_rgb_color(&mut rng), random_rgb_color(&mut rng)];
        Self {
            speed: 10.0,
            random_enabled: false,
            user_colors: [
                SkydimoRgb { r: 255, g: 0, b: 0 },
                SkydimoRgb { r: 0, g: 0, b: 255 },
            ],
            random_colors,
            progress: 0.0,
            last_elapsed: None,
            width: 0,
            height: DEFAULT_HEIGHT,
            row_cache: Vec::new(),
            rng,
        }
    }
}

impl GradientWaveEffect {
    fn resize(&mut self, width: u32, height: u32, led_count: u32) {
        self.width = if width == 0 {
            (led_count as usize).max(DEFAULT_WIDTH)
        } else {
            width as usize
        };
        self.height = (height as usize).max(DEFAULT_HEIGHT);
        self.row_cache.clear();
        self.row_cache.reserve(self.width);
    }

    fn update_params(&mut self, json: &str) {
        if let Some(speed) = number_field(json, "speed").filter(|speed| speed.is_finite()) {
            self.speed = speed.clamp(1.0, 30.0);
        }
        if let Some(colors) = color_pair_field(json, "colors") {
            self.user_colors = colors;
        }
        if let Some(random) = bool_field(json, "random") {
            let was_random = self.random_enabled;
            self.random_enabled = random;
            if random && !was_random {
                self.refresh_random_colors();
            }
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
        let axis_len = width as f64;
        let current_progress = self.progress;

        self.render_row(width, current_progress);

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

        self.advance(elapsed_seconds, axis_len, current_progress);
        0
    }

    fn render_row(&mut self, width: usize, current_progress: f64) {
        if self.row_cache.len() != width {
            self.row_cache.resize(width, SkydimoRgb::default());
        }

        let colors = if self.random_enabled {
            self.random_colors
        } else {
            self.user_colors
        };
        let start = colors[0];
        let finish = colors[1];
        let axis_len = width as f64;
        let inv_axis_len = 1.0 / axis_len;
        let double_axis_len = axis_len * 2.0;

        for (x, out) in self.row_cache.iter_mut().enumerate() {
            let pos = current_progress + x as f64;
            let gradient_pos = if pos > axis_len {
                double_axis_len - pos
            } else {
                pos
            };
            *out = lerp_rgb(start, finish, gradient_pos.abs() * inv_axis_len);
        }
    }

    fn advance(&mut self, elapsed_seconds: f64, axis_len: f64, current_progress: f64) {
        if !elapsed_seconds.is_finite() || elapsed_seconds < 0.0 {
            return;
        }

        let delta = match self.last_elapsed {
            Some(last) if elapsed_seconds >= last => elapsed_seconds - last,
            _ => elapsed_seconds,
        };
        self.last_elapsed = Some(elapsed_seconds);

        let increment = 0.1 * axis_len * self.speed * delta;
        let cycle_limit = axis_len * 2.0;
        if current_progress < cycle_limit {
            self.progress = current_progress + increment;
        } else {
            self.progress = 0.0;
        }
    }

    fn refresh_random_colors(&mut self) {
        self.random_colors = [
            random_rgb_color(&mut self.rng),
            random_rgb_color(&mut self.rng),
        ];
    }
}

unsafe extern "C" fn gradient_wave_create(
    host: *const SkydimoHostApiV1,
    out_instance: *mut *mut c_void,
) -> i32 {
    catch_ffi(|| {
        if out_instance.is_null() {
            return -1;
        }
        if !host.is_null() {
            let host_ref = unsafe { &*host };
            if host_ref.abi_version != SKYDIMO_NATIVE_C_ABI_VERSION {
                return -2;
            }
        }

        let effect = Box::new(GradientWaveEffect::default());
        unsafe {
            *out_instance = Box::into_raw(effect).cast::<c_void>();
        }
        0
    })
}

unsafe extern "C" fn gradient_wave_destroy(instance: *mut c_void) {
    let _ = catch_ffi(|| {
        if !instance.is_null() {
            unsafe {
                drop(Box::from_raw(instance.cast::<GradientWaveEffect>()));
            }
        }
        0
    });
}

unsafe extern "C" fn gradient_wave_resize(
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

unsafe extern "C" fn gradient_wave_update_params_json(
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

unsafe extern "C" fn gradient_wave_tick(
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

unsafe extern "C" fn gradient_wave_is_ready(instance: *mut c_void) -> i32 {
    catch_ffi(|| if instance.is_null() { -1 } else { 1 })
}

#[no_mangle]
/// # Safety
///
/// `out_api` must be writable for one `SkydimoPluginApiV1`. The host must pass
/// the ABI version declared by this plugin manifest.
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
                    create: Some(gradient_wave_create),
                    destroy: Some(gradient_wave_destroy),
                    resize: Some(gradient_wave_resize),
                    update_params_json: Some(gradient_wave_update_params_json),
                    tick: Some(gradient_wave_tick),
                    is_ready: Some(gradient_wave_is_ready),
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

fn effect_mut(instance: *mut c_void) -> Option<&'static mut GradientWaveEffect> {
    if instance.is_null() {
        None
    } else {
        Some(unsafe { &mut *instance.cast::<GradientWaveEffect>() })
    }
}

fn catch_ffi(f: impl FnOnce() -> i32) -> i32 {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(-100)
}

fn lerp_rgb(start: SkydimoRgb, finish: SkydimoRgb, blend: f64) -> SkydimoRgb {
    SkydimoRgb {
        r: lerp_channel(start.r, finish.r, blend),
        g: lerp_channel(start.g, finish.g, blend),
        b: lerp_channel(start.b, finish.b, blend),
    }
}

fn lerp_channel(start: u8, finish: u8, blend: f64) -> u8 {
    let value = start as f64 + blend * (finish as f64 - start as f64);
    value.trunc().clamp(0.0, 255.0) as u8
}

fn random_rgb_color(rng: &mut FastRng) -> SkydimoRgb {
    hsv_to_rgb(rng.next_unit() * 360.0, 1.0, 1.0)
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

fn number_field(json: &str, key: &str) -> Option<f64> {
    value_slice(json, key)?.parse::<f64>().ok()
}

fn bool_field(json: &str, key: &str) -> Option<bool> {
    match value_slice(json, key)? {
        "true" | "1" => Some(true),
        "false" | "0" => Some(false),
        _ => None,
    }
}

fn color_pair_field(json: &str, key: &str) -> Option<[SkydimoRgb; 2]> {
    let raw = value_slice(json, key)?;
    let bytes = raw.as_bytes();
    if bytes.first().copied() != Some(b'[') {
        return None;
    }

    let mut parsed = [SkydimoRgb::default(); 2];
    let mut count = 0usize;
    let mut i = 1usize;
    while i < bytes.len() {
        while i < bytes.len() && bytes[i] != b'"' {
            if bytes[i] == b']' {
                return None;
            }
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        i += 1;
        let start = i;
        let mut escaped = false;
        while i < bytes.len() {
            if escaped {
                escaped = false;
            } else if bytes[i] == b'\\' {
                escaped = true;
            } else if bytes[i] == b'"' {
                break;
            }
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        if let Some(color) = parse_hex_color(&raw[start..i]) {
            parsed[count] = color;
            count += 1;
            if count == 2 {
                return Some(parsed);
            }
        }
        i += 1;
    }

    None
}

fn value_slice<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let raw = value_start(json, key)?;
    if raw.starts_with('"') {
        let end = quoted_end(raw)?;
        return Some(raw[..end].trim());
    }
    if raw.starts_with('[') {
        let end = bracket_end(raw)?;
        return Some(raw[..end].trim());
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

fn value_start<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("\"{key}\"");
    let pos = json.find(&needle)?;
    let after_key = &json[pos + needle.len()..];
    let colon = after_key.find(':')?;
    Some(after_key[colon + 1..].trim_start())
}

fn quoted_end(raw: &str) -> Option<usize> {
    let bytes = raw.as_bytes();
    let mut i = 1usize;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'"' => return Some(i + 1),
            _ => i += 1,
        }
    }
    None
}

fn bracket_end(raw: &str) -> Option<usize> {
    let bytes = raw.as_bytes();
    let mut depth = 0usize;
    let mut in_string = false;
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if in_string => i += 2,
            b'"' => {
                in_string = !in_string;
                i += 1;
            }
            b'[' if !in_string => {
                depth += 1;
                i += 1;
            }
            b']' if !in_string => {
                depth = depth.saturating_sub(1);
                i += 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => i += 1,
        }
    }
    None
}

fn parse_hex_color(value: &str) -> Option<SkydimoRgb> {
    let mut hex = value.trim();
    if let Some(stripped) = hex.strip_prefix('#') {
        hex = stripped;
    }

    if hex.len() == 3 {
        let bytes = hex.as_bytes();
        return Some(SkydimoRgb {
            r: parse_hex_nibble(bytes[0])? * 17,
            g: parse_hex_nibble(bytes[1])? * 17,
            b: parse_hex_nibble(bytes[2])? * 17,
        });
    }
    if hex.len() != 6 {
        return None;
    }

    let bytes = hex.as_bytes();
    Some(SkydimoRgb {
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

#[cfg(test)]
mod tests {
    use super::{parse_hex_color, GradientWaveEffect, SkydimoRgb};

    #[test]
    fn parses_short_and_long_hex_colors() {
        assert_eq!(
            parse_hex_color("#0af"),
            Some(SkydimoRgb {
                r: 0,
                g: 170,
                b: 255
            })
        );
        assert_eq!(
            parse_hex_color("3366CC"),
            Some(SkydimoRgb {
                r: 51,
                g: 102,
                b: 204
            })
        );
    }

    #[test]
    fn updates_params_from_manifest_json() {
        let mut effect = GradientWaveEffect::default();
        effect.update_params(r##"{"speed":40,"random":true,"colors":["#112233","#445566"]}"##);

        assert_eq!(effect.speed, 30.0);
        assert!(effect.random_enabled);
        assert_eq!(
            effect.user_colors,
            [
                SkydimoRgb {
                    r: 17,
                    g: 34,
                    b: 51
                },
                SkydimoRgb {
                    r: 68,
                    g: 85,
                    b: 102
                }
            ]
        );
    }

    #[test]
    fn renders_gradient_row_and_copies_to_height() {
        let mut effect = GradientWaveEffect::default();
        effect.resize(4, 2, 8);
        let mut pixels = vec![SkydimoRgb::default(); 8];

        effect.tick(0.016, &mut pixels);

        assert_eq!(pixels[0], SkydimoRgb { r: 255, g: 0, b: 0 });
        assert_eq!(pixels[1], SkydimoRgb { r: 191, g: 0, b: 63 });
        assert_eq!(pixels[2], SkydimoRgb { r: 127, g: 0, b: 127 });
        assert_eq!(pixels[3], SkydimoRgb { r: 63, g: 0, b: 191 });
        assert_eq!(&pixels[0..4], &pixels[4..8]);
        assert!(effect.progress > 0.0);
    }
}
