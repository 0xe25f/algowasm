use crate::types::{TrackId, WaveShape, TRACK_COUNT};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::error::Error;
use std::fmt::{Display, Formatter};

/// Complete JSON-serialisable style profile.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
  pub id: String,
  pub display_name: String,
  pub version: String,
  pub tempo: TempoRules,
  pub scales: Vec<ScaleRule>,
  pub chord_rules: ChordRules,
  pub rhythm_rules: RhythmRules,
  pub bass_rules: BassRules,
  pub melody_rules: MelodyRules,
  pub arrangement_rules: ArrangementRules,
  pub automation_rules: AutomationRules,
  pub patches: Vec<Patch>,
  pub mixer: MixerProfile,
  pub limits: SafetyLimits
}

/// Tempo constraints.
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TempoRules {
  pub min_bpm: f32,
  pub max_bpm: f32,
  pub default_bpm: f32
}

/// A weighted scale choice.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScaleRule {
  pub name: String,
  pub weight: u32
}

/// Chord grammar controls.
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChordRules {
  pub cadence_weight: f32,
  pub movement_weight: f32,
  pub suspended_chance: f32,
  pub bright_borrow_chance: f32
}

/// Rhythm grammar controls.
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RhythmRules {
  pub kick_density: f32,
  pub hat_density: f32,
  pub percussion_density: f32,
  pub fill_chance: f32
}

/// Bassline controls.
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BassRules {
  pub density: f32,
  pub slide_chance: f32,
  pub accent_chance: f32,
  pub octave_jump_chance: f32
}

/// Melody controls.
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MelodyRules {
  pub density: f32,
  pub mutation_chance: f32,
  pub call_response_chance: f32,
  pub max_range_semitones: i16
}

/// Arrangement controls.
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArrangementRules {
  pub min_section_bars: u8,
  pub max_section_bars: u8,
  pub future_bars: u8,
  pub reset_chance: f32
}

/// Automation controls.
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomationRules {
  pub filter_motion: f32,
  pub send_motion: f32,
  pub pan_motion: f32
}

/// Procedural patch for a track.
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Patch {
  pub track: TrackId,
  pub wave: WaveShape,
  pub gain: f32,
  pub pan: f32,
  pub attack_ms: f32,
  pub decay_ms: f32,
  pub sustain: f32,
  pub release_ms: f32,
  pub filter_cutoff: f32,
  pub resonance: f32,
  pub delay_send: f32,
  pub reverb_send: f32
}

/// Mixer profile.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MixerProfile {
  pub master_gain: f32,
  pub limiter_drive: f32,
  pub delay_feedback: f32,
  pub reverb_mix: f32,
  pub tracks: Vec<MixerTrackProfile>
}

/// Track-level mix settings.
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MixerTrackProfile {
  pub track: TrackId,
  pub gain: f32,
  pub pan: f32
}

/// Profile safety limits.
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SafetyLimits {
  pub min_sample_rate: u32,
  pub max_sample_rate: u32,
  pub min_bpm: f32,
  pub max_bpm: f32,
  pub max_tracks: usize,
  pub max_voices: usize,
  pub max_bars_queued: usize,
  pub max_events_per_block: usize
}

/// A clear profile validation error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileError {
  message: String
}

impl ProfileError {
  /// Creates a profile error.
  pub fn new(message: impl Into<String>) -> Self {
    Self {
      message: message.into()
    }
  }

  /// Returns the public error message.
  pub fn message(&self) -> &str {
    &self.message
  }
}

impl Display for ProfileError {
  fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
    formatter.write_str(&self.message)
  }
}

impl Error for ProfileError {}

impl Profile {
  /// Parses and validates a profile from JSON.
  pub fn from_json(json: &str) -> Result<Self, ProfileError> {
    let profile: Profile = serde_json::from_str(json)
      .map_err(|error| ProfileError::new(format!("Profile JSON is malformed: {error}.")))?;
    profile.validate()?;
    Ok(profile)
  }

  /// Returns this profile as JSON.
  pub fn to_json(&self) -> Result<String, ProfileError> {
    serde_json::to_string(self)
      .map_err(|error| ProfileError::new(format!("Profile could not be written as JSON: {error}.")))
  }

  /// Validates profile limits and public fields.
  pub fn validate(&self) -> Result<(), ProfileError> {
    if self.id.trim().is_empty() {
      return Err(ProfileError::new("Profile id is required."));
    }

    if self.display_name.trim().is_empty() {
      return Err(ProfileError::new("Profile display name is required."));
    }

    if self.version.trim().is_empty() {
      return Err(ProfileError::new("Profile version is required."));
    }

    if self.scales.is_empty() {
      return Err(ProfileError::new("Profile must include at least one scale."));
    }

    if self.limits.max_tracks == 0 || self.limits.max_tracks > TRACK_COUNT {
      return Err(ProfileError::new(
        "Profile limits must allow between 1 and 11 built-in tracks."
      ));
    }

    if self.limits.max_voices == 0 || self.limits.max_voices > 64 {
      return Err(ProfileError::new("Profile max voices must be between 1 and 64."));
    }

    if self.limits.max_bars_queued == 0 || self.limits.max_bars_queued > 64 {
      return Err(ProfileError::new("Profile max queued bars must be between 1 and 64."));
    }

    if self.limits.min_sample_rate < 22_050 || self.limits.max_sample_rate > 96_000 {
      return Err(ProfileError::new(
        "Profile sample rate limits must stay within 22050 to 96000 Hz."
      ));
    }

    if self.tempo.min_bpm < self.limits.min_bpm || self.tempo.max_bpm > self.limits.max_bpm {
      return Err(ProfileError::new("Profile tempo range exceeds its safety limits."));
    }

    if !(self.tempo.min_bpm <= self.tempo.default_bpm
      && self.tempo.default_bpm <= self.tempo.max_bpm)
    {
      return Err(ProfileError::new(
        "Profile default tempo must sit inside the tempo range."
      ));
    }

    if self.tempo.min_bpm < 40.0 || self.tempo.max_bpm > 240.0 {
      return Err(ProfileError::new("Profile tempo must stay between 40 and 240 BPM."));
    }

    for scale in &self.scales {
      if scale.name.trim().is_empty() || scale.weight == 0 {
        return Err(ProfileError::new(
          "Each scale must have a name and a positive weight."
        ));
      }
    }

    validate_unit("chord suspended chance", self.chord_rules.suspended_chance)?;
    validate_unit("chord bright borrow chance", self.chord_rules.bright_borrow_chance)?;
    validate_unit("kick density", self.rhythm_rules.kick_density)?;
    validate_unit("hat density", self.rhythm_rules.hat_density)?;
    validate_unit("percussion density", self.rhythm_rules.percussion_density)?;
    validate_unit("fill chance", self.rhythm_rules.fill_chance)?;
    validate_unit("bass density", self.bass_rules.density)?;
    validate_unit("bass slide chance", self.bass_rules.slide_chance)?;
    validate_unit("bass accent chance", self.bass_rules.accent_chance)?;
    validate_unit("bass octave jump chance", self.bass_rules.octave_jump_chance)?;
    validate_unit("melody density", self.melody_rules.density)?;
    validate_unit("melody mutation chance", self.melody_rules.mutation_chance)?;
    validate_unit(
      "melody call and response chance",
      self.melody_rules.call_response_chance
    )?;
    validate_unit("automation filter motion", self.automation_rules.filter_motion)?;
    validate_unit("automation send motion", self.automation_rules.send_motion)?;
    validate_unit("automation pan motion", self.automation_rules.pan_motion)?;

    if self.arrangement_rules.min_section_bars == 0
      || self.arrangement_rules.min_section_bars > self.arrangement_rules.max_section_bars
    {
      return Err(ProfileError::new(
        "Arrangement section bars must have a valid minimum and maximum."
      ));
    }

    if self.arrangement_rules.future_bars == 0
      || usize::from(self.arrangement_rules.future_bars) > self.limits.max_bars_queued
    {
      return Err(ProfileError::new(
        "Arrangement future bars must fit inside the profile safety limit."
      ));
    }

    let mut patch_tracks = HashSet::new();

    for patch in &self.patches {
      patch_tracks.insert(patch.track);
      validate_gain("patch gain", patch.gain)?;
      validate_pan("patch pan", patch.pan)?;
      validate_unit("patch sustain", patch.sustain)?;
      validate_unit("patch delay send", patch.delay_send)?;
      validate_unit("patch reverb send", patch.reverb_send)?;

      if patch.attack_ms < 0.0
        || patch.decay_ms < 0.0
        || patch.release_ms < 0.0
        || patch.filter_cutoff < 20.0
        || patch.filter_cutoff > 20_000.0
        || patch.resonance < 0.0
        || patch.resonance > 1.0
      {
        return Err(ProfileError::new(
          "Patch envelope, cutoff, and resonance values must stay in safe ranges."
        ));
      }
    }

    if patch_tracks.len() != TRACK_COUNT {
      return Err(ProfileError::new(
        "Profile must provide one patch for each built-in track."
      ));
    }

    validate_gain("master gain", self.mixer.master_gain)?;
    validate_gain("limiter drive", self.mixer.limiter_drive)?;
    validate_unit("delay feedback", self.mixer.delay_feedback)?;
    validate_unit("reverb mix", self.mixer.reverb_mix)?;

    let mut mixer_tracks = HashSet::new();

    for track in &self.mixer.tracks {
      mixer_tracks.insert(track.track);
      validate_gain("mixer track gain", track.gain)?;
      validate_pan("mixer track pan", track.pan)?;
    }

    if mixer_tracks.len() != TRACK_COUNT {
      return Err(ProfileError::new(
        "Profile mixer must provide one entry for each built-in track."
      ));
    }

    Ok(())
  }
}

fn validate_unit(name: &str, value: f32) -> Result<(), ProfileError> {
  if !(0.0..=1.0).contains(&value) || !value.is_finite() {
    return Err(ProfileError::new(format!("{name} must be between 0 and 1.")));
  }

  Ok(())
}

fn validate_gain(name: &str, value: f32) -> Result<(), ProfileError> {
  if !(0.0..=2.0).contains(&value) || !value.is_finite() {
    return Err(ProfileError::new(format!("{name} must be between 0 and 2.")));
  }

  Ok(())
}

fn validate_pan(name: &str, value: f32) -> Result<(), ProfileError> {
  if !(-1.0..=1.0).contains(&value) || !value.is_finite() {
    return Err(ProfileError::new(format!("{name} must be between -1 and 1.")));
  }

  Ok(())
}

/// Returns the built-in mid-1990s Amiga-inspired house profile.
pub fn amiga_house_95ish() -> Profile {
  Profile {
    id: "amiga_house_95ish".to_string(),
    display_name: "Amiga House 95ish".to_string(),
    version: "0.1.0".to_string(),
    tempo: TempoRules {
      min_bpm: 118.0,
      max_bpm: 138.0,
      default_bpm: 126.0
    },
    scales: vec![
      ScaleRule {
        name: "natural_minor".to_string(),
        weight: 34
      },
      ScaleRule {
        name: "dorian".to_string(),
        weight: 24
      },
      ScaleRule {
        name: "aeolian".to_string(),
        weight: 20
      },
      ScaleRule {
        name: "phrygian_dominant".to_string(),
        weight: 10
      },
      ScaleRule {
        name: "minor_pentatonic".to_string(),
        weight: 12
      }
    ],
    chord_rules: ChordRules {
      cadence_weight: 0.75,
      movement_weight: 0.6,
      suspended_chance: 0.24,
      bright_borrow_chance: 0.08
    },
    rhythm_rules: RhythmRules {
      kick_density: 1.0,
      hat_density: 0.72,
      percussion_density: 0.28,
      fill_chance: 0.32
    },
    bass_rules: BassRules {
      density: 0.62,
      slide_chance: 0.18,
      accent_chance: 0.34,
      octave_jump_chance: 0.16
    },
    melody_rules: MelodyRules {
      density: 0.32,
      mutation_chance: 0.22,
      call_response_chance: 0.55,
      max_range_semitones: 14
    },
    arrangement_rules: ArrangementRules {
      min_section_bars: 4,
      max_section_bars: 8,
      future_bars: 8,
      reset_chance: 0.08
    },
    automation_rules: AutomationRules {
      filter_motion: 0.46,
      send_motion: 0.24,
      pan_motion: 0.18
    },
    patches: built_in_patches(),
    mixer: built_in_mixer(),
    limits: SafetyLimits {
      min_sample_rate: 22_050,
      max_sample_rate: 96_000,
      min_bpm: 90.0,
      max_bpm: 160.0,
      max_tracks: TRACK_COUNT,
      max_voices: 48,
      max_bars_queued: 16,
      max_events_per_block: 128
    }
  }
}

fn built_in_patches() -> Vec<Patch> {
  vec![
    patch(TrackId::Kick, WaveShape::Sine, 1.0, 0.0, 0.0, 120.0, 0.0, 10.0, 120.0, 0.1, 0.0, 0.0),
    patch(TrackId::Snare, WaveShape::Noise, 0.75, 0.0, 0.0, 90.0, 0.0, 30.0, 5_000.0, 0.2, 0.05, 0.08),
    patch(TrackId::ClosedHat, WaveShape::Noise, 0.42, 0.15, 0.0, 35.0, 0.0, 10.0, 9_000.0, 0.2, 0.04, 0.05),
    patch(TrackId::OpenHat, WaveShape::Noise, 0.32, 0.25, 0.0, 240.0, 0.0, 50.0, 8_500.0, 0.2, 0.08, 0.08),
    patch(TrackId::Percussion, WaveShape::Triangle, 0.34, -0.2, 1.0, 80.0, 0.0, 20.0, 2_800.0, 0.3, 0.06, 0.08),
    patch(TrackId::Bass, WaveShape::Saw, 0.72, 0.0, 4.0, 90.0, 0.35, 70.0, 680.0, 0.62, 0.08, 0.04),
    patch(TrackId::Chords, WaveShape::Pulse, 0.38, -0.15, 5.0, 120.0, 0.18, 120.0, 1_800.0, 0.36, 0.16, 0.18),
    patch(TrackId::Pad, WaveShape::Triangle, 0.22, 0.0, 300.0, 700.0, 0.62, 600.0, 2_400.0, 0.22, 0.18, 0.32),
    patch(TrackId::Lead, WaveShape::Square, 0.36, 0.1, 8.0, 130.0, 0.24, 90.0, 2_800.0, 0.28, 0.22, 0.14),
    patch(TrackId::Arp, WaveShape::Pulse, 0.28, 0.18, 2.0, 80.0, 0.18, 50.0, 3_400.0, 0.24, 0.16, 0.12),
    patch(TrackId::Fx, WaveShape::Noise, 0.22, 0.0, 0.0, 600.0, 0.0, 120.0, 5_500.0, 0.2, 0.2, 0.26)
  ]
}

// The built-in patch table is easier to audit when the values stay in columns.
#[allow(clippy::too_many_arguments)]
fn patch(
  track: TrackId,
  wave: WaveShape,
  gain: f32,
  pan: f32,
  attack_ms: f32,
  decay_ms: f32,
  sustain: f32,
  release_ms: f32,
  filter_cutoff: f32,
  resonance: f32,
  delay_send: f32,
  reverb_send: f32
) -> Patch {
  Patch {
    track,
    wave,
    gain,
    pan,
    attack_ms,
    decay_ms,
    sustain,
    release_ms,
    filter_cutoff,
    resonance,
    delay_send,
    reverb_send
  }
}

fn built_in_mixer() -> MixerProfile {
  MixerProfile {
    master_gain: 0.82,
    limiter_drive: 0.96,
    delay_feedback: 0.34,
    reverb_mix: 0.16,
    tracks: TrackId::ALL
      .iter()
      .map(|track| MixerTrackProfile {
        track: *track,
        gain: match track {
          TrackId::Kick => 1.0,
          TrackId::Snare => 0.8,
          TrackId::ClosedHat => 0.55,
          TrackId::OpenHat => 0.45,
          TrackId::Percussion => 0.42,
          TrackId::Bass => 0.78,
          TrackId::Chords => 0.5,
          TrackId::Pad => 0.34,
          TrackId::Lead => 0.48,
          TrackId::Arp => 0.36,
          TrackId::Fx => 0.32
        },
        pan: match track {
          TrackId::ClosedHat => 0.18,
          TrackId::OpenHat => 0.28,
          TrackId::Percussion => -0.24,
          TrackId::Chords => -0.12,
          TrackId::Lead => 0.08,
          TrackId::Arp => 0.2,
          _ => 0.0
        }
      })
      .collect()
  }
}

/// Returns the built-in slower, broken-beat downtempo profile.
///
/// This profile keeps the same 11-track engine and chord grammar as
/// `amiga_house_95ish`, but drops the tempo, thins the four-on-the-floor kick,
/// and raises the fill chance so bars 12 to 15 roll more often, giving a
/// broken-beat feel without any new rhythm algorithm.
pub fn downtempo_breakbeat() -> Profile {
  Profile {
    id: "downtempo_breakbeat".to_string(),
    display_name: "Downtempo Breakbeat".to_string(),
    version: "0.1.0".to_string(),
    tempo: TempoRules {
      min_bpm: 88.0,
      max_bpm: 112.0,
      default_bpm: 98.0
    },
    scales: vec![
      ScaleRule {
        name: "natural_minor".to_string(),
        weight: 26
      },
      ScaleRule {
        name: "dorian".to_string(),
        weight: 30
      },
      ScaleRule {
        name: "aeolian".to_string(),
        weight: 16
      },
      ScaleRule {
        name: "phrygian_dominant".to_string(),
        weight: 16
      },
      ScaleRule {
        name: "minor_pentatonic".to_string(),
        weight: 12
      }
    ],
    chord_rules: ChordRules {
      cadence_weight: 0.7,
      movement_weight: 0.5,
      suspended_chance: 0.3,
      bright_borrow_chance: 0.05
    },
    rhythm_rules: RhythmRules {
      kick_density: 0.8,
      hat_density: 0.6,
      percussion_density: 0.5,
      fill_chance: 0.55
    },
    bass_rules: BassRules {
      density: 0.55,
      slide_chance: 0.14,
      accent_chance: 0.4,
      octave_jump_chance: 0.12
    },
    melody_rules: MelodyRules {
      density: 0.26,
      mutation_chance: 0.22,
      call_response_chance: 0.5,
      max_range_semitones: 12
    },
    arrangement_rules: ArrangementRules {
      min_section_bars: 4,
      max_section_bars: 8,
      future_bars: 8,
      reset_chance: 0.07
    },
    automation_rules: AutomationRules {
      filter_motion: 0.5,
      send_motion: 0.3,
      pan_motion: 0.16
    },
    patches: downtempo_breakbeat_patches(),
    mixer: downtempo_breakbeat_mixer(),
    limits: SafetyLimits {
      min_sample_rate: 22_050,
      max_sample_rate: 96_000,
      min_bpm: 70.0,
      max_bpm: 130.0,
      max_tracks: TRACK_COUNT,
      max_voices: 48,
      max_bars_queued: 16,
      max_events_per_block: 128
    }
  }
}

fn downtempo_breakbeat_patches() -> Vec<Patch> {
  vec![
    patch(TrackId::Kick, WaveShape::Sine, 0.95, 0.0, 0.0, 130.0, 0.0, 130.0, 100.0, 0.1, 0.0, 0.05),
    patch(TrackId::Snare, WaveShape::Noise, 0.7, 0.0, 0.0, 100.0, 0.0, 40.0, 4_200.0, 0.2, 0.1, 0.18),
    patch(TrackId::ClosedHat, WaveShape::Noise, 0.36, 0.12, 0.0, 30.0, 0.0, 8.0, 8_000.0, 0.2, 0.05, 0.1),
    patch(TrackId::OpenHat, WaveShape::Noise, 0.3, 0.22, 0.0, 220.0, 0.0, 45.0, 7_500.0, 0.2, 0.1, 0.14),
    patch(TrackId::Percussion, WaveShape::Triangle, 0.4, -0.22, 1.0, 90.0, 0.0, 25.0, 2_400.0, 0.3, 0.12, 0.16),
    patch(TrackId::Bass, WaveShape::Sine, 0.75, 0.0, 6.0, 110.0, 0.45, 90.0, 520.0, 0.3, 0.06, 0.08),
    patch(TrackId::Chords, WaveShape::Triangle, 0.34, -0.14, 20.0, 160.0, 0.3, 220.0, 1_500.0, 0.28, 0.22, 0.3),
    patch(TrackId::Pad, WaveShape::Triangle, 0.26, 0.0, 400.0, 800.0, 0.65, 900.0, 1_800.0, 0.2, 0.24, 0.42),
    patch(TrackId::Lead, WaveShape::Triangle, 0.3, 0.1, 14.0, 140.0, 0.22, 160.0, 2_200.0, 0.24, 0.28, 0.22),
    patch(TrackId::Arp, WaveShape::Pulse, 0.22, 0.18, 3.0, 70.0, 0.16, 55.0, 3_000.0, 0.22, 0.2, 0.16),
    patch(TrackId::Fx, WaveShape::Noise, 0.2, 0.0, 0.0, 700.0, 0.0, 150.0, 5_000.0, 0.2, 0.26, 0.34)
  ]
}

fn downtempo_breakbeat_mixer() -> MixerProfile {
  MixerProfile {
    master_gain: 0.8,
    limiter_drive: 0.9,
    delay_feedback: 0.4,
    reverb_mix: 0.3,
    tracks: TrackId::ALL
      .iter()
      .map(|track| MixerTrackProfile {
        track: *track,
        gain: match track {
          TrackId::Kick => 1.0,
          TrackId::Snare => 0.78,
          TrackId::ClosedHat => 0.5,
          TrackId::OpenHat => 0.42,
          TrackId::Percussion => 0.46,
          TrackId::Bass => 0.76,
          TrackId::Chords => 0.46,
          TrackId::Pad => 0.4,
          TrackId::Lead => 0.4,
          TrackId::Arp => 0.32,
          TrackId::Fx => 0.3
        },
        pan: match track {
          TrackId::ClosedHat => 0.18,
          TrackId::OpenHat => 0.28,
          TrackId::Percussion => -0.24,
          TrackId::Chords => -0.12,
          TrackId::Lead => 0.08,
          TrackId::Arp => 0.2,
          _ => 0.0
        }
      })
      .collect()
  }
}

/// Returns the built-in hypnotic, roomy dub and deep house profile.
///
/// This profile keeps the same tempo family, chord grammar, and four-on-the-
/// floor kick as `amiga_house_95ish`, but favours longer sections, sparser
/// percussion, and warmer, more heavily sent patches for a dubbed-out, deep
/// house character.
pub fn dub_deep_house() -> Profile {
  Profile {
    id: "dub_deep_house".to_string(),
    display_name: "Dub Deep House".to_string(),
    version: "0.1.0".to_string(),
    tempo: TempoRules {
      min_bpm: 118.0,
      max_bpm: 124.0,
      default_bpm: 121.0
    },
    scales: vec![
      ScaleRule {
        name: "natural_minor".to_string(),
        weight: 30
      },
      ScaleRule {
        name: "dorian".to_string(),
        weight: 30
      },
      ScaleRule {
        name: "aeolian".to_string(),
        weight: 24
      },
      ScaleRule {
        name: "phrygian_dominant".to_string(),
        weight: 6
      },
      ScaleRule {
        name: "minor_pentatonic".to_string(),
        weight: 10
      }
    ],
    chord_rules: ChordRules {
      cadence_weight: 0.8,
      movement_weight: 0.4,
      suspended_chance: 0.3,
      bright_borrow_chance: 0.1
    },
    rhythm_rules: RhythmRules {
      kick_density: 1.0,
      hat_density: 0.5,
      percussion_density: 0.22,
      fill_chance: 0.18
    },
    bass_rules: BassRules {
      density: 0.48,
      slide_chance: 0.05,
      accent_chance: 0.22,
      octave_jump_chance: 0.05
    },
    melody_rules: MelodyRules {
      density: 0.2,
      mutation_chance: 0.18,
      call_response_chance: 0.4,
      max_range_semitones: 10
    },
    arrangement_rules: ArrangementRules {
      min_section_bars: 8,
      max_section_bars: 16,
      future_bars: 12,
      reset_chance: 0.05
    },
    automation_rules: AutomationRules {
      filter_motion: 0.6,
      send_motion: 0.42,
      pan_motion: 0.24
    },
    patches: dub_deep_house_patches(),
    mixer: dub_deep_house_mixer(),
    limits: SafetyLimits {
      min_sample_rate: 22_050,
      max_sample_rate: 96_000,
      min_bpm: 100.0,
      max_bpm: 130.0,
      max_tracks: TRACK_COUNT,
      max_voices: 48,
      max_bars_queued: 24,
      max_events_per_block: 128
    }
  }
}

fn dub_deep_house_patches() -> Vec<Patch> {
  vec![
    patch(TrackId::Kick, WaveShape::Sine, 1.0, 0.0, 0.0, 130.0, 0.0, 140.0, 110.0, 0.1, 0.0, 0.06),
    patch(TrackId::Snare, WaveShape::Noise, 0.6, 0.0, 0.0, 80.0, 0.0, 30.0, 4_500.0, 0.2, 0.08, 0.22),
    patch(TrackId::ClosedHat, WaveShape::Noise, 0.4, 0.16, 0.0, 28.0, 0.0, 8.0, 8_200.0, 0.2, 0.05, 0.12),
    patch(TrackId::OpenHat, WaveShape::Noise, 0.3, 0.26, 0.0, 200.0, 0.0, 40.0, 7_800.0, 0.2, 0.1, 0.16),
    patch(TrackId::Percussion, WaveShape::Triangle, 0.3, -0.2, 1.0, 70.0, 0.0, 20.0, 2_600.0, 0.28, 0.1, 0.14),
    patch(TrackId::Bass, WaveShape::Triangle, 0.8, 0.0, 8.0, 120.0, 0.55, 110.0, 420.0, 0.2, 0.04, 0.06),
    patch(TrackId::Chords, WaveShape::Triangle, 0.3, -0.1, 40.0, 200.0, 0.4, 380.0, 1_200.0, 0.24, 0.3, 0.38),
    patch(TrackId::Pad, WaveShape::Triangle, 0.28, 0.0, 600.0, 900.0, 0.7, 1_100.0, 1_600.0, 0.18, 0.26, 0.46),
    patch(TrackId::Lead, WaveShape::Triangle, 0.24, 0.06, 20.0, 150.0, 0.2, 200.0, 1_900.0, 0.2, 0.32, 0.3),
    patch(TrackId::Arp, WaveShape::Triangle, 0.2, 0.16, 4.0, 80.0, 0.14, 70.0, 2_600.0, 0.2, 0.26, 0.22),
    patch(TrackId::Fx, WaveShape::Noise, 0.18, 0.0, 0.0, 800.0, 0.0, 180.0, 4_800.0, 0.2, 0.3, 0.4)
  ]
}

fn dub_deep_house_mixer() -> MixerProfile {
  MixerProfile {
    master_gain: 0.78,
    limiter_drive: 0.85,
    delay_feedback: 0.46,
    reverb_mix: 0.36,
    tracks: TrackId::ALL
      .iter()
      .map(|track| MixerTrackProfile {
        track: *track,
        gain: match track {
          TrackId::Kick => 1.0,
          TrackId::Snare => 0.62,
          TrackId::ClosedHat => 0.46,
          TrackId::OpenHat => 0.38,
          TrackId::Percussion => 0.34,
          TrackId::Bass => 0.8,
          TrackId::Chords => 0.4,
          TrackId::Pad => 0.36,
          TrackId::Lead => 0.34,
          TrackId::Arp => 0.28,
          TrackId::Fx => 0.26
        },
        pan: match track {
          TrackId::ClosedHat => 0.18,
          TrackId::OpenHat => 0.28,
          TrackId::Percussion => -0.24,
          TrackId::Chords => -0.12,
          TrackId::Lead => 0.08,
          TrackId::Arp => 0.2,
          _ => 0.0
        }
      })
      .collect()
  }
}

#[cfg(test)]
mod tests {
  use super::{amiga_house_95ish, downtempo_breakbeat, dub_deep_house, Profile};

  #[test]
  fn built_in_profile_is_valid() {
    let profile = amiga_house_95ish();
    assert!(profile.validate().is_ok());
  }

  #[test]
  fn downtempo_breakbeat_profile_is_valid() {
    let profile = downtempo_breakbeat();
    assert!(profile.validate().is_ok());
  }

  #[test]
  fn dub_deep_house_profile_is_valid() {
    let profile = dub_deep_house();
    assert!(profile.validate().is_ok());
  }

  #[test]
  fn profile_json_round_trips() {
    let profile = amiga_house_95ish();
    let json = profile.to_json().expect("profile should convert to JSON");
    let parsed = Profile::from_json(&json).expect("profile should parse");
    assert_eq!(parsed.id, "amiga_house_95ish");
  }

  #[test]
  fn invalid_tempo_is_rejected() {
    let mut profile = amiga_house_95ish();
    profile.tempo.min_bpm = 160.0;
    profile.tempo.max_bpm = 120.0;
    assert!(profile.validate().is_err());
  }
}

