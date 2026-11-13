/// Stable track count used by the built-in mixer and FFI boundary.
pub const TRACK_COUNT: usize = 11;

/// Stable public track identifiers.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
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

  /// Returns the track for a compact mixer index, as used on the WASM boundary.
  pub fn from_index(index: usize) -> Option<Self> {
    Self::ALL.get(index).copied()
  }
}

/// Sections used by the rolling endless arrangement.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
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
  /// Returns the stable numeric identifier used on the WASM boundary.
  pub const fn as_index(self) -> u8 {
    match self {
      SectionKind::Intro => 0,
      SectionKind::GrooveA => 1,
      SectionKind::GrooveB => 2,
      SectionKind::Build => 3,
      SectionKind::Drop => 4,
      SectionKind::Variation => 5,
      SectionKind::Breakdown => 6,
      SectionKind::Fill => 7,
      SectionKind::Transition => 8,
      SectionKind::Reset => 9
    }
  }

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
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Mood {
  Dark,
  Bright,
  Tense,
  Calm
}

impl Mood {
  /// Returns the mood for its numeric identifier on the WASM boundary.
  pub const fn from_index(index: u32) -> Option<Self> {
    match index {
      0 => Some(Mood::Dark),
      1 => Some(Mood::Bright),
      2 => Some(Mood::Tense),
      3 => Some(Mood::Calm),
      _ => None
    }
  }
}

/// Oscillator shape used by procedural patches.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WaveShape {
  Sine,
  Triangle,
  Saw,
  Square,
  Pulse,
  Noise
}

impl WaveShape {
  /// Returns the wave shape for its numeric identifier on the WASM boundary.
  pub const fn from_index(index: u32) -> Option<Self> {
    match index {
      0 => Some(WaveShape::Sine),
      1 => Some(WaveShape::Triangle),
      2 => Some(WaveShape::Saw),
      3 => Some(WaveShape::Square),
      4 => Some(WaveShape::Pulse),
      5 => Some(WaveShape::Noise),
      _ => None
    }
  }
}

/// Number of `f64` values each event occupies in the binary event buffer.
pub const EVENT_WIRE_SIZE: usize = 5;

/// A compact event emitted by the deterministic scheduler.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MusicEvent {
  NoteOn {
    sample: u64,
    track: TrackId,
    note: i16,
    velocity: f32
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

impl MusicEvent {
  /// Encodes the event as `[kind, sample, a, b, c]`.
  ///
  /// The kinds are 0 for note-on (track, note, velocity), 3 for section
  /// change (section, bar), 4 for bar (bar), and 5 for beat (bar, beat).
  /// Kinds 1 and 2 are reserved for note-off and control events. JavaScript
  /// decodes this layout, so it must stay in step with the package's decoder.
  pub fn to_wire(self) -> [f64; EVENT_WIRE_SIZE] {
    match self {
      MusicEvent::NoteOn {
        sample,
        track,
        note,
        velocity
      } => [0.0, sample as f64, track.as_index() as f64, f64::from(note), f64::from(velocity)],
      MusicEvent::SectionChange {
        sample,
        section,
        bar
      } => [3.0, sample as f64, f64::from(section.as_index()), bar as f64, 0.0],
      MusicEvent::Bar {
        sample,
        bar
      } => [4.0, sample as f64, bar as f64, 0.0, 0.0],
      MusicEvent::Beat {
        sample,
        bar,
        beat
      } => [5.0, sample as f64, bar as f64, f64::from(beat), 0.0]
    }
  }
}
