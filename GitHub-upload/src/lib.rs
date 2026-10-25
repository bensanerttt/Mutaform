//! Mutaform – a pitch / formant / harmonic voice mangler.

use nih_plug::prelude::*;
use std::sync::Arc;

pub mod chain;
pub mod dsp;
pub mod editor;
pub mod engine;
pub mod fx;
pub mod layers;
pub mod params;
pub mod pitch;
pub mod presets;
pub mod rb;

use dsp::{one_pole_coef, DelayLine, FreqShifter};
use chain::{Chain, ChainControls};
use fx::FxSlot;
use params::{MidiMode, MutaformParams, NUM_FX_SLOTS};

/// Values shared with the editor.
pub struct SharedState {
    /// Detected input fundamental in Hz (0 when unvoiced).
    pub input_hz: AtomicF32,
    /// Output peak level (linear).
    pub out_peak: AtomicF32,
    /// Note the key lock is steering to (MIDI number, <0 = inactive).
    pub target_midi: AtomicF32,
    /// Host tempo in BPM (0 = unknown), for showing synced delay times.
    pub tempo: AtomicF32,
}

/// Harmonic shifter + FM stage applied to the wet signal.
pub struct HarmonicStage {
    sr: f32,
    shift: [FreqShifter; 2],
    fm_dl: [DelayLine; 2],
    fm_phase: f32,
    base_delay: f32,
    f0_smooth: f32,
    neutral: f32,
}

impl HarmonicStage {
    pub fn new(sr: f32) -> Self {
        let base_delay = (0.009 * sr).ceil() + 2.0;
        Self {
            sr,
            shift: [FreqShifter::new(), FreqShifter::new()],
            fm_dl: [DelayLine::new((base_delay * 2.5) as usize), DelayLine::new((base_delay * 2.5) as usize)],
            fm_phase: 0.0,
            base_delay,
            f0_smooth: 220.0,
            neutral: 1.0,
        }
    }

    pub fn latency(&self) -> usize {
        self.base_delay as usize
    }

    fn reset(&mut self) {
        self.shift = [FreqShifter::new(), FreqShifter::new()];
        self.fm_dl.iter_mut().for_each(DelayLine::clear);
        self.fm_phase = 0.0;
    }

    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub fn process(&mut self, l: f32, r: f32, f0: f32, harmonics: f32, ratio: f32, fm: f32, snap: bool) -> (f32, f32) {
        let sr = self.sr;
        self.f0_smooth += (f0.clamp(30.0, 3000.0) - self.f0_smooth) * one_pole_coef(sr, 0.02);
        let f0 = self.f0_smooth;

        let h = if snap { (harmonics * 4.0).round() / 4.0 } else { harmonics };
        let dphase = 2.0 * std::f32::consts::PI * h * f0 / sr;
        // The shifter always runs (keeps its state warm); at Harmonics = 0 we crossfade back to the
        // untouched signal so the plugin is fully transparent.
        let sl = self.shift[0].process(l, dphase);
        let sr_ = self.shift[1].process(r, dphase);
        let target = if h == 0.0 { 1.0 } else { 0.0 };
        self.neutral += (target - self.neutral) * one_pole_coef(sr, 0.03);
        if (self.neutral - target).abs() < 1e-4 {
            self.neutral = target;
        }
        let nm = self.neutral;
        let mut l = sl + (l - sl) * nm;
        let mut r = sr_ + (r - sr_) * nm;

        let ratio = if snap {
            if ratio >= 1.0 {
                ratio.round()
            } else {
                1.0 / (1.0 / ratio).round()
            }
        } else {
            ratio
        };
        let mod_hz = f0 * ratio;
        self.fm_phase += mod_hz / sr;
        self.fm_phase -= self.fm_phase.floor();
        let depth = (3.0 * fm * fm / (2.0 * std::f32::consts::PI * f0) * sr).min(self.base_delay - 2.0);
        let d = self.base_delay + depth * (2.0 * std::f32::consts::PI * self.fm_phase).sin();
        self.fm_dl[0].push(l);
        self.fm_dl[1].push(r);
        l = self.fm_dl[0].read(d);
        r = self.fm_dl[1].read(d);
        (l, r)
    }
}

pub struct Mutaform {
    params: Arc<MutaformParams>,
    shared: Arc<SharedState>,
    chain: Option<Chain>,
    harm: Option<HarmonicStage>,
    fx: Vec<FxSlot>,
    dry: [DelayLine; 2],
    dry_delay: usize,
    peak: f32,
    peak_decay: f32,
    sr: f32,
    last_process: Option<std::time::Instant>,
    resume_samples: u64,
    fade_len: u64,
    bypass_mix: f32,
    bypass_coef: f32,
    lim_gain: f32,
    lim_release: f32,
}

impl Default for Mutaform {
    fn default() -> Self {
        Self {
            params: Arc::new(MutaformParams::default()),
            shared: Arc::new(SharedState { input_hz: AtomicF32::new(0.0), out_peak: AtomicF32::new(0.0), target_midi: AtomicF32::new(-1.0), tempo: AtomicF32::new(0.0) }),
            chain: None,
            harm: None,
            fx: Vec::new(),
            dry: [DelayLine::new(4), DelayLine::new(4)],
            dry_delay: 0,
            peak: 0.0,
            peak_decay: 0.999,
            sr: 48_000.0,
            last_process: None,
            resume_samples: 0,
            fade_len: 480,
            bypass_mix: 0.0,
            bypass_coef: 0.01,
            lim_gain: 1.0,
            lim_release: 0.0005,
        }
    }
}

impl Plugin for Mutaform {
    const NAME: &'static str = "Mutaform";
    const VENDOR: &'static str = "Bendik Sandvold-Ertsås";
    const URL: &'static str = "";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            ..AudioIOLayout::const_default()
        },
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(1),
            main_output_channels: NonZeroU32::new(2),
            ..AudioIOLayout::const_default()
        },
    ];

    const MIDI_INPUT: MidiConfig = MidiConfig::Basic;
    const SAMPLE_ACCURATE_AUTOMATION: bool = false;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        editor::create(self.params.clone(), self.shared.clone())
    }

    fn initialize(
        &mut self,
        _layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        context: &mut impl InitContext<Self>,
    ) -> bool {
        let sr = buffer_config.sample_rate;
        let mut chain = Chain::new(sr);
        chain.set_detect(self.params.detect.value());
        chain.set_mode(self.params.midi_mode.value());
        let harm = HarmonicStage::new(sr);
        self.dry_delay = chain.latency() + harm.latency();
        self.dry = [DelayLine::new(self.dry_delay + 4), DelayLine::new(self.dry_delay + 4)];
        context.set_latency_samples(self.dry_delay as u32);
        self.chain = Some(chain);
        self.harm = Some(harm);
        self.fx = (0..NUM_FX_SLOTS).map(|_| FxSlot::new(sr)).collect();
        self.peak_decay = (-1.0 / (0.3 * sr)).exp();
        self.sr = sr;
        self.fade_len = (0.01 * sr) as u64;
        self.bypass_coef = one_pole_coef(sr, 0.015);
        self.lim_release = one_pole_coef(sr, 0.08);
        self.resume_samples = 0;
        self.last_process = None;
        true
    }

    fn reset(&mut self) {
        self.clear_state();
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // If the host stopped calling us for a while (plugin switched off/bypassed by the host,
        // playback paused in some hosts), our buffers hold stale audio from back then. Playing that
        // out would be heard as a "hiccup" when switching back on, so start clean and fade in.
        let now = std::time::Instant::now();
        if let Some(last) = self.last_process {
            let gap = now.duration_since(last).as_secs_f32();
            let block = buffer.samples() as f32 / self.sr;
            if gap > 0.25f32.max(8.0 * block) {
                self.clear_state();
            }
        }
        self.last_process = Some(now);
        self.process_inner(buffer, aux, context)
    }
}

impl Mutaform {
    fn clear_state(&mut self) {
        self.resume_samples = 0;
        if let Some(c) = self.chain.as_mut() {
            c.reset();
        }
        if let Some(h) = self.harm.as_mut() {
            h.reset();
        }
        self.fx.iter_mut().for_each(FxSlot::reset);
        self.dry.iter_mut().for_each(DelayLine::clear);
    }

    fn process_inner(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let (Some(chain), Some(harm)) = (self.chain.as_mut(), self.harm.as_mut()) else {
            return ProcessStatus::Normal;
        };
        let p = &self.params;
        chain.set_detect(p.detect.value());
        chain.set_mode(p.midi_mode.value());
        let snap = p.snap.value();
        let bypass = p.bypass.value();
        let glide_ms = p.glide.value();
        let midi_mode = p.midi_mode.value();
        let key_lock = p.key_lock.value() && midi_mode == MidiMode::Off;
        let key_root = p.key.value().semitone();
        let scale_mask = p.scale.value().mask();
        let kinds: [params::FxKind; NUM_FX_SLOTS] = std::array::from_fn(|i| p.fx[i].kind.value());
        let tempo = context.transport().tempo.unwrap_or(0.0) as f32;
        self.shared.tempo.store(tempo, std::sync::atomic::Ordering::Relaxed);

        let num_channels = buffer.channels();
        let mut next_event = context.next_event();

        for (sample_id, mut channel_samples) in buffer.iter_samples().enumerate() {
            let ctl = ChainControls {
                pitch_st: p.pitch.smoothed.next(),
                formant_st: p.formant.smoothed.next(),
                alternator: p.alternator.smoothed.next(),
                octave: p.octave.smoothed.next(),
                smear: p.smear.smoothed.next(),
                stereo: p.stereo.smoothed.next(),
                detune: p.detune.smoothed.next(),
                glide_ms,
                key_lock,
                key_root,
                scale_mask,
            };
            let ectl = ctl.engine(true, true);

            while let Some(event) = next_event {
                if event.timing() > sample_id as u32 {
                    break;
                }
                match event {
                    NoteEvent::NoteOn { note, velocity, .. } => chain.engine.note_on(note, velocity, &ectl),
                    NoteEvent::NoteOff { note, .. } => chain.engine.note_off(note, &ectl),
                    NoteEvent::Choke { .. } => chain.engine.all_notes_off(),
                    NoteEvent::MidiCC { cc: 123, .. } | NoteEvent::MidiCC { cc: 120, .. } => {
                        chain.engine.all_notes_off()
                    }
                    _ => {}
                }
                next_event = context.next_event();
            }

            let xl = *channel_samples.get_mut(0).unwrap();
            let xr = if num_channels > 1 { *channel_samples.get_mut(1).unwrap() } else { xl };

            let (mut wl, mut wr) = chain.process(xl, xr, &ctl);

            let harmonics = p.harmonics.smoothed.next();
            let ratio = p.ratio.smoothed.next();
            let fm = p.fm.smoothed.next();
            let (hl, hr) = harm.process(wl, wr, chain.f0_ref(), harmonics, ratio, fm, snap);
            wl = hl;
            wr = hr;

            for (i, slot) in self.fx.iter_mut().enumerate() {
                let sp = &p.fx[i];
                let a = sp.a.smoothed.next();
                let b = sp.b.smoothed.next();
                let mix = sp.mix.smoothed.next();
                let (l, r) = slot.process(kinds[i], a, b, mix, wl, wr, tempo);
                wl = l;
                wr = r;
            }

            // Safety limiter on the processed signal: transparent below -0.2 dBFS, catches peaks
            // from EQ boosts / compression.
            let pk = wl.abs().max(wr.abs());
            let ceiling = 0.977;
            if pk * self.lim_gain > ceiling {
                self.lim_gain = ceiling / pk;
            } else {
                self.lim_gain += (1.0 - self.lim_gain) * self.lim_release;
            }
            wl *= self.lim_gain;
            wr *= self.lim_gain;

            self.dry[0].push(xl);
            self.dry[1].push(xr);
            let dl = self.dry[0].read_int(self.dry_delay);
            let dr = self.dry[1].read_int(self.dry_delay);
            let mix = p.dry_wet.smoothed.next();
            let gain = p.output.smoothed.next();
            let mut ol = (dl + (wl - dl) * mix) * gain;
            let mut or = (dr + (wr - dr) * mix) * gain;

            // Host-integrated bypass: smooth crossfade to the latency-aligned dry signal.
            let bt = if bypass { 1.0 } else { 0.0 };
            self.bypass_mix += (bt - self.bypass_mix) * self.bypass_coef;
            if (self.bypass_mix - bt).abs() < 1e-4 {
                self.bypass_mix = bt;
            }
            if self.bypass_mix > 0.0 {
                ol += (dl - ol) * self.bypass_mix;
                or += (dr - or) * self.bypass_mix;
            }

            // Fade in after a (re)start: silence while the latency buffer fills, then 10 ms ramp,
            // so audio that starts mid-waveform doesn't click.
            if self.resume_samples < self.dry_delay as u64 + self.fade_len {
                let t = self.resume_samples.saturating_sub(self.dry_delay as u64) as f32 / self.fade_len as f32;
                ol *= t;
                or *= t;
                self.resume_samples += 1;
            }

            // Safety: kill NaNs/denormal blowups.
            let ol = if ol.is_finite() { ol.clamp(-4.0, 4.0) } else { 0.0 };
            let or = if or.is_finite() { or.clamp(-4.0, 4.0) } else { 0.0 };

            self.peak = (self.peak * self.peak_decay).max(ol.abs().max(or.abs()));

            *channel_samples.get_mut(0).unwrap() = ol;
            if num_channels > 1 {
                *channel_samples.get_mut(1).unwrap() = or;
            }
        }

        // Drain any events beyond the buffer (shouldn't happen).
        while let Some(event) = next_event {
            if let NoteEvent::NoteOff { note, .. } = event {
                let ctl = ChainControls {
                    pitch_st: 0.0,
                    formant_st: 0.0,
                    alternator: 0.0,
                    octave: 0.0,
                    smear: 0.0,
                    stereo: 0.0,
                    detune: 0.0,
                    glide_ms,
                    key_lock,
                    key_root,
                    scale_mask,
                };
                chain.engine.note_off(note, &ctl.engine(true, true));
            }
            next_event = context.next_event();
        }

        let hz = if chain.engine.detector.voiced { chain.engine.detector.freq() } else { 0.0 };
        self.shared.input_hz.store(hz, std::sync::atomic::Ordering::Relaxed);
        self.shared.out_peak.store(self.peak, std::sync::atomic::Ordering::Relaxed);
        let target = if key_lock { chain.target_midi() } else { -1.0 };
        self.shared.target_midi.store(target, std::sync::atomic::Ordering::Relaxed);

        ProcessStatus::Normal
    }
}

impl ClapPlugin for Mutaform {
    const CLAP_ID: &'static str = "no.bendik.mutaform";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("Pitch, formant and harmonic voice mangler");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] =
        &[ClapFeature::AudioEffect, ClapFeature::Stereo, ClapFeature::PitchShifter, ClapFeature::Distortion];
}

impl Vst3Plugin for Mutaform {
    const VST3_CLASS_ID: [u8; 16] = *b"BendikMutaform01";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::PitchShift, Vst3SubCategory::Stereo];
}

nih_export_clap!(Mutaform);
nih_export_vst3!(Mutaform);
