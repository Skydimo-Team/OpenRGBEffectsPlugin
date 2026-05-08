mod abi;
#[allow(dead_code)]
mod color;
#[allow(dead_code)]
mod effects;
#[allow(dead_code)]
mod host;
#[allow(dead_code)]
mod json;
#[allow(dead_code)]
mod rng;

use std::ffi::{c_char, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};

use abi::{
    SkydimoControllerApiV1, SkydimoEffectApiV1, SkydimoExtensionApiV1, SkydimoHostApiV1,
    SkydimoPluginApiV1, SkydimoRgb, SKYDIMO_NATIVE_C_ABI_MIN_VERSION,
    SKYDIMO_NATIVE_C_ABI_VERSION, SKYDIMO_PLUGIN_KIND_EFFECT,
};
use effects::EffectInstance;
use host::NativeHost;

unsafe extern "C" fn effect_create(
    host: *const SkydimoHostApiV1,
    out_instance: *mut *mut c_void,
) -> i32 {
    catch_ffi(|| {
        if out_instance.is_null() || host.is_null() {
            return -1;
        }

        let host = unsafe { &*host };
        if !(SKYDIMO_NATIVE_C_ABI_MIN_VERSION..=SKYDIMO_NATIVE_C_ABI_VERSION)
            .contains(&host.abi_version)
        {
            return -2;
        }

        let native_host = NativeHost::from_api(host);
        let instance = EffectInstance::create(native_host);

        unsafe {
            *out_instance = Box::into_raw(Box::new(instance)).cast::<c_void>();
        }
        0
    })
}

unsafe extern "C" fn effect_destroy(instance: *mut c_void) {
    let _ = catch_ffi(|| {
        if !instance.is_null() {
            unsafe {
                drop(Box::from_raw(instance.cast::<EffectInstance>()));
            }
        }
        0
    });
}

unsafe extern "C" fn effect_resize(
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

unsafe extern "C" fn effect_update_params_json(
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

unsafe extern "C" fn effect_tick(
    instance: *mut c_void,
    elapsed_seconds: f64,
    buffer: *mut SkydimoRgb,
    len: usize,
) -> i32 {
    catch_ffi(|| {
        let Some(effect) = effect_mut(instance) else {
            return -1;
        };
        let pixels = if len == 0 {
            &mut []
        } else {
            if buffer.is_null() {
                return -2;
            }
            unsafe { std::slice::from_raw_parts_mut(buffer, len) }
        };
        effect.tick(elapsed_seconds, pixels)
    })
}

unsafe extern "C" fn effect_is_ready(instance: *mut c_void) -> i32 {
    catch_ffi(|| {
        let Some(effect) = effect_mut(instance) else {
            return -1;
        };
        i32::from(effect.is_ready())
    })
}

#[no_mangle]
/// # Safety
///
/// Skydimo Core must pass an initialized `SkydimoPluginApiV1` out pointer and a
/// supported ABI version. The function never stores the host pointer and returns
/// a negative status instead of unwinding across the C ABI boundary.
pub unsafe extern "C" fn skydimo_plugin_get_api(
    requested_abi_version: u32,
    _host: *const SkydimoHostApiV1,
    out_api: *mut SkydimoPluginApiV1,
) -> i32 {
    catch_ffi(|| {
        if out_api.is_null()
            || !(SKYDIMO_NATIVE_C_ABI_MIN_VERSION..=SKYDIMO_NATIVE_C_ABI_VERSION)
                .contains(&requested_abi_version)
        {
            return -1;
        }

        unsafe {
            *out_api = SkydimoPluginApiV1 {
                size: std::mem::size_of::<SkydimoPluginApiV1>() as u32,
                abi_version: requested_abi_version,
                kind_mask: SKYDIMO_PLUGIN_KIND_EFFECT,
                effect: SkydimoEffectApiV1 {
                    size: std::mem::size_of::<SkydimoEffectApiV1>() as u32,
                    create: Some(effect_create),
                    destroy: Some(effect_destroy),
                    resize: Some(effect_resize),
                    update_params_json: Some(effect_update_params_json),
                    tick: Some(effect_tick),
                    is_ready: Some(effect_is_ready),
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

fn effect_mut(instance: *mut c_void) -> Option<&'static mut EffectInstance> {
    if instance.is_null() {
        None
    } else {
        Some(unsafe { &mut *instance.cast::<EffectInstance>() })
    }
}

fn catch_ffi(f: impl FnOnce() -> i32) -> i32 {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(-100)
}
