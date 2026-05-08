mod abi;

use std::ffi::{c_char, c_void};

use abi::{
    SkydimoControllerApiV1, SkydimoEffectApiV1, SkydimoExtensionApiV1, SkydimoHostApiV1,
    SkydimoPluginApiV1, SkydimoRgb, SKYDIMO_NATIVE_C_ABI_VERSION, SKYDIMO_PLUGIN_KIND_EFFECT,
};

#[derive(Clone, Copy)]
struct SingleColorEffect {
    color: SkydimoRgb,
}

impl Default for SingleColorEffect {
    fn default() -> Self {
        Self {
            color: SkydimoRgb {
                r: 255,
                g: 255,
                b: 255,
            },
        }
    }
}

impl SingleColorEffect {
    fn update_params(&mut self, bytes: &[u8]) {
        if let Some(color) = parse_color_param(bytes) {
            self.color = color;
        }
    }

    fn tick(&self, pixels: &mut [SkydimoRgb]) {
        fill_rgb(pixels, self.color);
    }
}

unsafe extern "C" fn single_color_create(
    _host: *const SkydimoHostApiV1,
    out_instance: *mut *mut c_void,
) -> i32 {
    if out_instance.is_null() {
        return -1;
    }

    let effect = Box::new(SingleColorEffect::default());
    unsafe {
        *out_instance = Box::into_raw(effect).cast::<c_void>();
    }
    0
}

unsafe extern "C" fn single_color_destroy(instance: *mut c_void) {
    if !instance.is_null() {
        unsafe {
            drop(Box::from_raw(instance.cast::<SingleColorEffect>()));
        }
    }
}

unsafe extern "C" fn single_color_resize(
    instance: *mut c_void,
    _width: u32,
    _height: u32,
    _led_count: u32,
) -> i32 {
    if instance.is_null() {
        -1
    } else {
        0
    }
}

unsafe extern "C" fn single_color_update_params_json(
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

    let bytes = unsafe { std::slice::from_raw_parts(ptr.cast::<u8>(), len) };
    effect.update_params(bytes);
    0
}

unsafe extern "C" fn single_color_tick(
    instance: *mut c_void,
    _elapsed_seconds: f64,
    buffer: *mut SkydimoRgb,
    len: usize,
) -> i32 {
    let Some(effect) = effect_mut(instance) else {
        return -1;
    };
    if buffer.is_null() && len > 0 {
        return -1;
    }
    if len == 0 {
        return 0;
    }

    let pixels = unsafe { std::slice::from_raw_parts_mut(buffer, len) };
    effect.tick(pixels);
    0
}

unsafe extern "C" fn single_color_is_ready(instance: *mut c_void) -> i32 {
    if instance.is_null() {
        -1
    } else {
        1
    }
}

#[no_mangle]
/// # Safety
///
/// `out_api` must be a valid, writable pointer to a host-compatible
/// `SkydimoPluginApiV1`. The host must pass the ABI version declared in the
/// plugin manifest.
pub unsafe extern "C" fn skydimo_plugin_get_api(
    requested_abi_version: u32,
    _host: *const SkydimoHostApiV1,
    out_api: *mut SkydimoPluginApiV1,
) -> i32 {
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
                create: Some(single_color_create),
                destroy: Some(single_color_destroy),
                resize: Some(single_color_resize),
                update_params_json: Some(single_color_update_params_json),
                tick: Some(single_color_tick),
                is_ready: Some(single_color_is_ready),
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
}

unsafe fn effect_mut(instance: *mut c_void) -> Option<&'static mut SingleColorEffect> {
    if instance.is_null() {
        None
    } else {
        Some(unsafe { &mut *instance.cast::<SingleColorEffect>() })
    }
}

fn parse_color_param(bytes: &[u8]) -> Option<SkydimoRgb> {
    let json = std::str::from_utf8(bytes).ok()?;
    let key_pos = json.find("\"color\"")?;
    let after_key = &json[key_pos + "\"color\"".len()..];
    let colon_pos = after_key.find(':')?;
    let mut raw = after_key[colon_pos + 1..].trim_start();
    raw = raw.strip_prefix('"')?;
    let end = raw.find('"')?;
    parse_hex_color(&raw[..end])
}

fn parse_hex_color(raw: &str) -> Option<SkydimoRgb> {
    let mut hex = raw.trim();
    if let Some(stripped) = hex.strip_prefix('#') {
        hex = stripped;
    }

    let bytes = hex.as_bytes();
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

fn fill_rgb(buffer: &mut [SkydimoRgb], color: SkydimoRgb) {
    if buffer.is_empty() {
        return;
    }

    buffer[0] = color;
    let mut filled = 1usize;
    while filled < buffer.len() {
        let copy_len = filled.min(buffer.len() - filled);
        unsafe {
            std::ptr::copy_nonoverlapping(
                buffer.as_ptr(),
                buffer.as_mut_ptr().add(filled),
                copy_len,
            );
        }
        filled += copy_len;
    }
}

#[cfg(test)]
mod tests {
    use super::{fill_rgb, parse_color_param, parse_hex_color};
    use crate::abi::SkydimoRgb;

    #[test]
    fn parses_full_and_short_hex_colors() {
        assert_eq!(
            parse_hex_color("#0a7Ff2"),
            Some(SkydimoRgb {
                r: 10,
                g: 127,
                b: 242,
            })
        );
        assert_eq!(
            parse_hex_color("#0af"),
            Some(SkydimoRgb {
                r: 0,
                g: 170,
                b: 255,
            })
        );
    }

    #[test]
    fn extracts_color_param_from_json() {
        assert_eq!(
            parse_color_param(br##"{"color":"#123456"}"##),
            Some(SkydimoRgb {
                r: 18,
                g: 52,
                b: 86,
            })
        );
    }

    #[test]
    fn fills_whole_buffer() {
        let color = SkydimoRgb { r: 1, g: 2, b: 3 };
        let mut buffer = [SkydimoRgb::default(); 17];
        fill_rgb(&mut buffer, color);
        assert!(buffer.iter().all(|pixel| *pixel == color));
    }
}
