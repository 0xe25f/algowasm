use serde::{Deserialize, Serialize};

/// Stable track count used by the built-in mixer and FFI boundary.
pub const TRACK_COUNT: usize = 11;

/// Stable public track identifiers.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TrackId {
  Kick,
  Snare,
  ClosedHat,
  OpenHat,
  Percussion,
  Bass,
  Chords,
  Pad,
  Lead,
  Arp,
  Fx
}

impl TrackId {
  /// All built-in tracks in stable order.
  pub const ALL: [TrackId; TRACK_COUNT] = [
    TrackId::Kick,
    TrackId::Snare,
    TrackId::ClosedHat,
    TrackId::OpenHat,
    TrackId::Percussion,
    TrackId::Bass,
    TrackId::Chords,
    TrackId::Pad,
    TrackId::Lead,
    TrackId::Arp,
    TrackId::Fx
  ];

  /// Returns the compact mixer index for this track.
  pub const fn as_index(self) -> usize {
    match self {
      TrackId::Kick => 0,
      TrackId::Snare => 1,
      TrackId::ClosedHat => 2,
      TrackId::OpenHat => 3,
      TrackId::Percussion => 4,
      TrackId::Bass => 5,
      TrackId::Chords => 6,
      TrackId::Pad => 7,
      TrackId::Lead => 8,
      TrackId::Arp => 9,
      TrackId::Fx => 10
    }
  }

  /// Returns the public string identifier.
  pub const fn as_str(self) -> &'static str {
    match self {
      TrackId::Kick => "kick",
      TrackId::Snare => "snare",
      TrackId::ClosedHat => "closed_hat",
      TrackId::OpenHat => "open_hat",
      TrackId::Percussion => "percussion",
      TrackId::Bass => "bass",
      TrackId::Chords => "chords",
      TrackId::Pad => "pad",
      TrackId::Lead => "lead",
      TrackId::Arp => "arp",
      TrackId::Fx => "fx"
    }
  }

  /// Parses a public track identifier.
  pub fn from_id(value: &str) -> Option<Self> {
    match value {
      "kick" => Some(TrackId::Kick),
      "snare" => Some(TrackId::Snare),
      "closed_hat" => Some(TrackId::ClosedHat),
      "open_hat" => Some(TrackId::OpenHat),
      "percussion" => Some(TrackId::Percussion),
      "bass" => Some(TrackId::Bass),
      "chords" => Some(TrackId::Chords),
      "pad" => Some(TrackId::Pad),
      "lead" => Some(TrackId::Lead),
      "arp" => Some(TrackId::Arp),
      "fx" => Some(TrackId::Fx),
      _ => None
    }
  }
}

/// Sections used by the rolling endless arrangement.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SectionKind {
  Intro,
  GrooveA,
  GrooveB,
  Build,
  Drop,
  Variation,
  Breakdown,
  Fill,
  Transition,
  Reset
}

impl SectionKind {
  /// Returns the public string identifier.
  pub const fn as_str(self) -> &'static str {
    match self {
      SectionKind::Intro => "intro",
      SectionKind::GrooveA => "groove_a",
      SectionKind::GrooveB => "groove_b",
      SectionKind::Build => "build",
      SectionKind::Drop => "drop",
      SectionKind::Variation => "variation",
      SectionKind::Breakdown => "breakdown",
      SectionKind::Fill => "fill",
      SectionKind::Transition => "transition",
      SectionKind::Reset => "reset"
    }
  }
}

/// Live mood control exposed to frontends.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Mood {
  Dark,
  Bright,
  Tense,
  Calm
}

impl Mood {
  /// Parses a public mood identifier.
  pub fn from_id(value: &str) -> Option<Self> {
    match value {
      "dark" => Some(Mood::Dark),
      "bright" => Some(Mood::Bright),
      "tense" => Some(Mood::Tense),
      "calm" => Some(Mood::Calm),
      _ => None
    }
  }
}

/// Oscillator shape used by procedural patches.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WaveShape {
  Sine,
  Triangle,
  Saw,
  Square,
  Pulse,
  Noise
}

/// A compact event emitted by the deterministic scheduler.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum MusicEvent {
  NoteOn {
    sample: u64,
    track: TrackId,
    note: i16,
    velocity: f32
  },
  NoteOff {
    sample: u64,
    track: TrackId,
    note: i16
  },
  Control {
    sample: u64,
    track: TrackId,
    name: &'static str,
    value: f32
  },
  SectionChange {
    sample: u64,
    section: SectionKind,
    bar: u64
  },
  Bar {
    sample: u64,
    bar: u64
  },
  Beat {
    sample: u64,
    bar: u64,
    beat: u8
  }
}
