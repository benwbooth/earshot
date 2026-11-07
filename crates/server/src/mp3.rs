//! MP3 encoding with LAME, the system's libmp3lame (see build.rs): the
//! relay's sound as a stream every browser plays from an <audio> element.

use std::ffi::c_void;
#[cfg(earshot_lame)]
use std::ffi::{c_int, c_short, c_uchar};

/// Whether this build has LAME (otherwise there's no relay).
pub const AVAILABLE: bool = cfg!(earshot_lame);

#[cfg(earshot_lame)]
unsafe extern "C" {
    fn lame_init() -> *mut c_void;
    fn lame_set_in_samplerate(flags: *mut c_void, rate: c_int) -> c_int;
    fn lame_set_num_channels(flags: *mut c_void, channels: c_int) -> c_int;
    fn lame_set_brate(flags: *mut c_void, kbps: c_int) -> c_int;
    fn lame_set_mode(flags: *mut c_void, mode: c_int) -> c_int;
    fn lame_set_quality(flags: *mut c_void, quality: c_int) -> c_int;
    fn lame_set_bWriteVbrTag(flags: *mut c_void, write: c_int) -> c_int;
    fn lame_init_params(flags: *mut c_void) -> c_int;
    fn lame_encode_buffer_interleaved(flags: *mut c_void, pcm: *mut c_short, frames: c_int, out: *mut c_uchar, room: c_int) -> c_int;
    fn lame_encode_flush_nogap(flags: *mut c_void, out: *mut c_uchar, room: c_int) -> c_int;
    fn lame_close(flags: *mut c_void) -> c_int;
}

/// Constant-bitrate joint stereo MP3, 16-bit interleaved stereo in.
pub struct Encoder {
    #[cfg_attr(not(earshot_lame), allow(dead_code))]
    flags: *mut c_void,
}

// LAME's state belongs to one encoder, used by one thread at a time.
unsafe impl Send for Encoder {}

impl Encoder {
    #[cfg(earshot_lame)]
    pub fn new(rate: u32, kbps: u32) -> Result<Encoder, String> {
        // SAFETY: a fresh LAME state, set up before use and closed on drop.
        unsafe {
            let flags = lame_init();
            if flags.is_null() {
                return Err("LAME didn't start".into());
            }
            let encoder = Encoder { flags };
            const JOINT_STEREO: c_int = 1;
            let set = lame_set_in_samplerate(flags, rate as c_int)
                | lame_set_num_channels(flags, 2)
                | lame_set_brate(flags, kbps as c_int)
                | lame_set_mode(flags, JOINT_STEREO)
                | lame_set_quality(flags, 5)
                | lame_set_bWriteVbrTag(flags, 0);
            if set != 0 || lame_init_params(flags) < 0 {
                return Err("LAME didn't take its settings".into());
            }
            Ok(encoder)
        }
    }

    #[cfg(not(earshot_lame))]
    pub fn new(_rate: u32, _kbps: u32) -> Result<Encoder, String> {
        Err("This build of Earshot's server has no MP3 encoder (LAME)".into())
    }

    /// Encodes interleaved stereo samples, adding the MP3 made so far to `out`.
    pub fn encode(&mut self, samples: &[i16], out: &mut Vec<u8>) -> Result<(), String> {
        #[cfg(earshot_lame)]
        {
            let frames = samples.len() / 2;
            // LAME's worst case: 1.25 × samples + 7200 bytes.
            let room = frames * 5 / 4 + 7200;
            out.reserve(room);
            let start = out.len();
            // SAFETY: `pcm` holds `frames` stereo frames (LAME only reads it);
            // `out` has `room` bytes free past `start`.
            let written = unsafe {
                lame_encode_buffer_interleaved(self.flags, samples.as_ptr() as *mut c_short, frames as c_int, out.as_mut_ptr().add(start), room as c_int)
            };
            if written < 0 {
                return Err(format!("LAME couldn't encode ({written})"));
            }
            unsafe { out.set_len(start + written as usize) };
            Ok(())
        }
        #[cfg(not(earshot_lame))]
        {
            let _ = (samples, out);
            Err("no MP3 encoder".into())
        }
    }

    /// The last of the sound, so a finished stream ends cleanly.
    pub fn flush(&mut self, out: &mut Vec<u8>) {
        #[cfg(earshot_lame)]
        {
            let room = 7200;
            out.reserve(room);
            let start = out.len();
            // SAFETY: `out` has `room` bytes free past `start`.
            let written = unsafe { lame_encode_flush_nogap(self.flags, out.as_mut_ptr().add(start), room as c_int) };
            if written > 0 {
                unsafe { out.set_len(start + written as usize) };
            }
        }
        #[cfg(not(earshot_lame))]
        let _ = out;
    }
}

impl Drop for Encoder {
    fn drop(&mut self) {
        #[cfg(earshot_lame)]
        // SAFETY: made by lame_init, closed once.
        unsafe {
            lame_close(self.flags);
        }
    }
}

#[cfg(all(test, earshot_lame))]
mod tests {
    use super::*;

    #[test]
    fn a_second_of_tone_makes_a_second_of_mp3() {
        let rate = 44_100;
        let mut encoder = Encoder::new(rate, 160).expect("LAME");
        let tone: Vec<i16> = (0..rate as usize)
            .flat_map(|i| {
                let s = ((i as f64 * 440.0 * std::f64::consts::TAU / rate as f64).sin() * 8000.0) as i16;
                [s, s]
            })
            .collect();
        let mut mp3 = Vec::new();
        encoder.encode(&tone, &mut mp3).expect("encode");
        encoder.flush(&mut mp3);
        // A second at 160 kbit/s is about 20 kB of MPEG frames, each starting with a sync word.
        assert!(mp3.len() > 15_000 && mp3.len() < 25_000, "{} bytes", mp3.len());
        assert_eq!(mp3[0], 0xFF);
        assert_eq!(mp3[1] & 0xE0, 0xE0);
    }
}
