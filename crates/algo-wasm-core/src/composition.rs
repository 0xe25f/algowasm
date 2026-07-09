use crate::profile::Profile;
use crate::rng::Rng64;
use crate::types::{SectionKind, TrackId};

/// Steps per bar used by the first AlgoWASM generator.
pub const STEPS_PER_BAR: usize = 16;

/// Bars held in a generated section pattern.
pub const PATTERN_BARS: usize = 4;

/// A short scheduled note on the step grid.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NoteStep {
  pub note: i16,
  pub velocity: f32,
  pub length_steps: u8,
  pub accent: bool,
  pub slide: bool
}

impl NoteStep {
  /// Creates a note step.
  pub const fn new(note: i16, velocity: f32, length_steps: u8, accent: bool, slide: bool) -> Self {
    Self {
      note,
      velocity,
      length_steps,
      accent,
      slide
    }
  }
}

/// A single bar of generated material.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BarPattern {
  pub kick: [bool; STEPS_PER_BAR],
  pub snare: [bool; STEPS_PER_BAR],
  pub closed_hat: [f32; STEPS_PER_BAR],
  pub open_hat: [f32; STEPS_PER_BAR],
  pub percussion: [Option<NoteStep>; STEPS_PER_BAR],
  pub bass: [Option<NoteStep>; STEPS_PER_BAR],
  pub chords: [Option<NoteStep>; STEPS_PER_BAR],
  pub pad: [Option<NoteStep>; STEPS_PER_BAR],
  pub lead: [Option<NoteStep>; STEPS_PER_BAR],
  pub arp: [Option<NoteStep>; STEPS_PER_BAR],
  pub fx: [Option<NoteStep>; STEPS_PER_BAR]
}

impl BarPattern {
  /// Creates an empty bar pattern.
  pub const fn empty() -> Self {
    Self {
      kick: [false; STEPS_PER_BAR],
      snare: [false; STEPS_PER_BAR],
      closed_hat: [0.0; STEPS_PER_BAR],
      open_hat: [0.0; STEPS_PER_BAR],
      percussion: [None; STEPS_PER_BAR],
      bass: [None; STEPS_PER_BAR],
      chords: [None; STEPS_PER_BAR],
      pad: [None; STEPS_PER_BAR],
      lead: [None; STEPS_PER_BAR],
      arp: [None; STEPS_PER_BAR],
      fx: [None; STEPS_PER_BAR]
    }
  }
}

/// A generated chord in MIDI note numbers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Chord {
  pub root_note: i16,
  pub notes: [i16; 4],
  pub count: usize
}

impl Chord {
  /// Returns a safe chord tone.
  pub fn tone(&self, index: usize) -> i16 {
    self.notes[index % self.count.max(1)]
  }
}

/// A generated rolling section.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PatternBank {
  pub section: SectionKind,
  pub length_bars: u8,
  pub key_root: i16,
  pub scale: ScaleKind,
  pub chords: [Chord; PATTERN_BARS],
  pub bars: [BarPattern; PATTERN_BARS],
  pub energy: f32
}

impl PatternBank {
  /// Generates deterministic material for the next section.
  pub fn generate(
    rng: &mut Rng64,
    profile: &Profile,
    previous: Option<&PatternBank>,
    section: SectionKind,
    energy: f32
  ) -> Self {
    let key = ScalePicker::pick(rng, profile);
    let chords = ChordGrammar::progression(rng, key);
    let mut bars = [BarPattern::empty(); PATTERN_BARS];

    for index in 0..PATTERN_BARS {
      bars[index] = RhythmGrammar::bar(rng, profile, section, energy, index);
      BasslineGenerator::apply(rng, profile, key, chords[index], energy, &mut bars[index]);
      MelodyGenerator::apply(rng, profile, key, chords[index], previous, energy, &mut bars[index]);
      ArpGenerator::apply(rng, chords[index], energy, &mut bars[index]);
      ChordStabGenerator::apply(rng, chords[index], section, energy, &mut bars[index]);
      AutomationGenerator::apply_fx(rng, section, energy, &mut bars[index]);
    }

    if let Some(previous) = previous {
      MutationEngine::recall(previous, rng, energy, &mut bars);
    }

    let min_bars = profile.arrangement_rules.min_section_bars;
    let max_bars = profile.arrangement_rules.max_section_bars;
    let span = max_bars.saturating_sub(min_bars).saturating_add(1);
    let length_bars = min_bars.saturating_add(rng.range_usize(usize::from(span)) as u8);

    Self {
      section,
      length_bars,
      key_root: key.root,
      scale: key.scale,
      chords,
      bars,
      energy
    }
  }
}

/// Scale choices used by the built-in profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScaleKind {
  NaturalMinor,
  Dorian,
  Aeolian,
  PhrygianDominant,
  MinorPentatonic
}

impl ScaleKind {
  /// Converts a profile scale id into a scale.
  pub fn from_profile_id(value: &str) -> Self {
    match value {
      "dorian" => ScaleKind::Dorian,
      "aeolian" => ScaleKind::Aeolian,
      "phrygian_dominant" => ScaleKind::PhrygianDominant,
      "minor_pentatonic" => ScaleKind::MinorPentatonic,
      _ => ScaleKind::NaturalMinor
    }
  }

  /// Returns the semitone intervals in one octave.
  pub const fn intervals(self) -> &'static [i16] {
    match self {
      ScaleKind::NaturalMinor => &[0, 2, 3, 5, 7, 8, 10],
      ScaleKind::Dorian => &[0, 2, 3, 5, 7, 9, 10],
      ScaleKind::Aeolian => &[0, 2, 3, 5, 7, 8, 10],
      ScaleKind::PhrygianDominant => &[0, 1, 4, 5, 7, 8, 10],
      ScaleKind::MinorPentatonic => &[0, 3, 5, 7, 10]
    }
  }

  /// Quantises a chromatic offset to the nearest scale tone at or below it.
  pub fn quantise(self, root: i16, chromatic: i16) -> i16 {
    let octave = chromatic.div_euclid(12);
    let degree = chromatic.rem_euclid(12);
    let mut selected = 0;

    for interval in self.intervals() {
      if *interval <= degree {
        selected = *interval;
      }
    }

    root + octave * 12 + selected
  }
}

/// A selected key.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Key {
  pub root: i16,
  pub scale: ScaleKind
}

/// Chooses a key from profile scale weights.
pub struct ScalePicker;

impl ScalePicker {
  /// Picks a root and scale.
  pub fn pick(rng: &mut Rng64, profile: &Profile) -> Key {
    let weights: Vec<u32> = profile.scales.iter().map(|scale| scale.weight).collect();
    let scale_index = rng.weighted_index(&weights);
    let scale_name = profile
      .scales
      .get(scale_index)
      .map(|scale| scale.name.as_str())
      .unwrap_or("natural_minor");
    let roots = [36, 38, 39, 41, 43, 45, 46, 48];

    Key {
      root: roots[rng.range_usize(roots.len())],
      scale: ScaleKind::from_profile_id(scale_name)
    }
  }
}

/// Generates reliable minor electronic chord progressions.
pub struct ChordGrammar;

impl ChordGrammar {
  /// Generates a four-bar progression.
  pub fn progression(rng: &mut Rng64, key: Key) -> [Chord; PATTERN_BARS] {
    const PROGRESSIONS: [[i16; 4]; 5] = [
      [0, 8, 10, 0],
      [0, 5, 10, 8],
      [0, 10, 8, 10],
      [0, 7, 8, 5],
      [0, 5, 0, 10]
    ];

    let progression = PROGRESSIONS[rng.range_usize(PROGRESSIONS.len())];
    [
      Self::chord_for(key, progression[0], rng.chance(0.2)),
      Self::chord_for(key, progression[1], rng.chance(0.18)),
      Self::chord_for(key, progression[2], rng.chance(0.18)),
      Self::chord_for(key, progression[3], rng.chance(0.24))
    ]
  }

  fn chord_for(key: Key, offset: i16, suspended: bool) -> Chord {
    let root = key.root + offset + 24;
    let third = if suspended { 5 } else { 3 };
    let notes = [root, root + third, root + 7, root + 10];

    Chord {
      root_note: root,
      notes,
      count: 4
    }
  }
}

/// Generates coordinated drum and percussion patterns.
pub struct RhythmGrammar;

impl RhythmGrammar {
  /// Creates one bar of rhythm.
  pub fn bar(
    rng: &mut Rng64,
    profile: &Profile,
    section: SectionKind,
    energy: f32,
    bar_index: usize
  ) -> BarPattern {
    let mut pattern = BarPattern::empty();
    let section_density = density_for_section(section) * (0.55 + energy * 0.65);

    for step in [0, 4, 8, 12] {
      pattern.kick[step] = rng.chance(profile.rhythm_rules.kick_density);
    }

    pattern.snare[4] = section != SectionKind::Intro || bar_index > 0;
    pattern.snare[12] = section != SectionKind::Intro;

    let hat_pattern = euclidean(7 + (energy * 5.0) as usize, STEPS_PER_BAR);

    for (step, hat_active) in hat_pattern.iter().enumerate().take(STEPS_PER_BAR) {
      if step % 2 == 1 || *hat_active {
        let chance = profile.rhythm_rules.hat_density * section_density;
        pattern.closed_hat[step] = if rng.chance(chance) {
          0.34 + rng.next_f32() * 0.38
        } else {
          0.0
        };
      }

      if step == 6 || step == 14 {
        pattern.open_hat[step] = if rng.chance(0.28 + energy * 0.35) { 0.55 } else { 0.0 };
      }

      if rng.chance(profile.rhythm_rules.percussion_density * section_density * 0.45) {
        let note = 62 + rng.range_usize(8) as i16;
        pattern.percussion[step] = Some(NoteStep::new(note, 0.3 + energy * 0.3, 1, false, false));
      }
    }

    if bar_index == PATTERN_BARS - 1 && rng.chance(profile.rhythm_rules.fill_chance) {
      for step in 12..STEPS_PER_BAR {
        pattern.snare[step] = step % 2 == 0;
        pattern.closed_hat[step] = 0.65;
      }
    }

    pattern
  }
}

/// Returns a Euclidean rhythm as a fixed 16-step pattern.
pub fn euclidean(pulses: usize, steps: usize) -> [bool; STEPS_PER_BAR] {
  let mut result = [false; STEPS_PER_BAR];

  if pulses == 0 || steps == 0 {
    return result;
  }

  let usable_steps = steps.min(STEPS_PER_BAR);

  for (step, value) in result.iter_mut().enumerate().take(usable_steps) {
    *value = (step * pulses) % usable_steps < pulses;
  }

  result
}

/// Generates monophonic acid-inspired basslines.
pub struct BasslineGenerator;

impl BasslineGenerator {
  /// Applies a bassline to an existing bar.
  pub fn apply(
    rng: &mut Rng64,
    profile: &Profile,
    key: Key,
    chord: Chord,
    energy: f32,
    bar: &mut BarPattern
  ) {
    for step in 0..STEPS_PER_BAR {
      let strong = step % 4 == 0;
      let offbeat = step % 4 == 2;
      let density = profile.bass_rules.density * (0.65 + energy * 0.7);

      if strong || (offbeat && rng.chance(density)) || rng.chance(density * 0.18) {
        let base = if rng.chance(0.72) {
          chord.root_note - 24
        } else {
          chord.tone(rng.range_usize(3)) - 24
        };
        let chromatic = base - key.root;
        let note = key.scale.quantise(key.root, chromatic)
          + if rng.chance(profile.bass_rules.octave_jump_chance) { 12 } else { 0 };
        let accent = rng.chance(profile.bass_rules.accent_chance);
        let slide = rng.chance(profile.bass_rules.slide_chance);
        let velocity = if accent { 0.88 } else { 0.58 + energy * 0.24 };

        bar.bass[step] = Some(NoteStep::new(note, velocity, if slide { 3 } else { 2 }, accent, slide));
      }
    }
  }
}

/// Generates lead motifs with phrase memory.
pub struct MelodyGenerator;

impl MelodyGenerator {
  /// Applies lead material to an existing bar.
  pub fn apply(
    rng: &mut Rng64,
    profile: &Profile,
    key: Key,
    chord: Chord,
    previous: Option<&PatternBank>,
    energy: f32,
    bar: &mut BarPattern
  ) {
    let recall = previous.and_then(|bank| {
      if rng.chance(0.42) {
        Some(bank.bars[rng.range_usize(PATTERN_BARS)].lead)
      } else {
        None
      }
    });

    if let Some(recalled) = recall {
      for (step, note) in recalled.iter().enumerate() {
        if let Some(note) = note {
          if rng.chance(0.72) {
            let transpose = if rng.chance(0.35) { 12 } else { 0 };
            bar.lead[step] = Some(NoteStep::new(
              note.note + transpose,
              (note.velocity * 0.92).clamp(0.2, 0.9),
              note.length_steps,
              note.accent,
              note.slide
            ));
          }
        }
      }

      return;
    }

    let scale = key.scale.intervals();
    let density = profile.melody_rules.density * (0.45 + energy * 0.95);

    for step in 0..STEPS_PER_BAR {
      let phrase_anchor = step == 2 || step == 10;

      if phrase_anchor || rng.chance(density * if step % 2 == 0 { 0.5 } else { 0.2 }) {
        let note = if phrase_anchor || rng.chance(0.55) {
          chord.tone(rng.range_usize(4)) + 12
        } else {
          key.root + 36 + scale[rng.range_usize(scale.len())]
        };
        let clamped = note.clamp(key.root + 24, key.root + 24 + profile.melody_rules.max_range_semitones);
        bar.lead[step] = Some(NoteStep::new(clamped, 0.38 + energy * 0.28, 2, phrase_anchor, false));
      }
    }
  }
}

/// Generates arpeggios constrained by chord tones.
pub struct ArpGenerator;

impl ArpGenerator {
  /// Applies arpeggio material.
  pub fn apply(rng: &mut Rng64, chord: Chord, energy: f32, bar: &mut BarPattern) {
    if energy < 0.42 {
      return;
    }

    for step in (1..STEPS_PER_BAR).step_by(2) {
      if rng.chance(energy * 0.45) {
        let tone = chord.tone(step / 2) + 12;
        bar.arp[step] = Some(NoteStep::new(tone, 0.24 + energy * 0.18, 1, false, false));
      }
    }
  }
}

/// Generates stabs and pad roots.
pub struct ChordStabGenerator;

impl ChordStabGenerator {
  /// Applies chord support to a bar.
  pub fn apply(
    rng: &mut Rng64,
    chord: Chord,
    section: SectionKind,
    energy: f32,
    bar: &mut BarPattern
  ) {
    if section == SectionKind::Breakdown || section == SectionKind::Intro {
      bar.pad[0] = Some(NoteStep::new(chord.root_note, 0.24 + energy * 0.18, 12, false, false));
    }

    for step in [3, 7, 11, 15] {
      if rng.chance(0.35 + energy * 0.32) {
        bar.chords[step] = Some(NoteStep::new(chord.root_note, 0.34 + energy * 0.2, 2, true, false));
      }
    }
  }
}

/// Mutates and recalls previous material.
pub struct MutationEngine;

impl MutationEngine {
  /// Recalls previous phrase identity into the new section.
  pub fn recall(previous: &PatternBank, rng: &mut Rng64, energy: f32, bars: &mut [BarPattern; 4]) {
    if rng.chance(0.55) {
      bars[0].lead = previous.bars[2].lead;
      bars[2].lead = previous.bars[0].lead;
    }

    if energy > 0.5 && rng.chance(0.4) {
      for bar in bars {
        for step in [5, 13] {
          if bar.closed_hat[step] > 0.0 {
            bar.closed_hat[step] = (bar.closed_hat[step] + 0.18).clamp(0.0, 1.0);
          }
        }
      }
    }
  }
}

/// Generates slow movement hooks.
pub struct AutomationGenerator;

impl AutomationGenerator {
  /// Adds sparse effects hits.
  pub fn apply_fx(rng: &mut Rng64, section: SectionKind, energy: f32, bar: &mut BarPattern) {
    if matches!(section, SectionKind::Transition | SectionKind::Build | SectionKind::Fill)
      && rng.chance(0.5 + energy * 0.3)
    {
      bar.fx[15] = Some(NoteStep::new(72, 0.32 + energy * 0.24, 1, false, false));
    }
  }
}

/// Generates the next arrangement section.
pub struct ArrangementGenerator;

impl ArrangementGenerator {
  /// Chooses the next section from the current one and energy.
  pub fn next(rng: &mut Rng64, current: SectionKind, energy: f32, reset_chance: f32) -> SectionKind {
    if rng.chance(reset_chance) {
      return SectionKind::Reset;
    }

    match current {
      SectionKind::Intro => SectionKind::GrooveA,
      SectionKind::GrooveA => {
        if energy > 0.65 {
          SectionKind::Build
        } else {
          SectionKind::GrooveB
        }
      }
      SectionKind::GrooveB => {
        if energy > 0.55 {
          SectionKind::Drop
        } else {
          SectionKind::Variation
        }
      }
      SectionKind::Build => SectionKind::Drop,
      SectionKind::Drop => {
        if rng.chance(0.38) {
          SectionKind::Fill
        } else {
          SectionKind::Variation
        }
      }
      SectionKind::Variation => SectionKind::Breakdown,
      SectionKind::Breakdown => SectionKind::Transition,
      SectionKind::Fill => SectionKind::GrooveA,
      SectionKind::Transition => SectionKind::Drop,
      SectionKind::Reset => SectionKind::Intro
    }
  }
}

fn density_for_section(section: SectionKind) -> f32 {
  match section {
    SectionKind::Intro => 0.45,
    SectionKind::GrooveA => 0.78,
    SectionKind::GrooveB => 0.86,
    SectionKind::Build => 0.92,
    SectionKind::Drop => 1.0,
    SectionKind::Variation => 0.82,
    SectionKind::Breakdown => 0.42,
    SectionKind::Fill => 0.9,
    SectionKind::Transition => 0.68,
    SectionKind::Reset => 0.35
  }
}

/// Returns the preferred synth instrument for a track.
pub fn instrument_note_for_track(track: TrackId, note: NoteStep) -> NoteStep {
  match track {
    TrackId::Kick => NoteStep::new(36, note.velocity, 2, note.accent, false),
    TrackId::Snare => NoteStep::new(38, note.velocity, 2, note.accent, false),
    TrackId::ClosedHat => NoteStep::new(42, note.velocity, 1, false, false),
    TrackId::OpenHat => NoteStep::new(46, note.velocity, 4, false, false),
    _ => note
  }
}

#[cfg(test)]
mod tests {
  use super::{euclidean, BarPattern, PatternBank, RhythmGrammar, STEPS_PER_BAR};
  use crate::profile::amiga_house_95ish;
  use crate::rng::Rng64;
  use crate::types::SectionKind;

  #[test]
  fn euclidean_pattern_fits_bar_length() {
    let pattern = euclidean(5, STEPS_PER_BAR);
    assert_eq!(pattern.len(), STEPS_PER_BAR);
    assert_eq!(pattern.iter().filter(|value| **value).count(), 5);
  }

  #[test]
  fn rhythm_generation_fits_bar_length() {
    let mut rng = Rng64::new(7);
    let profile = amiga_house_95ish();
    let bar = RhythmGrammar::bar(&mut rng, &profile, SectionKind::Drop, 0.8, 0);
    assert_eq!(bar.kick.len(), STEPS_PER_BAR);
    assert!(bar.kick[0]);
  }

  #[test]
  fn same_seed_generates_same_pattern() {
    let profile = amiga_house_95ish();
    let mut left_rng = Rng64::new(99);
    let mut right_rng = Rng64::new(99);
    let left = PatternBank::generate(&mut left_rng, &profile, None, SectionKind::Intro, 0.5);
    let right = PatternBank::generate(&mut right_rng, &profile, None, SectionKind::Intro, 0.5);

    assert_eq!(left, right);
  }

  #[test]
  fn empty_bar_has_no_notes() {
    let bar = BarPattern::empty();
    assert!(bar.bass.iter().all(Option::is_none));
  }
}
