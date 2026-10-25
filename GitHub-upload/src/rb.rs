//! Safe wrapper around Rubber Band's real-time "live" pitch shifter (R3 engine) with formant
//! control, a fixed-size block FIFO and pitch-dependent latency compensation so the output is
//! always aligned to a fixed, reported plugin latency.

use crate::dsp::DelayLine;
use std::os::raw::{c_int, c_uint};

#[repr(C)]
struct RubberBandLiveState_ {
    _private: [u8; 0],
}
type LiveState = *mut RubberBandLiveState_;

extern "C" {
    fn rubberband_live_new(sample_rate: c_uint, channels: c_uint, options: c_int) -> LiveState;
    fn rubberband_live_delete(s: LiveState);
    fn rubberband_live_reset(s: LiveState);
    fn rubberband_live_set_pitch_scale(s: LiveState, scale: f64);
    fn rubberband_live_set_formant_scale(s: LiveState, scale: f64);
    fn rubberband_live_get_block_size(s: LiveState) -> c_uint;
    fn rubberband_live_shift(s: LiveState, input: *const *const f32, output: *const *mut f32);
    fn rubberband_live_set_default_debug_level(level: c_int);
}

const OPT_FORMANT_PRESERVED: c_int = 0x0100_0000;
const OPT_CHANNELS_TOGETHER: c_int = 0x1000_0000;

/// Measured steady-state latency (samples, excluding our block FIFO) of the live shifter for
/// pitch shifts of -24..=24 semitones in steps of 3, at ~44.1/48 kHz and ~88.2/96 kHz.
const LAT_LO: [f32; 17] = [
    5425.0, 4730.0, 4180.0, 3707.0, 3299.0, 2987.0, 2692.0, 2379.0, 2112.0, 2227.0, 2477.0, 2684.0, 3120.0,
    3448.0, 3867.0, 4474.0, 5192.0,
];
const LAT_HI: [f32; 17] = [
    10446.0, 9121.0, 7991.0, 7074.0, 6359.0, 5656.0, 5054.0, 4570.0, 4160.0, 4560.0, 4995.0, 5355.0, 6245.0,
    6925.0, 7835.0, 9043.0, 10421.0,
];

fn table_latency(sr: f32, semis: f32) -> f32 {
    let x = ((semis.clamp(-24.0, 24.0) + 24.0) / 3.0).clamp(0.0, 16.0);
    let i = (x.floor() as usize).min(15);
    let f = x - i as f32;
    let interp = |t: &[f32; 17]| t[i] + (t[i + 1] - t[i]) * f;
    if sr < 70_000.0 {
        interp(&LAT_LO)
    } else {
        interp(&LAT_HI) * (sr / 96_000.0).max(1.0)
    }
}

pub struct RbShifter {
    st: LiveState,
    block: usize,
    inb: [Vec<f32>; 2],
    outb: [Vec<f32>; 2],
    pos: usize,
    comp: [DelayLine; 2],
    comp_cur: f32,
    sr: f32,
    fixed_latency: usize,
    cur_scale: f64,
    cur_formant: f64,
}

// The raw pointer is only ever used from the audio thread that owns the plugin.
unsafe impl Send for RbShifter {}

impl RbShifter {
    pub fn new(sr: f32) -> Self {
        unsafe { rubberband_live_set_default_debug_level(-1) };
        let st = unsafe {
            rubberband_live_new(sr as c_uint, 2, OPT_FORMANT_PRESERVED | OPT_CHANNELS_TOGETHER)
        };
        assert!(!st.is_null(), "Rubber Band failed to initialise");
        let block = unsafe { rubberband_live_get_block_size(st) } as usize;
        let fixed_latency = Self::fixed_latency_for(sr, block);
        let mut s = Self {
            st,
            block,
            inb: [vec![0.0; block], vec![0.0; block]],
            outb: [vec![0.0; block], vec![0.0; block]],
            pos: 0,
            comp: [DelayLine::new(fixed_latency + 16), DelayLine::new(fixed_latency + 16)],
            comp_cur: 0.0,
            sr,
            fixed_latency,
            cur_scale: 1.0,
            cur_formant: 1.0,
        };
        s.comp_cur = s.comp_target(0.0);
        s
    }

    /// Latency of this processor: aligned for shifts within +-12 semitones.
    fn fixed_latency_for(sr: f32, block: usize) -> usize {
        (block as f32 + table_latency(sr, -12.0).max(table_latency(sr, 12.0))).ceil() as usize + 8
    }

    pub fn latency(&self) -> usize {
        self.fixed_latency
    }

    fn comp_target(&self, semis: f32) -> f32 {
        (self.fixed_latency as f32 - self.block as f32 - table_latency(self.sr, semis)).max(0.0).round()
    }

    pub fn reset(&mut self) {
        unsafe { rubberband_live_reset(self.st) };
        for c in 0..2 {
            self.inb[c].iter_mut().for_each(|x| *x = 0.0);
            self.outb[c].iter_mut().for_each(|x| *x = 0.0);
            self.comp[c].clear();
        }
        self.pos = 0;
        self.cur_scale = 1.0;
        self.cur_formant = 1.0;
        unsafe {
            rubberband_live_set_pitch_scale(self.st, 1.0);
            rubberband_live_set_formant_scale(self.st, 1.0);
        }
    }

    /// Process one stereo sample. `semis` = pitch shift, `formant` = formant ratio (1 = keep).
    /// Output is delayed by exactly `latency()` samples for |semis| <= 12.
    #[inline]
    pub fn process(&mut self, l: f32, r: f32, semis: f32, formant: f32) -> (f32, f32) {
        self.inb[0][self.pos] = l;
        self.inb[1][self.pos] = r;
        let ol = self.outb[0][self.pos];
        let or = self.outb[1][self.pos];
        self.pos += 1;
        if self.pos == self.block {
            self.pos = 0;
            let scale = (semis as f64 / 12.0).exp2();
            if (scale - self.cur_scale).abs() > 1e-6 {
                self.cur_scale = scale;
                unsafe { rubberband_live_set_pitch_scale(self.st, scale) };
            }
            let f = formant as f64;
            if (f - self.cur_formant).abs() > 1e-6 {
                self.cur_formant = f;
                unsafe { rubberband_live_set_formant_scale(self.st, f) };
            }
            let ins = [self.inb[0].as_ptr(), self.inb[1].as_ptr()];
            let outs = [self.outb[0].as_mut_ptr(), self.outb[1].as_mut_ptr()];
            unsafe { rubberband_live_shift(self.st, ins.as_ptr(), outs.as_ptr()) };
        }
        // Latency compensation, slewed slowly so it never sounds like a pitch bend.
        let target = self.comp_target(semis);
        let max_step = 0.02;
        self.comp_cur += (target - self.comp_cur).clamp(-max_step, max_step);
        self.comp[0].push(ol);
        self.comp[1].push(or);
        (self.comp[0].read(self.comp_cur), self.comp[1].read(self.comp_cur))
    }
}

impl Drop for RbShifter {
    fn drop(&mut self) {
        unsafe { rubberband_live_delete(self.st) };
    }
}
