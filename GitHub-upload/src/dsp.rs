//! Small DSP building blocks shared by the engine and the effects.

use std::f32::consts::PI;

/// Tiny xorshift RNG (allocation free, deterministic).
#[derive(Clone, Copy)]
pub struct Rng(u32);

impl Rng {
    pub fn new(seed: u32) -> Self {
        Self(seed.max(1))
    }
    #[inline]
    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }
    /// Uniform in [0, 1).
    #[inline]
    pub fn uni(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / 16_777_216.0
    }
    /// Uniform in [-1, 1).
    #[inline]
    pub fn bi(&mut self) -> f32 {
        self.uni() * 2.0 - 1.0
    }
}

/// RBJ biquad, transposed direct form II.
#[derive(Clone, Copy, Default)]
pub struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Biquad {
    pub fn lowpass(sr: f32, fc: f32, q: f32) -> Self {
        let w = 2.0 * PI * (fc / sr).min(0.49);
        let (s, c) = w.sin_cos();
        let alpha = s / (2.0 * q);
        let a0 = 1.0 + alpha;
        Self {
            b0: (1.0 - c) / 2.0 / a0,
            b1: (1.0 - c) / a0,
            b2: (1.0 - c) / 2.0 / a0,
            a1: -2.0 * c / a0,
            a2: (1.0 - alpha) / a0,
            z1: 0.0,
            z2: 0.0,
        }
    }
    pub fn highpass(sr: f32, fc: f32, q: f32) -> Self {
        let w = 2.0 * PI * (fc / sr).min(0.49);
        let (s, c) = w.sin_cos();
        let alpha = s / (2.0 * q);
        let a0 = 1.0 + alpha;
        Self {
            b0: (1.0 + c) / 2.0 / a0,
            b1: -(1.0 + c) / a0,
            b2: (1.0 + c) / 2.0 / a0,
            a1: -2.0 * c / a0,
            a2: (1.0 - alpha) / a0,
            z1: 0.0,
            z2: 0.0,
        }
    }
    fn set_coefs(&mut self, b0: f32, b1: f32, b2: f32, a0: f32, a1: f32, a2: f32) {
        self.b0 = b0 / a0;
        self.b1 = b1 / a0;
        self.b2 = b2 / a0;
        self.a1 = a1 / a0;
        self.a2 = a2 / a0;
    }

    /// RBJ peaking EQ (keeps state).
    pub fn set_peak(&mut self, sr: f32, fc: f32, q: f32, gain_db: f32) {
        let a = 10f32.powf(gain_db / 40.0);
        let w = 2.0 * PI * (fc / sr).min(0.49);
        let (s, c) = w.sin_cos();
        let alpha = s / (2.0 * q);
        self.set_coefs(1.0 + alpha * a, -2.0 * c, 1.0 - alpha * a, 1.0 + alpha / a, -2.0 * c, 1.0 - alpha / a);
    }

    /// RBJ high shelf, slope 1 (keeps state).
    pub fn set_high_shelf(&mut self, sr: f32, fc: f32, gain_db: f32) {
        let a = 10f32.powf(gain_db / 40.0);
        let w = 2.0 * PI * (fc / sr).min(0.45);
        let (s, c) = w.sin_cos();
        let alpha = s / 2.0 * 2f32.sqrt();
        let sa = 2.0 * a.sqrt() * alpha;
        self.set_coefs(
            a * ((a + 1.0) + (a - 1.0) * c + sa),
            -2.0 * a * ((a - 1.0) + (a + 1.0) * c),
            a * ((a + 1.0) + (a - 1.0) * c - sa),
            (a + 1.0) - (a - 1.0) * c + sa,
            2.0 * ((a - 1.0) - (a + 1.0) * c),
            (a + 1.0) - (a - 1.0) * c - sa,
        );
    }

    /// Update to new low-pass coefficients while keeping the filter state.
    pub fn set_lowpass(&mut self, sr: f32, fc: f32, q: f32) {
        let n = Self::lowpass(sr, fc, q);
        self.b0 = n.b0;
        self.b1 = n.b1;
        self.b2 = n.b2;
        self.a1 = n.a1;
        self.a2 = n.a2;
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
    pub fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }
}

/// One section of a second order allpass used by the Hilbert transformer:
/// y[n] = a^2 (x[n] + y[n-2]) - x[n-2]
#[derive(Clone, Copy, Default)]
struct AllpassSection {
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl AllpassSection {
    fn new(a: f32) -> Self {
        Self { a2: a * a, ..Default::default() }
    }
    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let y = self.a2 * (x + self.y2) - self.x2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

/// IIR Hilbert transformer (two allpass chains with ~90 degree phase difference over most of
/// the audio band). Coefficients from Olli Niemitalo's public-domain design.
#[derive(Clone, Copy)]
pub struct Hilbert {
    a: [AllpassSection; 4],
    b: [AllpassSection; 4],
    a_delay: f32,
}

impl Hilbert {
    pub fn new() -> Self {
        const CA: [f32; 4] = [0.692_387_8, 0.936_065_43, 0.988_229_5, 0.998_748_84];
        const CB: [f32; 4] = [0.402_192_12, 0.856_171_1, 0.972_290_95, 0.995_288_5];
        Self {
            a: CA.map(AllpassSection::new),
            b: CB.map(AllpassSection::new),
            a_delay: 0.0,
        }
    }
    /// Returns (in-phase, quadrature).
    #[inline]
    pub fn process(&mut self, x: f32) -> (f32, f32) {
        let mut ya = x;
        for s in self.a.iter_mut() {
            ya = s.process(ya);
        }
        let mut yb = x;
        for s in self.b.iter_mut() {
            yb = s.process(yb);
        }
        let i = self.a_delay;
        self.a_delay = ya;
        (i, yb)
    }
}

/// Single sideband frequency shifter.
#[derive(Clone, Copy)]
pub struct FreqShifter {
    hilbert: Hilbert,
    phase: f32,
}

impl FreqShifter {
    pub fn new() -> Self {
        Self { hilbert: Hilbert::new(), phase: 0.0 }
    }
    /// `dphase` = 2*pi*shift_hz/sr. Positive shifts move all partials up.
    #[inline]
    pub fn process(&mut self, x: f32, dphase: f32) -> f32 {
        let (i, q) = self.hilbert.process(x);
        self.phase += dphase;
        if self.phase > PI {
            self.phase -= 2.0 * PI;
        } else if self.phase < -PI {
            self.phase += 2.0 * PI;
        }
        let (s, c) = self.phase.sin_cos();
        i * c - q * s
    }
}

/// Power-of-two ring buffer with fractional reads.
pub struct DelayLine {
    buf: Vec<f32>,
    mask: usize,
    pos: usize,
}

impl DelayLine {
    pub fn new(min_len: usize) -> Self {
        let len = min_len.max(4).next_power_of_two();
        Self { buf: vec![0.0; len], mask: len - 1, pos: 0 }
    }
    #[inline]
    pub fn push(&mut self, x: f32) {
        self.pos = (self.pos + 1) & self.mask;
        self.buf[self.pos] = x;
    }
    /// Read `delay` samples back (0 = the sample just pushed).
    #[inline]
    pub fn read(&self, delay: f32) -> f32 {
        let d = delay.clamp(0.0, (self.mask - 2) as f32);
        let di = d as usize;
        let frac = d - di as f32;
        let a = self.buf[(self.pos.wrapping_sub(di)) & self.mask];
        let b = self.buf[(self.pos.wrapping_sub(di + 1)) & self.mask];
        a + (b - a) * frac
    }
    #[inline]
    pub fn read_int(&self, delay: usize) -> f32 {
        self.buf[(self.pos.wrapping_sub(delay)) & self.mask]
    }
    pub fn clear(&mut self) {
        self.buf.iter_mut().for_each(|x| *x = 0.0);
    }
}

#[inline]
pub fn one_pole_coef(sr: f32, time_s: f32) -> f32 {
    if time_s <= 0.0 {
        1.0
    } else {
        1.0 - (-1.0 / (time_s * sr)).exp()
    }
}
