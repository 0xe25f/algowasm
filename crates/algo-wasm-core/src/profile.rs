//! Style profiles, their validation, and the flat numeric layout used to
//! pass them across the WASM boundary.
//!
//! A profile holds no heap data, so it can be copied freely and read from the
//! audio thread without allocating.

use crate::composition::ScaleKind;
use crate::types::{TrackId, WaveShape, TRACK_COUNT};
use std::error::Error;
use std::fmt::{Display, Formatter};

/// Largest number of weighted scales a profile may list.
pub const MAX_SCALES: usize = 8;

/// Largest number of events the engine buffers between drains.
pub const MAX_EVENTS_LIMIT: usize = 512;

/// Version of the flat profile layout. JavaScript writes this as the first
/// value, so a mismatched package and binary fail loudly instead of reading
/// values from the wrong slots.
pub const PROFILE_WIRE_VERSION: f32 = 1.0;

/// Number of values stored for each patch in the flat layout.
pub const PATCH_WIRE_FIELDS: usize = 18;

/// Number of values before the scale list in the flat layout.
const WIRE_HEADER_FIELDS: usize = 41;

/// Total number of `f32` values in the flat profile layout.
pub const PROFILE_WIRE_LEN: usize =
  WIRE_HEADER_FIELDS + 1 + MAX_SCALES * 2 + TRACK_COUNT * PATCH_WIRE_FIELDS + TRACK_COUNT * 2;

/// Complete style profile.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Profile {
  pub tempo: TempoRules,
  /// Weighted scale choices. Only the first `scale_count` entries are used.
  pub scales: [ScaleRule; MAX_SCALES],
  pub scale_count: usize,
  pub chord_rules: ChordRules,
  pub rhythm_rules: RhythmRules,
  pub bass_rules: BassRules,
  pub melody_rules: MelodyRules,
  pub arrangement_rules: ArrangementRules,
  pub automation_rules: AutomationRules,
  /// One patch per built-in track, in `TrackId::ALL` order.
  pub patches: [Patch; TRACK_COUNT],
  pub mixer: MixerProfile,
  pub limits: SafetyLimits
}

/// Tempo constraints.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TempoRules {
  pub min_bpm: f32,
  pub max_bpm: f32,
  pub default_bpm: f32
}

/// A weighted scale choice.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScaleRule {
  pub scale: ScaleKind,
  pub weight: u32
}

/// Chord grammar controls.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChordRules {
  pub cadence_weight: f32,
  pub movement_weight: f32,
  pub suspended_chance: f32,
  pub bright_borrow_chance: f32
}

/// Rhythm grammar controls.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RhythmRules {
  pub kick_density: f32,
  pub hat_density: f32,
  pub percussion_density: f32,
  pub fill_chance: f32,
  /// Chance that a broken-beat song puts its snare on beat three only, which
  /// gives a half-time feel. Ignored for four-on-the-floor profiles.
  pub half_time_chance: f32
}

/// Bassline controls.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BassRules {
  pub density: f32,
  pub slide_chance: f32,
  pub accent_chance: f32,
  pub octave_jump_chance: f32,
  /// Chance that a song holds each bass note until the next one starts,
  /// instead of using a short seeded gate.
  pub sustain_chance: f32
}

/// Melody controls.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MelodyRules {
  pub density: f32,
  pub mutation_chance: f32,
  pub call_response_chance: f32,
  pub max_range_semitones: i16
}

/// Arrangement controls.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArrangementRules {
  pub min_section_bars: u8,
  pub max_section_bars: u8,
  pub future_bars: u8,
  pub reset_chance: f32
}

/// Automation controls.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AutomationRules {
  pub filter_motion: f32,
  pub send_motion: f32,
  pub pan_motion: f32
}

/// Procedural patch for a track.
#[derive(Clone, Copy, Debug, PartialEq)]
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
  pub reverb_send: f32,
  /// Detune of the second oscillator in cents. Zero turns it off.
  pub detune_cents: f32,
  /// How far the filter opens at note start, in octaves above the cutoff.
  pub filter_env_octaves: f32,
  /// Time the filter envelope takes to fall back to the cutoff.
  pub filter_env_decay_ms: f32,
  /// Filter LFO rate in cycles per beat. Zero turns it off.
  pub lfo_rate: f32,
  /// Filter LFO depth in octaves either side of the cutoff.
  pub lfo_depth: f32,
  /// Stereo spread from 0 (centred) to 1 (wide).
  pub width: f32,
  /// How much the kick ducks this track, from 0 (none) to 1 (full).
  pub duck: f32
}

/// Mixer profile.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MixerProfile {
  pub master_gain: f32,
  pub limiter_drive: f32,
  pub delay_feedback: f32,
  pub reverb_mix: f32,
  /// One entry per built-in track, in `TrackId::ALL` order.
  pub tracks: [MixerTrackProfile; TRACK_COUNT]
}

/// Track-level mix settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MixerTrackProfile {
  pub track: TrackId,
  pub gain: f32,
  pub pan: f32
}

/// Profile safety limits.
#[derive(Clone, Copy, Debug, PartialEq)]
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
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProfileError {
  message: &'static str
}

impl ProfileError {
  /// Creates a profile error.
  pub const fn new(message: &'static str) -> Self {
    Self {
      message
    }
  }

  /// Returns the public error message.
  pub const fn message(&self) -> &'static str {
    self.message
  }
}

impl Display for ProfileError {
  fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
    formatter.write_str(self.message)
  }
}

impl Error for ProfileError {}

/// Reads values from the flat layout in order.
struct WireReader<'a> {
  values: &'a [f32],
  index: usize
}

impl WireReader<'_> {
  /// Returns the next value. Callers check the total length first, so a
  /// missing value can only mean the layout constants are wrong.
  fn next(&mut self) -> Result<f32, ProfileError> {
    let value = self
      .values
      .get(self.index)
      .copied()
      .ok_or(ProfileError::new("Profile data ended early."))?;
    self.index += 1;

    if value.is_finite() {
      Ok(value)
    } else {
      Err(ProfileError::new("Profile values must be finite numbers."))
    }
  }

  /// Returns the next value as a whole number inside a range. Fractions are
  /// rejected rather than truncated, so a bad value never changes silently.
  fn int(&mut self, min: f32, max: f32, message: &'static str) -> Result<u32, ProfileError> {
    let value = self.next()?;

    if value.fract() != 0.0 || value < min || value > max {
      return Err(ProfileError::new(message));
    }

    Ok(value as u32)
  }
}

impl Profile {
  /// Reads and validates a profile from the flat numeric layout.
  ///
  /// The layout is `[version, tempo x3, chords x4, rhythm x5, bass x5,
  /// melody x4, arrangement x4, automation x3, mixer x4, limits x8,
  /// scale count, (scale id, weight) x8, patches x11, mixer tracks x11]`.
  /// Patches and mixer tracks are stored in `TrackId::ALL` order. The
  /// package's `encodeProfile` writes the same layout.
  pub fn from_wire(values: &[f32]) -> Result<Self, ProfileError> {
    if values.len() != PROFILE_WIRE_LEN || values[0] != PROFILE_WIRE_VERSION {
      return Err(ProfileError::new(
        "Profile data does not match this engine. Use the WASM file and JavaScript from the same release."
      ));
    }

    let mut wire = WireReader {
      values,
      index: 1
    };
    let tempo = TempoRules {
      min_bpm: wire.next()?,
      max_bpm: wire.next()?,
      default_bpm: wire.next()?
    };
    let chord_rules = ChordRules {
      cadence_weight: wire.next()?,
      movement_weight: wire.next()?,
      suspended_chance: wire.next()?,
      bright_borrow_chance: wire.next()?
    };
    let rhythm_rules = RhythmRules {
      kick_density: wire.next()?,
      hat_density: wire.next()?,
      percussion_density: wire.next()?,
      fill_chance: wire.next()?,
      half_time_chance: wire.next()?
    };
    let bass_rules = BassRules {
      density: wire.next()?,
      slide_chance: wire.next()?,
      accent_chance: wire.next()?,
      octave_jump_chance: wire.next()?,
      sustain_chance: wire.next()?
    };
    let melody_rules = MelodyRules {
      density: wire.next()?,
      mutation_chance: wire.next()?,
      call_response_chance: wire.next()?,
      max_range_semitones: wire.int(1.0, 48.0, "Melody max range must be a whole number from 1 to 48.")?
        as i16
    };
    let section_message = "Arrangement bar counts must be whole numbers from 1 to 64.";
    let arrangement_rules = ArrangementRules {
      min_section_bars: wire.int(1.0, 64.0, section_message)? as u8,
      max_section_bars: wire.int(1.0, 64.0, section_message)? as u8,
      future_bars: wire.int(1.0, 64.0, section_message)? as u8,
      reset_chance: wire.next()?
    };
    let automation_rules = AutomationRules {
      filter_motion: wire.next()?,
      send_motion: wire.next()?,
      pan_motion: wire.next()?
    };
    let master_gain = wire.next()?;
    let limiter_drive = wire.next()?;
    let delay_feedback = wire.next()?;
    let reverb_mix = wire.next()?;
    let rate_message = "Profile sample rate limits must stay within 22050 to 96000 Hz.";
    let limits = SafetyLimits {
      min_sample_rate: wire.int(22_050.0, 96_000.0, rate_message)?,
      max_sample_rate: wire.int(22_050.0, 96_000.0, rate_message)?,
      min_bpm: wire.next()?,
      max_bpm: wire.next()?,
      max_tracks: wire.int(11.0, 11.0, "Profile max tracks must be 11, the built-in track count.")?
        as usize,
      max_voices: wire.int(1.0, 64.0, "Profile max voices must be between 1 and 64.")? as usize,
      max_bars_queued: wire.int(1.0, 64.0, "Profile max queued bars must be between 1 and 64.")?
        as usize,
      max_events_per_block: wire.int(
        1.0,
        MAX_EVENTS_LIMIT as f32,
        "Profile max events per block must be between 1 and 512."
      )? as usize
    };
    let scale_count =
      wire.int(1.0, MAX_SCALES as f32, "Profile must include between 1 and 8 scales.")? as usize;
    let mut scales = [ScaleRule::default(); MAX_SCALES];

    for (index, rule) in scales.iter_mut().enumerate() {
      let id = wire.next()?;
      let weight = wire.next()?;

      if index >= scale_count {
        continue;
      }

      let scale = ScaleKind::from_index(id).ok_or(ProfileError::new(
        "Scale name is not recognised. Use natural_minor, dorian, aeolian, phrygian_dominant, or minor_pentatonic."
      ))?;

      if weight.fract() != 0.0 || !(1.0..=1_000_000.0).contains(&weight) {
        return Err(ProfileError::new(
          "Each scale weight must be a whole number from 1 to 1000000."
        ));
      }

      *rule = ScaleRule {
        scale,
        weight: weight as u32
      };
    }

    let mut patches = [Patch::silent(TrackId::Kick); TRACK_COUNT];

    for (patch, track) in patches.iter_mut().zip(TrackId::ALL) {
      let wave = wire.int(0.0, 5.0, "Patch wave shape is not recognised.")?;

      *patch = Patch {
        track,
        wave: WaveShape::from_index(wave)
          .ok_or(ProfileError::new("Patch wave shape is not recognised."))?,
        gain: wire.next()?,
        pan: wire.next()?,
        attack_ms: wire.next()?,
        decay_ms: wire.next()?,
        sustain: wire.next()?,
        release_ms: wire.next()?,
        filter_cutoff: wire.next()?,
        resonance: wire.next()?,
        delay_send: wire.next()?,
        reverb_send: wire.next()?,
        detune_cents: wire.next()?,
        filter_env_octaves: wire.next()?,
        filter_env_decay_ms: wire.next()?,
        lfo_rate: wire.next()?,
        lfo_depth: wire.next()?,
        width: wire.next()?,
        duck: wire.next()?
      };
    }

    let mut tracks = [MixerTrackProfile {
      track: TrackId::Kick,
      gain: 1.0,
      pan: 0.0
    }; TRACK_COUNT];

    for (entry, track) in tracks.iter_mut().zip(TrackId::ALL) {
      *entry = MixerTrackProfile {
        track,
        gain: wire.next()?,
        pan: wire.next()?
      };
    }

    let profile = Profile {
      tempo,
      scales,
      scale_count,
      chord_rules,
      rhythm_rules,
      bass_rules,
      melody_rules,
      arrangement_rules,
      automation_rules,
      patches,
      mixer: MixerProfile {
        master_gain,
        limiter_drive,
        delay_feedback,
        reverb_mix,
        tracks
      },
      limits
    };
    profile.validate()?;
    Ok(profile)
  }

  /// Returns the scale rules that are in use.
  pub fn active_scales(&self) -> &[ScaleRule] {
    &self.scales[..self.scale_count.min(MAX_SCALES)]
  }

  /// Validates profile limits and public fields.
  pub fn validate(&self) -> Result<(), ProfileError> {
    if self.scale_count == 0 || self.scale_count > MAX_SCALES {
      return Err(ProfileError::new("Profile must include between 1 and 8 scales."));
    }

    for rule in self.active_scales() {
      if rule.weight == 0 || rule.weight > 1_000_000 {
        return Err(ProfileError::new(
          "Each scale weight must be a whole number from 1 to 1000000."
        ));
      }
    }

    if self.limits.max_tracks != TRACK_COUNT {
      return Err(ProfileError::new(
        "Profile max tracks must be 11, the built-in track count."
      ));
    }

    if self.limits.max_voices == 0 || self.limits.max_voices > 64 {
      return Err(ProfileError::new("Profile max voices must be between 1 and 64."));
    }

    if self.limits.max_bars_queued == 0 || self.limits.max_bars_queued > 64 {
      return Err(ProfileError::new("Profile max queued bars must be between 1 and 64."));
    }

    if self.limits.max_events_per_block == 0 || self.limits.max_events_per_block > MAX_EVENTS_LIMIT {
      return Err(ProfileError::new(
        "Profile max events per block must be between 1 and 512."
      ));
    }

    if self.limits.min_sample_rate < 22_050
      || self.limits.max_sample_rate > 96_000
      || self.limits.min_sample_rate > self.limits.max_sample_rate
    {
      return Err(ProfileError::new(
        "Profile sample rate limits must stay within 22050 to 96000 Hz."
      ));
    }

    // Written so that a NaN anywhere fails the check instead of passing it.
    let tempo_ordered = self.tempo.min_bpm >= 40.0
      && self.tempo.max_bpm <= 240.0
      && self.tempo.min_bpm <= self.tempo.default_bpm
      && self.tempo.default_bpm <= self.tempo.max_bpm;

    if !tempo_ordered {
      return Err(ProfileError::new(
        "Profile tempo must be ordered and stay between 40 and 240 BPM."
      ));
    }

    if !in_range(self.limits.min_bpm, 1.0, 1_000.0) || !in_range(self.limits.max_bpm, 1.0, 1_000.0) {
      return Err(ProfileError::new("Profile tempo limits must be between 1 and 1000 BPM."));
    }

    let tempo_inside_limits =
      self.tempo.min_bpm >= self.limits.min_bpm && self.tempo.max_bpm <= self.limits.max_bpm;

    if !tempo_inside_limits {
      return Err(ProfileError::new("Profile tempo range exceeds its safety limits."));
    }

    let weights_valid = in_range(self.chord_rules.cadence_weight, 0.0, 4.0)
      && in_range(self.chord_rules.movement_weight, 0.0, 4.0);

    if !weights_valid {
      return Err(ProfileError::new("Chord cadence and movement weights must be between 0 and 4."));
    }

    let units = [
      self.chord_rules.suspended_chance,
      self.chord_rules.bright_borrow_chance,
      self.rhythm_rules.kick_density,
      self.rhythm_rules.hat_density,
      self.rhythm_rules.percussion_density,
      self.rhythm_rules.fill_chance,
      self.rhythm_rules.half_time_chance,
      self.bass_rules.density,
      self.bass_rules.slide_chance,
      self.bass_rules.accent_chance,
      self.bass_rules.octave_jump_chance,
      self.bass_rules.sustain_chance,
      self.melody_rules.density,
      self.melody_rules.mutation_chance,
      self.melody_rules.call_response_chance,
      self.arrangement_rules.reset_chance,
      self.automation_rules.filter_motion,
      self.automation_rules.send_motion,
      self.automation_rules.pan_motion,
      self.mixer.delay_feedback,
      self.mixer.reverb_mix
    ];

    if units.iter().any(|value| !in_range(*value, 0.0, 1.0)) {
      return Err(ProfileError::new(
        "Profile chances, densities, motion amounts, delay feedback, and reverb mix must be between 0 and 1."
      ));
    }

    if !(1..=48).contains(&self.melody_rules.max_range_semitones) {
      return Err(ProfileError::new(
        "Melody max range must be a whole number from 1 to 48."
      ));
    }

    if self.arrangement_rules.min_section_bars == 0
      || self.arrangement_rules.min_section_bars > self.arrangement_rules.max_section_bars
      || self.arrangement_rules.max_section_bars > 64
    {
      return Err(ProfileError::new(
        "Arrangement section bars must have a valid minimum and maximum up to 64."
      ));
    }

    if self.arrangement_rules.future_bars == 0
      || usize::from(self.arrangement_rules.future_bars) > self.limits.max_bars_queued
    {
      return Err(ProfileError::new(
        "Arrangement future bars must fit inside the profile safety limit."
      ));
    }

    for (patch, track) in self.patches.iter().zip(TrackId::ALL) {
      if patch.track != track {
        return Err(ProfileError::new(
          "Profile must provide one patch for each built-in track."
        ));
      }

      let levels_valid = in_range(patch.gain, 0.0, 2.0)
        && in_range(patch.pan, -1.0, 1.0)
        && in_range(patch.sustain, 0.0, 1.0)
        && in_range(patch.delay_send, 0.0, 1.0)
        && in_range(patch.reverb_send, 0.0, 1.0)
        && in_range(patch.width, 0.0, 1.0)
        && in_range(patch.duck, 0.0, 1.0);

      if !levels_valid {
        return Err(ProfileError::new(
          "Patch gain must be 0 to 2, pan -1 to 1, and sustain, sends, width, and duck 0 to 1."
        ));
      }

      let shape_valid = in_range(patch.attack_ms, 0.0, 10_000.0)
        && in_range(patch.decay_ms, 0.0, 10_000.0)
        && in_range(patch.release_ms, 0.0, 10_000.0)
        && in_range(patch.filter_cutoff, 20.0, 20_000.0)
        && in_range(patch.resonance, 0.0, 1.0)
        && in_range(patch.detune_cents, 0.0, 50.0)
        && in_range(patch.filter_env_octaves, 0.0, 6.0)
        && in_range(patch.filter_env_decay_ms, 1.0, 10_000.0)
        && in_range(patch.lfo_rate, 0.0, 8.0)
        && in_range(patch.lfo_depth, 0.0, 4.0);

      if !shape_valid {
        return Err(ProfileError::new(
          "Patch envelope, cutoff, resonance, detune, filter envelope, and LFO values must stay in their documented ranges."
        ));
      }
    }

    if !in_range(self.mixer.master_gain, 0.0, 2.0) || !in_range(self.mixer.limiter_drive, 0.0, 2.0) {
      return Err(ProfileError::new("Master gain and limiter drive must be between 0 and 2."));
    }

    for (entry, track) in self.mixer.tracks.iter().zip(TrackId::ALL) {
      if entry.track != track || !in_range(entry.gain, 0.0, 2.0) || !in_range(entry.pan, -1.0, 1.0)
      {
        return Err(ProfileError::new(
          "Profile mixer must provide one entry per built-in track, with gain 0 to 2 and pan -1 to 1."
        ));
      }
    }

    Ok(())
  }
}

impl Patch {
  /// Returns a silent placeholder patch for a track.
  pub const fn silent(track: TrackId) -> Self {
    Self {
      track,
      wave: WaveShape::Sine,
      gain: 0.0,
      pan: 0.0,
      attack_ms: 0.0,
      decay_ms: 0.0,
      sustain: 0.0,
      release_ms: 0.0,
      filter_cutoff: 1_000.0,
      resonance: 0.0,
      delay_send: 0.0,
      reverb_send: 0.0,
      detune_cents: 0.0,
      filter_env_octaves: 0.0,
      filter_env_decay_ms: 120.0,
      lfo_rate: 0.0,
      lfo_depth: 0.0,
      width: 0.0,
      duck: 0.0
    }
  }
}

/// Returns true when a value is finite and inside an inclusive range.
fn in_range(value: f32, min: f32, max: f32) -> bool {
  value.is_finite() && value >= min && value <= max
}

#[cfg(test)]
mod tests {
  use super::{Profile, PROFILE_WIRE_LEN};
  use crate::builtin::{all, AMIGA_HOUSE_95ISH_WIRE};

  #[test]
  fn built_in_profiles_validate() {
    for profile in all() {
      assert_eq!(profile.validate(), Ok(()));
    }
  }

  #[test]
  fn hostile_values_are_rejected() {
    // Slot 40 is maxEventsPerBlock and slot 1 is the minimum tempo.
    for (slot, value) in [(40, 4_000_000_000.0), (40, 1.5), (1, f32::NAN), (0, 99.0)] {
      let mut wire = AMIGA_HOUSE_95ISH_WIRE;
      wire[slot] = value;
      assert!(Profile::from_wire(&wire).is_err(), "slot {slot} accepted {value}");
    }

    assert!(Profile::from_wire(&AMIGA_HOUSE_95ISH_WIRE[..PROFILE_WIRE_LEN - 1]).is_err());
  }
}
