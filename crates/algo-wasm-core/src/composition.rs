use crate::genome::{has_step, rotate_steps, ArpShape, BassLine, FillStyle, Motif, SongGenome};
use crate::profile::{ChordRules, Profile};
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
///
/// Drum lanes that carry no pitch store a velocity, where `0.0` means silent.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BarPattern {
  pub kick: [bool; STEPS_PER_BAR],
  pub snare: [f32; STEPS_PER_BAR],
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
      snare: [0.0; STEPS_PER_BAR],
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

/// A generated, voiced chord in MIDI note numbers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Chord {
  /// The chord root in the chord octave, before inversion.
  pub root_note: i16,
  /// The chord root as a degree of the key's seven-note harmony scale.
  pub degree: u8,
  /// Voiced chord tones, lowest first.
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
///
/// A section plays an A phrase of four bars, then a B variation of the same
/// four bars, and repeats. The final bar of the section may be swapped for a
/// fill.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PatternBank {
  pub section: SectionKind,
  pub length_bars: u8,
  pub key_root: i16,
  pub scale: ScaleKind,
  pub chords: [Chord; PATTERN_BARS],
  pub bars: [BarPattern; PATTERN_BARS],
  pub bars_b: [BarPattern; PATTERN_BARS],
  pub fill_bar: Option<BarPattern>,
  pub energy: f32
}

impl PatternBank {
  /// Returns the bar to play at a bar offset inside the section.
  pub fn bar_at(&self, section_bar: u8) -> &BarPattern {
    if section_bar.saturating_add(1) == self.length_bars {
      if let Some(fill) = &self.fill_bar {
        return fill;
      }
    }

    let slot = usize::from(section_bar) % (PATTERN_BARS * 2);

    if slot >= PATTERN_BARS {
      &self.bars_b[slot - PATTERN_BARS]
    } else {
      &self.bars[slot]
    }
  }

  /// Returns the chord to play at a bar offset inside the section.
  pub fn chord_at(&self, section_bar: u8) -> Chord {
    self.chords[usize::from(section_bar) % PATTERN_BARS]
  }

  /// Generates deterministic material for the next section.
  pub fn generate(
    rng: &mut Rng64,
    profile: &Profile,
    genome: &SongGenome,
    previous: Option<&PatternBank>,
    section: SectionKind,
    key: Key,
    energy: f32
  ) -> Self {
    let progression = ChordGrammar::progression_for_section(rng, profile, genome, section);
    let previous_chord = previous.map(|bank| bank.chords[PATTERN_BARS - 1]);
    let chords =
      ChordGrammar::voice(rng, profile.chord_rules, genome, key, progression, previous_chord);
    let layers = Layers::for_section(section, genome, energy);
    let mutation = profile.melody_rules.mutation_chance;

    let bass_a = genome.bass_line.for_section(rng, profile, energy, mutation);
    let bass_b = bass_a.mutated(rng, mutation * 1.5, profile.bass_rules.octave_jump_chance);
    let (lead_a, lead_b) = MelodyGenerator::phrases(rng, profile, genome, section, energy);

    let mut bars = [BarPattern::empty(); PATTERN_BARS];
    let mut bars_b = [BarPattern::empty(); PATTERN_BARS];

    for index in 0..PATTERN_BARS {
      for (variation, target) in [(false, &mut bars[index]), (true, &mut bars_b[index])] {
        let context = BarContext {
          profile,
          genome,
          key,
          chord: chords[index],
          section,
          energy,
          bar_index: index,
          variation,
          layers
        };
        *target = RhythmGrammar::bar(rng, &context);

        if layers.bass && !(section == SectionKind::Intro && index < 2) {
          let line = if variation { &bass_b } else { &bass_a };
          BasslineGenerator::apply(rng, &context, line, target);
        }

        if layers.lead {
          let phrase = if variation { &lead_b } else { &lead_a };
          MelodyGenerator::apply(&context, &phrase[index], target);
        }

        if layers.arp {
          ArpGenerator::apply(rng, &context, target);
        }

        ChordStabGenerator::apply(rng, &context, target);
        AutomationGenerator::apply_fx(rng, &context, target);
      }
    }

    if let Some(previous) = previous {
      MutationEngine::recall(previous, rng, energy, &mut bars);
    }

    let length_bars = section_length(rng, profile);
    let fill_bar =
      FillGenerator::fill_for(rng, profile, genome, section, energy, length_bars, &bars, &bars_b);

    Self {
      section,
      length_bars,
      key_root: key.root,
      scale: key.scale,
      chords,
      bars,
      bars_b,
      fill_bar,
      energy
    }
  }
}

/// Picks a section length, preferring whole four-bar phrases.
fn section_length(rng: &mut Rng64, profile: &Profile) -> u8 {
  let min_bars = profile.arrangement_rules.min_section_bars;
  let max_bars = profile.arrangement_rules.max_section_bars;
  let phrase_lengths: Vec<u8> = (min_bars..=max_bars).filter(|bars| bars % 4 == 0).collect();

  if !phrase_lengths.is_empty() {
    return phrase_lengths[rng.range_usize(phrase_lengths.len())];
  }

  let span = max_bars.saturating_sub(min_bars).saturating_add(1);
  min_bars.saturating_add(rng.range_usize(usize::from(span)) as u8)
}

/// Shared inputs for generating one bar.
struct BarContext<'a> {
  profile: &'a Profile,
  genome: &'a SongGenome,
  key: Key,
  chord: Chord,
  section: SectionKind,
  energy: f32,
  bar_index: usize,
  variation: bool,
  layers: Layers
}

/// Which parts play in a section. Bringing layers in and out is what gives
/// an arrangement its shape.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layers {
  pub kick: bool,
  pub bass: bool,
  pub lead: bool,
  pub arp: bool,
  pub stabs: bool,
  pub pad: bool,
  pub percussion: bool
}

impl Layers {
  /// Returns the layers for a section.
  pub fn for_section(section: SectionKind, genome: &SongGenome, energy: f32) -> Self {
    let arp_energy = energy >= 0.3;

    match section {
      SectionKind::Intro => Self {
        kick: true,
        bass: true,
        lead: false,
        arp: false,
        stabs: genome.intro_stabs,
        pad: true,
        percussion: true
      },
      SectionKind::GrooveA => Self {
        kick: true,
        bass: true,
        lead: genome.lead_in_groove_a,
        arp: !genome.lead_in_groove_a && arp_energy,
        stabs: true,
        pad: genome.pad_bed,
        percussion: true
      },
      SectionKind::GrooveB => Self {
        kick: true,
        bass: true,
        lead: true,
        arp: arp_energy && energy > 0.55,
        stabs: true,
        pad: genome.pad_bed,
        percussion: true
      },
      SectionKind::Build => Self {
        kick: true,
        bass: true,
        lead: false,
        arp: arp_energy,
        stabs: true,
        pad: genome.pad_bed,
        percussion: true
      },
      SectionKind::Drop | SectionKind::Fill => Self {
        kick: true,
        bass: true,
        lead: true,
        arp: arp_energy,
        stabs: true,
        pad: genome.pad_bed,
        percussion: true
      },
      SectionKind::Variation => Self {
        kick: true,
        bass: true,
        lead: false,
        arp: true,
        stabs: true,
        pad: true,
        percussion: true
      },
      SectionKind::Breakdown => Self {
        kick: false,
        bass: genome.breakdown_bass,
        lead: true,
        arp: false,
        stabs: false,
        pad: true,
        percussion: false
      },
      SectionKind::Transition => Self {
        kick: true,
        bass: true,
        lead: false,
        arp: arp_energy,
        stabs: true,
        pad: true,
        percussion: true
      },
      SectionKind::Reset => Self {
        kick: true,
        bass: false,
        lead: false,
        arp: false,
        stabs: false,
        pad: true,
        percussion: true
      }
    }
  }
}

/// Scale choices used by the built-in profile.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ScaleKind {
  #[default]
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

  /// Returns the seven-note scale that chords are built from.
  ///
  /// The pentatonic scale borrows its natural minor parent so that chords can
  /// still be stacked in thirds.
  pub const fn harmony_intervals(self) -> &'static [i16] {
    match self {
      ScaleKind::MinorPentatonic => ScaleKind::NaturalMinor.intervals(),
      _ => self.intervals()
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

  /// Returns true when a note belongs to this scale for the given root.
  pub fn contains(self, root: i16, note: i16) -> bool {
    self.intervals().contains(&(note - root).rem_euclid(12))
  }
}

/// A selected key.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Key {
  pub root: i16,
  pub scale: ScaleKind
}

impl Key {
  /// Returns the semitone offset of a harmony-scale degree from the key root.
  ///
  /// Degrees past seven continue into the next octave, and negative degrees
  /// fall into the octave below.
  pub fn degree_offset(self, degree: i16) -> i16 {
    let scale = self.scale.harmony_intervals();
    let len = scale.len() as i16;
    scale[degree.rem_euclid(len) as usize] + degree.div_euclid(len) * 12
  }

  /// Returns a note for a harmony degree, quantised to the melodic scale.
  pub fn melodic_note(self, degree: i16, base: i16) -> i16 {
    let chromatic = self.degree_offset(degree) + base - self.root;
    self.scale.quantise(self.root, chromatic)
  }
}

/// Chooses a key from profile scale weights.
pub struct ScalePicker;

impl ScalePicker {
  /// Lowest root note a key may use.
  pub const LOWEST_ROOT: i16 = 36;

  /// Picks a root and scale.
  pub fn pick(rng: &mut Rng64, profile: &Profile) -> Key {
    let weights: Vec<u32> = profile.scales.iter().map(|scale| scale.weight).collect();
    let scale_index = rng.weighted_index(&weights);
    let scale_name = profile
      .scales
      .get(scale_index)
      .map(|scale| scale.name.as_str())
      .unwrap_or("natural_minor");

    Key {
      root: Self::LOWEST_ROOT + rng.range_usize(12) as i16,
      scale: ScaleKind::from_profile_id(scale_name)
    }
  }
}

/// Chord progressions as harmony-scale degrees (0 is the tonic).
pub const PROGRESSIONS: [[u8; 4]; 16] = [
  [0, 5, 6, 0],
  [0, 3, 6, 5],
  [0, 6, 5, 6],
  [0, 4, 5, 3],
  [0, 3, 0, 6],
  [0, 2, 5, 6],
  [5, 6, 0, 0],
  [3, 4, 0, 0],
  [0, 0, 3, 3],
  [0, 5, 3, 4],
  [5, 3, 0, 6],
  [0, 6, 3, 5],
  [2, 5, 0, 6],
  [0, 3, 5, 4],
  [0, 0, 5, 6],
  [3, 5, 6, 0]
];

/// Generates chord progressions and voicings.
pub struct ChordGrammar;

impl ChordGrammar {
  /// Picks a progression, weighted by the profile's chord rules.
  ///
  /// `cadenceWeight` favours progressions that resolve to, or lean strongly
  /// back towards, the tonic. `movementWeight` favours progressions with more
  /// distinct chords.
  pub fn pick_progression(rng: &mut Rng64, rules: ChordRules) -> [u8; 4] {
    let weights: Vec<u32> = PROGRESSIONS
      .iter()
      .map(|progression| {
        let last = progression[3];
        let cadence = match last {
          0 => 1.0,
          4 | 6 => 0.7,
          5 => 0.4,
          _ => 0.2
        };
        let mut distinct = 0;

        for (index, degree) in progression.iter().enumerate() {
          if !progression[..index].contains(degree) {
            distinct += 1;
          }
        }

        let movement = (distinct as f32 - 1.0) / 3.0;
        let score = 0.4 + rules.cadence_weight * cadence + rules.movement_weight * movement;
        (score * 100.0).max(1.0) as u32
      })
      .collect();

    PROGRESSIONS[rng.weighted_index(&weights)]
  }

  /// Chooses the progression for a section, reusing the song's own
  /// progressions most of the time so that songs keep their identity.
  pub fn progression_for_section(
    rng: &mut Rng64,
    profile: &Profile,
    genome: &SongGenome,
    section: SectionKind
  ) -> [u8; 4] {
    match section {
      SectionKind::GrooveA | SectionKind::Drop | SectionKind::Fill | SectionKind::Intro => {
        if rng.chance(0.85) {
          genome.signature_progression
        } else {
          genome.alt_progression
        }
      }
      SectionKind::Breakdown | SectionKind::Variation => {
        if rng.chance(0.3) {
          Self::pick_progression(rng, profile.chord_rules)
        } else {
          genome.alt_progression
        }
      }
      _ => {
        if rng.chance(profile.chord_rules.movement_weight * 0.5) {
          Self::pick_progression(rng, profile.chord_rules)
        } else {
          genome.signature_progression
        }
      }
    }
  }

  /// Builds and voices four chords, leading smoothly from the previous chord.
  pub fn voice(
    rng: &mut Rng64,
    rules: ChordRules,
    genome: &SongGenome,
    key: Key,
    progression: [u8; 4],
    previous: Option<Chord>
  ) -> [Chord; PATTERN_BARS] {
    let mut chords = [Chord {
      root_note: 0,
      degree: 0,
      notes: [0; 4],
      count: 1
    }; PATTERN_BARS];
    let mut last = previous;

    for (index, degree) in progression.iter().enumerate() {
      let suspended = rng.chance(rules.suspended_chance);
      let borrowed = *degree != 0 && rng.chance(rules.bright_borrow_chance);
      let chord = Self::chord_for(key, *degree, genome.seventh_chords, suspended, borrowed, last);
      chords[index] = chord;
      last = Some(chord);
    }

    chords
  }

  /// Builds a chord by stacking thirds from the key's harmony scale.
  pub fn chord_for(
    key: Key,
    degree: u8,
    seventh: bool,
    suspended: bool,
    borrowed: bool,
    previous: Option<Chord>
  ) -> Chord {
    let base = key.root + 24;
    let degree = i16::from(degree);
    let root = base + key.degree_offset(degree);
    let mut third = base + key.degree_offset(degree + if suspended { 3 } else { 2 });

    if borrowed && !suspended && third - root == 3 {
      third += 1;
    }

    let fifth = base + key.degree_offset(degree + 4);
    let seventh_note = base + key.degree_offset(degree + 6);
    let count = if seventh { 4 } else { 3 };
    let stacked = [root, third, fifth, seventh_note];
    let notes = Self::lead_voices(stacked, count, previous);

    Chord {
      root_note: root,
      degree: degree as u8,
      notes,
      count
    }
  }

  /// Picks the inversion that moves least from the previous chord, while
  /// keeping the voicing near the middle of the keyboard.
  fn lead_voices(stacked: [i16; 4], count: usize, previous: Option<Chord>) -> [i16; 4] {
    const CENTRE: f32 = 64.0;
    let mut best = stacked;
    let mut best_cost = f32::MAX;

    for inversion in 0..count {
      for shift in [-12, 0] {
        let mut candidate = stacked;

        for (index, note) in candidate.iter_mut().enumerate().take(count) {
          *note += shift + if index < inversion { 12 } else { 0 };
        }

        candidate[..count].sort_unstable();

        let sum: f32 = candidate[..count].iter().map(|note| f32::from(*note)).sum();
        let mean = sum / count as f32;
        let mut cost = (mean - CENTRE).abs() * 0.6;

        if let Some(previous) = previous {
          for (index, note) in candidate.iter().enumerate().take(count) {
            cost += f32::from((note - previous.tone(index)).abs());
          }
        }

        if cost < best_cost {
          best_cost = cost;
          best = candidate;
        }
      }
    }

    for index in count..4 {
      best[index] = best[index % count.max(1)];
    }

    best
  }
}

/// Generates coordinated drum and percussion patterns.
pub struct RhythmGrammar;

impl RhythmGrammar {
  fn bar(rng: &mut Rng64, context: &BarContext<'_>) -> BarPattern {
    let profile = context.profile;
    let groove = &context.genome.groove;
    let section = context.section;
    let energy = context.energy;
    let bar_index = context.bar_index;
    let mut pattern = BarPattern::empty();
    let section_density = density_for_section(section) * (0.55 + energy * 0.65);

    if context.layers.kick {
      for step in 0..STEPS_PER_BAR {
        if has_step(groove.kick, step) {
          pattern.kick[step] =
            step == 0 || groove.four_on_floor || rng.chance(profile.rhythm_rules.kick_density);
        }
      }

      if let Some(pickup) = groove.pickup_kick {
        if (bar_index == PATTERN_BARS - 1 || context.variation) && rng.chance(0.2 + energy * 0.4) {
          pattern.kick[pickup] = true;
        }
      }
    }

    match section {
      SectionKind::Breakdown => pattern.snare[12] = if bar_index % 2 == 1 { 0.45 } else { 0.0 },
      SectionKind::Reset => pattern.snare[12] = 0.6,
      _ => {
        for step in 0..STEPS_PER_BAR {
          if has_step(groove.snare, step) && (section != SectionKind::Intro || bar_index > 0) {
            pattern.snare[step] = 0.72;
          }
        }

        for step in 0..STEPS_PER_BAR {
          if has_step(groove.ghosts, step)
            && pattern.snare[step] == 0.0
            && rng.chance(0.3 + energy * 0.4)
          {
            pattern.snare[step] = 0.18 + rng.next_f32() * 0.14;
          }
        }
      }
    }

    if section == SectionKind::Build && bar_index >= 2 {
      let start = if bar_index == 2 { 8 } else { 0 };

      for step in start..STEPS_PER_BAR {
        let sixteenths = bar_index == PATTERN_BARS - 1 && step >= 8;

        if step % 2 == 0 || sixteenths {
          pattern.snare[step] = pattern.snare[step].max(0.3 + step as f32 / 16.0 * 0.45);
        }
      }
    }

    let rotation = groove.hat_rotation + if context.variation { 2 } else { 0 };
    let pulses = (groove.hat_pulses + (energy * 3.0) as usize).min(STEPS_PER_BAR);
    let hat_euclid = rotate_steps(euclidean_mask(pulses), rotation);
    let hat_chance = profile.rhythm_rules.hat_density * section_density;

    for step in 0..STEPS_PER_BAR {
      if has_step(groove.hat_base, step) || has_step(hat_euclid, step) {
        let accented = has_step(groove.hat_accent, step);
        let chance = if accented { hat_chance + 0.25 } else { hat_chance };

        if rng.chance(chance) {
          pattern.closed_hat[step] = if accented {
            0.55 + rng.next_f32() * 0.2
          } else {
            0.28 + rng.next_f32() * 0.22
          };
        }
      }

      if has_step(groove.open_hat, step)
        && section != SectionKind::Breakdown
        && rng.chance(0.3 + energy * 0.4)
      {
        pattern.open_hat[step] = 0.5 + rng.next_f32() * 0.1;
        pattern.closed_hat[step] = 0.0;
      }
    }

    if context.layers.percussion {
      let shift = if context.variation { 3 } else { 0 };
      let perc_steps =
        rotate_steps(euclidean_mask(groove.perc_pulses), groove.perc_rotation + shift);
      let chance =
        (profile.rhythm_rules.percussion_density * section_density * 1.6).clamp(0.0, 0.95);

      for step in 0..STEPS_PER_BAR {
        if has_step(perc_steps, step) && rng.chance(chance) {
          let note = groove.perc_notes[(step + bar_index) % groove.perc_notes.len()];
          pattern.percussion[step] = Some(NoteStep::new(note, 0.3 + energy * 0.3, 1, false, false));
        }
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

/// Returns a Euclidean rhythm as a 16-step bit mask.
pub fn euclidean_mask(pulses: usize) -> u16 {
  euclidean(pulses, STEPS_PER_BAR)
    .iter()
    .enumerate()
    .fold(0, |mask, (step, active)| if *active { mask | (1 << step) } else { mask })
}

/// Places a bassline on the chords of a bar.
pub struct BasslineGenerator;

impl BasslineGenerator {
  fn apply(rng: &mut Rng64, context: &BarContext<'_>, line: &BassLine, bar: &mut BarPattern) {
    let profile = context.profile;
    let sparse = context.section == SectionKind::Breakdown;

    for step in 0..STEPS_PER_BAR {
      if !has_step(line.onsets, step) || (sparse && step % 8 != 0) {
        continue;
      }

      let degree = i16::from(context.chord.degree) + i16::from(line.degrees[step]);
      let mut note = context.key.melodic_note(degree, context.key.root);

      while note > 55 {
        note -= 12;
      }

      while note < 33 {
        note += 12;
      }

      if has_step(line.octaves, step) {
        note += 12;
      }

      let accent =
        has_step(line.accents, step) || rng.chance(profile.bass_rules.accent_chance * 0.25);
      let slide = has_step(line.slides, step);
      let velocity = if accent { 0.88 } else { 0.58 + context.energy * 0.24 };
      let gap = next_onset_gap(line.onsets, step);
      let length = gap.min(context.genome.bass_gate).max(1) + u8::from(slide);

      bar.bass[step] = Some(NoteStep::new(note, velocity, length, accent, slide));
    }
  }
}

/// Returns the number of steps until the next onset, wrapping to the next bar.
fn next_onset_gap(onsets: u16, step: usize) -> u8 {
  for gap in 1..=STEPS_PER_BAR {
    if has_step(onsets, (step + gap) % STEPS_PER_BAR) {
      return gap as u8;
    }
  }

  STEPS_PER_BAR as u8
}

/// Generates lead phrases from the song motif.
pub struct MelodyGenerator;

impl MelodyGenerator {
  /// Returns the A and B four-bar lead phrases for a section.
  ///
  /// Each phrase runs call, response, call, cadence. `callResponseChance`
  /// decides whether the response answers the call or simply repeats it, and
  /// `mutationChance` decides how far each section strays from the song motif.
  pub fn phrases(
    rng: &mut Rng64,
    profile: &Profile,
    genome: &SongGenome,
    section: SectionKind,
    energy: f32
  ) -> ([Motif; PATTERN_BARS], [Motif; PATTERN_BARS]) {
    let rules = profile.melody_rules;
    let keep = if section == SectionKind::Breakdown { 0.6 } else { 0.55 + energy * 0.6 };
    let call = genome
      .lead_motif
      .mutated(rng, rules.mutation_chance)
      .thinned(rng, keep, genome.groove.lead_anchors);
    let phrase = |rng: &mut Rng64, call: Motif| {
      let response = if rng.chance(rules.call_response_chance) {
        call.response(rng)
      } else {
        call
      };
      let second_call = call.mutated(rng, rules.mutation_chance * 0.5);
      [call, response, second_call, response.with_cadence()]
    };
    let phrase_a = phrase(rng, call);
    let call_b = call.mutated(rng, rules.mutation_chance * 1.5);
    let mut phrase_b = phrase(rng, call_b);

    if matches!(section, SectionKind::Drop | SectionKind::Fill) && rng.chance(0.35) {
      for motif in &mut phrase_b {
        motif.octave_up = true;
      }
    }

    (phrase_a, phrase_b)
  }

  fn apply(context: &BarContext<'_>, motif: &Motif, bar: &mut BarPattern) {
    let key = context.key;
    let low = key.root + 22;
    let high = low + context.profile.melody_rules.max_range_semitones.max(12);
    let breakdown = context.section == SectionKind::Breakdown;

    for step in 0..STEPS_PER_BAR {
      if !has_step(motif.onsets, step) {
        continue;
      }

      let degree = i16::from(context.chord.degree) + i16::from(motif.degrees[step]);
      let mut note = key.melodic_note(degree, key.root + 24);

      while note > high {
        note -= 12;
      }

      while note < low {
        note += 12;
      }

      if motif.octave_up {
        note += 12;
      }

      let anchor = context.genome.groove.lead_anchors.contains(&step);
      let length =
        if breakdown { motif.lengths[step].saturating_mul(2) } else { motif.lengths[step] };
      bar.lead[step] = Some(NoteStep::new(
        note,
        0.38 + context.energy * 0.28,
        length.max(1),
        anchor,
        false
      ));
    }
  }
}

/// Generates arpeggios constrained by chord tones.
pub struct ArpGenerator;

impl ArpGenerator {
  fn apply(rng: &mut Rng64, context: &BarContext<'_>, bar: &mut BarPattern) {
    let genome = context.genome;
    let chord = context.chord;
    let count = chord.count.max(1);
    let cycle = if genome.arp_two_octaves { count * 2 } else { count };
    let mut position = 0;

    for step in 0..STEPS_PER_BAR {
      if !has_step(genome.arp_steps, step) {
        continue;
      }

      if rng.chance(0.5 + context.energy * 0.45) {
        let slot = position % cycle;
        let octave = if slot >= count { 12 } else { 0 };
        let index = genome.arp_shape.index(slot % count, count, position / cycle);
        let tone = chord.tone(index) + 12 + octave;
        bar.arp[step] = Some(NoteStep::new(tone, 0.24 + context.energy * 0.18, 1, false, false));
      }

      position += 1;
    }
  }
}

impl ArpShape {
  /// Returns which chord tone to play for a position in the arpeggio cycle.
  pub fn index(self, slot: usize, count: usize, pass: usize) -> usize {
    let top = count.saturating_sub(1);

    match self {
      ArpShape::Up => slot,
      ArpShape::Down => top - slot.min(top),
      ArpShape::UpDown => {
        if pass % 2 == 0 {
          slot
        } else {
          top - slot.min(top)
        }
      }
      ArpShape::Alternate => {
        if slot % 2 == 1 {
          top
        } else {
          slot / 2
        }
      }
      ArpShape::Ordered(order) => usize::from(order[slot % order.len()]) % count.max(1)
    }
  }
}

/// Generates stabs and pad chords.
pub struct ChordStabGenerator;

impl ChordStabGenerator {
  fn apply(rng: &mut Rng64, context: &BarContext<'_>, bar: &mut BarPattern) {
    let genome = context.genome;
    let chord = context.chord;
    let energy = context.energy;

    if context.layers.pad {
      let featured = matches!(context.section, SectionKind::Breakdown | SectionKind::Intro);
      let velocity = if genome.pad_bed && !featured {
        0.16 + energy * 0.1
      } else {
        0.24 + energy * 0.18
      };
      bar.pad[0] = Some(NoteStep::new(chord.root_note, velocity, 16, false, false));
    }

    if !context.layers.stabs {
      return;
    }

    let use_alt = context.variation || context.section == SectionKind::Variation;
    let steps = if use_alt { genome.groove.alt_stabs } else { genome.groove.stabs };

    for step in 0..STEPS_PER_BAR {
      if has_step(steps, step) && rng.chance(0.5 + energy * 0.4) {
        bar.chords[step] = Some(NoteStep::new(
          chord.root_note,
          0.34 + energy * 0.2,
          genome.stab_length,
          true,
          false
        ));
      }
    }
  }
}

/// Mutates and recalls previous material.
pub struct MutationEngine;

impl MutationEngine {
  /// Carries a little rhythmic identity from the previous section.
  pub fn recall(previous: &PatternBank, rng: &mut Rng64, energy: f32, bars: &mut [BarPattern; 4]) {
    if energy > 0.5 && rng.chance(0.4) {
      for bar in bars.iter_mut() {
        for step in [5, 13] {
          if bar.closed_hat[step] > 0.0 {
            bar.closed_hat[step] = (bar.closed_hat[step] + 0.18).clamp(0.0, 1.0);
          }
        }
      }
    }

    if rng.chance(0.25) {
      bars[0].percussion = previous.bars[0].percussion;
    }
  }
}

/// Generates the section-ending fill.
pub struct FillGenerator;

impl FillGenerator {
  #[allow(clippy::too_many_arguments)]
  fn fill_for(
    rng: &mut Rng64,
    profile: &Profile,
    genome: &SongGenome,
    section: SectionKind,
    energy: f32,
    length_bars: u8,
    bars: &[BarPattern; PATTERN_BARS],
    bars_b: &[BarPattern; PATTERN_BARS]
  ) -> Option<BarPattern> {
    let chance = match section {
      SectionKind::Fill | SectionKind::Build => 1.0,
      SectionKind::Breakdown | SectionKind::Reset => profile.rhythm_rules.fill_chance * 0.3,
      _ => profile.rhythm_rules.fill_chance
    };

    if !rng.chance(chance) {
      return None;
    }

    let slot = usize::from(length_bars.saturating_sub(1)) % (PATTERN_BARS * 2);
    let mut bar = if slot >= PATTERN_BARS {
      bars_b[slot - PATTERN_BARS]
    } else {
      bars[slot]
    };
    let style = if rng.chance(0.75) {
      genome.fill_style
    } else {
      genome.alt_fill_style
    };

    match style {
      FillStyle::SnareEighths => {
        for step in 12..STEPS_PER_BAR {
          bar.snare[step] = if step % 2 == 0 { 0.62 } else { 0.0 };
          bar.closed_hat[step] = 0.65;
        }
      }
      FillStyle::SnareSixteenths => {
        for step in 12..STEPS_PER_BAR {
          bar.snare[step] = 0.35 + (step - 12) as f32 * 0.13;
        }
      }
      FillStyle::HalfBarRoll => {
        for step in 8..STEPS_PER_BAR {
          if step % 2 == 0 || step >= 12 {
            bar.snare[step] = 0.3 + (step - 8) as f32 * 0.06;
          }
        }
      }
      FillStyle::KickDrop => {
        for step in 8..STEPS_PER_BAR {
          bar.kick[step] = false;
          bar.closed_hat[step] = 0.0;
          bar.bass[step] = None;
        }

        bar.snare[14] = 0.7;
        bar.snare[15] = 0.8;
      }
      FillStyle::PercussionRun => {
        for (offset, step) in (10..STEPS_PER_BAR).enumerate() {
          let notes = genome.groove.perc_notes;
          let note = notes[offset % notes.len()] - offset as i16;
          bar.percussion[step] = Some(NoteStep::new(note, 0.4 + energy * 0.3, 1, false, false));
        }

        bar.snare[15] = 0.7;
      }
    }

    bar.fx[15] = Some(NoteStep::new(72, 0.3 + energy * 0.25, 1, false, false));
    Some(bar)
  }
}

/// Generates sparse effects hits.
pub struct AutomationGenerator;

impl AutomationGenerator {
  fn apply_fx(rng: &mut Rng64, context: &BarContext<'_>, bar: &mut BarPattern) {
    if matches!(context.section, SectionKind::Transition | SectionKind::Build)
      && context.bar_index == PATTERN_BARS - 1
      && rng.chance(0.5 + context.energy * 0.3)
    {
      bar.fx[15] = Some(NoteStep::new(72, 0.32 + context.energy * 0.24, 1, false, false));
    }

    if context.section == SectionKind::Drop && context.bar_index == 0 && !context.variation {
      bar.fx[0] = Some(NoteStep::new(72, 0.28 + context.energy * 0.2, 1, false, false));
    }
  }
}

/// Generates the next arrangement section.
pub struct ArrangementGenerator;

impl ArrangementGenerator {
  /// Chooses the next section with seeded, energy-weighted branching.
  pub fn next(rng: &mut Rng64, current: SectionKind, energy: f32, reset_chance: f32) -> SectionKind {
    use SectionKind::{
      Breakdown, Build, Drop, Fill, GrooveA, GrooveB, Intro, Reset, Transition, Variation
    };

    if current != Intro && rng.chance(reset_chance) {
      return Reset;
    }

    let high = energy;
    let low = 1.0 - energy;
    let options: &[(SectionKind, f32)] = match current {
      Intro => &[(GrooveA, 0.8), (Build, 0.1 + high * 0.3)],
      GrooveA => &[
        (GrooveB, 0.45),
        (Build, 0.15 + high * 0.35),
        (Variation, 0.2),
        (Breakdown, 0.05 + low * 0.15)
      ],
      GrooveB => &[
        (Drop, 0.2 + high * 0.45),
        (Variation, 0.3),
        (Build, 0.2),
        (Breakdown, 0.05 + low * 0.2)
      ],
      Build => &[(Drop, 0.85), (Fill, 0.15)],
      Drop => &[(Fill, 0.25), (Variation, 0.3), (GrooveB, 0.2), (Breakdown, 0.15 + low * 0.15)],
      Variation => &[(Breakdown, 0.35), (GrooveA, 0.3), (Build, 0.2 + high * 0.2)],
      Breakdown => &[(Build, 0.5), (Transition, 0.4), (GrooveA, 0.1)],
      Fill => &[(GrooveA, 0.4), (Drop, 0.3 + high * 0.2), (GrooveB, 0.2)],
      Transition => &[(Drop, 0.5 + high * 0.2), (GrooveA, 0.4)],
      Reset => &[(Intro, 1.0)]
    };
    let weights: Vec<u32> =
      options.iter().map(|(_, weight)| (weight * 100.0).max(1.0) as u32).collect();

    options[rng.weighted_index(&weights)].0
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
  use super::{euclidean, ArrangementGenerator, BarPattern, Key, PatternBank, STEPS_PER_BAR};
  use crate::genome::SongGenome;
  use crate::profile::{amiga_house_95ish, downtempo_breakbeat, dub_deep_house};
  use crate::rng::Rng64;
  use crate::types::SectionKind;
  use std::collections::HashSet;

  fn bank_for(seed: u64, section: SectionKind) -> (SongGenome, PatternBank) {
    let profile = amiga_house_95ish();
    let genome = SongGenome::generate(seed, &profile);
    let mut rng = Rng64::new(seed);
    let key = genome.home_key;
    let bank = PatternBank::generate(&mut rng, &profile, &genome, None, section, key, 0.6);
    (genome, bank)
  }

  #[test]
  fn euclidean_pattern_fits_bar_length() {
    let pattern = euclidean(5, STEPS_PER_BAR);
    assert_eq!(pattern.len(), STEPS_PER_BAR);
    assert_eq!(pattern.iter().filter(|value| **value).count(), 5);
  }

  #[test]
  fn house_kick_stays_four_on_the_floor() {
    for seed in 0..16 {
      let (_, bank) = bank_for(seed, SectionKind::Drop);

      for bar in &bank.bars {
        for step in [0, 4, 8, 12] {
          assert!(bar.kick[step], "seed {seed} lost the kick on step {step}");
        }
      }
    }
  }

  #[test]
  fn broken_profile_kick_varies_between_seeds() {
    let profile = downtempo_breakbeat();
    let patterns: HashSet<u16> = (0..24)
      .map(|seed| SongGenome::generate(seed, &profile).groove.kick)
      .collect();

    assert!(patterns.len() >= 3);
  }

  #[test]
  fn same_seed_generates_same_pattern() {
    let (_, left) = bank_for(99, SectionKind::Intro);
    let (_, right) = bank_for(99, SectionKind::Intro);

    assert_eq!(left, right);
  }

  #[test]
  fn empty_bar_has_no_notes() {
    let bar = BarPattern::empty();
    assert!(bar.bass.iter().all(Option::is_none));
  }

  #[test]
  fn lead_and_bass_stay_in_scale() {
    for seed in 0..32 {
      let (genome, bank) = bank_for(seed, SectionKind::Drop);
      let key = genome.home_key;

      for bar in bank.bars.iter().chain(bank.bars_b.iter()) {
        for note in bar.lead.iter().chain(bar.bass.iter()).flatten() {
          let in_key = key.scale.contains(key.root, note.note);
          assert!(in_key, "seed {seed} note {} out of key", note.note);
        }
      }
    }
  }

  #[test]
  fn lead_uses_a_real_melodic_range() {
    for seed in 0..16 {
      let (_, bank) = bank_for(seed, SectionKind::Drop);
      let notes: HashSet<i16> = bank
        .bars
        .iter()
        .flat_map(|bar| bar.lead.iter().flatten().map(|note| note.note))
        .collect();

      assert!(notes.len() >= 3, "seed {seed} lead only used {notes:?}");
    }
  }

  #[test]
  fn chords_follow_the_scale() {
    let profile = dub_deep_house();

    for seed in 0..32 {
      let genome = SongGenome::generate(seed, &profile);
      let key = genome.home_key;
      let mut rng = Rng64::new(seed);
      let mut rules = profile.chord_rules;
      rules.bright_borrow_chance = 0.0;
      let chords = super::ChordGrammar::voice(&mut rng, rules, &genome, key, [0, 3, 5, 6], None);
      let harmony = Key {
        root: key.root,
        scale: match key.scale {
          super::ScaleKind::MinorPentatonic => super::ScaleKind::NaturalMinor,
          other => other
        }
      };

      for chord in chords {
        for index in 0..chord.count {
          assert!(harmony.scale.contains(harmony.root, chord.tone(index)));
        }
      }
    }
  }

  #[test]
  fn arrangement_branches_differently_between_seeds() {
    let paths: HashSet<Vec<SectionKind>> = (0..16)
      .map(|seed| {
        let mut rng = Rng64::new(seed);
        let mut section = SectionKind::Intro;
        let mut path = Vec::new();

        for _ in 0..8 {
          section = ArrangementGenerator::next(&mut rng, section, 0.5, 0.05);
          path.push(section);
        }

        path
      })
      .collect();

    assert!(paths.len() >= 8);
  }
}
