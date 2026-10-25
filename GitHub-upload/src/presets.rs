//! Factory presets. Applied from the GUI through the host's parameter setter so they are
//! recorded as normal parameter changes (undo/automation friendly).

use crate::params::{DetectRange, FxKind, MidiMode, MutaformParams, NUM_FX_SLOTS};
use nih_plug::prelude::*;

#[derive(Clone, Copy)]
pub struct FxPreset {
    pub kind: FxKind,
    pub a: f32,
    pub b: f32,
    pub mix: f32,
}

const FX_OFF: FxPreset = FxPreset { kind: FxKind::Off, a: 0.5, b: 0.5, mix: 0.5 };

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Init,
    Vocal,
    Character,
    Fx,
    Midi,
}

impl Category {
    pub fn label(self) -> &'static str {
        match self {
            Category::Init => "",
            Category::Vocal => "VOCAL CHAINS",
            Category::Character => "CHARACTER",
            Category::Fx => "SOUND DESIGN",
            Category::Midi => "MIDI",
        }
    }
}

#[derive(Clone, Copy)]
pub struct Preset {
    pub name: &'static str,
    pub category: Category,
    pub pitch: f32,
    pub formant: f32,
    pub harmonics: f32,
    pub ratio: f32,
    pub fm: f32,
    pub snap: bool,
    pub alternator: f32,
    pub octave: f32,
    pub glide: f32,
    pub smear: f32,
    pub stereo: f32,
    pub detune: f32,
    pub dry_wet: f32,
    pub midi_mode: MidiMode,
    pub detect: DetectRange,
    pub key_lock: bool,
    pub fx: [FxPreset; NUM_FX_SLOTS],
}

const INIT: Preset = Preset {
    name: "Init",
    category: Category::Init,
    pitch: 0.0,
    formant: 0.0,
    harmonics: 0.0,
    ratio: 1.0,
    fm: 0.0,
    snap: false,
    alternator: 0.0,
    octave: 0.0,
    glide: 0.0,
    smear: 0.0,
    stereo: 0.0,
    detune: 0.0,
    dry_wet: 1.0,
    midi_mode: MidiMode::Off,
    detect: DetectRange::Mid,
    key_lock: false,
    fx: [FX_OFF; NUM_FX_SLOTS],
};

const fn fx(kind: FxKind, a: f32, b: f32, mix: f32) -> FxPreset {
    FxPreset { kind, a, b, mix }
}

// Handy building blocks. Knob values are 0..1 (see fx.rs for the mappings).
const fn hpf(a: f32) -> FxPreset {
    fx(FxKind::HighPass, a, 0.1, 1.0) // a=0.25 ~ 110 Hz
}
const fn lpf(a: f32) -> FxPreset {
    fx(FxKind::LowPass, a, 0.15, 1.0)
}
const fn comp(amount: f32, speed: f32) -> FxPreset {
    fx(FxKind::Compressor, amount, speed, 1.0)
}
const fn eq(presence: f32, air: f32) -> FxPreset {
    fx(FxKind::Eq, presence, air, 1.0) // 0.5 = flat
}
const fn deess(amount: f32) -> FxPreset {
    fx(FxKind::DeEsser, amount, 0.33, 1.0) // ~6 kHz
}
// Delay times are tempo synced: 0.455 = 1/8, 0.636 = 1/8D, 0.727 = 1/4, 0.818 = 1/4D, 0.909 = 1/2.
const fn delay(time: f32, feedback: f32, mix: f32) -> FxPreset {
    fx(FxKind::Delay, time, feedback, mix)
}
const fn verb(size: f32, damp: f32, mix: f32) -> FxPreset {
    fx(FxKind::Reverb, size, damp, mix)
}

const CLEAN_POP: [FxPreset; NUM_FX_SLOTS] =
    [hpf(0.25), comp(0.4, 0.5), eq(0.65, 0.7), deess(0.5), delay(0.455, 0.3, 0.12), verb(0.55, 0.5, 0.18)];

pub const PRESETS: &[Preset] = &[
    INIT,
    // ---------------------------------------------------------------- vocal chains (no pitch change)
    Preset { name: "Clean Pop Vocal", category: Category::Vocal, fx: CLEAN_POP, ..INIT },
    Preset {
        name: "Radio Ready",
        category: Category::Vocal,
        fx: [hpf(0.26), comp(0.65, 0.65), eq(0.75, 0.72), deess(0.6), fx(FxKind::Drive, 0.15, 0.7, 0.3), verb(0.35, 0.6, 0.1)],
        ..INIT
    },
    Preset {
        name: "Big Hall Ballad",
        category: Category::Vocal,
        fx: [hpf(0.25), comp(0.45, 0.4), eq(0.62, 0.7), deess(0.5), delay(0.727, 0.4, 0.2), verb(0.9, 0.35, 0.35)],
        ..INIT
    },
    Preset {
        name: "Trap Lead",
        category: Category::Vocal,
        detune: 0.25,
        stereo: 0.25,
        fx: [hpf(0.27), comp(0.6, 0.7), eq(0.7, 0.75), deess(0.55), delay(0.727, 0.35, 0.18), verb(0.6, 0.5, 0.16)],
        ..INIT
    },
    Preset {
        name: "Intimate & Dry",
        category: Category::Vocal,
        fx: [hpf(0.23), comp(0.5, 0.45), eq(0.58, 0.62), deess(0.45), FX_OFF, verb(0.25, 0.6, 0.08)],
        ..INIT
    },
    Preset {
        name: "Wide Hook",
        category: Category::Vocal,
        detune: 0.45,
        stereo: 0.6,
        fx: [
            hpf(0.26),
            comp(0.5, 0.5),
            eq(0.65, 0.72),
            fx(FxKind::Chorus, 0.25, 0.4, 0.25),
            delay(0.636, 0.3, 0.15),
            verb(0.7, 0.45, 0.25),
        ],
        ..INIT
    },
    Preset {
        name: "Double Tracked",
        category: Category::Vocal,
        detune: 0.55,
        stereo: 0.35,
        fx: [hpf(0.25), comp(0.5, 0.5), eq(0.62, 0.68), deess(0.5), FX_OFF, verb(0.45, 0.5, 0.12)],
        ..INIT
    },
    Preset {
        name: "Echo Throw",
        category: Category::Vocal,
        fx: [hpf(0.25), comp(0.45, 0.5), eq(0.65, 0.7), deess(0.5), delay(0.727, 0.55, 0.3), verb(0.65, 0.5, 0.2)],
        ..INIT
    },
    Preset {
        name: "Whisper Air",
        category: Category::Vocal,
        fx: [hpf(0.3), comp(0.55, 0.6), eq(0.55, 0.85), deess(0.65), FX_OFF, verb(0.6, 0.4, 0.22)],
        ..INIT
    },
    Preset {
        name: "Ambient Wash",
        category: Category::Vocal,
        smear: 0.35,
        stereo: 0.5,
        fx: [hpf(0.3), comp(0.4, 0.4), eq(0.6, 0.75), delay(0.818, 0.6, 0.3), verb(0.95, 0.3, 0.5), FX_OFF],
        ..INIT
    },
    Preset {
        name: "Lo-Fi Bedroom",
        category: Category::Vocal,
        fx: [
            fx(FxKind::HighPass, 0.39, 0.2, 1.0),
            lpf(0.767),
            comp(0.55, 0.4),
            fx(FxKind::Crush, 0.45, 0.1, 0.35),
            fx(FxKind::Chorus, 0.2, 0.5, 0.3),
            verb(0.5, 0.6, 0.25),
        ],
        ..INIT
    },
    Preset {
        name: "Telephone",
        category: Category::Vocal,
        formant: 1.0,
        fx: [
            fx(FxKind::HighPass, 0.466, 0.3, 1.0),
            fx(FxKind::LowPass, 0.725, 0.3, 1.0),
            comp(0.7, 0.6),
            fx(FxKind::Drive, 0.45, 0.5, 0.6),
            FX_OFF,
            verb(0.2, 0.7, 0.08),
        ],
        ..INIT
    },
    Preset { name: "Key Locked Pop", category: Category::Vocal, key_lock: true, fx: CLEAN_POP, ..INIT },
    // ---------------------------------------------------------------- character (pitch / formant)
    Preset {
        name: "Deep Ogre",
        category: Category::Character,
        pitch: -12.0,
        formant: -6.0,
        fx: [hpf(0.15), comp(0.4, 0.5), FX_OFF, FX_OFF, FX_OFF, verb(0.4, 0.6, 0.12)],
        ..INIT
    },
    Preset { name: "Helium", category: Category::Character, pitch: 12.0, formant: 7.0, ..INIT },
    Preset {
        name: "Gender Flip",
        category: Category::Character,
        pitch: 5.0,
        formant: 3.5,
        fx: [hpf(0.25), comp(0.4, 0.5), eq(0.6, 0.65), FX_OFF, FX_OFF, verb(0.45, 0.5, 0.12)],
        ..INIT
    },
    Preset {
        name: "Big Guy",
        category: Category::Character,
        pitch: -5.0,
        formant: -3.0,
        fx: [hpf(0.18), comp(0.4, 0.5), FX_OFF, FX_OFF, FX_OFF, verb(0.45, 0.5, 0.12)],
        ..INIT
    },
    Preset {
        name: "Chipmunk Chop",
        category: Category::Character,
        pitch: 12.0,
        formant: 4.0,
        fx: [hpf(0.3), comp(0.5, 0.6), eq(0.6, 0.6), FX_OFF, delay(0.455, 0.25, 0.15), verb(0.4, 0.5, 0.12)],
        ..INIT
    },
    Preset {
        name: "Octave Down Ad-lib",
        category: Category::Character,
        pitch: -12.0,
        formant: -3.0,
        dry_wet: 0.5,
        fx: [hpf(0.15), comp(0.5, 0.5), FX_OFF, FX_OFF, delay(0.727, 0.3, 0.15), verb(0.6, 0.5, 0.2)],
        ..INIT
    },
    Preset {
        name: "Octave Stack",
        category: Category::Character,
        octave: -0.6,
        detune: 0.3,
        stereo: 0.3,
        fx: [hpf(0.2), comp(0.45, 0.5), FX_OFF, FX_OFF, FX_OFF, verb(0.6, 0.5, 0.18)],
        ..INIT
    },
    // ---------------------------------------------------------------- sound design
    Preset {
        name: "Robot Choir",
        category: Category::Fx,
        harmonics: 0.5,
        snap: true,
        detune: 0.6,
        stereo: 0.5,
        fx: [fx(FxKind::Chorus, 0.3, 0.5, 0.4), verb(0.6, 0.4, 0.3), FX_OFF, FX_OFF, FX_OFF, FX_OFF],
        ..INIT
    },
    Preset {
        name: "Sub Alternator",
        category: Category::Fx,
        alternator: 0.8,
        formant: -2.0,
        fx: [lpf(0.75), FX_OFF, FX_OFF, FX_OFF, FX_OFF, FX_OFF],
        ..INIT
    },
    Preset { name: "Hollow Down", category: Category::Fx, alternator: -0.8, ..INIT },
    Preset {
        name: "FM Growl",
        category: Category::Fx,
        pitch: -12.0,
        fm: 0.6,
        ratio: 2.0,
        snap: true,
        fx: [fx(FxKind::Drive, 0.4, 0.6, 0.6), FX_OFF, FX_OFF, FX_OFF, FX_OFF, FX_OFF],
        ..INIT
    },
    Preset {
        name: "Metal Throat",
        category: Category::Fx,
        harmonics: 0.37,
        fm: 0.25,
        ratio: 1.5,
        fx: [hpf(0.25), verb(0.5, 0.5, 0.25), FX_OFF, FX_OFF, FX_OFF, FX_OFF],
        ..INIT
    },
    Preset {
        name: "Smear Cloud",
        category: Category::Fx,
        smear: 0.8,
        stereo: 0.9,
        detune: 0.5,
        fx: [delay(0.636, 0.45, 0.35), verb(0.85, 0.3, 0.5), FX_OFF, FX_OFF, FX_OFF, FX_OFF],
        ..INIT
    },
    Preset {
        name: "Wobble Monster",
        category: Category::Fx,
        pitch: -7.0,
        formant: -4.0,
        harmonics: -0.25,
        snap: true,
        fx: [fx(FxKind::Tremolo, 0.35, 0.7, 1.0), fx(FxKind::Drive, 0.5, 0.5, 0.5), FX_OFF, FX_OFF, FX_OFF, FX_OFF],
        ..INIT
    },
    // ---------------------------------------------------------------- MIDI
    Preset {
        name: "MIDI Mono Lead",
        category: Category::Midi,
        midi_mode: MidiMode::Mono,
        glide: 80.0,
        stereo: 0.2,
        fx: [hpf(0.25), comp(0.45, 0.5), FX_OFF, FX_OFF, delay(0.455, 0.3, 0.12), verb(0.5, 0.5, 0.15)],
        ..INIT
    },
    Preset {
        name: "MIDI Poly Harmonizer",
        category: Category::Midi,
        midi_mode: MidiMode::Poly,
        detune: 0.25,
        stereo: 0.4,
        dry_wet: 0.6,
        fx: [verb(0.6, 0.5, 0.3), FX_OFF, FX_OFF, FX_OFF, FX_OFF, FX_OFF],
        ..INIT
    },
];

pub fn apply(p: &Preset, params: &MutaformParams, setter: &ParamSetter) {
    fn set<P: Param>(setter: &ParamSetter, param: &P, v: P::Plain) {
        setter.begin_set_parameter(param);
        setter.set_parameter(param, v);
        setter.end_set_parameter(param);
    }
    set(setter, &params.pitch, p.pitch);
    set(setter, &params.formant, p.formant);
    set(setter, &params.harmonics, p.harmonics);
    set(setter, &params.ratio, p.ratio);
    set(setter, &params.fm, p.fm);
    set(setter, &params.snap, p.snap);
    set(setter, &params.alternator, p.alternator);
    set(setter, &params.octave, p.octave);
    set(setter, &params.glide, p.glide);
    set(setter, &params.smear, p.smear);
    set(setter, &params.stereo, p.stereo);
    set(setter, &params.detune, p.detune);
    set(setter, &params.dry_wet, p.dry_wet);
    set(setter, &params.midi_mode, p.midi_mode);
    set(setter, &params.detect, p.detect);
    set(setter, &params.key_lock, p.key_lock);
    for (slot, f) in params.fx.iter().zip(p.fx.iter()) {
        set(setter, &slot.kind, f.kind);
        set(setter, &slot.a, f.a);
        set(setter, &slot.b, f.b);
        set(setter, &slot.mix, f.mix);
    }
}

// ------------------------------------------------------------------------------------------------
// User presets (JSON files with every parameter's normalized value, keyed by parameter ID)

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Serialize, Deserialize)]
struct UserPresetFile {
    name: String,
    plugin: String,
    version: u32,
    params: BTreeMap<String, f32>,
}

pub struct UserPreset {
    pub name: String,
    pub path: PathBuf,
}

/// Where user presets are stored:
/// - Windows: %APPDATA%\\Mutaform\\Presets
/// - macOS: ~/Library/Application Support/Mutaform/Presets
/// - Linux: ~/.config/Mutaform/Presets
pub fn user_dir() -> PathBuf {
    #[cfg(target_os = "macos")]
    let base = std::env::var_os("HOME")
        .map(|h| PathBuf::from(h).join("Library").join("Application Support"))
        .unwrap_or_else(|| PathBuf::from("."));
    #[cfg(not(target_os = "macos"))]
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("Mutaform").join("Presets")
}

pub fn list_user() -> Vec<UserPreset> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(user_dir()) {
        for e in rd.flatten() {
            let path = e.path();
            if path.extension().map(|x| x == "json").unwrap_or(false) {
                let name = std::fs::read_to_string(&path)
                    .ok()
                    .and_then(|s| serde_json::from_str::<UserPresetFile>(&s).ok())
                    .map(|f| f.name)
                    .unwrap_or_else(|| path.file_stem().unwrap_or_default().to_string_lossy().to_string());
                out.push(UserPreset { name, path });
            }
        }
    }
    out.sort_by_key(|p| p.name.to_lowercase());
    out
}

fn file_name_for(name: &str) -> String {
    let clean: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' { c } else { '_' })
        .collect();
    let clean = clean.trim().to_string();
    if clean.is_empty() {
        "Preset".into()
    } else {
        clean
    }
}

/// Save every parameter (except bypass) under `name`. Overwrites a preset with the same name.
pub fn save_user(name: &str, params: &MutaformParams) -> Result<PathBuf, String> {
    let dir = user_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut map = BTreeMap::new();
    for (id, ptr, _) in params.param_map() {
        if id == "bypass" {
            continue;
        }
        map.insert(id, unsafe { ptr.unmodulated_normalized_value() });
    }
    let file = UserPresetFile { name: name.trim().to_string(), plugin: "Mutaform".into(), version: 2, params: map };
    let path = dir.join(format!("{}.json", file_name_for(name)));
    let json = serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Load a user preset through the host's parameter setter. Returns the preset's name.
pub fn load_user(path: &std::path::Path, params: &MutaformParams, setter: &ParamSetter) -> Result<String, String> {
    let s = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let file: UserPresetFile = serde_json::from_str(&s).map_err(|e| e.to_string())?;
    for (id, ptr, _) in params.param_map() {
        if let Some(v) = file.params.get(&id) {
            unsafe {
                setter.raw_context.raw_begin_set_parameter(ptr);
                setter.raw_context.raw_set_parameter_normalized(ptr, v.clamp(0.0, 1.0));
                setter.raw_context.raw_end_set_parameter(ptr);
            }
        }
    }
    Ok(file.name)
}

pub fn delete_user(path: &std::path::Path) -> Result<(), String> {
    std::fs::remove_file(path).map_err(|e| e.to_string())
}
