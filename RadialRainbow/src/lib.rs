mod abi;

use std::ffi::{c_char, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};

use abi::{
    SkydimoControllerApiV1, SkydimoEffectApiV1, SkydimoExtensionApiV1, SkydimoHostApiV1,
    SkydimoPluginApiV1, SkydimoRgb, SKYDIMO_NATIVE_C_ABI_VERSION, SKYDIMO_PLUGIN_KIND_EFFECT,
};

const UINT32_WRAP: f64 = 4_294_967_296.0;
const SHAPE_CIRCLES: u32 = 0;
const SHAPE_SQUARES: u32 = 1;

#[derive(Clone, Copy)]
struct Config {
    speed: f64,
    frequency: f64,
    cx_shift: f64,
    cy_shift: f64,
    shape: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            speed: 100.0,
            frequency: 50.0,
            cx_shift: 50.0,
            cy_shift: 50.0,
            shape: SHAPE_CIRCLES,
        }
    }
}

struct RadialRainbowEffect {
    config: Config,
    width: usize,
    height: usize,
    progress: f64,
    last_t: Option<f64>,
    hue_offsets: Vec<f64>,
    cache_width: usize,
    cache_height: usize,
    cache_shape: u32,
    cache_frequency: f64,
    cache_cx_shift: f64,
    cache_cy_shift: f64,
    rainbow_lut: [SkydimoRgb; 360],
}

impl Default for RadialRainbowEffect {
    fn default() -> Self {
        Self {
            config: Config::default(),
            width: 0,
            height: 1,
            progress: 0.0,
            last_t: None,
            hue_offsets: Vec::new(),
            cache_width: 0,
            cache_height: 0,
            cache_shape: u32::MAX,
            cache_frequency: f64::NAN,
            cache_cx_shift: f64::NAN,
            cache_cy_shift: f64::NAN,
            rainbow_lut: build_rainbow_lut(),
        }
    }
}

impl RadialRainbowEffect {
    fn resize(&mut self, width: u32, height: u32, led_count: u32) {
        self.width = if width == 0 {
            led_count.max(1) as usize
        } else {
            width as usize
        };
        self.height = height.max(1) as usize;
        self.invalidate_cache();
    }

    fn update_params(&mut self, json: &str) {
        let old = self.config;

        if let Some(speed) = parse_number_field(json, "speed") {
            self.config.speed = speed.clamp(1.0, 200.0);
        }
        if let Some(frequency) = parse_number_field(json, "frequency") {
            self.config.frequency = frequency.clamp(1.0, 100.0);
        }
        if let Some(cx_shift) = parse_number_field(json, "cx") {
            self.config.cx_shift = cx_shift.clamp(0.0, 100.0);
        }
        if let Some(cy_shift) = parse_number_field(json, "cy") {
            self.config.cy_shift = cy_shift.clamp(0.0, 100.0);
        }
        if let Some(shape) = parse_number_field(json, "shape") {
            let next_shape = trunc_toward_zero(shape);
            if next_shape == SHAPE_CIRCLES as i64 || next_shape == SHAPE_SQUARES as i64 {
                self.config.shape = next_shape as u32;
            }
        }

        if old.frequency != self.config.frequency
            || old.cx_shift != self.config.cx_shift
            || old.cy_shift != self.config.cy_shift
            || old.shape != self.config.shape
        {
            self.invalidate_cache();
        }
    }

    fn tick(&mut self, elapsed_seconds: f64, pixels: &mut [SkydimoRgb]) {
        if pixels.is_empty() {
            return;
        }

        let (width, height) = self.effective_layout(pixels.len());
        self.ensure_hue_offsets(width, height);

        let current_progress = self.progress;
        let total = pixels.len().min(self.hue_offsets.len());
        for (pixel, offset) in pixels.iter_mut().zip(self.hue_offsets.iter()).take(total) {
            let hue = wrap_reference_hue(*offset - current_progress);
            *pixel = self.rainbow_lut[hue];
        }
        if total < pixels.len() {
            pixels[total..].fill(SkydimoRgb::default());
        }

        self.update_progress(elapsed_seconds);
    }

    fn effective_layout(&self, len: usize) -> (usize, usize) {
        let width = if self.width == 0 { len } else { self.width }.max(1);
        let height = if self.height == 0 { 1 } else { self.height }.max(1);
        (width, height)
    }

    fn ensure_hue_offsets(&mut self, width: usize, height: usize) {
        if self.cache_width == width
            && self.cache_height == height
            && self.cache_shape == self.config.shape
            && self.cache_frequency == self.config.frequency
            && self.cache_cx_shift == self.config.cx_shift
            && self.cache_cy_shift == self.config.cy_shift
            && self.hue_offsets.len() == width.saturating_mul(height)
        {
            return;
        }

        self.cache_width = width;
        self.cache_height = height;
        self.cache_shape = self.config.shape;
        self.cache_frequency = self.config.frequency;
        self.cache_cx_shift = self.config.cx_shift;
        self.cache_cy_shift = self.config.cy_shift;

        let total = width.saturating_mul(height);
        self.hue_offsets.clear();
        self.hue_offsets.reserve(total);
        if total == 0 {
            return;
        }

        let is_linear = height <= 1;
        let center_x = if is_linear {
            width as f64 * (self.config.cx_shift / 100.0)
        } else {
            width.saturating_sub(1) as f64 * (self.config.cx_shift / 100.0)
        };
        let center_y = if is_linear {
            0.0
        } else {
            height.saturating_sub(1) as f64 * (self.config.cy_shift / 100.0)
        };
        let band_width = self.config.frequency * 0.5;

        match self.config.shape {
            SHAPE_SQUARES => {
                for y in 0..height {
                    let dy = (center_y - y as f64).abs();
                    for x in 0..width {
                        let dx = (center_x - x as f64).abs();
                        self.hue_offsets.push(dx.max(dy) * band_width);
                    }
                }
            }
            _ => {
                for y in 0..height {
                    let dy = center_y - y as f64;
                    for x in 0..width {
                        let dx = center_x - x as f64;
                        self.hue_offsets
                            .push(dx.mul_add(dx, dy * dy).sqrt() * band_width);
                    }
                }
            }
        }
    }

    fn update_progress(&mut self, elapsed_seconds: f64) {
        if !elapsed_seconds.is_finite() || elapsed_seconds < 0.0 {
            return;
        }

        match self.last_t {
            Some(last_t) if elapsed_seconds >= last_t => {
                let dt = elapsed_seconds - last_t;
                if dt.is_finite() {
                    self.progress += self.config.speed * dt;
                    if self.progress >= UINT32_WRAP || self.progress < 0.0 {
                        self.progress = self.progress.rem_euclid(UINT32_WRAP);
                    }
                }
                self.last_t = Some(elapsed_seconds);
            }
            _ => {
                self.last_t = Some(elapsed_seconds);
            }
        }
    }

    fn invalidate_cache(&mut self) {
        self.cache_shape = u32::MAX;
    }
}

unsafe extern "C" fn radial_rainbow_create(
    _host: *const SkydimoHostApiV1,
    out_instance: *mut *mut c_void,
) -> i32 {
    catch_ffi(|| {
        if out_instance.is_null() {
            return -1;
        }
        let effect = Box::new(RadialRainbowEffect::default());
        unsafe {
            *out_instance = Box::into_raw(effect).cast::<c_void>();
        }
        0
    })
}

unsafe extern "C" fn radial_rainbow_destroy(instance: *mut c_void) {
    let _ = catch_ffi(|| {
        if !instance.is_null() {
            unsafe {
                drop(Box::from_raw(instance.cast::<RadialRainbowEffect>()));
            }
        }
        0
    });
}

unsafe extern "C" fn radial_rainbow_resize(
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

unsafe extern "C" fn radial_rainbow_update_params_json(
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

unsafe extern "C" fn radial_rainbow_tick(
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

unsafe extern "C" fn radial_rainbow_is_ready(instance: *mut c_void) -> i32 {
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
                    create: Some(radial_rainbow_create),
                    destroy: Some(radial_rainbow_destroy),
                    resize: Some(radial_rainbow_resize),
                    update_params_json: Some(radial_rainbow_update_params_json),
                    tick: Some(radial_rainbow_tick),
                    is_ready: Some(radial_rainbow_is_ready),
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

fn effect_mut(instance: *mut c_void) -> Option<&'static mut RadialRainbowEffect> {
    if instance.is_null() {
        None
    } else {
        Some(unsafe { &mut *instance.cast::<RadialRainbowEffect>() })
    }
}

fn catch_ffi(f: impl FnOnce() -> i32) -> i32 {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(-100)
}

fn build_rainbow_lut() -> [SkydimoRgb; 360] {
    let mut lut = [SkydimoRgb::default(); 360];
    for (hue, color) in lut.iter_mut().enumerate() {
        *color = reference_hsv_to_rgb(hue as u32);
    }
    lut
}

#[inline(always)]
fn wrap_reference_hue(raw_hue: f64) -> usize {
    if !raw_hue.is_finite() {
        return 0;
    }

    let mut hue = raw_hue.trunc();
    if !(0.0..UINT32_WRAP).contains(&hue) {
        hue = hue.rem_euclid(UINT32_WRAP);
    }
    hue as u32 as usize % 360
}

#[inline(always)]
fn reference_hsv_to_rgb(hue: u32) -> SkydimoRgb {
    let h = (hue % 360) as i64;
    let sector = h / 60;
    let saturation = 255i64;
    let value = 255i64;
    let p = ((256 * value - saturation * value) / 256) as u8;

    if sector % 2 == 1 {
        let q = ((256 * 60 * value - h * saturation * value
            + 60 * saturation * value * sector)
            / (256 * 60)) as u8;
        return match sector {
            1 => SkydimoRgb { r: q, g: 255, b: p },
            3 => SkydimoRgb { r: p, g: q, b: 255 },
            _ => SkydimoRgb { r: 255, g: p, b: q },
        };
    }

    let t = ((256 * 60 * value + h * saturation * value
        - 60 * saturation * value * (sector + 1))
        / (256 * 60)) as u8;
    match sector {
        0 => SkydimoRgb { r: 255, g: t, b: p },
        2 => SkydimoRgb { r: p, g: 255, b: t },
        _ => SkydimoRgb { r: t, g: p, b: 255 },
    }
}

fn trunc_toward_zero(value: f64) -> i64 {
    if !value.is_finite() {
        return i64::MIN;
    }
    if value < 0.0 {
        value.ceil() as i64
    } else {
        value.floor() as i64
    }
}

fn parse_number_field(json: &str, key: &str) -> Option<f64> {
    let raw = json_value_slice(json, key)?;
    raw.parse::<f64>().ok()
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
            if ch.is_ascii_digit() || matches!(ch, '-' | '+' | '.' | 'e' | 'E') {
                None
            } else {
                Some(idx)
            }
        })
        .unwrap_or(raw.len());
    Some(raw[..end].trim())
}

#[cfg(test)]
mod tests {
    use super::{
        reference_hsv_to_rgb, wrap_reference_hue, RadialRainbowEffect, SHAPE_SQUARES,
    };
    use crate::abi::SkydimoRgb;

    #[test]
    fn parses_params_without_json_dependency() {
        let mut effect = RadialRainbowEffect::default();
        effect.update_params(r#"{"speed":150,"frequency":75,"cx":25,"cy":80,"shape":1}"#);

        assert_eq!(effect.config.speed, 150.0);
        assert_eq!(effect.config.frequency, 75.0);
        assert_eq!(effect.config.cx_shift, 25.0);
        assert_eq!(effect.config.cy_shift, 80.0);
        assert_eq!(effect.config.shape, SHAPE_SQUARES);
    }

    #[test]
    fn clamps_invalid_params() {
        let mut effect = RadialRainbowEffect::default();
        effect.update_params(r#"{"shape":1}"#);
        effect.update_params(r#"{"speed":999,"frequency":-2,"cx":-1,"cy":101,"shape":7}"#);

        assert_eq!(effect.config.speed, 200.0);
        assert_eq!(effect.config.frequency, 1.0);
        assert_eq!(effect.config.cx_shift, 0.0);
        assert_eq!(effect.config.cy_shift, 100.0);
        assert_eq!(effect.config.shape, SHAPE_SQUARES);
        effect.update_params(r#"{"shape":-1}"#);
        assert_eq!(effect.config.shape, SHAPE_SQUARES);
    }

    #[test]
    fn reference_hsv_matches_primary_anchors() {
        assert_eq!(reference_hsv_to_rgb(0), SkydimoRgb { r: 255, g: 0, b: 0 });
        assert_eq!(reference_hsv_to_rgb(120), SkydimoRgb { r: 0, g: 255, b: 0 });
        assert_eq!(reference_hsv_to_rgb(240), SkydimoRgb { r: 0, g: 0, b: 255 });
    }

    #[test]
    fn hue_wrapping_matches_unsigned_reference() {
        assert_eq!(wrap_reference_hue(360.9), 0);
        assert_eq!(wrap_reference_hue(-1.0), 255);
        assert_eq!(wrap_reference_hue(4_294_967_296.0), 0);
    }

    #[test]
    fn renders_default_linear_center_as_red() {
        let mut effect = RadialRainbowEffect::default();
        effect.resize(4, 1, 4);

        let mut pixels = [SkydimoRgb::default(); 4];
        effect.tick(0.0, &mut pixels);

        assert_eq!(pixels[2], SkydimoRgb { r: 255, g: 0, b: 0 });
        assert!(pixels.iter().any(|pixel| *pixel != SkydimoRgb::default()));
        assert_eq!(effect.hue_offsets.len(), 4);
    }
}
