//! Offline test renderer: cargo run --release --example render -- <out_dir>
use mutaform::engine::{Engine, EngineControls};
use mutaform::fx::FxSlot;
use mutaform::params::{FxKind, MidiMode, DetectRange};
use mutaform::HarmonicStage;
use std::io::Write;

fn write_wav(path: &str, sr: u32, l: &[f32], r: &[f32]) {
    let mut f = std::fs::File::create(path).unwrap();
    let n = l.len() as u32;
    let data = n * 4;
    f.write_all(b"RIFF").unwrap();
    f.write_all(&(36 + data).to_le_bytes()).unwrap();
    f.write_all(b"WAVEfmt ").unwrap();
    f.write_all(&16u32.to_le_bytes()).unwrap();
    f.write_all(&1u16.to_le_bytes()).unwrap();
    f.write_all(&2u16.to_le_bytes()).unwrap();
    f.write_all(&sr.to_le_bytes()).unwrap();
    f.write_all(&(sr * 4).to_le_bytes()).unwrap();
    f.write_all(&4u16.to_le_bytes()).unwrap();
    f.write_all(&16u16.to_le_bytes()).unwrap();
    f.write_all(b"data").unwrap();
    f.write_all(&data.to_le_bytes()).unwrap();
    for i in 0..l.len() {
        for v in [l[i], r[i]] {
            let s = (v.clamp(-1.0, 1.0) * 32767.0) as i16;
            f.write_all(&s.to_le_bytes()).unwrap();
        }
    }
}

struct Reso { b0: f32, a1: f32, a2: f32, y1: f32, y2: f32 }
impl Reso {
    fn new(sr: f32, f: f32, bw: f32) -> Self {
        let r = (-std::f32::consts::PI * bw / sr).exp();
        let a1 = -2.0 * r * (2.0 * std::f32::consts::PI * f / sr).cos();
        let a2 = r * r;
        Self { b0: 1.0 - r, a1, a2, y1: 0.0, y2: 0.0 }
    }
    fn p(&mut self, x: f32) -> f32 { let y = self.b0 * x - self.a1 * self.y1 - self.a2 * self.y2; self.y2 = self.y1; self.y1 = y; y }
}

/// Synthetic sung vowel "ah": glottal-ish pulse train through 3 formants.
fn vowel(sr: f32, secs: f32, f0: f32) -> Vec<f32> {
    let n = (sr * secs) as usize;
    let mut f = [Reso::new(sr, 700.0, 110.0), Reso::new(sr, 1220.0, 120.0), Reso::new(sr, 2600.0, 160.0)];
    let mut ph = 0.0f32;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / sr;
        let fr = f0 * (1.0 + 0.01 * (2.0 * std::f32::consts::PI * 5.0 * t).sin());
        ph += fr / sr;
        if ph >= 1.0 { ph -= 1.0; }
        // Rosenberg-like pulse derivative.
        let g = if ph < 0.6 { (std::f32::consts::PI * ph / 0.6).sin().powi(2) } else { 0.0 };
        let x = g - 0.3;
        let y = f[0].p(x) * 1.0 + f[1].p(x) * 0.5 + f[2].p(x) * 0.25;
        let env = (t / 0.05).min(1.0) * ((secs - t) / 0.05).min(1.0);
        out.push(y * env * 2.0);
    }
    let peak = out.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
    out.iter_mut().for_each(|x| *x *= 0.5 / peak);
    out
}

struct Case { name: &'static str, ctl: EngineControls, mode: MidiMode, notes: &'static [u8], harm: f32, ratio: f32, fm: f32, snap: bool, fx: Option<FxKind> }

fn base() -> EngineControls {
    EngineControls { pitch_st: 0.0, formant_st: 0.0, alternator: 0.0, octave: 0.0, smear: 0.0, stereo: 0.0, detune: 0.0, glide_ms: 0.0, key_lock: false, key_root: 0, scale_mask: 0xFFF, render: true }
}

fn main() {
    let out_dir = std::env::args().nth(1).unwrap_or("/tmp/render".into());
    std::fs::create_dir_all(&out_dir).unwrap();
    let sr = 48_000.0;
    let input = vowel(sr, 3.0, 196.0); // G3
    write_wav(&format!("{out_dir}/input.wav"), sr as u32, &input, &input);

    let c = |name, ctl, mode, notes| Case { name, ctl, mode, notes, harm: 0.0, ratio: 1.0, fm: 0.0, snap: false, fx: None };
    let mut cases = vec![
        c("pitch0", base(), MidiMode::Off, &[]),
        c("pitch-12", EngineControls { pitch_st: -12.0, ..base() }, MidiMode::Off, &[]),
        c("pitch+7", EngineControls { pitch_st: 7.0, ..base() }, MidiMode::Off, &[]),
        c("formant+6", EngineControls { formant_st: 6.0, ..base() }, MidiMode::Off, &[]),
        c("formant-6", EngineControls { formant_st: -6.0, ..base() }, MidiMode::Off, &[]),
        c("midi60", base(), MidiMode::Mono, &[60]),
        c("poly", base(), MidiMode::Poly, &[60, 64, 67]),
        c("alt+1", EngineControls { alternator: 1.0, ..base() }, MidiMode::Off, &[]),
        c("alt-1", EngineControls { alternator: -1.0, ..base() }, MidiMode::Off, &[]),
        c("oct-1", EngineControls { octave: -1.0, ..base() }, MidiMode::Off, &[]),
        c("smear_stereo_detune", EngineControls { smear: 0.8, stereo: 1.0, detune: 1.0, ..base() }, MidiMode::Off, &[]),
    ];
    cases.push(Case { harm: 0.5, snap: true, ..c("harm0.5", base(), MidiMode::Off, &[]) });
    cases.push(Case { fm: 0.6, ratio: 2.0, snap: true, ..c("fm", base(), MidiMode::Off, &[]) });
    cases.push(Case { fx: Some(FxKind::Reverb), ..c("reverb", base(), MidiMode::Off, &[]) });
    // Stress case for CPU timing.
    cases.push(Case { harm: 0.3, fm: 0.4, fx: Some(FxKind::Reverb), ..c("stress", EngineControls { pitch_st: 12.0, formant_st: -12.0, octave: 0.7, smear: 0.5, stereo: 1.0, detune: 1.0, alternator: 0.3, ..base() }, MidiMode::Poly, &[48, 55, 60, 64, 67, 71, 74, 79]) });

    if std::env::args().nth(2).as_deref() == Some("file") {
        let path = std::env::args().nth(3).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let data_pos = bytes.windows(4).position(|w| w == b"data").unwrap() + 8;
        let inp: Vec<f32> = bytes[data_pos..].chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0).collect();
        let runs: [(&str, EngineControls); 6] = [
            ("pitch-5", EngineControls { pitch_st: -5.0, ..base() }),
            ("pitch-5_formant-3", EngineControls { pitch_st: -5.0, formant_st: -3.0, ..base() }),
            ("pitch+12", EngineControls { pitch_st: 12.0, ..base() }),
            ("oct_detune_alt", EngineControls { octave: -0.6, detune: 0.5, alternator: 0.5, ..base() }),
            ("keylock_Gmaj_p-5", EngineControls { pitch_st: -5.0, key_lock: true, key_root: 7, scale_mask: mutaform::params::Scale::Major.mask(), ..base() }),
            ("keylock_Cmaj", EngineControls { key_lock: true, key_root: 0, scale_mask: mutaform::params::Scale::Major.mask(), ..base() }),
        ];
        let mut track: Vec<f32> = Vec::new();
        for (name, ctl) in runs {
            let mut e = Engine::new(sr);
            let mut l = Vec::new();
            let (mut flips, mut prev_v, mut voiced_n, mut jumps, mut prev_hz) = (0, false, 0, 0, 0.0f32);
            for (i, &x) in inp.iter().enumerate() {
                let (a, _) = e.process(x, x, &ctl);
                l.push(a);
                if i % 480 == 0 {
                    if name.starts_with("pitch-5") { track.push(if e.detector.voiced { e.detector.freq() } else { 0.0 }); }
                    let v = e.detector.voiced;
                    if v != prev_v { flips += 1; }
                    if v { voiced_n += 1; let hz = e.detector.freq(); if prev_v && prev_hz > 0.0 { let r = hz / prev_hz; if r > 1.8 || r < 0.55 { jumps += 1; } } prev_hz = hz; }
                    prev_v = v;
                }
            }
            let l: Vec<f32> = l[e.latency()..].to_vec();
            if name.starts_with("keylock") { println!("  key note switches: {} ({:.1}/s)", e.key_switches, e.key_switches as f32 / (inp.len() as f32 / sr)); }
            println!("{name:20} voiced frames {voiced_n:5}  voicing flips {flips:4}  octave jumps {jumps:3}");
            write_wav(&format!("{out_dir}/{name}.wav"), sr as u32, &l, &l);
        }
        std::fs::write(format!("{out_dir}/track.txt"), track.iter().map(|v| format!("{v}\n")).collect::<String>()).unwrap();
        return;
    }
    if std::env::args().nth(2).as_deref() == Some("key") {
        // Sour input: G3 + 35 cents. Key lock to C major should pull it to G3 (196 Hz).
        let sour = vowel(sr, 3.0, 196.0 * (0.35f32 / 12.0).exp2());
        // Speech-like: vowel segments at different pitches separated by noisy "s" bursts.
        let mut speech = Vec::new();
        let mut rng = 12345u32;
        for (i, f) in [180.0f32, 205.0, 230.0, 190.0, 250.0, 170.0].iter().enumerate() {
            speech.extend(vowel(sr, 0.35, *f));
            let mut hp = 0.0f32; let mut prev = 0.0f32;
            for _ in 0..(sr * 0.09) as usize {
                rng ^= rng << 13; rng ^= rng >> 17; rng ^= rng << 5;
                let n = (rng as f32 / u32::MAX as f32) * 2.0 - 1.0;
                hp = 0.9 * (hp + n - prev); prev = n;
                speech.push(hp * 0.15 * if i % 2 == 0 { 1.0 } else { 0.5 });
            }
        }
        // Melody in C major sung 30 cents sharp, 250 ms per note.
        let mut melody = Vec::new();
        for n in [55u8, 57, 59, 60, 62, 64, 62, 60] {
            let f = 440.0 * ((n as f32 - 69.0 + 0.3) / 12.0).exp2();
            melody.extend(vowel(sr, 0.25, f));
        }
        {
            let ctl = EngineControls { key_lock: true, key_root: 0, scale_mask: mutaform::params::Scale::Major.mask(), ..base() };
            let mut e = Engine::new(sr);
            let mut out = Vec::new();
            for &x in &melody { let (a, _) = e.process(x, x, &ctl); out.push(a); }
            let out: Vec<f32> = out[e.latency()..].to_vec();
            write_wav(&format!("{out_dir}/melody_lock.wav"), sr as u32, &out, &out);
            write_wav(&format!("{out_dir}/melody_in.wav"), sr as u32, &melody, &melody);
        }
        write_wav(&format!("{out_dir}/sour_in.wav"), sr as u32, &sour, &sour);
        write_wav(&format!("{out_dir}/speech_in.wav"), sr as u32, &speech, &speech);
        let runs: [(&str, &Vec<f32>, EngineControls); 5] = [
            ("sour_nolock", &sour, base()),
            ("sour_lockCmaj", &sour, EngineControls { key_lock: true, key_root: 0, scale_mask: mutaform::params::Scale::Major.mask(), ..base() }),
            ("sour_lock_pitch+3", &sour, EngineControls { pitch_st: 3.0, key_lock: true, key_root: 0, scale_mask: mutaform::params::Scale::Major.mask(), ..base() }),
            ("speech_oct", &speech, EngineControls { octave: -0.6, detune: 0.5, alternator: 0.5, ..base() }),
            ("speech_pitch-5", &speech, EngineControls { pitch_st: -5.0, formant_st: -3.0, ..base() }),
        ];
        for (name, inp, ctl) in runs {
            let mut e = Engine::new(sr);
            let mut l = Vec::new();
            for &x in inp.iter() { let (a, _) = e.process(x, x, &ctl); l.push(a); }
            let lat = e.latency();
            let l: Vec<f32> = l[lat..].to_vec();
            // Click metric: largest |second difference| relative to RMS.
            let rms = (l.iter().map(|v| v * v).sum::<f32>() / l.len() as f32).sqrt();
            let mut md = 0.0f32;
            for i in 2..l.len() { md = md.max((l[i] - 2.0 * l[i-1] + l[i-2]).abs()); }
            println!("{name:20} rms {rms:.3}  max 2nd-diff/rms {:.2}", md / rms.max(1e-6));
            write_wav(&format!("{out_dir}/{name}.wav"), sr as u32, &l, &l);
        }
        return;
    }
    if std::env::args().nth(2).as_deref() == Some("long") {
        // 8 minutes of looped input through pitch +7, keep the last 3 s.
        let ctl = EngineControls { pitch_st: 7.0, ..base() };
        let mut e = Engine::new(sr);
        let total = (sr * 480.0) as usize;
        let mut l = Vec::new();
        for i in 0..total {
            let x = input[i % input.len()];
            let (a, _) = e.process(x, x, &ctl);
            if i >= total - input.len() { l.push(a); }
        }
        write_wav(&format!("{out_dir}/long_pitch+7.wav"), sr as u32, &l, &l);
        return;
    }
    for case in cases {
        let mut e = Engine::new(sr);
        e.set_detect(DetectRange::Mid);
        e.set_mode(case.mode);
        let mut h = HarmonicStage::new(sr);
        let mut fx = FxSlot::new(sr);
        for &n in case.notes { e.note_on(n, 1.0, &case.ctl); }
        let t0 = std::time::Instant::now();
        let mut l = Vec::with_capacity(input.len());
        let mut r = Vec::with_capacity(input.len());
        for &x in &input {
            let (a, b) = e.process(x, x, &case.ctl);
            let (a, b) = h.process(a, b, e.f0_ref(), case.harm, case.ratio, case.fm, case.snap);
            let (a, b) = match case.fx { Some(k) => fx.process(k, 0.6, 0.4, 0.4, a, b, 120.0), None => (a, b) };
            l.push(a); r.push(b);
        }
        let el = t0.elapsed().as_secs_f64();
        let nan = l.iter().chain(r.iter()).any(|v| !v.is_finite());
        let rms = (l.iter().map(|v| v * v).sum::<f32>() / l.len() as f32).sqrt();
        let peak = l.iter().chain(r.iter()).fold(0.0f32, |a, &b| a.max(b.abs()));
        if case.name == "pitch0" {
            let lat = e.latency() + h.latency();
            let maxerr = (0..input.len() - lat).map(|i| (l[i + lat] - input[i]).abs()).fold(0.0f32, f32::max);
            println!("  neutral check: max |out - delayed input| = {maxerr:e}");
        }
        println!("{:22} realtime x{:7.1}  rms {:.3}  peak {:.3}  nan {}  latency {}", case.name, 3.0 / el, rms, peak, nan, e.latency() + h.latency());
        write_wav(&format!("{out_dir}/{}.wav", case.name), sr as u32, &l, &r);
    }
}
