//! Offline test of the full voice chain.
//! cargo run --release --example chain_render -- <out_dir> [input.wav]
use mutaform::chain::{Chain, ChainControls};
use mutaform::fx::FxSlot;
use mutaform::params::{FxKind, MidiMode};
use mutaform::presets::PRESETS;
use mutaform::HarmonicStage;
use std::io::Write;

fn write_wav(path: &str, sr: u32, l: &[f32], r: &[f32]) {
    let mut f = std::fs::File::create(path).unwrap();
    let data = (l.len() * 4) as u32;
    f.write_all(b"RIFF").unwrap();
    f.write_all(&(36 + data).to_le_bytes()).unwrap();
    f.write_all(b"WAVEfmt ").unwrap();
    for v in [16u32] { f.write_all(&v.to_le_bytes()).unwrap(); }
    for v in [1u16, 2] { f.write_all(&v.to_le_bytes()).unwrap(); }
    f.write_all(&sr.to_le_bytes()).unwrap();
    f.write_all(&(sr * 4).to_le_bytes()).unwrap();
    for v in [4u16, 16] { f.write_all(&v.to_le_bytes()).unwrap(); }
    f.write_all(b"data").unwrap();
    f.write_all(&data.to_le_bytes()).unwrap();
    for i in 0..l.len() {
        for v in [l[i], r[i]] {
            f.write_all(&((v.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes()).unwrap();
        }
    }
}

fn read_wav_mono(path: &str) -> Vec<f32> {
    let bytes = std::fs::read(path).unwrap();
    let p = bytes.windows(4).position(|w| w == b"data").unwrap() + 8;
    bytes[p..].chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0).collect()
}

fn vowel(sr: f32, secs: f32, f0: f32) -> Vec<f32> {
    let n = (sr * secs) as usize;
    let res = |f: f32, bw: f32| {
        let r = (-std::f32::consts::PI * bw / sr).exp();
        (1.0 - r, -2.0 * r * (2.0 * std::f32::consts::PI * f / sr).cos(), r * r)
    };
    let fs = [res(700.0, 110.0), res(1220.0, 120.0), res(2600.0, 160.0)];
    let mut st = [(0.0f32, 0.0f32); 3];
    let mut ph = 0.0f32;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / sr;
        ph += f0 * (1.0 + 0.01 * (2.0 * std::f32::consts::PI * 5.0 * t).sin()) / sr;
        ph -= ph.floor();
        let x = if ph < 0.6 { (std::f32::consts::PI * ph / 0.6).sin().powi(2) } else { 0.0 } - 0.3;
        let mut y = 0.0;
        for (k, (b0, a1, a2)) in fs.iter().enumerate() {
            let v = b0 * x - a1 * st[k].0 - a2 * st[k].1;
            st[k].1 = st[k].0;
            st[k].0 = v;
            y += v * [1.0, 0.5, 0.25][k];
        }
        out.push(y * (t / 0.05).min(1.0) * ((secs - t) / 0.05).min(1.0));
    }
    let pk = out.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
    out.iter_mut().for_each(|x| *x *= 0.5 / pk);
    out
}

fn base() -> ChainControls {
    ChainControls {
        pitch_st: 0.0,
        formant_st: 0.0,
        alternator: 0.0,
        octave: 0.0,
        smear: 0.0,
        stereo: 0.0,
        detune: 0.0,
        glide_ms: 0.0,
        key_lock: false,
        key_root: 0,
        scale_mask: 0xFFF,
    }
}

struct Run {
    name: String,
    ctl: ChainControls,
    harm: f32,
    ratio: f32,
    fm: f32,
    snap: bool,
    mode: MidiMode,
    notes: Vec<u8>,
    fx: [(FxKind, f32, f32, f32); mutaform::params::NUM_FX_SLOTS],
}

fn run(sr: f32, input: &[f32], r: &Run) -> (Vec<f32>, Vec<f32>, f64, usize) {
    let mut c = Chain::new(sr);
    c.set_mode(r.mode);
    let mut h = HarmonicStage::new(sr);
    let mut fx: Vec<FxSlot> = (0..mutaform::params::NUM_FX_SLOTS).map(|_| FxSlot::new(sr)).collect();
    let ectl = r.ctl.engine(true, true);
    for &n in &r.notes {
        c.engine.note_on(n, 1.0, &ectl);
    }
    let lat = c.latency() + h.latency();
    let t0 = std::time::Instant::now();
    let (mut l, mut rr) = (Vec::new(), Vec::new());
    for &x in input.iter().chain(std::iter::repeat(&0.0).take(lat)) {
        let (a, b) = c.process(x, x, &r.ctl);
        let (mut a, mut b) = h.process(a, b, c.f0_ref(), r.harm, r.ratio, r.fm, r.snap);
        for (i, s) in fx.iter_mut().enumerate() {
            let (k, pa, pb, mx) = r.fx[i];
            let o = s.process(k, pa, pb, mx, a, b, 120.0);
            a = o.0;
            b = o.1;
        }
        l.push(a);
        rr.push(b);
    }
    let el = t0.elapsed().as_secs_f64();
    (l[lat..].to_vec(), rr[lat..].to_vec(), (input.len() as f64 / sr as f64) / el, lat)
}

fn main() {
    let out = std::env::args().nth(1).unwrap_or("/tmp/chain".into());
    std::fs::create_dir_all(&out).unwrap();
    let sr = 48_000.0;
    let input = match std::env::args().nth(2) {
        Some(p) => read_wav_mono(&p),
        None => vowel(sr, 3.0, 196.0),
    };
    write_wav(&format!("{out}/input.wav"), sr as u32, &input, &input);
    let off = (FxKind::Off, 0.5, 0.5, 0.5);
    let mk = |name: &str, ctl: ChainControls| Run {
        name: name.into(),
        ctl,
        harm: 0.0,
        ratio: 1.0,
        fm: 0.0,
        snap: false,
        mode: MidiMode::Off,
        notes: vec![],
        fx: [off; mutaform::params::NUM_FX_SLOTS],
    };
    let mut runs = vec![
        mk("neutral", base()),
        mk("pitch-5", ChainControls { pitch_st: -5.0, ..base() }),
        mk("pitch+7", ChainControls { pitch_st: 7.0, ..base() }),
        mk("pitch-12", ChainControls { pitch_st: -12.0, ..base() }),
        mk("formant+6", ChainControls { formant_st: 6.0, ..base() }),
        mk("formant-6", ChainControls { formant_st: -6.0, ..base() }),
        mk("oct-1", ChainControls { octave: -1.0, ..base() }),
        mk("alt+1", ChainControls { alternator: 1.0, ..base() }),
        mk("alt-1", ChainControls { alternator: -1.0, ..base() }),
        mk("detune1", ChainControls { detune: 1.0, ..base() }),
        mk("smear_stereo", ChainControls { smear: 0.8, stereo: 1.0, ..base() }),
        mk("keylock_Cmaj", ChainControls { key_lock: true, scale_mask: 0b1010_1011_0101, ..base() }),
    ];
    runs.push(Run { mode: MidiMode::Mono, notes: vec![60], ..mk("midi60", base()) });
    for p in PRESETS {
        runs.push(Run {
            name: format!("preset_{}", p.name.replace(' ', "_").replace('/', "_")),
            ctl: ChainControls {
                pitch_st: p.pitch,
                formant_st: p.formant,
                alternator: p.alternator,
                octave: p.octave,
                smear: p.smear,
                stereo: p.stereo,
                detune: p.detune,
                glide_ms: p.glide,
                ..base()
            },
            harm: p.harmonics,
            ratio: p.ratio,
            fm: p.fm,
            snap: p.snap,
            mode: p.midi_mode,
            notes: if p.midi_mode == MidiMode::Off { vec![] } else { vec![55, 62] },
            fx: std::array::from_fn(|i| (p.fx[i].kind, p.fx[i].a, p.fx[i].b, p.fx[i].mix)),
        });
    }
    for r in &runs {
        let (l, rr, rt, lat) = run(sr, &input, r);
        let nan = l.iter().chain(rr.iter()).any(|v| !v.is_finite());
        let rms = (l.iter().map(|v| v * v).sum::<f32>() / l.len() as f32).sqrt();
        let peak = l.iter().chain(rr.iter()).fold(0.0f32, |a, &b| a.max(b.abs()));
        let extra = if r.name == "neutral" {
            let e = (0..input.len()).map(|i| (l[i] - input[i]).abs()).fold(0.0f32, f32::max);
            format!("  max|out-in| {e:e}")
        } else {
            String::new()
        };
        println!("{:28} x{:5.1} realtime  rms {rms:.3} peak {peak:.3} nan {nan} lat {lat}{extra}", r.name, rt);
        write_wav(&format!("{out}/{}.wav", r.name), sr as u32, &l, &rr);
    }
}
