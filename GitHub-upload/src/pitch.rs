//! YIN pitch detector running on a decimated copy of the input.

use crate::dsp::Biquad;

pub struct PitchDetector {
    sr: f32,
    dec: usize,
    dec_count: usize,
    lp1: Biquad,
    lp2: Biquad,
    /// Ring of decimated samples.
    ring: Vec<f32>,
    ring_pos: usize,
    hop: usize,
    hop_count: usize,
    scratch: Vec<f32>,
    diff: Vec<f32>,
    min_lag: usize,
    max_lag: usize,
    /// Current estimate in full-rate samples.
    pub period: f32,
    pub voiced: bool,
    pub clarity: f32,
    octave_hold: u8,
    pending: f32,
}

const RING: usize = 2048;

impl PitchDetector {
    pub fn new(sr: f32) -> Self {
        let dec = ((sr / 11_025.0).floor() as usize).max(1);
        let sr_dec = sr / dec as f32;
        let mut d = Self {
            sr,
            dec,
            dec_count: 0,
            lp1: Biquad::lowpass(sr, sr_dec * 0.4, 0.541),
            lp2: Biquad::lowpass(sr, sr_dec * 0.4, 1.307),
            ring: vec![0.0; RING],
            ring_pos: 0,
            hop: 48,
            hop_count: 0,
            scratch: vec![0.0; RING],
            diff: vec![0.0; RING],
            min_lag: 2,
            max_lag: 100,
            period: sr / 200.0,
            voiced: false,
            clarity: 0.0,
            octave_hold: 0,
            pending: 0.0,
        };
        d.set_range(80.0, 900.0);
        d
    }

    pub fn set_range(&mut self, min_hz: f32, max_hz: f32) {
        let sr_dec = self.sr / self.dec as f32;
        self.max_lag = ((sr_dec / min_hz).ceil() as usize).clamp(4, RING / 2 - 2);
        self.min_lag = ((sr_dec / max_hz).floor() as usize).clamp(2, self.max_lag - 2);
        // Analyse roughly every 4 ms.
        self.hop = ((sr_dec * 0.004) as usize).max(8);
    }

    pub fn reset(&mut self) {
        self.ring.iter_mut().for_each(|x| *x = 0.0);
        self.lp1.reset();
        self.lp2.reset();
        self.voiced = false;
        self.clarity = 0.0;
    }

    #[inline]
    pub fn push(&mut self, x: f32) {
        let y = self.lp2.process(self.lp1.process(x));
        self.dec_count += 1;
        if self.dec_count < self.dec {
            return;
        }
        self.dec_count = 0;
        self.ring_pos = (self.ring_pos + 1) % RING;
        self.ring[self.ring_pos] = y;
        self.hop_count += 1;
        if self.hop_count >= self.hop {
            self.hop_count = 0;
            self.analyse();
        }
    }

    fn analyse(&mut self) {
        let max_lag = self.max_lag;
        let w = max_lag; // integration window
        let total = w + max_lag;
        // Linearise the most recent `total` samples.
        let start = (self.ring_pos + RING + 1 - total) % RING;
        let mut energy = 0.0;
        for i in 0..total {
            let v = self.ring[(start + i) % RING];
            self.scratch[i] = v;
            energy += v * v;
        }
        let rms = (energy / total as f32).sqrt();
        if rms < 0.0015 {
            self.voiced = false;
            self.clarity = 0.0;
            return;
        }

        let x = &self.scratch[..total];
        self.diff[0] = 1.0;
        let mut running = 0.0;
        for tau in 1..=max_lag {
            let mut s = 0.0;
            for j in 0..w {
                let d = x[j] - x[j + tau];
                s += d * d;
            }
            running += s;
            self.diff[tau] = if running > 0.0 { s * tau as f32 / running } else { 1.0 };
        }

        // First dip below threshold (and its local minimum), else the global minimum.
        const THRESH: f32 = 0.15;
        let mut best = 0usize;
        let mut tau = self.min_lag;
        while tau < max_lag {
            if self.diff[tau] < THRESH {
                while tau + 1 < max_lag && self.diff[tau + 1] < self.diff[tau] {
                    tau += 1;
                }
                best = tau;
                break;
            }
            tau += 1;
        }
        if best == 0 {
            let mut m = f32::MAX;
            for t in self.min_lag..max_lag {
                if self.diff[t] < m {
                    m = self.diff[t];
                    best = t;
                }
            }
        }
        let cmnd = self.diff[best];
        // Parabolic interpolation.
        let mut refined = best as f32;
        if best > 1 && best < max_lag {
            let (a, b, c) = (self.diff[best - 1], self.diff[best], self.diff[best + 1]);
            let den = a - 2.0 * b + c;
            if den.abs() > 1e-9 {
                refined += (0.5 * (a - c) / den).clamp(-1.0, 1.0);
            }
        }
        self.clarity = (1.0 - cmnd).clamp(0.0, 1.0);
        // Voicing with hysteresis: harder to become voiced than to stay voiced. This stops
        // breathy/noisy or polyphonic passages from flickering between states.
        let enter = cmnd < 0.15;
        let stay = cmnd < 0.25;
        let p = refined * self.dec as f32;
        let now_voiced = if self.voiced { stay } else { enter };
        if !now_voiced {
            self.voiced = false;
            self.octave_hold = 0;
            self.pending = 0.0;
            return;
        }
        if self.voiced {
            let r = p / self.period;
            // Octave-error guard: a sudden jump of ~2x or ~0.5x must persist for a few frames.
            let octave_jump = (r - 2.0).abs() < 0.12 || (r - 0.5).abs() < 0.06 || (r - 3.0).abs() < 0.15;
            if octave_jump && self.octave_hold < 3 {
                self.octave_hold += 1;
                return;
            }
            self.octave_hold = 0;
            if (r - 1.0).abs() < 0.12 {
                self.period += (p - self.period) * 0.6;
            } else {
                self.period = p;
            }
        } else {
            // Onset: require two consecutive frames that agree before trusting a new pitch.
            // Removes the short wrong-octave blips at the start of words/notes.
            if self.pending <= 0.0 || (p / self.pending - 1.0).abs() > 0.1 {
                self.pending = p;
                return;
            }
            self.pending = 0.0;
            self.period = p;
        }
        self.voiced = true;
    }

    pub fn freq(&self) -> f32 {
        self.sr / self.period
    }
}
