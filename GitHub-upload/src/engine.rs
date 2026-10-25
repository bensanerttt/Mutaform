//! Pitch-synchronous overlap-add (PSOLA) voice engine.
//!
//! The input is analysed for its fundamental period and cut into two-period, Hann windowed
//! grains centred on pitch marks. Each output "stream" re-places those grains at a new spacing
//! (which sets the output pitch) and can resample them (which moves the formants independently).

use crate::dsp::Rng;
use crate::params::{DetectRange, MidiMode};
use crate::pitch::PitchDetector;
use std::collections::VecDeque;

pub const MAX_VOICES: usize = 8;
const STREAMS: usize = 4; // main, octave, detune-left, detune-right
const MIN_HZ_GLOBAL: f32 = 55.0;
const MAX_MARKS: usize = 4096;

#[derive(Clone, Copy)]
struct Mark {
    pos: i64,
    period: f32,
    voiced: bool,
    /// Smoothed input pitch (MIDI note number) at this mark, for key correction.
    smidi: f32,
}

#[derive(Clone, Copy, Default)]
struct Stream {
    next_s: f64,
    counter: u32,
}

#[derive(Clone, Copy, Default)]
struct Voice {
    active: bool,
    note: u8,
    velocity: f32,
    /// Current and target pitch as log2(Hz) (MIDI modes) or log2(ratio) (Off mode).
    lf: f32,
    lf_target: f32,
    env: f32,
    releasing: bool,
    age: u64,
    streams: [Stream; STREAMS],
}

/// Per-sample control values for the engine.
#[derive(Clone, Copy)]
pub struct EngineControls {
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
    /// When false only analysis runs (pitch detection, marks); no grains are rendered.
    pub render: bool,
}

pub struct Engine {
    sr: f32,
    n: i64,
    size: usize,
    mask: usize,
    in_l: Vec<f32>,
    in_r: Vec<f32>,
    in_lp: Vec<f32>,
    out_l: Vec<f32>,
    out_r: Vec<f32>,
    epoch_lp: crate::dsp::Biquad,
    max_t: f32,
    latency: i64,
    unvoiced_t: f32,
    pub detector: PitchDetector,
    marks: VecDeque<Mark>,
    last_mark: f64,
    voices: [Voice; MAX_VOICES],
    mode: MidiMode,
    note_stack: [u8; 16],
    note_stack_len: usize,
    age: u64,
    rng: Rng,
    /// Last known output fundamental (Hz) for the harmonic processors.
    f0_ref: f32,
    /// Hann window lookup table over [-1, 1].
    window: Vec<f32>,
    poly_gain: f32,
    poly_coef: f32,
    mark_smidi: f32,
    mark_prev_voiced: bool,
    key_note: i32,
    key_corr: f32,
    /// Debug/test statistic: how many times the key lock changed target note.
    pub key_switches: u64,
    key_pending: i32,
    key_pending_time: f32,
    key_last_s: f64,
    /// Note the key lock is currently steering to (MIDI number, <0 = none). Read by the GUI.
    pub target_midi: f32,
    neutral_mix: f32,
}

const WIN_LEN: usize = 2048;
const WIN_HALF: usize = WIN_LEN / 2;

impl Engine {
    pub fn new(sr: f32) -> Self {
        let max_t = sr / MIN_HZ_GLOBAL;
        let latency = (3.0 * max_t).ceil() as i64 + 64;
        // History must cover latency + smear look-back + grain reach.
        let size = ((latency as f32 + 12.0 * max_t) as usize).next_power_of_two();
        let mut e = Self {
            sr,
            n: 0,
            size,
            mask: size - 1,
            in_l: vec![0.0; size],
            in_r: vec![0.0; size],
            in_lp: vec![0.0; size],
            out_l: vec![0.0; size],
            out_r: vec![0.0; size],
            epoch_lp: crate::dsp::Biquad::lowpass(sr, 1000.0, 0.707),
            max_t,
            latency,
            unvoiced_t: sr * 0.005,
            detector: PitchDetector::new(sr),
            marks: VecDeque::with_capacity(MAX_MARKS),
            last_mark: 0.0,
            voices: [Voice::default(); MAX_VOICES],
            mode: MidiMode::Off,
            note_stack: [0; 16],
            note_stack_len: 0,
            age: 0,
            rng: Rng::new(0x1234_5678),
            f0_ref: 220.0,
            window: (0..=WIN_LEN)
                .map(|i| 0.5 * (1.0 + (std::f32::consts::PI * (i as f32 / WIN_HALF as f32 - 1.0)).cos()))
                .collect(),
            poly_gain: 1.0,
            poly_coef: crate::dsp::one_pole_coef(sr, 0.03),
            mark_smidi: 60.0,
            mark_prev_voiced: false,
            key_note: -1000,
            key_corr: 0.0,
            key_switches: 0,
            key_pending: -1000,
            key_pending_time: 0.0,
            key_last_s: 0.0,
            target_midi: -1.0,
            neutral_mix: 1.0,
        };
        e.set_mode_internal(MidiMode::Off);
        e
    }

    pub fn latency(&self) -> usize {
        self.latency as usize
    }

    pub fn f0_ref(&self) -> f32 {
        self.f0_ref
    }

    pub fn reset(&mut self) {
        for b in [&mut self.in_l, &mut self.in_r, &mut self.in_lp, &mut self.out_l, &mut self.out_r] {
            b.iter_mut().for_each(|x| *x = 0.0);
        }
        self.detector.reset();
        self.epoch_lp.reset();
        self.marks.clear();
        self.n = 0;
        self.last_mark = 0.0;
        self.note_stack_len = 0;
        self.voices = [Voice::default(); MAX_VOICES];
        self.set_mode_internal(self.mode);
    }

    pub fn set_detect(&mut self, d: DetectRange) {
        let (lo, hi) = d.range_hz();
        self.detector.set_range(lo, hi);
    }

    pub fn set_mode(&mut self, mode: MidiMode) {
        if mode != self.mode {
            self.set_mode_internal(mode);
        }
    }

    fn set_mode_internal(&mut self, mode: MidiMode) {
        self.mode = mode;
        self.voices = [Voice::default(); MAX_VOICES];
        self.note_stack_len = 0;
        if mode == MidiMode::Off {
            let start = self.grain_limit();
            let v = &mut self.voices[0];
            v.active = true;
            v.env = 1.0;
            v.velocity = 1.0;
            for s in v.streams.iter_mut() {
                s.next_s = start;
            }
        }
    }

    #[inline]
    fn grain_limit(&self) -> f64 {
        // Output index currently being emitted is n-1. Grains centred at or before this limit
        // can be rendered: their input is available and their left tail is still in the future.
        ((self.n - 1) + self.latency) as f64 - self.max_t as f64
    }

    // ------------------------------------------------------------------ MIDI

    pub fn note_on(&mut self, note: u8, velocity: f32, ctl: &EngineControls) {
        match self.mode {
            MidiMode::Off => {}
            MidiMode::Mono => {
                self.stack_remove(note);
                if self.note_stack_len < self.note_stack.len() {
                    self.note_stack[self.note_stack_len] = note;
                    self.note_stack_len += 1;
                }
                let start = self.grain_limit();
                let target = note_lf(note, ctl.pitch_st);
                let v = &mut self.voices[0];
                if !v.active || v.releasing && v.env <= 0.0 {
                    v.lf = target;
                    for s in v.streams.iter_mut() {
                        s.next_s = start;
                    }
                }
                if !v.active {
                    v.env = 0.0;
                }
                v.active = true;
                v.releasing = false;
                v.note = note;
                v.velocity = velocity;
                v.lf_target = target;
            }
            MidiMode::Poly => {
                self.age += 1;
                // Retrigger an existing voice with the same note, else a free one, else the oldest.
                let mut idx = self.voices.iter().position(|v| v.active && v.note == note);
                if idx.is_none() {
                    idx = self.voices.iter().position(|v| !v.active);
                }
                let idx = idx.unwrap_or_else(|| {
                    let mut oldest = 0;
                    for (i, v) in self.voices.iter().enumerate() {
                        if v.age < self.voices[oldest].age {
                            oldest = i;
                        }
                    }
                    oldest
                });
                // Glide from the most recently played note.
                let last_lf = self
                    .voices
                    .iter()
                    .filter(|v| v.active)
                    .max_by_key(|v| v.age)
                    .map(|v| v.lf);
                let start = self.grain_limit();
                let target = note_lf(note, ctl.pitch_st);
                let age = self.age;
                let v = &mut self.voices[idx];
                if !v.active {
                    v.env = 0.0;
                    v.lf = last_lf.unwrap_or(target);
                    for s in v.streams.iter_mut() {
                        s.next_s = start;
                    }
                }
                v.active = true;
                v.releasing = false;
                v.note = note;
                v.velocity = velocity;
                v.lf_target = target;
                v.age = age;
            }
        }
    }

    pub fn note_off(&mut self, note: u8, ctl: &EngineControls) {
        match self.mode {
            MidiMode::Off => {}
            MidiMode::Mono => {
                self.stack_remove(note);
                let v = &mut self.voices[0];
                if self.note_stack_len > 0 {
                    let prev = self.note_stack[self.note_stack_len - 1];
                    v.note = prev;
                    v.lf_target = note_lf(prev, ctl.pitch_st);
                } else if v.note == note {
                    v.releasing = true;
                }
            }
            MidiMode::Poly => {
                for v in self.voices.iter_mut() {
                    if v.active && v.note == note {
                        v.releasing = true;
                    }
                }
            }
        }
    }

    pub fn all_notes_off(&mut self) {
        self.note_stack_len = 0;
        if self.mode != MidiMode::Off {
            for v in self.voices.iter_mut() {
                v.releasing = true;
            }
        }
    }

    fn stack_remove(&mut self, note: u8) {
        let mut j = 0;
        for i in 0..self.note_stack_len {
            if self.note_stack[i] != note {
                self.note_stack[j] = self.note_stack[i];
                j += 1;
            }
        }
        self.note_stack_len = j;
    }

    // ------------------------------------------------------------------ audio

    /// Process one stereo sample. Returns the wet signal delayed by `latency()` samples.
    #[inline]
    pub fn process(&mut self, xl: f32, xr: f32, ctl: &EngineControls) -> (f32, f32) {
        let idx = (self.n as usize) & self.mask;
        let mono = 0.5 * (xl + xr);
        self.in_l[idx] = xl;
        self.in_r[idx] = xr;
        self.in_lp[idx] = self.epoch_lp.process(mono);
        self.detector.push(mono);
        self.n += 1;

        self.update_marks();
        self.update_voices(ctl);

        let limit = self.grain_limit();
        let formant = (ctl.formant_st / 12.0).exp2();

        // Stream gains, frequency multipliers and pans.
        let o = ctl.octave;
        let d = ctl.detune;
        let det_cut = 1.0 - 0.4 * d;
        let gains = [
            (1.0 - 0.5 * o.abs()) * det_cut,
            0.85 * o.abs() * det_cut,
            0.6 * d,
            0.6 * d,
        ];
        let det_mult = (d * 0.4 / 12.0).exp2();
        let mults = [1.0, if o >= 0.0 { 2.0 } else { 0.5 }, 1.0 / det_mult, det_mult];
        let pans = [0.0, 0.0, -0.8, 0.8];

        let mode = self.mode;
        let active = self.voices.iter().filter(|v| v.active && !v.releasing).count().max(1);
        let target = 1.0 / (active as f32).sqrt();
        self.poly_gain += (target - self.poly_gain) * self.poly_coef;
        let poly_gain = self.poly_gain;
        let mut lowest_hz = f32::MAX;
        for vi in 0..MAX_VOICES {
            if !self.voices[vi].active {
                continue;
            }
            let v = self.voices[vi];
            let voice_gain = v.env * (0.35 + 0.65 * v.velocity) * poly_gain;
            if mode != MidiMode::Off {
                lowest_hz = lowest_hz.min(v.lf.exp2());
            }
            for si in 0..STREAMS {
                if !ctl.render || gains[si] < 1e-4 || voice_gain <= 0.0 {
                    // Keep the stream's clock current so it resumes cleanly.
                    if self.voices[vi].streams[si].next_s < limit {
                        self.voices[vi].streams[si].next_s = limit;
                    }
                    continue;
                }
                let mut guard = 0;
                while self.voices[vi].streams[si].next_s <= limit && guard < 64 {
                    guard += 1;
                    let s = self.voices[vi].streams[si].next_s;
                    let counter = self.voices[vi].streams[si].counter;
                    let hop = self.place_grain(
                        s,
                        counter,
                        v.lf,
                        mode,
                        si,
                        mults[si],
                        gains[si] * voice_gain,
                        pans[si],
                        formant,
                        ctl,
                        if si == 0 { voice_gain } else { 0.0 },
                    );
                    let st = &mut self.voices[vi].streams[si];
                    st.next_s += hop as f64;
                    st.counter = st.counter.wrapping_add(1);
                }
            }
        }

        // Reference fundamental for harmonics / FM.
        if mode == MidiMode::Off {
            if self.detector.voiced {
                self.f0_ref = self.detector.freq() * self.voices[0].lf.exp2();
            }
        } else if lowest_hz < f32::MAX {
            self.f0_ref = lowest_hz;
        }

        let r = ((self.n - 1) as usize) & self.mask;
        let wet = (self.out_l[r], self.out_r[r]);
        self.out_l[r] = 0.0;
        self.out_r[r] = 0.0;

        // Transparent when nothing is changed: crossfade to the exactly delay-aligned input, so an
        // untouched plugin doesn't add any resynthesis artifacts (stutter on chords, drums, etc.).
        let neutral = mode == MidiMode::Off
            && !ctl.key_lock
            && ctl.pitch_st.abs() < 1e-4
            && ctl.formant_st.abs() < 1e-4
            && ctl.alternator.abs() < 1e-4
            && ctl.octave.abs() < 1e-4
            && ctl.smear < 1e-4
            && ctl.stereo < 1e-4
            && ctl.detune < 1e-4;
        let target = if neutral { 1.0 } else { 0.0 };
        self.neutral_mix += (target - self.neutral_mix) * self.poly_coef;
        if (self.neutral_mix - target).abs() < 1e-4 {
            self.neutral_mix = target;
        }
        let nm = self.neutral_mix;
        if nm <= 0.0 {
            return wet;
        }
        let d = ((self.n - 1 - self.latency) as usize) & self.mask;
        let (dl, dr) = (self.in_l[d], self.in_r[d]);
        if nm >= 1.0 {
            (dl, dr)
        } else {
            (wet.0 + (dl - wet.0) * nm, wet.1 + (dr - wet.1) * nm)
        }
    }

    fn update_marks(&mut self) {
        let n = self.n as f64;
        // Recover after a reset/discontinuity.
        if self.last_mark < n - (self.size as f64) * 0.5 {
            self.last_mark = n - 1.0;
        }
        loop {
            let (t, voiced) = if self.detector.voiced {
                (self.detector.period.clamp(self.sr / 1600.0, self.max_t), true)
            } else {
                (self.unvoiced_t, false)
            };
            let c = self.last_mark + t as f64;
            let reach = if voiced { t as f64 * 0.25 } else { 0.0 };
            if c + reach + 1.0 >= n {
                break;
            }
            let mut pos = c;
            if voiced {
                // Align to the waveform peak of the low-passed signal near the expected position.
                let lo = (c - reach).ceil() as i64;
                let hi = (c + reach).floor() as i64;
                let mut best = f32::MIN;
                for p in lo..=hi {
                    let v = self.in_lp[(p as usize) & self.mask];
                    if v > best {
                        best = v;
                        pos = p as f64;
                    }
                }
            }
            // Smoothed pitch (~50 ms) so key correction fixes the average note but keeps vibrato.
            if voiced {
                let midi = 69.0 + 12.0 * (self.sr / t / 440.0).log2();
                if self.mark_prev_voiced {
                    let k = 1.0 - (-t / (0.03 * self.sr)).exp();
                    self.mark_smidi += (midi - self.mark_smidi) * k;
                } else {
                    self.mark_smidi = midi;
                }
            }
            self.mark_prev_voiced = voiced;
            if self.marks.len() >= MAX_MARKS {
                self.marks.pop_front();
            }
            self.marks.push_back(Mark { pos: pos as i64, period: t, voiced, smidi: self.mark_smidi });
            self.last_mark = pos;
        }
        // Drop marks that fell out of the history.
        let oldest = self.n - self.size as i64 + 2 * self.max_t as i64;
        while let Some(m) = self.marks.front() {
            if m.pos < oldest {
                self.marks.pop_front();
            } else {
                break;
            }
        }
    }

    fn update_voices(&mut self, ctl: &EngineControls) {
        let glide_coef = crate::dsp::one_pole_coef(self.sr, ctl.glide_ms * 0.001);
        let attack = 1.0 / (0.004 * self.sr);
        let release = 1.0 / (0.04 * self.sr);
        let mode = self.mode;
        for v in self.voices.iter_mut() {
            if !v.active {
                continue;
            }
            if mode == MidiMode::Off {
                v.lf_target = ctl.pitch_st / 12.0;
            } else {
                v.lf_target = note_lf(v.note, ctl.pitch_st);
            }
            v.lf += (v.lf_target - v.lf) * glide_coef;
            if v.releasing {
                v.env -= release;
                if v.env <= 0.0 {
                    v.env = 0.0;
                    v.active = false;
                }
            } else if v.env < 1.0 {
                v.env = (v.env + attack).min(1.0);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn place_grain(
        &mut self,
        s: f64,
        counter: u32,
        lf: f32,
        mode: MidiMode,
        stream: usize,
        mult: f32,
        gain: f32,
        pan: f32,
        formant: f32,
        ctl: &EngineControls,
        unvoiced_gain: f32,
    ) -> f32 {
        let avail = self.n - 1;
        if self.marks.is_empty() {
            return self.unvoiced_t;
        }
        // Nearest analysis mark to the matching input time.
        let t_in = s - self.latency as f64;
        let mut i = self.marks.partition_point(|m| (m.pos as f64) < t_in);
        if i >= self.marks.len() {
            i = self.marks.len() - 1;
        } else if i > 0 {
            let a = self.marks[i - 1].pos as f64;
            let b = self.marks[i].pos as f64;
            if (t_in - a) < (b - t_in) {
                i -= 1;
            }
        }
        // Smear: randomly reach back a few grains.
        if ctl.smear > 0.0 {
            let back = (self.rng.uni() * ctl.smear * 6.0) as usize;
            i = i.saturating_sub(back);
        }
        // Make sure the whole grain's input is available.
        while i > 0 && self.marks[i].pos + self.marks[i].period.ceil() as i64 + 1 > avail {
            i -= 1;
        }
        let m = self.marks[i];
        if m.pos + m.period.ceil() as i64 + 1 > avail {
            return self.unvoiced_t;
        }

        let t_a = m.period;
        if !m.voiced {
            return self.place_unvoiced_grain(s, counter, lf, mode, formant, pan, ctl, unvoiced_gain);
        }
        let mut hop = {
            let mut hz = match mode {
                MidiMode::Off => self.sr / t_a * lf.exp2(),
                _ => lf.exp2(),
            };
            if mode == MidiMode::Off && ctl.key_lock {
                if stream == 0 {
                    let raw = m.smidi + 12.0 * lf;
                    self.update_key(raw, s, ctl);
                }
                hz *= (self.key_corr / 12.0).exp2();
            }
            self.sr / (hz * mult).max(1.0)
        };
        if ctl.smear > 0.0 {
            hop *= 1.0 + ctl.smear * 0.35 * self.rng.bi();
        }
        hop = hop.clamp(8.0, 4.0 * self.max_t);

        // Cap the overlap factor so extreme settings (high pitch + low formant) stay cheap.
        let tg = (t_a / formant).min(2.0 * self.max_t).min(3.0 * hop).max(hop.min(t_a)).max(4.0);
        // Overlap normalisation: Hann windows of half-length tg spaced by hop sum to tg/hop.
        let mut g = gain * (hop / tg).powf(0.6).min(1.25);
        let alt = ctl.alternator;
        if counter & 1 == 1 && alt != 0.0 {
            g *= if alt > 0.0 { 1.0 - alt } else { 1.0 + 2.0 * alt };
        }
        if ctl.smear > 0.0 {
            g *= 1.0 - 0.5 * ctl.smear * self.rng.uni();
        }
        let (gl, gr) = self.grain_pan(pan, counter, g, ctl);
        self.overlap_add(s, m.pos, 0.0, tg, formant, gl, gr);
        hop
    }

    /// Unvoiced / unclear input (consonants, breaths, chords, noisy mixes): a plain granular
    /// resampling shifter centred exactly on the matching input time. It shifts *everything* by
    /// the Pitch amount, so the pitch never flips back to "unshifted" when the detector loses
    /// track (that flipping was heard as a crazy, autotune-like warble).
    #[allow(clippy::too_many_arguments)]
    fn place_unvoiced_grain(
        &mut self,
        s: f64,
        counter: u32,
        lf: f32,
        mode: MidiMode,
        formant: f32,
        pan: f32,
        ctl: &EngineControls,
        gain: f32,
    ) -> f32 {
        let mut ratio = if mode == MidiMode::Off { lf.exp2() } else { 1.0 };
        if mode == MidiMode::Off && ctl.key_lock {
            // Keep the last key correction through unclear/unvoiced bits, otherwise the sound
            // flips between corrected and uncorrected pitch many times a second.
            ratio *= (self.key_corr / 12.0).exp2();
        }
        let factor = (ratio * formant).clamp(0.125, 8.0);
        // Grain half-length: ~20 ms, limited so the forward read stays inside available input.
        let tg = (0.02 * self.sr).min((self.max_t - 4.0) / factor.max(1.0)).min(2.0 * self.max_t).max(16.0);
        let mut hop = tg * 0.5;
        if gain <= 0.0 {
            return hop;
        }
        let mut center_in = s - self.latency as f64;
        if ctl.smear > 0.0 {
            center_in -= (self.rng.uni() * ctl.smear * 0.03 * self.sr) as f64;
            hop *= 1.0 + ctl.smear * 0.3 * self.rng.bi();
        }
        let base = center_in.floor() as i64;
        let base_frac = (center_in - base as f64) as f32;
        // Hann windows at 50 % overlap sum to 2.
        let mut g = gain * 0.5;
        if ctl.smear > 0.0 {
            g *= 1.0 - 0.5 * ctl.smear * self.rng.uni();
        }
        let (gl, gr) = self.grain_pan(pan, counter, g, ctl);
        self.overlap_add(s, base, base_frac, tg, factor, gl, gr);
        hop
    }

    /// Key lock: pick the target scale note for the (smoothed) input pitch `raw` at output time
    /// `s`, and glide the correction towards it. A new note must be "meant" (stay closest for
    /// ~45 ms) before we switch, so wobbly or misdetected pitch doesn't make it jump around.
    fn update_key(&mut self, raw: f32, s: f64, ctl: &EngineControls) {
        let dt = (s - self.key_last_s).clamp(0.0, self.sr as f64) as f32;
        self.key_last_s = s;
        let candidate = snap_to_scale(raw, self.key_note, ctl.key_root, ctl.scale_mask);
        if self.key_note < -100 || (raw - self.key_note as f32).abs() > 1.0 {
            // First note, or a clear jump to somewhere else: switch straight away.
            if candidate != self.key_note {
                self.key_switches += 1;
            }
            self.key_note = candidate;
            self.key_pending = candidate;
            self.key_pending_time = 0.0;
        } else if candidate != self.key_note {
            if candidate == self.key_pending {
                self.key_pending_time += dt;
            } else {
                self.key_pending = candidate;
                self.key_pending_time = 0.0;
            }
            if self.key_pending_time > 0.045 * self.sr {
                self.key_note = candidate;
                self.key_switches += 1;
            }
        } else {
            self.key_pending = candidate;
            self.key_pending_time = 0.0;
        }
        self.target_midi = self.key_note as f32;
        let target = (self.key_note as f32 - raw).clamp(-3.0, 3.0);
        let coef = 1.0 - (-dt / (0.02 * self.sr)).exp();
        self.key_corr += (target - self.key_corr) * coef;
    }

    #[inline]
    fn grain_pan(&mut self, pan: f32, counter: u32, g: f32, ctl: &EngineControls) -> (f32, f32) {
        let mut p = pan;
        if ctl.stereo > 0.0 {
            let side = if counter & 1 == 0 { 0.65 } else { -0.65 };
            p += ctl.stereo * (side + 0.35 * self.rng.bi());
        }
        let p = p.clamp(-1.0, 1.0);
        (g * (1.0 - p).min(1.0), g * (1.0 + p).min(1.0))
    }

    /// Window a grain of half-length `tg` read around input position `base + base_frac`
    /// (resampled by `factor`) and add it to the output centred at time `s`.
    #[allow(clippy::too_many_arguments)]
    #[inline]
    fn overlap_add(&mut self, s: f64, base: i64, base_frac: f32, tg: f32, factor: f32, gl: f32, gr: f32) {
        // Fractional grain placement: avoids +-0.5 sample jitter (heard as fizz on high harmonics).
        let center = s.floor() as i64;
        let fs = (s - center as f64) as f32;
        let k_max = tg.floor() as i64;
        let win_step = WIN_HALF as f32 / tg;
        let mask = self.mask;
        let window = &self.window[..];
        let (in_l, in_r) = (&self.in_l[..], &self.in_r[..]);
        let (out_l, out_r) = (&mut self.out_l[..], &mut self.out_r[..]);
        for k in (-k_max + 1)..=k_max {
            let tau = k as f32 - fs;
            let w = window[((tau * win_step + WIN_HALF as f32 + 0.5) as i32).clamp(0, WIN_LEN as i32) as usize];
            let off = tau * factor + base_frac;
            let oi = off.floor();
            let frac = off - oi;
            let i0 = ((base + oi as i64) as usize) & mask;
            let i1 = (i0 + 1) & mask;
            let l = in_l[i0] + (in_l[i1] - in_l[i0]) * frac;
            let r = in_r[i0] + (in_r[i1] - in_r[i0]) * frac;
            let o = ((center + k) as usize) & mask;
            out_l[o] += w * gl * l;
            out_r[o] += w * gr * r;
        }
    }
}

#[inline]
fn note_lf(note: u8, pitch_st: f32) -> f32 {
    // log2(Hz)
    (440.0f32).log2() + (note as f32 - 69.0 + pitch_st) / 12.0
}

/// Nearest note in the scale to `raw` (MIDI number), with hysteresis around the current note so
/// a note sung between two scale steps doesn't flip back and forth.
pub fn snap_to_scale(raw: f32, current: i32, root: u8, mask: u16) -> i32 {
    let in_scale = |n: i32| mask & (1 << ((n - root as i32).rem_euclid(12))) != 0;
    if current > -100 && in_scale(current) && (raw - current as f32).abs() < 0.65 {
        return current;
    }
    let base = raw.round() as i32;
    let mut best = base;
    let mut best_d = f32::MAX;
    for n in base - 6..=base + 6 {
        if in_scale(n) {
            let d = (n as f32 - raw).abs();
            if d < best_d {
                best_d = d;
                best = n;
            }
        }
    }
    best
}
