use nih_plug::prelude::*;
use nih_plug_egui::EguiState;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

pub const NUM_FX_SLOTS: usize = 6;

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum MidiMode {
    #[id = "off"]
    #[name = "Off"]
    Off,
    #[id = "mono"]
    #[name = "Mono"]
    Mono,
    #[id = "poly"]
    #[name = "Poly"]
    Poly,
}

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetectRange {
    #[id = "low"]
    #[name = "Low"]
    Low,
    #[id = "mid"]
    #[name = "Mid"]
    Mid,
    #[id = "high"]
    #[name = "High"]
    High,
}

impl DetectRange {
    /// (min Hz, max Hz) searched by the pitch detector.
    pub fn range_hz(self) -> (f32, f32) {
        match self {
            DetectRange::Low => (55.0, 450.0),
            DetectRange::Mid => (80.0, 900.0),
            DetectRange::High => (150.0, 1500.0),
        }
    }
}


#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    #[id = "c"] #[name = "C"] C,
    #[id = "cs"] #[name = "C#"] Cs,
    #[id = "d"] #[name = "D"] D,
    #[id = "ds"] #[name = "D#"] Ds,
    #[id = "e"] #[name = "E"] E,
    #[id = "f"] #[name = "F"] F,
    #[id = "fs"] #[name = "F#"] Fs,
    #[id = "g"] #[name = "G"] G,
    #[id = "gs"] #[name = "G#"] Gs,
    #[id = "a"] #[name = "A"] A,
    #[id = "as"] #[name = "A#"] As,
    #[id = "b"] #[name = "B"] B,
}

impl Key {
    pub const ALL: [Key; 12] =
        [Key::C, Key::Cs, Key::D, Key::Ds, Key::E, Key::F, Key::Fs, Key::G, Key::Gs, Key::A, Key::As, Key::B];
    pub fn semitone(self) -> u8 {
        Key::ALL.iter().position(|k| *k == self).unwrap_or(0) as u8
    }
    pub fn label(self) -> &'static str {
        ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"][self.semitone() as usize]
    }
}

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scale {
    #[id = "major"] #[name = "Major"] Major,
    #[id = "minor"] #[name = "Minor"] Minor,
    #[id = "harmminor"] #[name = "Harmonic Minor"] HarmonicMinor,
    #[id = "dorian"] #[name = "Dorian"] Dorian,
    #[id = "mixolydian"] #[name = "Mixolydian"] Mixolydian,
    #[id = "pentmaj"] #[name = "Pentatonic Major"] PentatonicMajor,
    #[id = "pentmin"] #[name = "Pentatonic Minor"] PentatonicMinor,
    #[id = "blues"] #[name = "Blues"] Blues,
    #[id = "chromatic"] #[name = "Chromatic"] Chromatic,
}

impl Scale {
    pub const ALL: [Scale; 9] = [
        Scale::Major,
        Scale::Minor,
        Scale::HarmonicMinor,
        Scale::Dorian,
        Scale::Mixolydian,
        Scale::PentatonicMajor,
        Scale::PentatonicMinor,
        Scale::Blues,
        Scale::Chromatic,
    ];
    /// Bit i set = semitone i above the root is in the scale.
    pub fn mask(self) -> u16 {
        let notes: &[u8] = match self {
            Scale::Major => &[0, 2, 4, 5, 7, 9, 11],
            Scale::Minor => &[0, 2, 3, 5, 7, 8, 10],
            Scale::HarmonicMinor => &[0, 2, 3, 5, 7, 8, 11],
            Scale::Dorian => &[0, 2, 3, 5, 7, 9, 10],
            Scale::Mixolydian => &[0, 2, 4, 5, 7, 9, 10],
            Scale::PentatonicMajor => &[0, 2, 4, 7, 9],
            Scale::PentatonicMinor => &[0, 3, 5, 7, 10],
            Scale::Blues => &[0, 3, 5, 6, 7, 10],
            Scale::Chromatic => &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
        };
        notes.iter().fold(0u16, |m, &n| m | (1 << n))
    }
    pub fn label(self) -> &'static str {
        match self {
            Scale::Major => "Major",
            Scale::Minor => "Minor",
            Scale::HarmonicMinor => "Harm. Minor",
            Scale::Dorian => "Dorian",
            Scale::Mixolydian => "Mixolydian",
            Scale::PentatonicMajor => "Penta Major",
            Scale::PentatonicMinor => "Penta Minor",
            Scale::Blues => "Blues",
            Scale::Chromatic => "Chromatic",
        }
    }
}

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum FxKind {
    #[id = "off"]
    #[name = "Off"]
    Off,
    #[id = "lowpass"]
    #[name = "Low Pass"]
    LowPass,
    #[id = "highpass"]
    #[name = "High Pass"]
    HighPass,
    #[id = "drive"]
    #[name = "Drive"]
    Drive,
    #[id = "chorus"]
    #[name = "Chorus"]
    Chorus,
    #[id = "delay"]
    #[name = "Delay"]
    Delay,
    #[id = "reverb"]
    #[name = "Reverb"]
    Reverb,
    #[id = "crush"]
    #[name = "Crush"]
    Crush,
    #[id = "tremolo"]
    #[name = "Tremolo"]
    Tremolo,
    #[id = "compressor"]
    #[name = "Compressor"]
    Compressor,
    #[id = "eq"]
    #[name = "Vocal EQ"]
    Eq,
    #[id = "deesser"]
    #[name = "De-Esser"]
    DeEsser,
}

/// Tempo-synced delay divisions (in quarter notes) and their labels.
pub const DELAY_DIVS: [(f32, &str); 12] = [
    (0.125, "1/32"),
    (1.0 / 6.0, "1/16T"),
    (0.25, "1/16"),
    (0.375, "1/16D"),
    (1.0 / 3.0, "1/8T"),
    (0.5, "1/8"),
    (0.75, "1/8D"),
    (2.0 / 3.0, "1/4T"),
    (1.0, "1/4"),
    (1.5, "1/4D"),
    (2.0, "1/2"),
    (4.0, "1/1"),
];

/// Pick a delay division from the 0..1 knob value.
pub fn delay_div(a: f32) -> (f32, &'static str) {
    // Sort by length so turning the knob always gets longer.
    const ORDER: [usize; 12] = [0, 1, 2, 4, 3, 5, 7, 6, 8, 9, 10, 11];
    let i = ((a.clamp(0.0, 1.0) * 11.0).round() as usize).min(11);
    DELAY_DIVS[ORDER[i]]
}

impl FxKind {
    pub const ALL: [FxKind; 12] = [
        FxKind::Off,
        FxKind::Compressor,
        FxKind::Eq,
        FxKind::DeEsser,
        FxKind::LowPass,
        FxKind::HighPass,
        FxKind::Drive,
        FxKind::Chorus,
        FxKind::Delay,
        FxKind::Reverb,
        FxKind::Crush,
        FxKind::Tremolo,
    ];

    pub fn label(self) -> &'static str {
        match self {
            FxKind::Off => "OFF",
            FxKind::LowPass => "LOW PASS",
            FxKind::HighPass => "HIGH PASS",
            FxKind::Drive => "DRIVE",
            FxKind::Chorus => "CHORUS",
            FxKind::Delay => "DELAY",
            FxKind::Reverb => "REVERB",
            FxKind::Crush => "CRUSH",
            FxKind::Tremolo => "TREMOLO",
            FxKind::Compressor => "COMP",
            FxKind::Eq => "VOCAL EQ",
            FxKind::DeEsser => "DE-ESS",
        }
    }

    /// Labels for the A and B knobs.
    pub fn knob_labels(self) -> (&'static str, &'static str) {
        match self {
            FxKind::Off => ("A", "B"),
            FxKind::LowPass | FxKind::HighPass => ("CUTOFF", "RESO"),
            FxKind::Drive => ("DRIVE", "TONE"),
            FxKind::Chorus => ("RATE", "DEPTH"),
            FxKind::Delay => ("TIME", "FEEDBK"),
            FxKind::Reverb => ("SIZE", "DAMP"),
            FxKind::Crush => ("BITS", "RATE"),
            FxKind::Tremolo => ("RATE", "DEPTH"),
            FxKind::Compressor => ("AMOUNT", "SPEED"),
            FxKind::Eq => ("PRES", "AIR"),
            FxKind::DeEsser => ("AMOUNT", "FREQ"),
        }
    }
}

#[derive(Params)]
pub struct FxSlotParams {
    #[id = "fxkind"]
    pub kind: EnumParam<FxKind>,
    #[id = "fxa"]
    pub a: FloatParam,
    #[id = "fxb"]
    pub b: FloatParam,
    #[id = "fxmix"]
    pub mix: FloatParam,
}

impl FxSlotParams {
    fn new(index: usize) -> Self {
        let n = index + 1;
        Self {
            kind: EnumParam::new(format!("FX{n} Type"), FxKind::Off),
            a: FloatParam::new(format!("FX{n} A"), 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_smoother(SmoothingStyle::Linear(30.0))
                .with_unit("%")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),
            b: FloatParam::new(format!("FX{n} B"), 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_smoother(SmoothingStyle::Linear(30.0))
                .with_unit("%")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),
            mix: FloatParam::new(format!("FX{n} Mix"), 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_smoother(SmoothingStyle::Linear(30.0))
                .with_unit("%")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),
        }
    }
}

#[derive(Params)]
pub struct MutaformParams {
    #[persist = "editor-state"]
    pub editor_state: Arc<EguiState>,

    /// Index of the last loaded factory preset (only used by the GUI).
    #[persist = "preset-index"]
    pub preset_index: Arc<AtomicUsize>,

    /// Name of the last loaded preset (factory or user), shown in the GUI.
    #[persist = "preset-name"]
    pub preset_name: Arc<std::sync::RwLock<String>>,

    /// GUI colour theme (index into `editor::THEMES`).
    #[persist = "theme"]
    pub theme: Arc<AtomicUsize>,

    #[id = "bypass"]
    pub bypass: BoolParam,
    #[id = "pitch"]
    pub pitch: FloatParam,
    #[id = "formant"]
    pub formant: FloatParam,
    #[id = "harmonics"]
    pub harmonics: FloatParam,
    #[id = "ratio"]
    pub ratio: FloatParam,
    #[id = "fm"]
    pub fm: FloatParam,
    #[id = "snap"]
    pub snap: BoolParam,
    #[id = "alternator"]
    pub alternator: FloatParam,
    #[id = "octave"]
    pub octave: FloatParam,
    #[id = "glide"]
    pub glide: FloatParam,
    #[id = "smear"]
    pub smear: FloatParam,
    #[id = "stereo"]
    pub stereo: FloatParam,
    #[id = "detune"]
    pub detune: FloatParam,
    #[id = "drywet"]
    pub dry_wet: FloatParam,
    #[id = "output"]
    pub output: FloatParam,
    #[id = "midimode"]
    pub midi_mode: EnumParam<MidiMode>,
    #[id = "detect"]
    pub detect: EnumParam<DetectRange>,
    #[id = "keylock"]
    pub key_lock: BoolParam,
    #[id = "key"]
    pub key: EnumParam<Key>,
    #[id = "scale"]
    pub scale: EnumParam<Scale>,

    #[nested(array, group = "Effects")]
    pub fx: [FxSlotParams; NUM_FX_SLOTS],
}

fn bipolar(name: &str, decimals: usize) -> FloatParam {
    FloatParam::new(name, 0.0, FloatRange::Linear { min: -1.0, max: 1.0 })
        .with_smoother(SmoothingStyle::Linear(30.0))
        .with_value_to_string(formatters::v2s_f32_rounded(decimals))
}

fn percent(name: &str) -> FloatParam {
    FloatParam::new(name, 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
        .with_smoother(SmoothingStyle::Linear(30.0))
        .with_unit("%")
        .with_value_to_string(formatters::v2s_f32_percentage(0))
        .with_string_to_value(formatters::s2v_f32_percentage())
}

fn unipolar(name: &str, default: f32) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Linear { min: 0.0, max: 1.0 })
        .with_smoother(SmoothingStyle::Linear(30.0))
        .with_value_to_string(formatters::v2s_f32_rounded(2))
}

impl Default for MutaformParams {
    fn default() -> Self {
        Self {
            editor_state: EguiState::from_size(crate::editor::WIDTH, crate::editor::HEIGHT),
            preset_index: Arc::new(AtomicUsize::new(0)),
            theme: Arc::new(AtomicUsize::new(0)),
            preset_name: Arc::new(std::sync::RwLock::new(String::new())),

            bypass: BoolParam::new("Bypass", false).make_bypass(),
            pitch: FloatParam::new("Pitch", 0.0, FloatRange::Linear { min: -24.0, max: 24.0 })
                .with_smoother(SmoothingStyle::Linear(20.0))
                .with_unit(" st")
                .with_value_to_string(formatters::v2s_f32_rounded(2)),
            formant: FloatParam::new("Formant", 0.0, FloatRange::Linear { min: -12.0, max: 12.0 })
                .with_smoother(SmoothingStyle::Linear(20.0))
                .with_unit(" st")
                .with_value_to_string(formatters::v2s_f32_rounded(2)),
            harmonics: bipolar("Harmonics", 3),
            ratio: FloatParam::new(
                "Ratio",
                1.0,
                FloatRange::Skewed { min: 0.25, max: 4.0, factor: FloatRange::skew_factor(-1.5) },
            )
            .with_smoother(SmoothingStyle::Linear(30.0))
            .with_value_to_string(formatters::v2s_f32_rounded(3)),
            fm: unipolar("FM", 0.0),
            snap: BoolParam::new("Snap", false),
            alternator: bipolar("Alternator", 2),
            octave: bipolar("Octave", 3),
            glide: FloatParam::new(
                "Glide",
                0.0,
                FloatRange::Skewed { min: 0.0, max: 1000.0, factor: FloatRange::skew_factor(-1.5) },
            )
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(0)),
            smear: percent("Smear"),
            stereo: percent("Stereo"),
            detune: percent("Detune"),
            dry_wet: FloatParam::new("Dry/Wet", 1.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_smoother(SmoothingStyle::Linear(30.0))
                .with_unit("%")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),
            output: FloatParam::new(
                "Output",
                util::db_to_gain(0.0),
                FloatRange::Skewed {
                    min: util::db_to_gain(-24.0),
                    max: util::db_to_gain(12.0),
                    factor: FloatRange::gain_skew_factor(-24.0, 12.0),
                },
            )
            .with_smoother(SmoothingStyle::Logarithmic(30.0))
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_gain_to_db(1))
            .with_string_to_value(formatters::s2v_f32_gain_to_db()),
            midi_mode: EnumParam::new("MIDI Mode", MidiMode::Off),
            detect: EnumParam::new("Detect", DetectRange::Mid),
            key_lock: BoolParam::new("Key Lock", false),
            key: EnumParam::new("Key", Key::C),
            scale: EnumParam::new("Scale", Scale::Major),

            fx: std::array::from_fn(FxSlotParams::new),
        }
    }
}
