//! Effect layers applied to the pitch-shifted signal: key lock tracking, alternator, detune
//! doubler, smear cloud and stereo widener. None of these need grain-level pitch detection, so
//! they can't produce the warbling/robotic artefacts of a PSOLA resynthesis.

use crate::dsp::{Biquad, DelayLine, Rng};
use crate::engine::snap_to_scale;
use std::f32::consts::PI;

// ------------------------------------------------------------------------------------------------
// Key lock

/// Follows the input pitch and decides how many semitones (fractional) to add so the shifted
/// note lands on the chosen scale.
pub struct KeyTracker {
    sr: f32,
    smidi: f32,
    have_pitch: bool,
    note: i32,
    pending: i32,
    pending_time: f32,
    corr: f32,
    pub target_midi: f32,
}

impl KeyTracker {
    pub fn new(sr: f32) -> Self {
        Self {
            sr,
            smidi: 60.0,
            have_pitch: false,
            note: -1000,
            pending: -1000,
            pending_time: 0.0,
            corr: 0.0,
            target_midi: -1.0,
        }
    }

    pub fn reset(&mut self) {
        let sr = self.sr;
        *self = Self::new(sr);
    }

    /// Call once per sample. Returns the correction in semitones.
    #[inline]
    pub fn process(&mut self, voiced: bool, input_hz: f32, pitch_st: f32, root: u8, mask: u16) -> f32 {
        if voiced && input_hz > 20.0 {
            let midi = 69.0 + 12.0 * (input_hz / 440.0).log2();
            if self.have_pitch {
                self.smidi += (midi - self.smidi) * (1.0 / (0.03 * self.sr));
            } else {
                self.smidi = midi;
                self.have_pitch = true;
            }
            let raw = self.smidi + pitch_st;
            let cand = snap_to_scale(raw, self.note, root, mask);
            if self.note < -100 || (raw - self.note as f32).abs() > 1.0 {
                self.note = cand;
                self.pending = cand;
                self.pending_time = 0.0;
            } else if cand != self.note {
                if cand == self.pending {
                    self.pending_time += 1.0;
                } else {
                    self.pending = cand;
                    self.pending_time = 0.0;
                }
                if self.pending_time > 0.045 * self.sr {
                    self.note = cand;
                }
            } else {
                self.pending_time = 0.0;
            }
            let target = (self.note as f32 - raw).clamp(-3.0, 3.0);
            self.corr += (target - self.corr) * (1.0 / (0.02 * self.sr));
            self.target_midi = self.note as f32;
        } else {
            // Hold the correction through unvoiced/unclear parts.
            self.have_pitch = false;
        }
        self.corr
    }
}

// ------------------------------------------------------------------------------------------------
// Alternator

/// Treats every other pitch cycle differently (attenuated or polarity-flipped) by tracking the
/// cycles of the low-passed signal with a Schmitt trigger. Gives sub-octave / hollow octave-down.
pub struct Alternator {
    sr: f32,
    lp1: Biquad,
    lp2: Biquad,
    high: bool,
    flip: bool,
    env: f32,
    g: f32,
    fc: f32,
}

impl Alternator {
    pub fn new(sr: f32) -> Self {
        Self {
            sr,
            lp1: Biquad::lowpass(sr, 300.0, 0.7),
            lp2: Biquad::lowpass(sr, 300.0, 0.7),
            high: false,
            flip: false,
            env: 0.0,
            g: 1.0,
            fc: 300.0,
        }
    }

    pub fn reset(&mut self) {
        self.lp1.reset();
        self.lp2.reset();
        self.env = 0.0;
        self.g = 1.0;
    }

    /// `f0` = fundamental of the signal (Hz), `amount` -1..1, `voiced` 0..1.
    #[inline]
    pub fn process(&mut self, l: f32, r: f32, f0: f32, amount: f32, voiced: f32) -> (f32, f32) {
        let fc = (f0 * 1.25).clamp(60.0, 1500.0);
        if (fc / self.fc - 1.0).abs() > 0.03 {
            self.fc = fc;
            self.lp1.set_lowpass(self.sr, fc, 0.7);
            self.lp2.set_lowpass(self.sr, fc, 0.7);
        }
        let y = self.lp2.process(self.lp1.process(0.5 * (l + r)));
        let decay = 1.0 - 1.0 / (0.03 * self.sr);
        self.env = (self.env * decay).max(y.abs());
        let th = 0.2 * self.env;
        if !self.high && y > th {
            self.high = true;
            self.flip = !self.flip;
        } else if self.high && y < -th {
            self.high = false;
        }
        let odd = if amount >= 0.0 { 1.0 - amount } else { 1.0 + 2.0 * amount };
        let target = if self.flip { 1.0 + (odd - 1.0) * voiced } else { 1.0 };
        let step = 1.0 / (0.0015 * self.sr);
        self.g += (target - self.g).clamp(-step, step);
        (l * self.g, r * self.g)
    }
}

// ------------------------------------------------------------------------------------------------
// Detune doubler

/// Smooth delay-line pitch shifter for small shifts (a few cents): two crossfaded taps sweeping
/// through a 40 ms window. Ideal for doubling/detune voices.
pub struct MicroShift {
    buf: DelayLine,
    phase: f32,
    w: f32,
}

impl MicroShift {
    pub fn new(sr: f32) -> Self {
        let w = 0.04 * sr;
        Self { buf: DelayLine::new((w * 1.2) as usize + 8), phase: 0.0, w }
    }

    pub fn reset(&mut self) {
        self.buf.clear();
        self.phase = 0.0;
    }

    #[inline]
    pub fn process(&mut self, x: f32, ratio: f32) -> f32 {
        self.buf.push(x);
        let up = ratio >= 1.0;
        self.phase += (ratio - 1.0).abs() / self.w;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        let tap = |p: f32| if up { (1.0 - p) * self.w } else { p * self.w } + 2.0;
        let p2 = (self.phase + 0.5).fract();
        let g1 = (PI * self.phase).sin().powi(2);
        let g2 = (PI * p2).sin().powi(2);
        self.buf.read(tap(self.phase)) * g1 + self.buf.read(tap(p2)) * g2
    }
}

// ------------------------------------------------------------------------------------------------
// Smear cloud

#[derive(Clone, Copy, Default)]
struct CloudGrain {
    pos: usize,
    delay: f32,
    drift: f32,
    pan: f32,
}

/// Granular diffuser: overlapping grains read the recent signal from random delays (with a tiny
/// random drift) and random pan positions. Turns the voice into an airy, choir-like cloud.
pub struct Smear {
    sr: f32,
    l: DelayLine,
    r: DelayLine,
    grains: [CloudGrain; 4],
    len: usize,
    window: Vec<f32>,
    rng: Rng,
}

impl Smear {
    pub fn new(sr: f32) -> Self {
        let len = (0.045 * sr) as usize;
        let mut s = Self {
            sr,
            l: DelayLine::new((0.2 * sr) as usize),
            r: DelayLine::new((0.2 * sr) as usize),
            grains: [CloudGrain::default(); 4],
            len,
            window: (0..len).map(|i| (PI * i as f32 / len as f32).sin().powi(2)).collect(),
            rng: Rng::new(0x9e37_79b9),
        };
        for (i, g) in s.grains.iter_mut().enumerate() {
            g.pos = i * len / 4;
            g.delay = 2.0;
        }
        s
    }

    pub fn reset(&mut self) {
        self.l.clear();
        self.r.clear();
    }

    #[inline]
    pub fn process(&mut self, l: f32, r: f32, smear: f32, stereo: f32) -> (f32, f32) {
        self.l.push(l);
        self.r.push(r);
        if smear <= 0.0 {
            return (l, r);
        }
        let mut cl = 0.0;
        let mut cr = 0.0;
        let max_delay = 0.09 * self.sr;
        for g in self.grains.iter_mut() {
            if g.pos >= self.len {
                g.pos = 0;
                g.delay = 2.0 + self.rng.uni() * smear * max_delay;
                g.drift = self.rng.bi() * smear * 0.003;
                g.pan = (stereo.max(0.3 * smear) * self.rng.bi()).clamp(-1.0, 1.0);
            }
            let w = self.window[g.pos];
            g.pos += 1;
            g.delay = (g.delay + g.drift).clamp(2.0, max_delay + 4.0);
            let a = self.l.read(g.delay) * w;
            let b = self.r.read(g.delay) * w;
            cl += a * (1.0 - g.pan).min(1.0);
            cr += b * (1.0 + g.pan).min(1.0);
        }
        // Four sin^2 windows at 25 % offsets sum to 2.
        let wet = smear * 0.5;
        let dry = 1.0 - 0.55 * smear;
        (l * dry + cl * wet, r * dry + cr * wet)
    }
}

// ------------------------------------------------------------------------------------------------
// Stereo widener

struct Allpass {
    buf: DelayLine,
    d: usize,
    g: f32,
}

impl Allpass {
    fn new(d: usize, g: f32) -> Self {
        Self { buf: DelayLine::new(d + 4), d, g }
    }
    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let delayed = self.buf.read_int(self.d - 1);
        let y = -self.g * x + delayed;
        self.buf.push(x + self.g * y);
        y
    }
}

/// Adds a decorrelated side signal (allpass-diffused mid), mono compatible.
pub struct Widener {
    aps: [Allpass; 3],
}

impl Widener {
    pub fn new(sr: f32) -> Self {
        Self {
            aps: [
                Allpass::new((0.0031 * sr) as usize, 0.6),
                Allpass::new((0.0071 * sr) as usize, 0.6),
                Allpass::new((0.0113 * sr) as usize, 0.6),
            ],
        }
    }

    pub fn reset(&mut self) {
        for a in self.aps.iter_mut() {
            a.buf.clear();
        }
    }

    #[inline]
    pub fn process(&mut self, l: f32, r: f32, width: f32) -> (f32, f32) {
        let mut s = 0.5 * (l + r);
        for a in self.aps.iter_mut() {
            s = a.process(s);
        }
        if width <= 0.0 {
            return (l, r);
        }
        let w = width * 0.6;
        (l + s * w, r - s * w)
    }
}
