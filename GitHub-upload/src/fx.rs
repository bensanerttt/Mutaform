//! The four effect slots that process the wet signal.

use crate::dsp::{one_pole_coef, DelayLine};
use crate::params::FxKind;
use std::f32::consts::PI;

#[derive(Clone, Copy, Default)]
struct Svf {
    ic1: f32,
    ic2: f32,
}

impl Svf {
    /// Returns (lowpass, highpass).
    #[inline]
    fn process(&mut self, x: f32, g: f32, k: f32) -> (f32, f32) {
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;
        let v3 = x - self.ic2;
        let v1 = a1 * self.ic1 + a2 * v3;
        let v2 = self.ic2 + a2 * self.ic1 + a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        (v2, x - k * v1 - v2)
    }
}

struct Comb {
    buf: Vec<f32>,
    pos: usize,
    store: f32,
}

impl Comb {
    fn new(len: usize) -> Self {
        Self { buf: vec![0.0; len.max(1)], pos: 0, store: 0.0 }
    }
    #[inline]
    fn process(&mut self, x: f32, feedback: f32, damp: f32) -> f32 {
        let out = self.buf[self.pos];
        self.store = out * (1.0 - damp) + self.store * damp;
        self.buf[self.pos] = x + self.store * feedback;
        self.pos += 1;
        if self.pos >= self.buf.len() {
            self.pos = 0;
        }
        out
    }
    fn clear(&mut self) {
        self.buf.iter_mut().for_each(|x| *x = 0.0);
        self.store = 0.0;
    }
}

struct Allpass {
    buf: Vec<f32>,
    pos: usize,
}

impl Allpass {
    fn new(len: usize) -> Self {
        Self { buf: vec![0.0; len.max(1)], pos: 0 }
    }
    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let b = self.buf[self.pos];
        let out = b - x;
        self.buf[self.pos] = x + b * 0.5;
        self.pos += 1;
        if self.pos >= self.buf.len() {
            self.pos = 0;
        }
        out
    }
    fn clear(&mut self) {
        self.buf.iter_mut().for_each(|x| *x = 0.0);
    }
}

/// Small Schroeder/Moorer style stereo reverb.
struct Reverb {
    combs: [Vec<Comb>; 2],
    aps: [Vec<Allpass>; 2],
}

impl Reverb {
    fn new(sr: f32) -> Self {
        const COMBS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
        const APS: [usize; 4] = [556, 441, 341, 225];
        let s = sr / 44_100.0;
        let mk = |spread: usize| {
            (
                COMBS.iter().map(|&c| Comb::new(((c + spread) as f32 * s) as usize)).collect(),
                APS.iter().map(|&a| Allpass::new(((a + spread) as f32 * s) as usize)).collect(),
            )
        };
        let (c0, a0) = mk(0);
        let (c1, a1) = mk(23);
        Self { combs: [c0, c1], aps: [a0, a1] }
    }
    #[inline]
    fn process(&mut self, l: f32, r: f32, size: f32, damp: f32) -> (f32, f32) {
        let input = (l + r) * 0.015;
        let fb = 0.7 + 0.28 * size;
        let dm = damp * 0.4;
        let mut out = [0.0f32; 2];
        for ch in 0..2 {
            let mut acc = 0.0;
            for c in self.combs[ch].iter_mut() {
                acc += c.process(input, fb, dm);
            }
            for a in self.aps[ch].iter_mut() {
                acc = a.process(acc);
            }
            out[ch] = acc * 3.0;
        }
        (out[0], out[1])
    }
    fn clear(&mut self) {
        for ch in 0..2 {
            self.combs[ch].iter_mut().for_each(Comb::clear);
            self.aps[ch].iter_mut().for_each(Allpass::clear);
        }
    }
}

pub struct FxSlot {
    sr: f32,
    svf: [Svf; 2],
    tone: [f32; 2],
    dl: [DelayLine; 2],
    delay_time: f32,
    lfo: f32,
    reverb: Reverb,
    crush_hold: [f32; 2],
    crush_count: f32,
    last_kind: FxKind,
    comp_env: f32,
    comp_gain: f32,
    eq_pres: [crate::dsp::Biquad; 2],
    eq_air: [crate::dsp::Biquad; 2],
    eq_key: (f32, f32),
    ds_env_hi: f32,
    ds_env_all: f32,
    ds_red: f32,
}

impl FxSlot {
    pub fn new(sr: f32) -> Self {
        let max_delay = (sr * 2.1) as usize;
        Self {
            sr,
            svf: [Svf::default(); 2],
            tone: [0.0; 2],
            dl: [DelayLine::new(max_delay), DelayLine::new(max_delay)],
            delay_time: 0.0,
            lfo: 0.0,
            reverb: Reverb::new(sr),
            crush_hold: [0.0; 2],
            crush_count: 0.0,
            last_kind: FxKind::Off,
            comp_env: 0.0,
            comp_gain: 1.0,
            eq_pres: [crate::dsp::Biquad::default(); 2],
            eq_air: [crate::dsp::Biquad::default(); 2],
            eq_key: (-1.0, -1.0),
            ds_env_hi: 0.0,
            ds_env_all: 0.0,
            ds_red: 0.0,
        }
    }

    pub fn reset(&mut self) {
        self.svf = [Svf::default(); 2];
        self.tone = [0.0; 2];
        self.dl.iter_mut().for_each(DelayLine::clear);
        self.reverb.clear();
        self.crush_hold = [0.0; 2];
        self.lfo = 0.0;
        self.comp_env = 0.0;
        self.comp_gain = 1.0;
        self.eq_key = (-1.0, -1.0);
        self.eq_pres.iter_mut().chain(self.eq_air.iter_mut()).for_each(|b| b.reset());
        self.ds_env_hi = 0.0;
        self.ds_env_all = 0.0;
        self.ds_red = 0.0;
    }

    #[inline]
    /// `tempo` = host tempo in BPM (0 if unknown); used by the delay.
    #[allow(clippy::too_many_arguments)]
    pub fn process(&mut self, kind: FxKind, a: f32, b: f32, mix: f32, l: f32, r: f32, tempo: f32) -> (f32, f32) {
        if kind != self.last_kind {
            self.reset();
            self.last_kind = kind;
            self.delay_time = 0.0;
        }
        let sr = self.sr;
        let (wl, wr) = match kind {
            FxKind::Off => return (l, r),
            FxKind::LowPass | FxKind::HighPass => {
                let fc = 20.0 * 1000.0f32.powf(a);
                let g = (PI * (fc / sr).min(0.49)).tan();
                let k = 2.0 - 1.95 * b;
                let (l1, h1) = self.svf[0].process(l, g, k);
                let (l2, h2) = self.svf[1].process(r, g, k);
                if kind == FxKind::LowPass {
                    (l1, l2)
                } else {
                    (h1, h2)
                }
            }
            FxKind::Drive => {
                let drive = (a * 36.0 / 20.0 * std::f32::consts::LN_10).exp(); // 0..36 dB
                let comp = 1.0 / drive.sqrt();
                let fc = 800.0 * 25.0f32.powf(b);
                let c = 1.0 - (-2.0 * PI * fc / sr).exp();
                let dl = (l * drive).tanh() * comp;
                let dr = (r * drive).tanh() * comp;
                self.tone[0] += (dl - self.tone[0]) * c;
                self.tone[1] += (dr - self.tone[1]) * c;
                (self.tone[0], self.tone[1])
            }
            FxKind::Chorus => {
                let rate = 0.1 * 50.0f32.powf(a);
                self.lfo = (self.lfo + rate / sr).fract();
                let base = 0.010 * sr;
                let depth = b * 0.007 * sr;
                self.dl[0].push(l);
                self.dl[1].push(r);
                let ml = base + depth * 0.5 * (1.0 + (2.0 * PI * self.lfo).sin());
                let mr = base + depth * 0.5 * (1.0 + (2.0 * PI * (self.lfo + 0.25)).sin());
                (self.dl[0].read(ml), self.dl[1].read(mr))
            }
            FxKind::Delay => {
                let target = if tempo > 20.0 {
                    let (quarters, _) = crate::params::delay_div(a);
                    (quarters * 60.0 / tempo * sr).min(2.0 * sr)
                } else {
                    0.020 * 50.0f32.powf(a) * sr
                };
                if self.delay_time <= 0.0 {
                    self.delay_time = target;
                }
                self.delay_time += (target - self.delay_time) * one_pole_coef(sr, 0.08);
                let fb = b * 0.9;
                let yl = self.dl[0].read(self.delay_time);
                let yr = self.dl[1].read(self.delay_time);
                // Ping-pong: each side feeds the other.
                self.dl[0].push(0.5 * (l + r) + yr * fb);
                self.dl[1].push(yl * fb);
                (yl, yr)
            }
            FxKind::Reverb => self.reverb.process(l, r, a, b),
            FxKind::Crush => {
                let bits = 16.0 - a * 14.0;
                let steps = bits.exp2() * 0.5;
                let factor = 1.0 + b * 39.0;
                self.crush_count += 1.0;
                if self.crush_count >= factor {
                    self.crush_count -= factor;
                    self.crush_hold = [(l * steps).round() / steps, (r * steps).round() / steps];
                }
                (self.crush_hold[0], self.crush_hold[1])
            }
            FxKind::Compressor => {
                // Feed-forward, stereo-linked. A: amount (threshold + ratio), B: speed.
                let lvl = l.abs().max(r.abs());
                let att = 0.030 - 0.028 * b; // 30 ms .. 2 ms
                let rel = 0.30 - 0.24 * b; // 300 ms .. 60 ms
                let ca = 1.0 - (-1.0 / (att * sr)).exp();
                let cr = 1.0 - (-1.0 / (rel * sr)).exp();
                self.comp_env += (lvl - self.comp_env) * if lvl > self.comp_env { ca } else { cr };
                let thr = -6.0 - 30.0 * a;
                let ratio = 2.0 + 4.0 * a;
                let env_db = 20.0 * self.comp_env.max(1e-6).log10();
                let over = env_db - thr;
                let knee = 6.0;
                let red = if over <= -knee / 2.0 {
                    0.0
                } else if over >= knee / 2.0 {
                    over * (1.0 - 1.0 / ratio)
                } else {
                    (over + knee / 2.0).powi(2) / (2.0 * knee) * (1.0 - 1.0 / ratio)
                };
                let makeup = -thr * (1.0 - 1.0 / ratio) * 0.3;
                let g = 10f32.powf((makeup - red) / 20.0);
                (l * g, r * g)
            }
            FxKind::Eq => {
                // A: presence (peak at 3.5 kHz), B: air (shelf at 11 kHz); 0.5 = flat, +-9 dB.
                let key = ((a * 100.0).round(), (b * 100.0).round());
                if key != self.eq_key {
                    self.eq_key = key;
                    let pg = (a - 0.5) * 18.0;
                    let ag = (b - 0.5) * 18.0;
                    for c in 0..2 {
                        self.eq_pres[c].set_peak(sr, 3500.0, 0.9, pg);
                        self.eq_air[c].set_high_shelf(sr, 11000.0, ag);
                    }
                }
                let yl = self.eq_air[0].process(self.eq_pres[0].process(l));
                let yr = self.eq_air[1].process(self.eq_pres[1].process(r));
                (yl, yr)
            }
            FxKind::DeEsser => {
                // Dynamically subtracts part of the band above `freq` when it dominates.
                let fc = 4000.0 + 6000.0 * b;
                let g = (PI * (fc / sr).min(0.45)).tan();
                let k = 1.4;
                let (_, hl) = self.svf[0].process(l, g, k);
                let (_, hr) = self.svf[1].process(r, g, k);
                let hi = hl.abs().max(hr.abs());
                let all = l.abs().max(r.abs());
                let c_fast = 1.0 - (-1.0 / (0.002 * sr)).exp();
                let c_slow = 1.0 - (-1.0 / (0.06 * sr)).exp();
                self.ds_env_hi += (hi - self.ds_env_hi) * if hi > self.ds_env_hi { c_fast } else { c_slow };
                self.ds_env_all += (all - self.ds_env_all) * if all > self.ds_env_all { c_fast } else { c_slow };
                let ratio = self.ds_env_hi / self.ds_env_all.max(1e-5);
                let target = ((ratio - 0.35) * 2.5).clamp(0.0, 1.0) * a * 0.8;
                self.ds_red += (target - self.ds_red) * c_fast;
                (l - hl * self.ds_red, r - hr * self.ds_red)
            }
            FxKind::Tremolo => {
                let rate = 0.5 * 30.0f32.powf(a);
                self.lfo = (self.lfo + rate / sr).fract();
                let ml = 1.0 - b * 0.5 * (1.0 + (2.0 * PI * self.lfo).sin());
                let mr = 1.0 - b * 0.5 * (1.0 + (2.0 * PI * (self.lfo + 0.1)).sin());
                (l * ml, r * mr)
            }
        };
        (l + (wl - l) * mix, r + (wr - r) * mix)
    }
}
