use std::ffi::{c_char, c_void};

const SKYDIMO_NATIVE_C_ABI_VERSION: u32 = 3;
const SKYDIMO_PLUGIN_KIND_EFFECT: u32 = 1 << 0;

const AUDIO_BINS: usize = 256;
const DEFAULT_ART_SIZE: usize = 64;
const MAX_PALETTE_COLORS: usize = 8;
const GAUSSIAN_RADIUS: usize = 8;

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
    pub effect_audio_capture:
        Option<unsafe extern "C" fn(*mut c_void, usize, *mut SkydimoAudioFrameV1) -> i32>,
    pub effect_screen_capture:
        Option<unsafe extern "C" fn(*mut c_void, usize, usize, *mut SkydimoRgbFrameV1) -> i32>,
    pub effect_album_art:
        Option<unsafe extern "C" fn(*mut c_void, usize, usize, *mut SkydimoRgbFrameV1) -> i32>,
    pub get_plugin_id: Option<unsafe extern "C" fn(*mut c_void, *mut SkydimoStr) -> i32>,
}

impl Default for SkydimoHostApiV1 {
    fn default() -> Self {
        Self {
            size: std::mem::size_of::<Self>() as u32,
            abi_version: SKYDIMO_NATIVE_C_ABI_VERSION,
            host_ctx: std::ptr::null_mut(),
            log: None,
            call_json: None,
            controller_set_device_info: None,
            controller_add_output: None,
            controller_output_led_count: None,
            controller_get_rgb_bytes: None,
            controller_write: None,
            controller_read: None,
            controller_hid_send_feature_report: None,
            controller_hid_get_feature_report: None,
            extension_lock_leds: None,
            extension_unlock_leds: None,
            extension_set_leds_rgb: None,
            effect_audio_capture: None,
            effect_screen_capture: None,
            effect_album_art: None,
            get_plugin_id: None,
        }
    }
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
    speed: f32,
    avg_size: usize,
    repeat: f32,
    thickness: f32,
    glow: f32,
    oscillation: f32,
    color_mode: u32,
    color_change_speed: f32,
    use_album_art: bool,
    background_color: SkydimoRgb,
    wave_color: SkydimoRgb,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            speed: 50.0,
            avg_size: 8,
            repeat: 1.0,
            thickness: 10.0,
            glow: 50.0,
            oscillation: 0.0,
            color_mode: 0,
            color_change_speed: 50.0,
            use_album_art: false,
            background_color: SkydimoRgb { r: 0, g: 0, b: 0 },
            wave_color: SkydimoRgb { r: 0, g: 255, b: 0 },
        }
    }
}

#[derive(Clone, Copy, Default)]
struct PaletteEntry {
    color: SkydimoRgb,
    weight: f32,
}

#[derive(Clone, Copy)]
struct RenderContext<'a> {
    config: Config,
    color_time: f32,
    palette_time: f32,
    palette: &'a [PaletteEntry],
}

struct AudioSineEffect {
    host: SkydimoHostApiV1,
    config: Config,
    width: usize,
    height: usize,
    x_time: f32,
    oscillation_time: f32,
    color_time: f32,
    palette_time: f32,
    sine_values: Vec<f32>,
    palette: Vec<PaletteEntry>,
    palette_fingerprint: Option<u64>,
    blur_temp: Vec<SkydimoRgb>,
    blur_out: Vec<SkydimoRgb>,
    blur_kernel: Vec<f32>,
}

impl AudioSineEffect {
    fn new(host: SkydimoHostApiV1) -> Self {
        Self {
            host,
            config: Config::default(),
            width: 0,
            height: 1,
            x_time: 0.0,
            oscillation_time: 0.0,
            color_time: 0.0,
            palette_time: 0.0,
            sine_values: Vec::new(),
            palette: Vec::new(),
            palette_fingerprint: None,
            blur_temp: Vec::new(),
            blur_out: Vec::new(),
            blur_kernel: gaussian_kernel(GAUSSIAN_RADIUS),
        }
    }

    fn update_params(&mut self, json: &str) {
        if let Some(value) = json_number(json, "speed") {
            self.config.speed = value.clamp(1.0, 100.0);
        }
        if let Some(value) = json_number(json, "avgSize") {
            self.config.avg_size = (value.floor() as usize).clamp(1, AUDIO_BINS);
        }
        if let Some(value) = json_number(json, "repeat") {
            self.config.repeat = value.clamp(1.0, 40.0);
        }
        if let Some(value) = json_number(json, "thickness") {
            self.config.thickness = value.clamp(0.0, 100.0);
        }
        if let Some(value) = json_number(json, "glow") {
            self.config.glow = value.clamp(1.0, 100.0);
        }
        if let Some(value) = json_number(json, "oscillation") {
            self.config.oscillation = value.clamp(0.0, 100.0);
        }
        if let Some(value) = json_number(json, "colorMode") {
            self.config.color_mode = if value.round() >= 1.0 { 1 } else { 0 };
        }
        if let Some(value) = json_number(json, "colorChangeSpeed") {
            self.config.color_change_speed = value.clamp(0.0, 100.0);
        }
        if let Some(value) = json_bool(json, "useAlbumArt") {
            self.config.use_album_art = value;
            if !value {
                self.palette.clear();
                self.palette_fingerprint = None;
            }
        }
        if let Some(value) = json_string(json, "backgroundColor") {
            self.config.background_color = hex_to_rgb(value);
        }
        if let Some(value) = json_string(json, "waveColor") {
            self.config.wave_color = hex_to_rgb(value);
        }
    }

    fn tick(&mut self, pixels: &mut [SkydimoRgb]) {
        if pixels.is_empty() {
            return;
        }

        if self.config.use_album_art {
            self.refresh_album_palette();
        }

        let avg_size = self.config.avg_size.clamp(1, AUDIO_BINS);
        let Some(bins) = capture_audio_from_host(self.host, avg_size) else {
            pixels.fill(SkydimoRgb::default());
            return;
        };

        let width = self.width.max(1);
        let height = self.height.max(1);
        let is_linear = height == 1 || width == 1;
        let render_width = if is_linear { pixels.len() } else { width };
        let render_height = if is_linear { 1 } else { height };
        let height_mult = if self.config.oscillation > 0.0 {
            (self.oscillation_time * 0.1).sin()
        } else {
            1.0
        };

        compute_sine_values(
            &mut self.sine_values,
            render_width,
            bins,
            avg_size,
            self.config.repeat,
            self.x_time,
            height_mult,
        );

        let palette = if self.config.use_album_art {
            self.palette.as_slice()
        } else {
            &[]
        };
        render_audio_sine(
            pixels,
            render_width,
            render_height,
            &self.sine_values,
            RenderContext {
                config: self.config,
                color_time: self.color_time,
                palette_time: self.palette_time,
                palette,
            },
        );

        self.x_time += self.config.speed / 60.0;
        self.oscillation_time += self.config.oscillation / 60.0;
        self.color_time = (self.color_time + self.config.color_change_speed / 60.0).rem_euclid(360.0);
        if self.config.use_album_art {
            self.palette_time =
                (self.palette_time + self.config.color_change_speed / 60.0).rem_euclid(360.0);
        }
    }

    fn refresh_album_palette(&mut self) {
        let Some(capture) = self.host.effect_album_art else {
            self.palette.clear();
            self.palette_fingerprint = None;
            return;
        };

        let mut frame = SkydimoRgbFrameV1::default();
        let status = unsafe {
            capture(
                self.host.host_ctx,
                DEFAULT_ART_SIZE,
                DEFAULT_ART_SIZE,
                &mut frame,
            )
        };
        let expected = frame.width.saturating_mul(frame.height);
        if status <= 0 || frame.pixels.is_null() || expected == 0 || frame.pixels_len < expected {
            self.palette.clear();
            self.palette_fingerprint = None;
            return;
        }

        let pixels = unsafe { std::slice::from_raw_parts(frame.pixels, expected) };
        let fingerprint = art_fingerprint(pixels);
        if self.palette_fingerprint == Some(fingerprint) && !self.palette.is_empty() {
            return;
        }

        self.palette_fingerprint = Some(fingerprint);
        self.extract_palette(pixels, frame.width, frame.height);
    }

    fn extract_palette(&mut self, pixels: &[SkydimoRgb], width: usize, height: usize) {
        self.palette.clear();
        if pixels.is_empty() || width == 0 || height == 0 {
            return;
        }

        gaussian_blur_rgb(
            pixels,
            width,
            height,
            &self.blur_kernel,
            &mut self.blur_temp,
            &mut self.blur_out,
        );

        let grid_size = 4usize;
        let min_dist_sq = 30i32 * 30i32;
        for gy in 0..grid_size {
            for gx in 0..grid_size {
                let sx = (((gx * width) as f32 / grid_size as f32)
                    + (width as f32 / (grid_size * 2) as f32))
                    .floor() as usize;
                let sy = (((gy * height) as f32 / grid_size as f32)
                    + (height as f32 / (grid_size * 2) as f32))
                    .floor() as usize;
                let sx = sx.min(width - 1);
                let sy = sy.min(height - 1);
                let rgb = self.blur_out[sy * width + sx];
                if rgb.r.max(rgb.g).max(rgb.b) < 24 {
                    continue;
                }

                let mut closest_idx = None;
                let mut closest_dist = i32::MAX;
                for (idx, entry) in self.palette.iter().enumerate() {
                    let dist = rgb_distance_sq(entry.color, rgb);
                    if dist < closest_dist {
                        closest_dist = dist;
                        closest_idx = Some(idx);
                    }
                }

                if let Some(idx) = closest_idx {
                    if closest_dist < min_dist_sq {
                        self.palette[idx].weight += 1.0;
                        continue;
                    }
                }

                if self.palette.len() < MAX_PALETTE_COLORS {
                    self.palette.push(PaletteEntry {
                        color: rgb,
                        weight: 1.0,
                    });
                }
            }
        }

        if self.palette.is_empty() {
            return;
        }

        let total_weight = self.palette.iter().map(|entry| entry.weight).sum::<f32>();
        if total_weight <= 0.0 {
            let uniform = 1.0 / self.palette.len() as f32;
            for entry in &mut self.palette {
                entry.weight = uniform;
            }
        } else {
            for entry in &mut self.palette {
                entry.weight /= total_weight;
            }
        }
        self.palette
            .sort_by(|a, b| b.weight.partial_cmp(&a.weight).unwrap_or(std::cmp::Ordering::Equal));
    }
}

unsafe extern "C" fn audiosine_create(
    host: *const SkydimoHostApiV1,
    out_instance: *mut *mut c_void,
) -> i32 {
    if out_instance.is_null() {
        return -1;
    }

    let host = if host.is_null() {
        SkydimoHostApiV1::default()
    } else {
        *host
    };
    let effect = Box::new(AudioSineEffect::new(host));
    *out_instance = Box::into_raw(effect).cast::<c_void>();
    0
}

unsafe extern "C" fn audiosine_destroy(instance: *mut c_void) {
    if !instance.is_null() {
        drop(Box::from_raw(instance.cast::<AudioSineEffect>()));
    }
}

unsafe extern "C" fn audiosine_resize(
    instance: *mut c_void,
    width: u32,
    height: u32,
    led_count: u32,
) -> i32 {
    let Some(effect) = effect_mut(instance) else {
        return -1;
    };
    let fallback = (led_count as usize).max(1);
    effect.width = if width == 0 { fallback } else { width as usize };
    effect.height = height.max(1) as usize;
    0
}

unsafe extern "C" fn audiosine_update_params_json(
    instance: *mut c_void,
    ptr: *const c_char,
    len: usize,
) -> i32 {
    let Some(effect) = effect_mut(instance) else {
        return -1;
    };
    if ptr.is_null() || len == 0 {
        return 0;
    }
    let bytes = std::slice::from_raw_parts(ptr.cast::<u8>(), len);
    let Ok(json) = std::str::from_utf8(bytes) else {
        return -2;
    };
    effect.update_params(json);
    0
}

unsafe extern "C" fn audiosine_tick(
    instance: *mut c_void,
    _elapsed_seconds: f64,
    buffer: *mut SkydimoRgb,
    len: usize,
) -> i32 {
    let Some(effect) = effect_mut(instance) else {
        return -1;
    };
    if buffer.is_null() && len > 0 {
        return -2;
    }
    if len == 0 {
        return 0;
    }
    let pixels = std::slice::from_raw_parts_mut(buffer, len);
    effect.tick(pixels);
    0
}

unsafe extern "C" fn audiosine_is_ready(instance: *mut c_void) -> i32 {
    if instance.is_null() {
        -1
    } else {
        1
    }
}

#[no_mangle]
/// # Safety
///
/// `out_api` must be a valid, writable pointer to a `SkydimoPluginApiV1`.
/// `requested_abi_version` must match the Core native-c ABI version.
pub unsafe extern "C" fn skydimo_plugin_get_api(
    requested_abi_version: u32,
    _host: *const SkydimoHostApiV1,
    out_api: *mut SkydimoPluginApiV1,
) -> i32 {
    if out_api.is_null() || requested_abi_version != SKYDIMO_NATIVE_C_ABI_VERSION {
        return -1;
    }

    *out_api = SkydimoPluginApiV1 {
        size: std::mem::size_of::<SkydimoPluginApiV1>() as u32,
        abi_version: SKYDIMO_NATIVE_C_ABI_VERSION,
        kind_mask: SKYDIMO_PLUGIN_KIND_EFFECT,
        effect: SkydimoEffectApiV1 {
            size: std::mem::size_of::<SkydimoEffectApiV1>() as u32,
            create: Some(audiosine_create),
            destroy: Some(audiosine_destroy),
            resize: Some(audiosine_resize),
            update_params_json: Some(audiosine_update_params_json),
            tick: Some(audiosine_tick),
            is_ready: Some(audiosine_is_ready),
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
    0
}

unsafe fn effect_mut(instance: *mut c_void) -> Option<&'static mut AudioSineEffect> {
    if instance.is_null() {
        None
    } else {
        Some(&mut *instance.cast::<AudioSineEffect>())
    }
}

fn capture_audio_from_host(host: SkydimoHostApiV1, avg_size: usize) -> Option<&'static [f32]> {
    let capture = host.effect_audio_capture?;
    let mut frame = SkydimoAudioFrameV1::default();
    let status = unsafe { capture(host.host_ctx, avg_size, &mut frame) };
    if status <= 0 || frame.bins.ptr.is_null() || frame.bins.len == 0 {
        return None;
    }
    Some(unsafe { std::slice::from_raw_parts(frame.bins.ptr, frame.bins.len) })
}

fn compute_sine_values(
    out: &mut Vec<f32>,
    width: usize,
    bins: &[f32],
    avg_size: usize,
    repeat: f32,
    x_time: f32,
    height_mult: f32,
) {
    out.resize(width, 0.0);
    let denom = width as f32 + 1.0;
    let avg_size = avg_size.max(1);
    for (x, value) in out.iter_mut().enumerate() {
        let xp = (x as f32 + 1.0 + x_time) / denom;
        let mut sum = 0.0;
        for i in (0..AUDIO_BINS).step_by(avg_size) {
            let bin = bins.get(i).copied().unwrap_or(0.0);
            let harmonic = i as f32 / avg_size as f32;
            sum += height_mult * bin * (xp * 0.25 * repeat * harmonic * std::f32::consts::PI).sin();
        }
        *value = sum;
    }
}

fn render_audio_sine(
    pixels: &mut [SkydimoRgb],
    width: usize,
    height: usize,
    sine_values: &[f32],
    ctx: RenderContext<'_>,
) {
    if width == 0 || height == 0 {
        pixels.fill(SkydimoRgb::default());
        return;
    }

    if height == 1 {
        let max_x = pixels.len().saturating_sub(1).max(1) as f32;
        for (idx, pixel) in pixels.iter_mut().enumerate() {
            let x_percent = idx as f32 / max_x;
            let sine = sine_values.get(idx).copied().unwrap_or(0.0);
            *pixel = color_for_pixel(sine, 0.0, 1.0, x_percent, ctx);
        }
        return;
    }

    let max_x = width.saturating_sub(1).max(1) as f32;
    let total = pixels.len().min(width.saturating_mul(height));
    let mut idx = 0usize;
    for y in 0..height {
        if idx >= total {
            break;
        }
        for x in 0..width {
            if idx >= total {
                break;
            }
            let x_percent = x as f32 / max_x;
            let sine = sine_values.get(x).copied().unwrap_or(0.0);
            pixels[idx] = color_for_pixel(sine, y as f32, height as f32, x_percent, ctx);
            idx += 1;
        }
    }

    for pixel in &mut pixels[total..] {
        *pixel = SkydimoRgb::default();
    }
}

fn color_for_pixel(
    sine: f32,
    y: f32,
    height: f32,
    x_percent: f32,
    ctx: RenderContext<'_>,
) -> SkydimoRgb {
    let config = ctx.config;
    let half_h = height * 0.5;
    let peak = half_h + sine * half_h;
    let real_d = (peak - y).abs();
    let thick_threshold = 0.01 * config.thickness * height;
    let glow_exp = 0.01 * config.glow;
    let distance = if real_d > thick_threshold {
        (real_d / height).powf(glow_exp)
    } else {
        0.0
    }
    .clamp(0.0, 1.0);

    let mut rgb = if config.color_mode == 0 {
        if config.use_album_art && !ctx.palette.is_empty() {
            let sampled = sample_palette(
                ctx.palette,
                x_percent,
                (ctx.palette_time / 360.0).rem_euclid(1.0),
            );
            let (h, s, _) = rgb_to_hsv(sampled);
            hsv_to_rgb(h, s, 1.0 - distance)
        } else {
            hsv_to_rgb(ctx.color_time.rem_euclid(360.0), 1.0, 1.0 - distance)
        }
    } else {
        let base = if config.use_album_art && !ctx.palette.is_empty() {
            sample_palette(
                ctx.palette,
                x_percent,
                (ctx.palette_time / 360.0).rem_euclid(1.0),
            )
        } else {
            config.wave_color
        };
        scale_rgb(base, 1.0 - distance)
    };

    rgb = lerp_rgb(rgb, config.background_color, distance);
    rgb
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

fn rgb_to_hsv(rgb: SkydimoRgb) -> (f32, f32, f32) {
    let rf = rgb.r as f32 / 255.0;
    let gf = rgb.g as f32 / 255.0;
    let bf = rgb.b as f32 / 255.0;
    let maxc = rf.max(gf).max(bf);
    let minc = rf.min(gf).min(bf);
    let delta = maxc - minc;
    let value = maxc;
    let saturation = if maxc == 0.0 { 0.0 } else { delta / maxc };
    let hue = if delta == 0.0 {
        0.0
    } else if maxc == rf {
        60.0 * ((gf - bf) / delta).rem_euclid(6.0)
    } else if maxc == gf {
        60.0 * (((bf - rf) / delta) + 2.0)
    } else {
        60.0 * (((rf - gf) / delta) + 4.0)
    };
    (hue.rem_euclid(360.0), saturation, value)
}

fn scale_rgb(rgb: SkydimoRgb, factor: f32) -> SkydimoRgb {
    let factor = factor.clamp(0.0, 1.0);
    SkydimoRgb {
        r: to_u8(rgb.r as f32 * factor),
        g: to_u8(rgb.g as f32 * factor),
        b: to_u8(rgb.b as f32 * factor),
    }
}

fn lerp_rgb(a: SkydimoRgb, b: SkydimoRgb, t: f32) -> SkydimoRgb {
    let t = t.clamp(0.0, 1.0);
    let inv = 1.0 - t;
    SkydimoRgb {
        r: to_u8(a.r as f32 * inv + b.r as f32 * t),
        g: to_u8(a.g as f32 * inv + b.g as f32 * t),
        b: to_u8(a.b as f32 * inv + b.b as f32 * t),
    }
}

fn sample_palette(palette: &[PaletteEntry], pos_01: f32, flow_phase_01: f32) -> SkydimoRgb {
    if palette.is_empty() {
        return SkydimoRgb::default();
    }
    if palette.len() == 1 {
        return palette[0].color;
    }

    let mut pos = pos_01.clamp(0.0, 1.0) + flow_phase_01.clamp(0.0, 1.0);
    if pos > 1.0 {
        pos -= 1.0;
    }

    let mut cumulative = 0.0;
    let mut index0 = 0usize;
    let mut index1 = 1usize;
    let mut local_t = 0.0;

    for (i, entry) in palette.iter().enumerate() {
        if entry.weight <= 0.0 {
            continue;
        }
        let next_cum = cumulative + entry.weight;
        if pos <= next_cum || i == palette.len() - 1 {
            index0 = i;
            index1 = if i + 1 < palette.len() { i + 1 } else { 0 };
            local_t = ((pos - cumulative) / entry.weight.max(0.0001)).clamp(0.0, 1.0);
            break;
        }
        cumulative = next_cum;
    }

    lerp_rgb(palette[index0].color, palette[index1].color, local_t)
}

fn gaussian_kernel(radius: usize) -> Vec<f32> {
    if radius == 0 {
        return vec![1.0];
    }
    let sigma = radius as f32 / 3.0;
    let sigma2 = 2.0 * sigma * sigma;
    let mut kernel = Vec::with_capacity(radius * 2 + 1);
    let mut sum = 0.0;
    for i in 0..(radius * 2 + 1) {
        let x = i as isize - radius as isize;
        let value = (-((x * x) as f32) / sigma2).exp();
        kernel.push(value);
        sum += value;
    }
    if sum > 0.0 {
        for value in &mut kernel {
            *value /= sum;
        }
    }
    kernel
}

fn gaussian_blur_rgb(
    pixels: &[SkydimoRgb],
    width: usize,
    height: usize,
    kernel: &[f32],
    temp: &mut Vec<SkydimoRgb>,
    out: &mut Vec<SkydimoRgb>,
) {
    let total = width.saturating_mul(height).min(pixels.len());
    temp.resize(total, SkydimoRgb::default());
    out.resize(total, SkydimoRgb::default());
    if total == 0 {
        return;
    }

    let radius = kernel.len() / 2;
    for y in 0..height {
        for x in 0..width {
            let mut rs = 0.0;
            let mut gs = 0.0;
            let mut bs = 0.0;
            for (k, weight) in kernel.iter().enumerate() {
                let dx = k as isize - radius as isize;
                let sx = (x as isize + dx).clamp(0, width as isize - 1) as usize;
                let rgb = pixels[y * width + sx];
                rs += rgb.r as f32 * weight;
                gs += rgb.g as f32 * weight;
                bs += rgb.b as f32 * weight;
            }
            temp[y * width + x] = SkydimoRgb {
                r: to_u8(rs),
                g: to_u8(gs),
                b: to_u8(bs),
            };
        }
    }

    for y in 0..height {
        for x in 0..width {
            let mut rs = 0.0;
            let mut gs = 0.0;
            let mut bs = 0.0;
            for (k, weight) in kernel.iter().enumerate() {
                let dy = k as isize - radius as isize;
                let sy = (y as isize + dy).clamp(0, height as isize - 1) as usize;
                let rgb = temp[sy * width + x];
                rs += rgb.r as f32 * weight;
                gs += rgb.g as f32 * weight;
                bs += rgb.b as f32 * weight;
            }
            out[y * width + x] = SkydimoRgb {
                r: to_u8(rs),
                g: to_u8(gs),
                b: to_u8(bs),
            };
        }
    }
}

fn art_fingerprint(pixels: &[SkydimoRgb]) -> u64 {
    if pixels.is_empty() {
        return 0;
    }
    let mut h = 0x5555_5555u64;
    let step = (pixels.len() / 16).max(1);
    for rgb in pixels.iter().step_by(step) {
        let packed = ((rgb.r as u64) << 16) | ((rgb.g as u64) << 8) | rgb.b as u64;
        h = ((h * 31) + packed) % 0x7fff_ffff;
    }
    h
}

fn rgb_distance_sq(a: SkydimoRgb, b: SkydimoRgb) -> i32 {
    let dr = a.r as i32 - b.r as i32;
    let dg = a.g as i32 - b.g as i32;
    let db = a.b as i32 - b.b as i32;
    dr * dr + dg * dg + db * db
}

fn hex_to_rgb(raw: &str) -> SkydimoRgb {
    let trimmed = raw.trim();
    let hex = trimmed.strip_prefix('#').unwrap_or(trimmed);
    if hex.len() != 6 {
        return SkydimoRgb::default();
    }
    let Ok(r) = u8::from_str_radix(&hex[0..2], 16) else {
        return SkydimoRgb::default();
    };
    let Ok(g) = u8::from_str_radix(&hex[2..4], 16) else {
        return SkydimoRgb::default();
    };
    let Ok(b) = u8::from_str_radix(&hex[4..6], 16) else {
        return SkydimoRgb::default();
    };
    SkydimoRgb { r, g, b }
}

fn json_number(json: &str, key: &str) -> Option<f32> {
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
    raw[..end].trim().parse::<f32>().ok()
}

fn json_bool(json: &str, key: &str) -> Option<bool> {
    let raw = json_value_after_key(json, key)?;
    if raw.starts_with("true") {
        Some(true)
    } else if raw.starts_with("false") {
        Some(false)
    } else {
        json_number(json, key).map(|value| value != 0.0)
    }
}

fn json_string<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let raw = json_value_after_key(json, key)?;
    let raw = raw.strip_prefix('"')?;
    let mut escaped = false;
    for (idx, ch) in raw.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' => escaped = true,
            '"' => return Some(&raw[..idx]),
            _ => {}
        }
    }
    None
}

fn json_value_after_key<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("\"{key}\"");
    let pos = json.find(&needle)?;
    let after_key = &json[pos + needle.len()..];
    let colon_pos = after_key.find(':')?;
    Some(after_key[colon_pos + 1..].trim_start())
}

fn to_u8(value: f32) -> u8 {
    value.round().clamp(0.0, 255.0) as u8
}
