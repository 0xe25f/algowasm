//! Per-song identity drawn once from the seed.
//!
//! Everything that makes one song recognisably different from another lives
//! here: tempo, home key, groove, motifs, progressions, drum kit, patch
//! variations, and automation shapes. Sections then vary this material rather
//! than inventing unrelated material each time, which keeps a song coherent
//! while keeping different seeds clearly apart.

use crate::composition::{ChordGrammar, Key, ScalePicker, STEPS_PER_BAR};
use crate::profile::{Patch, Profile, TempoRules};
use crate::rng::Rng64;
use crate::synth::DrumKit;
use crate::types::{SectionKind, TrackId, WaveShape, TRACK_COUNT};

/// Mixes the seed so that the genome draws from its own random stream.
const GENOME_STREAM: u64 = 0x6a09_e667_f3bc_c908;

/// Returns true when a 16-step mask has the step set.
pub const fn has_step(mask: u16, step: usize) -> bool {
  step < STEPS_PER_BAR && mask & (1 << step) != 0
}

/// Rotates a 16-step mask later in the bar by the given number of steps.
pub const fn rotate_steps(mask: u16, steps: usize) -> u16 {
  mask.rotate_left((steps % STEPS_PER_BAR) as u32)
}

/// Builds a 16-step mask from a list of steps.
pub const fn steps(list: &[usize]) -> u16 {
  let mut mask = 0;
  let mut index = 0;

  while index < list.len() {
    mask |= 1 << list[index];
    index += 1;
  }

  mask
}

const OFFBEAT_EIGHTHS: u16 = steps(&[2, 6, 10, 14]);
const ALL_EIGHTHS: u16 = steps(&[0, 2, 4, 6, 8, 10, 12, 14]);
const ODD_SIXTEENTHS: u16 = steps(&[1, 3, 5, 7, 9, 11, 13, 15]);
const ALL_SIXTEENTHS: u16 = 0xffff;
const FOUR_ON_FLOOR: u16 = steps(&[0, 4, 8, 12]);

/// Arpeggio note orders.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum ArpShape {
  #[default]
  Up,
  Down,
  UpDown,
  Alternate,
  Ordered([u8; 4])
}

/// End-of-section fill shapes.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum FillStyle {
  #[default]
  SnareEighths,
  SnareSixteenths,
  HalfBarRoll,
  KickDrop,
  PercussionRun
}

/// The song's rhythmic fingerprint, as 16-step masks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Groove {
  pub four_on_floor: bool,
  pub kick: u16,
  pub pickup_kick: Option<usize>,
  pub snare: u16,
  pub ghosts: u16,
  pub hat_base: u16,
  pub hat_accent: u16,
  pub hat_pulses: usize,
  pub hat_rotation: usize,
  pub open_hat: u16,
  pub perc_pulses: usize,
  pub perc_rotation: usize,
  pub perc_notes: [i16; 3],
  pub stabs: u16,
  pub alt_stabs: u16,
  pub lead_anchors: [usize; 2],
  /// Delay applied to every second 16th step, as a fraction of a step.
  pub swing: f32
}

/// A one-bar melodic motif in harmony-scale degrees relative to the chord.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Motif {
  pub onsets: u16,
  pub degrees: [i8; STEPS_PER_BAR],
  pub lengths: [u8; STEPS_PER_BAR],
  pub octave_up: bool
}

impl Motif {
  fn generate(rng: &mut Rng64, profile: &Profile, anchors: [usize; 2]) -> Self {
    let density = profile.melody_rules.density;
    let mut onsets = 0;

    for step in 0..STEPS_PER_BAR {
      let weight = if step % 2 == 0 { 0.65 } else { 0.28 };

      if anchors.contains(&step) || rng.chance(density * weight * 1.3) {
        onsets |= 1 << step;
      }
    }

    let mut degrees = [0; STEPS_PER_BAR];
    let mut degree: i8 = rng.pick(&[0, 2, 4]);

    for (step, slot) in degrees.iter_mut().enumerate() {
      if !has_step(onsets, step) {
        continue;
      }

      if anchors.contains(&step) {
        degree = nearest_chord_tone(degree);
      } else {
        degree = (degree + rng.pick(&[-2, -1, -1, 0, 1, 1, 2, 3, -3])).clamp(-3, 9);
      }

      *slot = degree;
    }

    let mut motif = Self {
      onsets,
      degrees,
      lengths: [1; STEPS_PER_BAR],
      octave_up: false
    };
    let gate: u8 = rng.pick(&[1, 2, 2, 3]);
    motif.set_lengths(gate);
    motif
  }

  fn set_lengths(&mut self, gate: u8) {
    for step in 0..STEPS_PER_BAR {
      if has_step(self.onsets, step) {
        let mut gap = 1;

        while step + gap < STEPS_PER_BAR && !has_step(self.onsets, step + gap) {
          gap += 1;
        }

        self.lengths[step] = (gap as u8).min(gate.max(1));
      }
    }
  }

  /// Returns a copy with some notes nudged, dropped, or added.
  pub fn mutated(&self, rng: &mut Rng64, chance: f32) -> Self {
    let mut motif = *self;

    for step in 0..STEPS_PER_BAR {
      if has_step(motif.onsets, step) {
        if rng.chance(chance) {
          motif.degrees[step] = (motif.degrees[step] + rng.pick(&[-2, -1, 1, 2])).clamp(-3, 9);
        }

        if motif.onsets.count_ones() > 2 && rng.chance(chance * 0.25) {
          motif.onsets &= !(1 << step);
        }
      } else if step % 2 == 0 && rng.chance(chance * 0.2) {
        motif.onsets |= 1 << step;
        motif.degrees[step] = nearest_chord_tone(motif.degrees[step.saturating_sub(2)]);
        motif.lengths[step] = 1;
      }
    }

    motif
  }

  /// Returns a copy that keeps each non-anchor note with the given chance.
  pub fn thinned(&self, rng: &mut Rng64, keep: f32, anchors: [usize; 2]) -> Self {
    let mut motif = *self;

    if keep >= 1.0 {
      return motif;
    }

    for step in 0..STEPS_PER_BAR {
      if has_step(motif.onsets, step) && !anchors.contains(&step) && !rng.chance(keep) {
        motif.onsets &= !(1 << step);
      }
    }

    motif
  }

  /// Returns an answering phrase: the same rhythm with a moved second half.
  pub fn response(&self, rng: &mut Rng64) -> Self {
    let mut motif = *self;
    let shift: i8 = rng.pick(&[2, -2, 1, 3]);

    for step in STEPS_PER_BAR / 2..STEPS_PER_BAR {
      if has_step(motif.onsets, step) {
        motif.degrees[step] = (motif.degrees[step] + shift).clamp(-3, 9);
      }
    }

    if let Some(last) = motif.last_onset() {
      let original = self.degrees[last];
      let options: Vec<i8> = [0, 2, 4].into_iter().filter(|degree| *degree != original).collect();
      motif.degrees[last] = rng.pick(&options);
    }

    motif
  }

  /// Returns a copy that resolves to the chord root.
  pub fn with_cadence(&self) -> Self {
    let mut motif = *self;

    if let Some(last) = motif.last_onset() {
      motif.degrees[last] = if motif.degrees[last] > 3 { 7 } else { 0 };
    }

    motif
  }

  fn last_onset(&self) -> Option<usize> {
    (0..STEPS_PER_BAR).rev().find(|step| has_step(self.onsets, *step))
  }
}

fn nearest_chord_tone(degree: i8) -> i8 {
  let mut best = 0;

  for tone in [-3, 0, 2, 4, 7, 9] {
    if (tone - degree).abs() < (best - degree).abs() {
      best = tone;
    }
  }

  best
}

/// A one-bar bassline in harmony-scale degrees relative to the chord.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BassLine {
  pub onsets: u16,
  pub optional: u16,
  pub degrees: [i8; STEPS_PER_BAR],
  pub octaves: u16,
  pub accents: u16,
  pub slides: u16
}

impl BassLine {
  fn generate(rng: &mut Rng64, profile: &Profile) -> Self {
    let rules = profile.bass_rules;
    let density = rules.density;
    let templates: [(u16, u16, f32); 6] = [
      (OFFBEAT_EIGHTHS, steps(&[0, 3, 11, 15]), 3.0),
      (ALL_EIGHTHS, ODD_SIXTEENTHS, 2.0 * density),
      (steps(&[0, 3, 6, 10]), steps(&[8, 13, 14]), 3.0),
      (steps(&[0]), 0xfffe, 2.0 * density),
      (steps(&[0, 10]), steps(&[3, 6, 14]), 3.0 * (1.0 - density) + 0.5),
      (FOUR_ON_FLOOR, steps(&[2, 6, 10, 14, 15]), 2.0)
    ];
    let weights: Vec<u32> =
      templates.iter().map(|(_, _, weight)| (weight * 10.0).max(1.0) as u32).collect();
    let (onsets, optional, _) = templates[rng.weighted_index(&weights)];
    let mut degrees = [0; STEPS_PER_BAR];
    let mut octaves = 0;
    let mut accents = 0;
    let mut slides = 0;

    for (step, degree) in degrees.iter_mut().enumerate() {
      if step != 0 {
        *degree = if has_step(onsets, step) {
          rng.pick(&[0, 0, 0, 4, 7])
        } else {
          rng.pick(&[0, 0, 4, 7, 2, 6, -1, -3])
        };
      }

      if step != 0 && rng.chance(rules.octave_jump_chance) {
        octaves |= 1 << step;
      }

      if rng.chance(rules.accent_chance) {
        accents |= 1 << step;
      }

      if rng.chance(rules.slide_chance) {
        slides |= 1 << step;
      }
    }

    Self {
      onsets,
      optional,
      degrees,
      octaves,
      accents,
      slides
    }
  }

  /// Fills in optional notes for a section and applies a light mutation.
  pub fn for_section(
    &self,
    rng: &mut Rng64,
    profile: &Profile,
    energy: f32,
    mutation: f32
  ) -> Self {
    let density = (profile.bass_rules.density * (0.65 + energy * 0.7)).clamp(0.0, 1.0);
    let mut line = *self;

    for step in 0..STEPS_PER_BAR {
      if has_step(self.optional, step) && rng.chance(density * 0.6) {
        line.onsets |= 1 << step;
      }
    }

    line.mutated(rng, mutation, profile.bass_rules.octave_jump_chance)
  }

  /// Returns a copy with some degrees and octaves changed.
  pub fn mutated(&self, rng: &mut Rng64, chance: f32, octave_chance: f32) -> Self {
    let mut line = *self;

    for step in 1..STEPS_PER_BAR {
      if has_step(line.onsets, step) && rng.chance(chance) {
        line.degrees[step] = rng.pick(&[0, 4, 7, 2, -1]);

        if rng.chance(octave_chance) {
          line.octaves ^= 1 << step;
        }
      }
    }

    line
  }
}

/// Slow movement for one track.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TrackAutomation {
  pub period_bars: f32,
  pub phase: f32,
  /// Filter movement in octaves.
  pub filter_depth: f32,
  pub send_depth: f32,
  pub pan_depth: f32
}

/// Seeded per-track changes to a profile patch.
#[derive(Clone, Copy, Debug, PartialEq)]
struct PatchVariation {
  /// Which sibling wave to swap to, if any.
  wave_swap: Option<usize>,
  cutoff_octaves: f32,
  resonance: f32,
  attack: f32,
  decay: f32,
  sustain: f32,
  release: f32,
  sends: f32,
  gain: f32,
  pan: f32
}

impl PatchVariation {
  const NONE: Self = Self {
    wave_swap: None,
    cutoff_octaves: 0.0,
    resonance: 0.0,
    attack: 1.0,
    decay: 1.0,
    sustain: 0.0,
    release: 1.0,
    sends: 1.0,
    gain: 1.0,
    pan: 0.0
  };

  fn apply(&self, patch: Patch) -> Patch {
    let mut patch = patch;

    if let Some(pick) = self.wave_swap {
      patch.wave = sibling_wave(patch.wave, pick);
    }

    let cutoff = patch.filter_cutoff * 2.0_f32.powf(self.cutoff_octaves);
    patch.filter_cutoff = cutoff.clamp(20.0, 20_000.0);
    patch.resonance = (patch.resonance + self.resonance).clamp(0.0, 0.85);
    patch.attack_ms *= self.attack;
    patch.decay_ms *= self.decay;
    patch.sustain = (patch.sustain + self.sustain).clamp(0.0, 1.0);
    patch.release_ms *= self.release;
    patch.delay_send = (patch.delay_send * self.sends).clamp(0.0, 1.0);
    patch.reverb_send = (patch.reverb_send * self.sends).clamp(0.0, 1.0);
    patch.gain = (patch.gain * self.gain).clamp(0.0, 2.0);
    patch.pan = (patch.pan + self.pan).clamp(-1.0, 1.0);
    patch
  }
}

/// The per-song identity drawn once from a seed and a profile.
#[derive(Clone, Debug, PartialEq)]
pub struct SongGenome {
  /// Where the song's tempo sits in the profile range, from 0 to 1.
  pub tempo_position: f32,
  pub home_key: Key,
  pub groove: Groove,
  pub lead_motif: Motif,
  pub bass_line: BassLine,
  pub bass_gate: u8,
  pub signature_progression: [u8; 4],
  pub alt_progression: [u8; 4],
  pub seventh_chords: bool,
  pub arp_shape: ArpShape,
  pub arp_steps: u16,
  pub arp_two_octaves: bool,
  pub stab_length: u8,
  pub pad_bed: bool,
  pub lead_in_groove_a: bool,
  pub intro_stabs: bool,
  pub breakdown_bass: bool,
  pub fill_style: FillStyle,
  pub alt_fill_style: FillStyle,
  pub drums: DrumKit,
  pub automation: [TrackAutomation; TRACK_COUNT],
  patch_variations: [PatchVariation; TRACK_COUNT]
}

impl SongGenome {
  /// Draws a song identity from a seed. The same seed and profile always
  /// produce the same genome.
  pub fn generate(seed: u64, profile: &Profile) -> Self {
    let mut rng = Rng64::new(seed ^ GENOME_STREAM);
    let tempo_position = (rng.next_f32() + rng.next_f32()) * 0.5;
    let home_key = ScalePicker::pick(&mut rng, profile);
    let groove = Self::groove(&mut rng, profile);
    let lead_motif = Motif::generate(&mut rng, profile, groove.lead_anchors);
    let bass_line = BassLine::generate(&mut rng, profile);
    let signature_progression = ChordGrammar::pick_progression(&mut rng, profile.chord_rules);
    let mut alt_progression = ChordGrammar::pick_progression(&mut rng, profile.chord_rules);

    for _ in 0..4 {
      if alt_progression != signature_progression {
        break;
      }

      alt_progression = ChordGrammar::pick_progression(&mut rng, profile.chord_rules);
    }

    let arp_order = {
      let mut order = [0u8, 1, 2, 3];

      for index in (1..order.len()).rev() {
        order.swap(index, rng.range_usize(index + 1));
      }

      order
    };
    let arp_shape = rng.pick(&[
      ArpShape::Up,
      ArpShape::Down,
      ArpShape::UpDown,
      ArpShape::Alternate,
      ArpShape::Ordered(arp_order)
    ]);
    let arp_steps = rng.pick(&[
      ODD_SIXTEENTHS,
      ALL_SIXTEENTHS,
      steps(&[0, 3, 6, 9, 12, 15]),
      ALL_EIGHTHS,
      steps(&[0, 3, 6, 8, 11, 14])
    ]);
    let fill_styles = [
      FillStyle::SnareEighths,
      FillStyle::SnareSixteenths,
      FillStyle::HalfBarRoll,
      FillStyle::KickDrop,
      FillStyle::PercussionRun
    ];
    let fill_style = rng.pick(&fill_styles);
    let mut alt_fill_style = rng.pick(&fill_styles);

    if alt_fill_style == fill_style {
      alt_fill_style = FillStyle::SnareEighths;
    }

    Self {
      tempo_position,
      home_key,
      groove,
      lead_motif,
      bass_line,
      bass_gate: rng.pick(&[1, 2, 2, 3, 4]),
      signature_progression,
      alt_progression,
      seventh_chords: rng.chance(0.6),
      arp_shape,
      arp_steps,
      arp_two_octaves: rng.chance(0.4),
      stab_length: rng.pick(&[1, 2, 2, 3]),
      pad_bed: rng.chance(if profile.arrangement_rules.max_section_bars > 8 { 0.55 } else { 0.35 }),
      lead_in_groove_a: rng.chance(0.4),
      intro_stabs: rng.chance(0.4),
      breakdown_bass: rng.chance(0.4),
      fill_style,
      alt_fill_style,
      drums: Self::drum_kit(&mut rng),
      automation: Self::automation(&mut rng, profile),
      patch_variations: Self::patch_variations(&mut rng)
    }
  }

  fn groove(rng: &mut Rng64, profile: &Profile) -> Groove {
    let four_on_floor = profile.rhythm_rules.kick_density >= 0.999;
    let kick = if four_on_floor {
      FOUR_ON_FLOOR
    } else {
      rng.pick(&[
        steps(&[0, 10]),
        steps(&[0, 6, 10]),
        steps(&[0, 2, 10, 11]),
        steps(&[0, 7, 8]),
        steps(&[0, 8, 11, 14]),
        steps(&[0, 3, 10])
      ])
    };
    let pickup_kick = if four_on_floor {
      rng.pick(&[None, Some(14), Some(7), Some(15), Some(10)])
    } else {
      rng.pick(&[None, Some(15), Some(13)])
    };
    let snare = if four_on_floor || rng.chance(0.7) {
      steps(&[4, 12])
    } else {
      steps(&[8])
    };
    let ghost_count = if four_on_floor {
      rng.range_usize(3)
    } else {
      1 + rng.range_usize(3)
    };
    let mut ghosts = 0;

    for _ in 0..ghost_count {
      let step: usize = rng.pick(&[3, 7, 9, 11, 14, 15]);

      if !has_step(snare, step) {
        ghosts |= 1 << step;
      }
    }

    let stab_pool = [
      steps(&[3, 7, 11, 15]),
      steps(&[2, 6, 10, 14]),
      steps(&[0, 3, 6, 10]),
      steps(&[3, 6, 11, 14]),
      steps(&[2, 5, 10, 13]),
      steps(&[1, 4, 9, 12]),
      steps(&[0, 6, 12]),
      steps(&[3, 10]),
      steps(&[6, 14]),
      steps(&[0, 3, 8, 11])
    ];
    let stabs = rng.pick(&stab_pool);
    let mut alt_stabs = rng.pick(&stab_pool);

    if alt_stabs == stabs {
      alt_stabs = rotate_steps(stabs, 2);
    }

    let perc_base = 60 + rng.range_usize(10) as i16;
    let swing_max = if four_on_floor { 0.14 } else { 0.2 };

    Groove {
      four_on_floor,
      kick,
      pickup_kick,
      snare,
      ghosts,
      hat_base: rng.pick(&[
        OFFBEAT_EIGHTHS,
        ODD_SIXTEENTHS,
        ALL_EIGHTHS,
        ALL_SIXTEENTHS,
        OFFBEAT_EIGHTHS
      ]),
      hat_accent: rng.pick(&[
        OFFBEAT_EIGHTHS,
        steps(&[2, 5, 10, 13]),
        steps(&[0, 4, 8, 12]),
        steps(&[3, 7, 11, 15])
      ]),
      hat_pulses: 5 + rng.range_usize(6),
      hat_rotation: rng.range_usize(STEPS_PER_BAR),
      open_hat: rng.pick(&[
        OFFBEAT_EIGHTHS,
        steps(&[6, 14]),
        steps(&[14]),
        steps(&[10]),
        steps(&[7, 15]),
        steps(&[2, 10])
      ]),
      perc_pulses: 3 + rng.range_usize(5),
      perc_rotation: rng.range_usize(STEPS_PER_BAR),
      perc_notes: [perc_base, perc_base + rng.pick(&[3, 5, 7]), perc_base - rng.pick(&[2, 4])],
      stabs,
      alt_stabs,
      lead_anchors: rng.pick(&[[2, 10], [0, 8], [3, 11], [0, 6], [4, 12], [1, 9], [6, 14]]),
      swing: if rng.chance(0.3) { 0.0 } else { rng.range_f32(0.03, swing_max) }
    }
  }

  fn drum_kit(rng: &mut Rng64) -> DrumKit {
    let hat_partial_a = rng.range_f32(330.0, 560.0);

    DrumKit {
      kick_pitch: rng.range_f32(38.0, 54.0),
      kick_sweep: rng.range_f32(60.0, 130.0),
      kick_sweep_rate: rng.range_f32(28.0, 60.0),
      kick_decay: rng.range_f32(6.0, 12.0),
      kick_click: rng.range_f32(0.12, 0.42),
      snare_tone: rng.range_f32(150.0, 240.0),
      snare_decay: rng.range_f32(13.0, 26.0),
      snare_body: rng.range_f32(0.22, 0.52),
      hat_partial_a,
      hat_partial_b: hat_partial_a * rng.range_f32(1.45, 1.95),
      hat_closed_decay: rng.range_f32(26.0, 52.0),
      hat_open_decay: rng.range_f32(3.5, 7.5),
      percussion_decay: rng.range_f32(14.0, 34.0),
      fx_sweep_rate: rng.range_f32(1.0, 3.5)
    }
  }

  fn automation(rng: &mut Rng64, profile: &Profile) -> [TrackAutomation; TRACK_COUNT] {
    let rules = profile.automation_rules;
    let mut automation = [TrackAutomation::default(); TRACK_COUNT];

    for track in TrackId::ALL {
      let pans = matches!(
        track,
        TrackId::Percussion
          | TrackId::Chords
          | TrackId::Pad
          | TrackId::Lead
          | TrackId::Arp
          | TrackId::OpenHat
      );
      automation[track.as_index()] = TrackAutomation {
        period_bars: rng.pick(&[2.0, 4.0, 8.0, 16.0]),
        phase: rng.next_f32(),
        filter_depth: rules.filter_motion * rng.range_f32(0.3, 0.9),
        send_depth: rules.send_motion * rng.range_f32(0.4, 1.0),
        pan_depth: if pans { rules.pan_motion * rng.range_f32(0.4, 1.0) } else { 0.0 }
      };
    }

    automation
  }

  fn patch_variations(rng: &mut Rng64) -> [PatchVariation; TRACK_COUNT] {
    let mut variations = [PatchVariation::NONE; TRACK_COUNT];

    for track in [TrackId::Bass, TrackId::Chords, TrackId::Pad, TrackId::Lead, TrackId::Arp] {
      variations[track.as_index()] = PatchVariation {
        wave_swap: if rng.chance(0.35) { Some(rng.range_usize(2)) } else { None },
        cutoff_octaves: rng.range_f32(-0.6, 0.6),
        resonance: rng.range_f32(-0.1, 0.3),
        attack: rng.range_f32(0.6, 1.6),
        decay: rng.range_f32(0.6, 1.6),
        sustain: rng.range_f32(-0.12, 0.12),
        release: rng.range_f32(0.7, 1.5),
        sends: rng.range_f32(0.7, 1.35),
        gain: rng.range_f32(0.9, 1.1),
        pan: rng.range_f32(-0.1, 0.1)
      };
    }

    variations
  }

  /// Returns the song tempo inside the profile's tempo rules, leaning towards
  /// the default tempo.
  pub fn bpm(&self, tempo: TempoRules) -> f32 {
    let position = self.tempo_position;
    let bpm = if position < 0.5 {
      tempo.min_bpm + (tempo.default_bpm - tempo.min_bpm) * position * 2.0
    } else {
      tempo.default_bpm + (tempo.max_bpm - tempo.default_bpm) * (position - 0.5) * 2.0
    };

    bpm.round().clamp(tempo.min_bpm, tempo.max_bpm)
  }

  /// Returns the song tempo for an explicit tempo range.
  pub fn bpm_in_range(&self, min_bpm: f32, max_bpm: f32) -> f32 {
    (min_bpm + (max_bpm - min_bpm) * self.tempo_position).round().clamp(min_bpm, max_bpm)
  }

  /// Returns the profile patches with this song's variations applied.
  ///
  /// Wave swaps stay inside the patch's family: bright waves (saw, square,
  /// pulse) swap with each other, and soft waves (sine, triangle) swap with
  /// each other, so a profile keeps its character.
  pub fn patches(&self, base: &[Patch; TRACK_COUNT]) -> [Patch; TRACK_COUNT] {
    let mut patches = *base;

    for (index, patch) in patches.iter_mut().enumerate() {
      *patch = self.patch_variations[index].apply(*patch);
    }

    patches
  }

  /// Chooses the key for a section. Songs stay in their home key, with the
  /// occasional short move to a related key during high points.
  pub fn key_for_section(&self, rng: &mut Rng64, profile: &Profile, section: SectionKind) -> Key {
    let can_move =
      matches!(section, SectionKind::Drop | SectionKind::Variation | SectionKind::GrooveB);

    if can_move && rng.chance(0.08 + profile.chord_rules.movement_weight * 0.15) {
      let shift: i16 = rng.pick(&[5, 7, -2, 3, -5]);
      let offset = (self.home_key.root - ScalePicker::LOWEST_ROOT + shift).rem_euclid(12);

      return Key {
        root: ScalePicker::LOWEST_ROOT + offset,
        scale: self.home_key.scale
      };
    }

    self.home_key
  }
}

fn sibling_wave(wave: WaveShape, pick: usize) -> WaveShape {
  let pick = pick % 2;

  match wave {
    WaveShape::Saw => [WaveShape::Square, WaveShape::Pulse][pick],
    WaveShape::Square => [WaveShape::Saw, WaveShape::Pulse][pick],
    WaveShape::Pulse => [WaveShape::Saw, WaveShape::Square][pick],
    WaveShape::Sine => WaveShape::Triangle,
    WaveShape::Triangle => WaveShape::Sine,
    WaveShape::Noise => WaveShape::Noise
  }
}

#[cfg(test)]
mod tests {
  use super::{has_step, rotate_steps, steps, SongGenome};
  use crate::profile::{amiga_house_95ish, downtempo_breakbeat, dub_deep_house};
  use std::collections::HashSet;

  #[test]
  fn masks_round_trip() {
    let mask = steps(&[0, 4, 15]);
    assert!(has_step(mask, 0) && has_step(mask, 4) && has_step(mask, 15));
    assert!(has_step(rotate_steps(mask, 1), 0));
    assert!(has_step(rotate_steps(mask, 1), 5));
  }

  #[test]
  fn genome_is_deterministic() {
    let profile = amiga_house_95ish();
    assert_eq!(SongGenome::generate(7, &profile), SongGenome::generate(7, &profile));
  }

  #[test]
  fn tempo_varies_inside_profile_range() {
    for profile in [amiga_house_95ish(), downtempo_breakbeat(), dub_deep_house()] {
      let tempos: HashSet<u32> = (0..32)
        .map(|seed| {
          let bpm = SongGenome::generate(seed, &profile).bpm(profile.tempo);
          assert!(bpm >= profile.tempo.min_bpm && bpm <= profile.tempo.max_bpm);
          bpm as u32
        })
        .collect();

      assert!(tempos.len() >= 4, "{} tempos: {tempos:?}", profile.id);
    }
  }

  #[test]
  fn seeds_produce_distinct_identities() {
    let profile = amiga_house_95ish();
    let genomes: Vec<SongGenome> =
      (0..32).map(|seed| SongGenome::generate(seed, &profile)).collect();
    let keys: HashSet<(i16, String)> = genomes
      .iter()
      .map(|genome| (genome.home_key.root, format!("{:?}", genome.home_key.scale)))
      .collect();
    let grooves: HashSet<(u16, u16, u16)> = genomes
      .iter()
      .map(|genome| (genome.groove.stabs, genome.groove.open_hat, genome.groove.hat_base))
      .collect();
    let progressions: HashSet<[u8; 4]> =
      genomes.iter().map(|genome| genome.signature_progression).collect();
    let motifs: HashSet<u16> = genomes.iter().map(|genome| genome.lead_motif.onsets).collect();

    assert!(keys.len() >= 16, "only {} keys", keys.len());
    assert!(grooves.len() >= 20, "only {} grooves", grooves.len());
    assert!(progressions.len() >= 6, "only {} progressions", progressions.len());
    assert!(motifs.len() >= 20, "only {} motifs", motifs.len());
  }

  #[test]
  fn patch_variations_stay_valid() {
    for profile in [amiga_house_95ish(), downtempo_breakbeat(), dub_deep_house()] {
      for seed in 0..32 {
        let genome = SongGenome::generate(seed, &profile);
        let mut varied = profile.clone();
        let mut base = [profile.patches[0]; crate::types::TRACK_COUNT];

        for patch in &profile.patches {
          base[patch.track.as_index()] = *patch;
        }

        varied.patches = genome.patches(&base).to_vec();
        assert!(varied.validate().is_ok());
      }
    }
  }
}
