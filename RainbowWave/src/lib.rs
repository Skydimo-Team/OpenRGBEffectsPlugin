mod abi;

use std::ffi::{c_char, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};

use abi::{
    SkydimoControllerApiV1, SkydimoEffectApiV1, SkydimoExtensionApiV1, SkydimoHostApiV1,
    SkydimoPluginApiV1, SkydimoRgb, SKYDIMO_NATIVE_C_ABI_VERSION, SKYDIMO_PLUGIN_KIND_EFFECT,
};

const DEFAULT_SPEED: u32 = 40;
const DEFAULT_FREQUENCY: u32 = 10;
const HUE_COUNT: usize = 360;

struct RainbowWaveEffect {
    speed: u32,
    frequency: u32,
    progress: f32,
    last_elapsed: Option<f64>,
    width: usize,
    height: usize,
    row_cache: Vec<SkydimoRgb>,
    hue_palette: [SkydimoRgb; HUE_COUNT],
}

impl RainbowWaveEffect {
    fn new() -> Self {
        Self {
            speed: DEFAULT_SPEED,
            frequency: DEFAULT_FREQUENCY,
            progress: 0.0,
            last_elapsed: None,
            width: 0,
            height: 1,
            row_cache: Vec::new(),
            hue_palette: std::array::from_fn(|hue| hsv_to_rgb(hue as f32, 1.0, 1.0)),
        }
    }

    fn resize(&mut self, width: u32, height: u32, led_count: u32) {
        let fallback = (led_count as usize).max(1);
        self.width = if width == 0 { fallback } else { width as usize };
        self.height = (height as usize).max(1);
        self.row_cache.clear();
        self.row_cache.reserve(self.width);
    }

    fn update_params_json(&mut self, bytes: &[u8]) {
        if let Some(speed) = json_number(bytes, b"speed") {
            self.speed = rounded_clamped(speed, 1, 100);
        }
        if let Some(frequency) = json_number(bytes, b"frequency") {
            self.frequency = rounded_clamped(frequency, 1, 50);
        }
    }

    fn tick(&mut self, elapsed_seconds: f64, pixels: &mut [SkydimoRgb]) {
        if pixels.is_empty() {
            self.advance(elapsed_seconds);
            return;
        }

        let width = if self.width == 0 {
            pixels.len()
        } else {
            self.width
        }
        .max(1);
        let height = self.height.max(1);

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
    }

    fn render_row(&mut self, width: usize) {
        if self.row_cache.len() != width {
            self.row_cache.resize(width, SkydimoRgb::default());
        }

        let frequency = self.frequency as f32;
        let mut hue = self.progress * frequency;
        for pixel in &mut self.row_cache {
            let index = hue.floor().rem_euclid(HUE_COUNT as f32) as usize;
            *pixel = self.hue_palette[index];
            hue += frequency;
        }
    }

    fn advance(&mut self, elapsed_seconds: f64) {
        let current_progress = self.progress;
        let delta = if elapsed_seconds.is_finite() && elapsed_seconds >= 0.0 {
            let delta = match self.last_elapsed {
                Some(last) if elapsed_seconds >= last => elapsed_seconds - last,
                _ => elapsed_seconds,
            };
            self.last_elapsed = Some(elapsed_seconds);
            delta as f32
        } else {
            0.0
        };

        if current_progress < 360.0 {
            self.progress = current_progress + self.speed as f32 * delta;
        } else {
            self.progress = 0.0;
        }
    }
}

unsafe extern "C" fn rainbow_wave_create(
    host: *const SkydimoHostApiV1,
    out_instance: *mut *mut c_void,
) -> i32 {
    catch_ffi(|| {
        if out_instance.is_null() {
            return -1;
        }
        if !host.is_null() {
            let host = unsafe { &*host };
            if host.abi_version != SKYDIMO_NATIVE_C_ABI_VERSION {
                return -2;
            }
        }

        let effect = Box::new(RainbowWaveEffect::new());
        unsafe {
            *out_instance = Box::into_raw(effect).cast::<c_void>();
        }
        0
    })
}

unsafe extern "C" fn rainbow_wave_destroy(instance: *mut c_void) {
    let _ = catch_ffi(|| {
        if !instance.is_null() {
            unsafe {
                drop(Box::from_raw(instance.cast::<RainbowWaveEffect>()));
            }
        }
        0
    });
}

unsafe extern "C" fn rainbow_wave_resize(
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

unsafe extern "C" fn rainbow_wave_update_params_json(
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
        effect.update_params_json(bytes);
        0
    })
}

unsafe extern "C" fn rainbow_wave_tick(
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
            effect.tick(elapsed_seconds, &mut []);
            return 0;
        }

        let pixels = unsafe { std::slice::from_raw_parts_mut(buffer, len) };
        effect.tick(elapsed_seconds, pixels);
        0
    })
}

unsafe extern "C" fn rainbow_wave_is_ready(instance: *mut c_void) -> i32 {
    catch_ffi(|| if instance.is_null() { -1 } else { 1 })
}

#[no_mangle]
/// # Safety
///
/// `out_api` must point to writable storage for one `SkydimoPluginApiV1`.
/// `requested_abi_version` must be the native-c ABI declared in manifest.json.
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
                    create: Some(rainbow_wave_create),
                    destroy: Some(rainbow_wave_destroy),
                    resize: Some(rainbow_wave_resize),
                    update_params_json: Some(rainbow_wave_update_params_json),
                    tick: Some(rainbow_wave_tick),
                    is_ready: Some(rainbow_wave_is_ready),
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

fn effect_mut(instance: *mut c_void) -> Option<&'static mut RainbowWaveEffect> {
    if instance.is_null() {
        None
    } else {
        Some(unsafe { &mut *instance.cast::<RainbowWaveEffect>() })
    }
}

fn catch_ffi(f: impl FnOnce() -> i32) -> i32 {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(-100)
}

fn rounded_clamped(value: f32, min: u32, max: u32) -> u32 {
    if !value.is_finite() {
        return min;
    }
    (value + 0.5).floor().clamp(min as f32, max as f32) as u32
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

fn to_u8(value: f32) -> u8 {
    value.round().clamp(0.0, 255.0) as u8
}

fn json_number(bytes: &[u8], key: &[u8]) -> Option<f32> {
    let mut pos = json_value_start(bytes, key)?;
    let quoted = bytes.get(pos).copied() == Some(b'"');
    if quoted {
        pos += 1;
    }

    let start = pos;
    while pos < bytes.len()
        && matches!(
            bytes[pos],
            b'0'..=b'9' | b'+' | b'-' | b'.' | b'e' | b'E'
        )
    {
        pos += 1;
    }
    if pos == start {
        return None;
    }
    if quoted && bytes.get(pos).copied() != Some(b'"') {
        return None;
    }

    std::str::from_utf8(&bytes[start..pos]).ok()?.parse().ok()
}

fn json_value_start(bytes: &[u8], key: &[u8]) -> Option<usize> {
    let key_pos = find_json_key(bytes, key)?;
    let mut pos = key_pos + key.len() + 2;
    pos = skip_ascii_ws(bytes, pos);
    if bytes.get(pos).copied()? != b':' {
        return None;
    }
    Some(skip_ascii_ws(bytes, pos + 1))
}

fn find_json_key(bytes: &[u8], key: &[u8]) -> Option<usize> {
    if key.is_empty() || bytes.len() < key.len() + 2 {
        return None;
    }

    let last = bytes.len() - key.len() - 1;
    let mut pos = 0usize;
    while pos < last {
        if bytes[pos] == b'"'
            && bytes[pos + 1..].starts_with(key)
            && bytes.get(pos + key.len() + 1).copied() == Some(b'"')
        {
            return Some(pos);
        }
        pos += 1;
    }
    None
}

fn skip_ascii_ws(bytes: &[u8], mut pos: usize) -> usize {
    while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
        pos += 1;
    }
    pos
}

#[cfg(test)]
mod tests {
    use std::ffi::c_void;

    use super::{
        hsv_to_rgb, json_number, rainbow_wave_create, rainbow_wave_destroy, rainbow_wave_tick,
        rounded_clamped, skydimo_plugin_get_api, RainbowWaveEffect,
    };
    use crate::abi::{
        SkydimoPluginApiV1, SkydimoRgb, SKYDIMO_NATIVE_C_ABI_VERSION, SKYDIMO_PLUGIN_KIND_EFFECT,
    };

    #[test]
    fn parses_and_clamps_params() {
        let json = br#"{"speed":125,"frequency":"7"}"#;
        assert_eq!(json_number(json, b"speed"), Some(125.0));
        assert_eq!(json_number(json, b"frequency"), Some(7.0));
        assert_eq!(rounded_clamped(125.0, 1, 100), 100);
        assert_eq!(rounded_clamped(0.0, 1, 50), 1);
    }

    #[test]
    fn hsv_conversion_matches_host_anchors() {
        assert_eq!(hsv_to_rgb(0.0, 1.0, 1.0), SkydimoRgb { r: 255, g: 0, b: 0 });
        assert_eq!(hsv_to_rgb(120.0, 1.0, 1.0), SkydimoRgb { r: 0, g: 255, b: 0 });
        assert_eq!(hsv_to_rgb(240.0, 1.0, 1.0), SkydimoRgb { r: 0, g: 0, b: 255 });
    }

    #[test]
    fn renders_horizontal_wave_and_repeats_rows() {
        let mut effect = RainbowWaveEffect::new();
        effect.resize(4, 2, 8);
        let mut pixels = [SkydimoRgb::default(); 8];

        effect.tick(0.0, &mut pixels);

        let expected = [
            hsv_to_rgb(0.0, 1.0, 1.0),
            hsv_to_rgb(10.0, 1.0, 1.0),
            hsv_to_rgb(20.0, 1.0, 1.0),
            hsv_to_rgb(30.0, 1.0, 1.0),
        ];
        assert_eq!(&pixels[..4], &expected);
        assert_eq!(&pixels[4..], &expected);
    }

    #[test]
    fn advances_after_render_like_lua() {
        let mut effect = RainbowWaveEffect::new();
        let mut pixels = [SkydimoRgb::default(); 1];

        effect.tick(1.0, &mut pixels);

        assert_eq!(pixels[0], hsv_to_rgb(0.0, 1.0, 1.0));
        assert_eq!(effect.progress, 40.0);
    }

    #[test]
    fn exports_effect_api_for_current_abi() {
        let mut api = SkydimoPluginApiV1::default();
        let status = unsafe {
            skydimo_plugin_get_api(
                SKYDIMO_NATIVE_C_ABI_VERSION,
                std::ptr::null(),
                &mut api,
            )
        };

        assert_eq!(status, 0);
        assert_eq!(api.abi_version, SKYDIMO_NATIVE_C_ABI_VERSION);
        assert_eq!(api.kind_mask, SKYDIMO_PLUGIN_KIND_EFFECT);
        assert!(api.effect.create.is_some());
        assert!(api.effect.tick.is_some());
    }

    #[test]
    fn ffi_create_tick_destroy_round_trip() {
        let mut instance: *mut c_void = std::ptr::null_mut();
        assert_eq!(unsafe { rainbow_wave_create(std::ptr::null(), &mut instance) }, 0);
        assert!(!instance.is_null());

        let mut pixels = [SkydimoRgb::default(); 4];
        assert_eq!(
            unsafe { rainbow_wave_tick(instance, 0.0, pixels.as_mut_ptr(), pixels.len()) },
            0
        );
        unsafe { rainbow_wave_destroy(instance) };

        assert_eq!(pixels[0], SkydimoRgb { r: 255, g: 0, b: 0 });
    }
}
