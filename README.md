# Mutaform: voice mangler (VST3 / CLAP for Windows)

Mutaform is an effect plugin for pitch, formant and harmonic manipulation of vocals and other monophonic sounds. It was written from scratch in Rust using [nih-plug](https://github.com/robbert-vdh/nih-plug). No code or graphics were taken from other products.

![Mutaform](screenshot.png)

## Installation

1. Right-click **Installer.bat** and choose **Run as administrator**.
   It copies `Mutaform.vst3` to `C:\Program Files\Common Files\VST3\` and `Mutaform.clap` to `C:\Program Files\Common Files\CLAP\`.
   You can also copy the files there yourself.
2. Open FL Studio and go to **Options → Manage plugins → Find more plugins**. Mutaform will show up under *Effects*.
3. Put Mutaform on a Mixer track, for example your vocal track.

## Controls

| Control | What it does |
|---|---|
| **PITCH** | Transposes ±24 semitones while keeping the formants (the character of the voice). |
| **FORMANT** | Shifts the formants ±12 semitones without changing the pitch. Turning it down sounds "big/dark"; turning it up sounds "small/bright". |
| **HARMONICS** | Shifts every overtone by a fraction of the fundamental, for metallic and robotic tones. |
| **RATIO** | Ratio between the FM modulator and the fundamental. |
| **FM** | Frequency modulation that follows the pitch: growl, buzz and bell-like tones. |
| **SNAP** | Locks Harmonics to steps of 0.25 and Ratio to whole ratios (1, 2, 3 … or ½, ⅓ …) so the result stays musical. |
| **ALTERNATOR** | Treats every other pitch period differently. Positive values give a buzzy sub-octave; negative values give a hollow octave down. |
| **OCTAVE** | Blends in an octave below (turn left) or above (turn right). |
| **SMEAR** | Randomly smears the grains for an airy, choir-like or cloudy sound. |
| **STEREO** | Spreads the grains across the stereo field. |
| **DETUNE** | Adds two voices detuned by up to ±40 cents and panned left and right. |
| **GLIDE** | Portamento between MIDI notes and when the pitch changes, 0–1000 ms. |
| **DETECT** | Frequency range used by the pitch detector: LOW (bass and deep voices), MID (normal vocals) or HIGH (high voices, whistle). |
| **MIDI** | OFF: the Pitch knob is in control. MONO/POLY: MIDI notes set the pitch (a vocoder-style harmonizer, up to 8 voices in POLY). |
| **KEY LOCK** (top bar) | Turn it ON and pick a key (C–B) and a scale (Major, Minor, Harm. Minor, Dorian, Mixolydian, Penta Major/Minor, Blues, Chromatic). Each note is pulled to the nearest note in the scale, while vibrato and natural movement are kept. The display on the right shows the note it is locking to ("LOCK → G3"). Key Lock works when MIDI is set to OFF. |
| **DRY/WET**, **OUTPUT** | Mix and output level. |
| **Power button** (top left) | Bypass with a smooth crossfade. This is the plugin's own bypass parameter, so automate it in FL Studio (right-click it → *Create automation clip*) instead of switching the mixer slot on and off. |
| **FX 1–4** | Four effect slots on the processed signal: Low Pass, High Pass, Drive, Chorus, Delay, Reverb, Crush and Tremolo. Click the name to choose an effect and the power button to switch the slot on or off. |

Mouse: drag up and down to adjust a knob. **Shift + drag** adjusts in finer steps, **double-click** resets to the default, and the **scroll wheel** works too.
Choose presets with the arrows at the top, or click the preset name to get a list.

## Controlling Mutaform with MIDI in FL Studio (MONO/POLY)

1. Set Mutaform to **MONO** or **POLY**.
2. Add a **MIDI Out** channel to the Channel Rack and set its **Port** to, for example, 1.
3. Open the Mutaform window, click the gear icon (wrapper settings) and set **MIDI input port** to the same number (1).
4. The notes you write in the Piano Roll on the MIDI Out channel now set the pitch of the vocal.

## Good to know

- Mutaform works best on **monophonic** sources: a single voice, a lead or a bass. Chords and full mixes give more "glitchy" results, which can also sound cool.
- **If parts of a song sound muddy or strange:**
  - Use a clean vocal track without reverb or backing vocals. If you only have the finished song, FL Studio 2024 and newer can pull out the vocal with **Stem separation** (right-click the audio clip).
  - Put reverb and delay **after** Mutaform, not before it.
  - Pick the right **DETECT** range: HIGH for bright voices, so bass and instruments don't confuse the pitch detector, and LOW for deep voices.
  - S, t and breath sounds now pass through without the Octave, Detune and Alternator copies, so they don't crackle.
- With every knob at its default (the Init preset), Mutaform is fully transparent: the sound passes through untouched, only delayed by the latency. Processing fades in smoothly as soon as you change something.
- **Switching on and off with automation:** automate the power button (Bypass) or Dry/Wet for clean switching. If you switch the mixer slot itself on and off instead, FL Studio stops sending audio to the plugin. When it comes back on, the plugin clears its buffers and fades in, so there is no glitch of old audio, but you will get a short gap of about 65 ms (the latency).
- The plugin has about 65 ms of latency at 48 kHz, because it needs to look a few pitch periods ahead. FL Studio compensates for this during playback, but you will hear the delay if you record live through the effect.
- Both VST3 and CLAP versions are included. FL Studio 2024 and newer support both, so use whichever you prefer.

## Building from source

The source code is in `Mutaform-source.zip`. On Windows:

```powershell
# Install Rust from https://rustup.rs (MSVC toolchain)
cd mutaform
cargo build --release --lib
# To package it, copy target\release\mutaform.dll to
#   Mutaform.vst3\Contents\x86_64-win\Mutaform.vst3
# and/or to Mutaform.clap
```

Source files:

- `src/engine.rs`: the PSOLA engine (pitch, formant, alternator, octave, smear, stereo, detune, MIDI voices, key lock)
- `src/pitch.rs`: YIN pitch detection
- `src/lib.rs`: plugin setup, the harmonics/FM stage and the signal chain
- `src/fx.rs`: the four effect slots
- `src/presets.rs`: factory presets (add your own here)
- `src/editor.rs`: the user interface
- `examples/render.rs`: an offline test that renders WAV files for each setting

Version 1.1.0 adds Key Lock and better pitch detection (fewer octave jumps and blips), and makes s and t sounds cleaner.
Version 1.1.1 removes the stutter when the plugin is inserted but nothing has been changed (transparent bypass at default settings).
Version 1.1.2 adds a Bypass button with a smooth crossfade and removes the glitch heard when switching the plugin back on.

License: GPL-3.0, because nih-plug's VST3 bindings are GPL.
