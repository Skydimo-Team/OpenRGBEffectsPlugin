use crate::abi::{
    EffectAudioCaptureFn, EffectRgbCaptureFn, HostLogFn, HostPluginIdFn, SkydimoAudioFrameV1,
    SkydimoHostApiV1, SkydimoRgb, SkydimoRgbFrameV1, SkydimoStr,
};
use std::ffi::{c_char, c_void};

const SKYDIMO_LOG_WARN: u32 = 3;

#[derive(Clone, Copy)]
pub struct NativeHost {
    host_ctx: *mut c_void,
    log: Option<HostLogFn>,
    audio_capture: Option<EffectAudioCaptureFn>,
    album_art: Option<EffectRgbCaptureFn>,
    get_plugin_id: Option<HostPluginIdFn>,
}

impl NativeHost {
    pub fn from_api(api: &SkydimoHostApiV1) -> Self {
        Self {
            host_ctx: api.host_ctx,
            log: api.log,
            audio_capture: api.effect_audio_capture,
            album_art: api.effect_album_art,
            get_plugin_id: api.get_plugin_id,
        }
    }

    pub fn plugin_id(&self) -> Option<String> {
        let get_plugin_id = self.get_plugin_id?;
        let mut out = SkydimoStr::default();
        let status = unsafe { get_plugin_id(self.host_ctx, &mut out) };
        if status < 0 || out.ptr.is_null() || out.len == 0 {
            return None;
        }
        let bytes = unsafe { std::slice::from_raw_parts(out.ptr.cast::<u8>(), out.len) };
        Some(String::from_utf8_lossy(bytes).into_owned())
    }

    pub fn warn(&self, message: &str) {
        if let Some(log) = self.log {
            unsafe {
                log(
                    self.host_ctx,
                    SKYDIMO_LOG_WARN,
                    message.as_ptr().cast::<c_char>(),
                    message.len(),
                );
            }
        }
    }

    pub fn capture_audio_into(&self, avg_size: usize, out: &mut [f32]) -> Option<usize> {
        let capture = self.audio_capture?;
        let mut frame = SkydimoAudioFrameV1::default();
        let status = unsafe { capture(self.host_ctx, avg_size, &mut frame) };
        if status <= 0 || frame.bins.ptr.is_null() || frame.bins.len == 0 || out.is_empty() {
            return None;
        }

        let len = frame.bins.len.min(out.len());
        let bins = unsafe { std::slice::from_raw_parts(frame.bins.ptr, len) };
        out[..len].copy_from_slice(bins);
        Some(len)
    }

    pub fn capture_audio_amplitude(&self, avg_size: usize) -> Option<f32> {
        let capture = self.audio_capture?;
        let mut frame = SkydimoAudioFrameV1::default();
        let status = unsafe { capture(self.host_ctx, avg_size, &mut frame) };
        (status > 0).then_some(frame.amplitude)
    }

    pub fn with_album_art<R>(
        &self,
        width: usize,
        height: usize,
        f: impl FnOnce(usize, usize, &[SkydimoRgb]) -> R,
    ) -> Option<R> {
        let capture = self.album_art?;
        let mut frame = SkydimoRgbFrameV1::default();
        let status = unsafe { capture(self.host_ctx, width, height, &mut frame) };
        if status <= 0 || frame.pixels.is_null() || frame.pixels_len == 0 {
            return None;
        }

        let total = frame
            .width
            .saturating_mul(frame.height)
            .min(frame.pixels_len);
        if total == 0 {
            return None;
        }

        let pixels = unsafe { std::slice::from_raw_parts(frame.pixels, total) };
        Some(f(frame.width, frame.height, pixels))
    }

    pub fn capture_album_art_into(
        &self,
        width: usize,
        height: usize,
        out: &mut Vec<SkydimoRgb>,
    ) -> Option<(usize, usize)> {
        self.with_album_art(width, height, |width, height, pixels| {
            out.clear();
            out.extend_from_slice(pixels);
            (width, height)
        })
    }
}
