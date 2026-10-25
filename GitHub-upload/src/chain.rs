//! The voice processing chain:
//!
//! input ─┬─ Rubber Band (pitch + formant + key lock) ─┬─ + octave layer (2nd Rubber Band)
//!        └─ PSOLA engine (MIDI modes only) ───────────┘
//!     → alternator → detune doubler → smear cloud → stereo widener → (neutral bypass)
//!
//! Everything is aligned to one fixed latency so the dry signal and mode switches line up.

use crate::dsp::{one_pole_coef, DelayLine};
use crate::engine::{Engine, EngineControls};
use crate::layers::{Alternator, KeyTracker, MicroShift, Smear, Widener};
use crate::params::{DetectRange, MidiMode};
use crate::rb::RbShifter;

#[derive(Clone, Copy)]
pub struct ChainControls {
    pub pitch_st: f32,
    pub formant_st: f32,
    pub alternator: f32,
    pub octave: f32,
    pub smear: f32,
    pub stereo: f32,
    pub detune: f32,
    pub glide_ms: f32,
    pub key_lock: bool,
    pub key_root: u8,
    pub scale_mask: u16,
}

impl ChainControls {
    pub fn engine(&self, midi: bool, render: bool) -> EngineControls {
        EngineControls {
            pitch_st: self.pitch_st,
            formant_st: self.formant_st,
            alternator: 0.0,
            octave: if midi { self.octave } else { 0.0 },
            smear: 0.0,
            stereo: 0.0,
            detune: 0.0,
            glide_ms: self.glide_ms,
            key_lock: false,
            key_root: self.key_root,
            scale_mask: self.scale_mask,
            render,
        }
    }
}

const F0_DECIM: usize = 32;

pub struct Chain {
    sr: f32,
    pub engine: Engine,
    rb_main: RbShifter,
    rb_oct: RbShifter,
    oct_hold: u32,
    psola_delay: [DelayLine; 2],
    psola_extra: usize,
    dry: [DelayLine; 2],
    latency: usize,
    key: KeyTracker,
    alternator: Alternator,
    micro: [MicroShift; 2],
    smear: Smear,
    widener: Widener,
    mode: MidiMode,
    mode_mix: f32,
    neutral_mix: f32,
    coef30: f32,
    f0_ring: Vec<(f32, f32)>,
    f0_pos: usize,
    f0_count: usize,
    f0_delay: usize,
    f0_last: f32,
    f0_out: f32,
    voiced_out: f32,
    voiced_sm: f32,
}

impl Chain {
    pub fn new(sr: f32) -> Self {
        let engine = Engine::new(sr);
        let rb_main = RbShifter::new(sr);
        let rb_oct = RbShifter::new(sr);
        let latency = rb_main.latency().max(engine.latency());
        let psola_extra = latency - engine.latency();
        let f0_delay = (latency / F0_DECIM).max(1);
        Self {
            sr,
            engine,
            rb_main,
            rb_oct,
            oct_hold: 0,
            psola_delay: [DelayLine::new(psola_extra + 8), DelayLine::new(psola_extra + 8)],
            psola_extra,
            dry: [DelayLine::new(latency + 8), DelayLine::new(latency + 8)],
            latency,
            key: KeyTracker::new(sr),
            alternator: Alternator::new(sr),
            micro: [MicroShift::new(sr), MicroShift::new(sr)],
            smear: Smear::new(sr),
            widener: Widener::new(sr),
            mode: MidiMode::Off,
            mode_mix: 0.0,
            neutral_mix: 1.0,
            coef30: one_pole_coef(sr, 0.03),
            f0_ring: vec![(220.0, 0.0); f0_delay + 4],
            f0_pos: 0,
            f0_count: 0,
            f0_delay,
            f0_last: 220.0,
            f0_out: 220.0,
            voiced_out: 0.0,
            voiced_sm: 0.0,
        }
    }

    pub fn latency(&self) -> usize {
        self.latency
    }

    /// Fundamental of the output signal (Hz), time-aligned with the output.
    pub fn f0_ref(&self) -> f32 {
        self.f0_out
    }

    pub fn target_midi(&self) -> f32 {
        self.key.target_midi
    }

    pub fn reset(&mut self) {
        self.engine.reset();
        self.rb_main.reset();
        self.rb_oct.reset();
        self.oct_hold = 0;
        for d in self.psola_delay.iter_mut().chain(self.dry.iter_mut()) {
            d.clear();
        }
        self.key.reset();
        self.alternator.reset();
        self.micro.iter_mut().for_each(MicroShift::reset);
        self.smear.reset();
        self.widener.reset();
        self.f0_ring.iter_mut().for_each(|x| *x = (220.0, 0.0));
    }

    pub fn set_detect(&mut self, d: DetectRange) {
        self.engine.set_detect(d);
    }

    pub fn set_mode(&mut self, m: MidiMode) {
        self.mode = m;
        self.engine.set_mode(m);
    }

    #[inline]
    pub fn process(&mut self, xl: f32, xr: f32, c: &ChainControls) -> (f32, f32) {
        let midi = self.mode != MidiMode::Off;

        // --- PSOLA engine: always analyses (pitch detection), only renders in MIDI modes.
        let render = midi || self.mode_mix > 0.0;
        let (el, er) = self.engine.process(xl, xr, &c.engine(midi, render));
        self.psola_delay[0].push(el);
        self.psola_delay[1].push(er);
        let pl = self.psola_delay[0].read_int(self.psola_extra);
        let pr = self.psola_delay[1].read_int(self.psola_extra);

        // --- Key lock + main Rubber Band shifter.
        let det_voiced = self.engine.detector.voiced;
        let det_hz = self.engine.detector.freq();
        let corr = if c.key_lock && !midi {
            self.key.process(det_voiced, det_hz, c.pitch_st, c.key_root, c.scale_mask)
        } else {
            self.key.target_midi = -1.0;
            0.0
        };
        let semis = c.pitch_st + corr;
        let formant = (c.formant_st / 12.0).exp2();
        let (ml, mr) = self.rb_main.process(xl, xr, semis, formant);

        // --- Octave layer (pitch-knob mode). In MIDI modes the PSOLA engine makes the octave.
        let o = c.octave;
        let (mut wl, mut wr) = (ml, mr);
        if !midi {
            if o.abs() > 1e-4 {
                self.oct_hold = self.sr as u32;
            }
            if self.oct_hold > 0 {
                self.oct_hold -= 1;
                let os = semis + if o >= 0.0 { 12.0 } else { -12.0 };
                let (ol, or) = self.rb_oct.process(xl, xr, os, formant);
                let main_g = 1.0 - 0.5 * o.abs();
                let oct_g = 0.85 * o.abs();
                wl = ml * main_g + ol * oct_g;
                wr = mr * main_g + or * oct_g;
                if self.oct_hold == 0 {
                    self.rb_oct.reset();
                }
            }
        }

        // --- Crossfade between Rubber Band path and PSOLA (MIDI) path.
        let mt = if midi { 1.0 } else { 0.0 };
        self.mode_mix += (mt - self.mode_mix) * self.coef30;
        if (self.mode_mix - mt).abs() < 1e-4 {
            self.mode_mix = mt;
        }
        if self.mode_mix > 0.0 {
            wl += (pl - wl) * self.mode_mix;
            wr += (pr - wr) * self.mode_mix;
        }

        // --- Output fundamental, delayed to line up with the output.
        let f0_now = if midi {
            self.engine.f0_ref()
        } else if det_voiced {
            det_hz * (semis / 12.0).exp2()
        } else {
            self.f0_last
        };
        if det_voiced || midi {
            self.f0_last = f0_now;
        }
        self.f0_count += 1;
        if self.f0_count >= F0_DECIM {
            self.f0_count = 0;
            let len = self.f0_ring.len();
            self.f0_ring[self.f0_pos] = (f0_now, if det_voiced || midi { 1.0 } else { 0.0 });
            let read = (self.f0_pos + len - self.f0_delay) % len;
            self.f0_out = self.f0_ring[read].0;
            self.voiced_out = self.f0_ring[read].1;
            self.f0_pos = (self.f0_pos + 1) % len;
        }
        self.voiced_sm += (self.voiced_out - self.voiced_sm) * self.coef30;

        // --- Alternator.
        let (al, ar) = self.alternator.process(wl, wr, self.f0_out, c.alternator, self.voiced_sm);
        wl = al;
        wr = ar;

        // --- Detune doubler: two voices, slightly sharp (left) and flat (right).
        let d = c.detune;
        let cents = d * 35.0;
        let vl = self.micro[0].process(wl, (cents / 1200.0).exp2());
        let vr = self.micro[1].process(wr, (-cents / 1200.0).exp2());
        if d > 0.0 {
            let main = 1.0 - 0.35 * d;
            let g = 0.6 * d;
            let (l0, r0) = (wl, wr);
            wl = l0 * main + g * (vl + 0.2 * vr);
            wr = r0 * main + g * (vr + 0.2 * vl);
        }

        // --- Smear cloud and stereo width.
        let (sl, sr_) = self.smear.process(wl, wr, c.smear, c.stereo);
        let (sl, sr_) = self.widener.process(sl, sr_, c.stereo);
        wl = sl;
        wr = sr_;

        // --- Neutral settings: fully transparent (exactly delayed input).
        self.dry[0].push(xl);
        self.dry[1].push(xr);
        let neutral = !midi
            && !c.key_lock
            && c.pitch_st.abs() < 1e-4
            && c.formant_st.abs() < 1e-4
            && c.alternator.abs() < 1e-4
            && o.abs() < 1e-4
            && c.smear < 1e-4
            && c.stereo < 1e-4
            && d < 1e-4;
        let nt = if neutral { 1.0 } else { 0.0 };
        self.neutral_mix += (nt - self.neutral_mix) * self.coef30;
        if (self.neutral_mix - nt).abs() < 1e-4 {
            self.neutral_mix = nt;
        }
        if self.neutral_mix > 0.0 {
            let dl = self.dry[0].read_int(self.latency);
            let dr = self.dry[1].read_int(self.latency);
            wl += (dl - wl) * self.neutral_mix;
            wr += (dr - wr) * self.neutral_mix;
        }
        (wl, wr)
    }
}
