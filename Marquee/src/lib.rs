use std::ffi::{c_char, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};

const SKYDIMO_NATIVE_C_ABI_VERSION: u32 = 3;
const SKYDIMO_PLUGIN_KIND_EFFECT: u32 = 1 << 0;
const HUE_STEPS: usize = 360;
const PROGRESS_WRAP: f64 = 232_792_560.0;

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
struct MarqueeConfig {
    speed: f64,
    spacing: usize,
    random_enabled: bool,
    color: SkydimoRgb,
}

impl Default for MarqueeConfig {
    fn default() -> Self {
        Self {
            speed: 50.0,
            spacing: 2,
            random_enabled: false,
            color: SkydimoRgb { r: 255, g: 0, b: 0 },
        }
    }
}

struct MarqueeEffect {
    config: MarqueeConfig,
    progress: f64,
    last_elapsed: Option<f64>,
    random_hue: usize,
    width: usize,
    height: usize,
    hue_table: [SkydimoRgb; HUE_STEPS],
}

impl MarqueeEffect {
    fn new() -> Self {
        Self {
            config: MarqueeConfig::default(),
            progress: 0.0,
            last_elapsed: None,
            random_hue: 0,
            width: 0,
            height: 1,
            hue_table: build_hue_table(),
        }
    }

    fn resize(&mut self, width: u32, height: u32, led_count: u32) {
        let fallback = (led_count as usize).max(1);
        self.width = if width == 0 { fallback } else { width as usize };
        self.height = height.max(1) as usize;
    }

    fn update_params(&mut self, bytes: &[u8]) {
        if let Some(speed) = json_number(bytes, b"speed").filter(|value| value.is_finite()) {
            self.config.speed = speed.clamp(1.0, 200.0);
        }

        if let Some(spacing) = json_number(bytes, b"spacing").filter(|value| value.is_finite()) {
            self.config.spacing = spacing.floor().clamp(2.0, 20.0) as usize;
        }

        if let Some(random_enabled) = json_bool(bytes, b"random") {
            self.config.random_enabled = random_enabled;
        }

        if let Some(color) = json_string(bytes, b"color").and_then(parse_hex_color) {
            self.config.color = color;
        }
    }

    fn tick(&mut self, elapsed_seconds: f64, pixels: &mut [SkydimoRgb]) {
        if pixels.is_empty() {
            return;
        }

        let width = if self.width == 0 {
            pixels.len()
        } else {
            self.width
        }
        .max(1);
        let height = self.height.max(1);
        let render_len = pixels.len().min(width.saturating_mul(height));
        if render_len == 0 {
            return;
        }

        let color = self.active_color();
        let row_len = width.min(render_len);
        self.render_row(&mut pixels[..row_len], color);

        let mut filled = row_len;
        while filled < render_len {
            let copy_len = row_len.min(render_len - filled);
            unsafe {
                std::ptr::copy_nonoverlapping(
                    pixels.as_ptr(),
                    pixels.as_mut_ptr().add(filled),
                    copy_len,
                );
            }
            filled += copy_len;
        }

        if render_len < pixels.len() {
            pixels[render_len..].fill(SkydimoRgb::default());
        }

        self.advance_progress(elapsed_seconds);
        if self.config.random_enabled {
            self.random_hue = (self.random_hue + 1) % HUE_STEPS;
        }
    }

    fn render_row(&self, row: &mut [SkydimoRgb], color: SkydimoRgb) {
        row.fill(SkydimoRgb::default());

        let spacing = self.config.spacing.max(1);
        let shift_mod = self.shift_mod(spacing);
        let mut x = if shift_mod == 0 {
            0
        } else {
            spacing - shift_mod
        };

        while x < row.len() {
            row[x] = color;
            x += spacing;
        }
    }

    #[inline]
    fn active_color(&self) -> SkydimoRgb {
        if self.config.random_enabled {
            self.hue_table[self.random_hue]
        } else {
            self.config.color
        }
    }

    #[inline]
    fn shift_mod(&self, spacing: usize) -> usize {
        self.progress
            .floor()
            .rem_euclid(spacing as f64) as usize
    }

    fn advance_progress(&mut self, elapsed_seconds: f64) {
        let delta = if elapsed_seconds.is_finite() && elapsed_seconds >= 0.0 {
            let delta = match self.last_elapsed {
                None => elapsed_seconds,
                Some(last) if elapsed_seconds < last => elapsed_seconds,
                Some(last) => elapsed_seconds - last,
            };
            self.last_elapsed = Some(elapsed_seconds);
            delta
        } else {
            0.0
        };

        if delta <= 0.0 {
            return;
        }

        self.progress += 0.1 * self.config.speed * delta;
        if self.progress >= PROGRESS_WRAP {
            self.progress = self.progress.rem_euclid(PROGRESS_WRAP);
        }
    }
}

unsafe extern "C" fn marquee_create(
    _host: *const SkydimoHostApiV1,
    out_instance: *mut *mut c_void,
) -> i32 {
    catch_ffi(|| {
        if out_instance.is_null() {
            return -1;
        }
        let effect = Box::new(MarqueeEffect::new());
        unsafe {
            *out_instance = Box::into_raw(effect).cast::<c_void>();
        }
        0
    })
}

unsafe extern "C" fn marquee_destroy(instance: *mut c_void) {
    let _ = catch_ffi(|| {
        if !instance.is_null() {
            unsafe {
                drop(Box::from_raw(instance.cast::<MarqueeEffect>()));
            }
        }
        0
    });
}

unsafe extern "C" fn marquee_resize(
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

unsafe extern "C" fn marquee_update_params_json(
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
        effect.update_params(bytes);
        0
    })
}

unsafe extern "C" fn marquee_tick(
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

unsafe extern "C" fn marquee_is_ready(instance: *mut c_void) -> i32 {
    catch_ffi(|| if instance.is_null() { -1 } else { 1 })
}

#[no_mangle]
/// # Safety
///
/// `out_api` must be a valid, writable pointer to a `SkydimoPluginApiV1`.
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
                    create: Some(marquee_create),
                    destroy: Some(marquee_destroy),
                    resize: Some(marquee_resize),
                    update_params_json: Some(marquee_update_params_json),
                    tick: Some(marquee_tick),
                    is_ready: Some(marquee_is_ready),
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

fn effect_mut(instance: *mut c_void) -> Option<&'static mut MarqueeEffect> {
    if instance.is_null() {
        None
    } else {
        Some(unsafe { &mut *instance.cast::<MarqueeEffect>() })
    }
}

fn catch_ffi(f: impl FnOnce() -> i32) -> i32 {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(-100)
}

fn build_hue_table() -> [SkydimoRgb; HUE_STEPS] {
    std::array::from_fn(|hue| hsv_to_rgb(hue as f32, 1.0, 1.0))
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

fn json_number(bytes: &[u8], key: &[u8]) -> Option<f64> {
    let start = json_value_start(bytes, key)?;
    let mut end = start;
    while end < bytes.len()
        && matches!(
            bytes[end],
            b'0'..=b'9' | b'+' | b'-' | b'.' | b'e' | b'E'
        )
    {
        end += 1;
    }
    if end == start {
        return None;
    }
    std::str::from_utf8(&bytes[start..end]).ok()?.parse().ok()
}

fn json_bool(bytes: &[u8], key: &[u8]) -> Option<bool> {
    let start = json_value_start(bytes, key)?;
    if bytes[start..].starts_with(b"true") {
        Some(true)
    } else if bytes[start..].starts_with(b"false") {
        Some(false)
    } else {
        None
    }
}

fn json_string<'a>(bytes: &'a [u8], key: &[u8]) -> Option<&'a [u8]> {
    let mut pos = json_value_start(bytes, key)?;
    if bytes.get(pos).copied()? != b'"' {
        return None;
    }
    pos += 1;
    let start = pos;
    let mut escaped = false;
    while pos < bytes.len() {
        let byte = bytes[pos];
        if escaped {
            escaped = false;
        } else if byte == b'\\' {
            escaped = true;
        } else if byte == b'"' {
            return Some(&bytes[start..pos]);
        }
        pos += 1;
    }
    None
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

fn parse_hex_color(raw: &[u8]) -> Option<SkydimoRgb> {
    let mut bytes = trim_ascii(raw);
    if let Some(stripped) = bytes.strip_prefix(b"#") {
        bytes = stripped;
    }

    match bytes.len() {
        3 => Some(SkydimoRgb {
            r: parse_hex_nibble(bytes[0])? * 17,
            g: parse_hex_nibble(bytes[1])? * 17,
            b: parse_hex_nibble(bytes[2])? * 17,
        }),
        6 => Some(SkydimoRgb {
            r: parse_hex_byte(bytes[0], bytes[1])?,
            g: parse_hex_byte(bytes[2], bytes[3])?,
            b: parse_hex_byte(bytes[4], bytes[5])?,
        }),
        _ => None,
    }
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

fn trim_ascii(mut bytes: &[u8]) -> &[u8] {
    while let Some((first, rest)) = bytes.split_first() {
        if !first.is_ascii_whitespace() {
            break;
        }
        bytes = rest;
    }
    while let Some((last, rest)) = bytes.split_last() {
        if !last.is_ascii_whitespace() {
            break;
        }
        bytes = rest;
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::{
        json_bool, json_number, parse_hex_color, skydimo_plugin_get_api, MarqueeEffect,
        SkydimoPluginApiV1, SkydimoRgb, SKYDIMO_NATIVE_C_ABI_VERSION, SKYDIMO_PLUGIN_KIND_EFFECT,
    };

    #[test]
    fn parses_manifest_params() {
        let json = br##"{"speed":125,"spacing":7.9,"random":true,"color":"#0af"}"##;
        assert_eq!(json_number(json, b"speed"), Some(125.0));
        assert_eq!(json_number(json, b"spacing"), Some(7.9));
        assert_eq!(json_bool(json, b"random"), Some(true));
        assert_eq!(
            parse_hex_color(b"#0af"),
            Some(SkydimoRgb {
                r: 0,
                g: 170,
                b: 255,
            })
        );
    }

    #[test]
    fn renders_first_row_and_repeats_for_matrix() {
        let mut effect = MarqueeEffect::new();
        effect.resize(6, 2, 12);
        let mut pixels = vec![SkydimoRgb { r: 1, g: 1, b: 1 }; 12];

        effect.tick(0.0, &mut pixels);

        let expected = [
            SkydimoRgb { r: 255, g: 0, b: 0 },
            SkydimoRgb::default(),
            SkydimoRgb { r: 255, g: 0, b: 0 },
            SkydimoRgb::default(),
            SkydimoRgb { r: 255, g: 0, b: 0 },
            SkydimoRgb::default(),
        ];
        assert_eq!(&pixels[..6], &expected);
        assert_eq!(&pixels[6..12], &expected);
    }

    #[test]
    fn renders_with_current_progress_before_advancing() {
        let mut effect = MarqueeEffect::new();
        effect.resize(4, 1, 4);
        let mut pixels = vec![SkydimoRgb::default(); 4];

        effect.tick(1.0, &mut pixels);
        assert_eq!(pixels[0], SkydimoRgb { r: 255, g: 0, b: 0 });
        assert_eq!(pixels[1], SkydimoRgb::default());

        effect.tick(1.2, &mut pixels);
        assert_eq!(pixels[0], SkydimoRgb::default());
        assert_eq!(pixels[1], SkydimoRgb { r: 255, g: 0, b: 0 });
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
