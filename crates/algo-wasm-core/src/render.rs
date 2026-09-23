use crate::composition::{
  instrument_note_for_track, ArrangementGenerator, Chord, NoteStep, PatternBank, STEPS_PER_BAR
};
use crate::dsp::{DcBlocker, SoftLimiter, StereoDelay};
use crate::export::write_midi;
use crate::genome::SongGenome;
use crate::profile::{amiga_house_95ish, Patch, Profile, ProfileError};
use crate::rng::Rng64;
use crate::synth::{Voice, MAX_VOICES};
use crate::types::{Mood, MusicEvent, SectionKind, TrackId, TRACK_COUNT};
use serde::Serialize;
use std::error::Error;
use std::f32::consts::TAU;
use std::fmt::{Display, Formatter};

/// Per-frame smoothing applied to automation, so that moves never click.
const AUTOMATION_SMOOTHING: f32 = 0.002;

/// Rendering options for offline audio generation.
#[derive(Clone, Copy, Debug)]
pub struct RenderOptions {
  pub sample_rate: u32,
  pub seconds: f32,
  pub seed: u64
}

/// Engine metrics suitable for non-realtime reporting.
#[derive(Clone, Copy, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineMetrics {
  pub rendered_frames: u64,
  pub active_voices: usize,
  pub dropped_voices: u64,
  pub recent_events: usize
}

/// Public track snapshot.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackSnapshot {
  pub id: &'static str,
  pub muted: bool,
  pub solo: bool,
  pub level: f32
}

/// Public state snapshot.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
  pub seed: String,
  pub sample_rate: u32,
  pub bpm: f32,
  pub current_bar: u64,
  pub current_beat: u8,
  pub current_section: &'static str,
  pub energy: f32,
  pub intensity: f32,
  pub is_playing: bool,
  pub tracks: Vec<TrackSnapshot>
}

/// Engine construction and rendering errors.
#[derive(Clone, Debug)]
pub struct EngineError {
  message: String
}

impl EngineError {
  /// Creates an engine error.
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

impl Display for EngineError {
  fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
    formatter.write_str(&self.message)
  }
}

impl Error for EngineError {}

impl From<ProfileError> for EngineError {
  fn from(error: ProfileError) -> Self {
    Self::new(error.message().to_string())
  }
}

/// Deterministic composition and render engine.
pub struct Engine {
  profile: Profile,
  seed: u64,
  sample_rate: f32,
  bpm: f32,
  current_sample: u64,
  next_step_sample: u64,
  step_index: usize,
  bar_index: u64,
  section_bar: u8,
  section_change_pending: bool,
  playing: bool,
  rng: Rng64,
  genome: SongGenome,
  pattern_bank: PatternBank,
  previous_bank: Option<PatternBank>,
  voices: [Voice; MAX_VOICES],
  base_patches: [Patch; TRACK_COUNT],
  patches: [Patch; TRACK_COUNT],
  automation: Automation,
  track_gain: [f32; TRACK_COUNT],
  track_pan: [f32; TRACK_COUNT],
  muted: [bool; TRACK_COUNT],
  solo: [bool; TRACK_COUNT],
  volume: f32,
  energy: f32,
  intensity: f32,
  mood: Mood,
  delay: StereoDelay,
  dc_left: DcBlocker,
  dc_right: DcBlocker,
  limiter: SoftLimiter,
  events: Vec<MusicEvent>,
  metrics: EngineMetrics
}

impl Engine {
  /// Creates an engine with the built-in profile.
  pub fn new_default(sample_rate: u32, seed: u64) -> Result<Self, EngineError> {
    Self::new(sample_rate, seed, amiga_house_95ish())
  }

  /// Creates an engine from a validated profile.
  pub fn new(sample_rate: u32, seed: u64, profile: Profile) -> Result<Self, EngineError> {
    profile.validate()?;

    if sample_rate < profile.limits.min_sample_rate || sample_rate > profile.limits.max_sample_rate {
      return Err(EngineError::new(
        "Sample rate is outside the safe range declared by the profile."
      ));
    }

    let mut rng = Rng64::new(seed);
    let genome = SongGenome::generate(seed, &profile);
    let pattern_bank = PatternBank::generate(
      &mut rng,
      &profile,
      &genome,
      None,
      SectionKind::Intro,
      genome.home_key,
      0.5
    );
    let base_patches = patch_array(&profile)?;
    let patches = genome.patches(&base_patches);
    let (track_gain, track_pan) = mixer_arrays(&profile)?;
    let mut limiter = SoftLimiter::new(profile.mixer.limiter_drive);
    limiter.set_drive(profile.mixer.limiter_drive);

    Ok(Self {
      bpm: genome.bpm(profile.tempo),
      sample_rate: sample_rate as f32,
      seed,
      current_sample: 0,
      next_step_sample: 0,
      step_index: 0,
      bar_index: 0,
      section_bar: 0,
      section_change_pending: true,
      playing: false,
      rng,
      genome,
      pattern_bank,
      previous_bank: None,
      voices: [Voice::silent(); MAX_VOICES],
      base_patches,
      patches,
      automation: Automation::new(),
      track_gain,
      track_pan,
      muted: [false; TRACK_COUNT],
      solo: [false; TRACK_COUNT],
      volume: 0.8,
      energy: 0.5,
      intensity: 0.5,
      mood: Mood::Dark,
      delay: StereoDelay::new(sample_rate as f32, profile.mixer.delay_feedback),
      dc_left: DcBlocker::new(),
      dc_right: DcBlocker::new(),
      limiter,
      events: Vec::with_capacity(profile.limits.max_events_per_block),
      metrics: EngineMetrics::default(),
      profile
    })
  }

  /// Starts playback. Repeated calls are safe.
  pub fn start(&mut self) {
    self.playing = true;
  }

  /// Pauses playback without resetting transport.
  pub fn pause(&mut self) {
    self.playing = false;
  }

  /// Resumes playback.
  pub fn resume(&mut self) {
    self.playing = true;
  }

  /// Stops playback and clears active voices.
  pub fn stop(&mut self) {
    self.playing = false;
    self.current_sample = 0;
    self.next_step_sample = 0;
    self.step_index = 0;
    self.bar_index = 0;
    self.section_bar = 0;
    self.section_change_pending = true;
    self.voices = [Voice::silent(); MAX_VOICES];
  }

  /// Re-seeds the engine and regenerates the song identity and material.
  pub fn set_seed(&mut self, seed: u64) {
    self.seed = seed;
    self.rng = Rng64::new(seed);
    self.genome = SongGenome::generate(seed, &self.profile);
    self.bpm = self.genome.bpm(self.profile.tempo);
    self.patches = self.genome.patches(&self.base_patches);
    self.automation = Automation::new();
    self.previous_bank = None;
    self.pattern_bank = PatternBank::generate(
      &mut self.rng,
      &self.profile,
      &self.genome,
      None,
      SectionKind::Intro,
      self.genome.home_key,
      self.energy
    );
    self.stop();
  }

  /// Applies a new profile.
  pub fn set_profile(&mut self, profile: Profile) -> Result<(), EngineError> {
    profile.validate()?;
    self.base_patches = patch_array(&profile)?;
    let (gain, pan) = mixer_arrays(&profile)?;
    self.track_gain = gain;
    self.track_pan = pan;
    self.limiter.set_drive(profile.mixer.limiter_drive);
    self.delay = StereoDelay::new(self.sample_rate, profile.mixer.delay_feedback);
    self.profile = profile;
    self.set_seed(self.seed);
    Ok(())
  }

  /// Sets master volume, clamped to a safe range.
  pub fn set_volume(&mut self, volume: f32) {
    self.volume = volume.clamp(0.0, 1.0);
  }

  /// Sets energy, clamped to a musical range.
  pub fn set_energy(&mut self, value: f32) {
    self.energy = value.clamp(0.0, 1.0);
  }

  /// Sets intensity, clamped to a musical range.
  pub fn set_intensity(&mut self, value: f32) {
    self.intensity = value.clamp(0.0, 1.0);
  }

  /// Sets mood.
  pub fn set_mood(&mut self, mood: Mood) {
    self.mood = mood;
  }

  /// Sets a valid tempo range and picks the song's tempo inside it.
  ///
  /// The song keeps its relative place in the range, so a seed that plays
  /// near the top of the profile range also plays near the top of the new one.
  pub fn set_tempo_range(&mut self, min_bpm: f32, max_bpm: f32) -> Result<(), EngineError> {
    if min_bpm > max_bpm
      || min_bpm < self.profile.limits.min_bpm
      || max_bpm > self.profile.limits.max_bpm
    {
      return Err(EngineError::new(
        "Tempo range must stay ordered and inside the profile safety limits."
      ));
    }

    self.profile.tempo.min_bpm = min_bpm;
    self.profile.tempo.max_bpm = max_bpm;
    self.bpm = self.genome.bpm_in_range(min_bpm, max_bpm);
    Ok(())
  }

  /// Mutes or unmutes a track.
  pub fn set_muted(&mut self, track: TrackId, muted: bool) {
    self.muted[track.as_index()] = muted;
  }

  /// Solos or unsolos a track.
  pub fn set_solo(&mut self, track: TrackId, solo: bool) {
    self.solo[track.as_index()] = solo;
  }

  /// Renders interleaved stereo samples.
  pub fn render_interleaved(&mut self, output: &mut [f32]) {
    self.events.clear();

    if output.is_empty() {
      return;
    }

    if !self.playing {
      output.fill(0.0);
      return;
    }

    let frames = output.len() / 2;

    for frame in 0..frames {
      while self.current_sample >= self.next_step_sample {
        let step = self.step_index;
        self.trigger_step();
        self.next_step_sample = self.next_step_sample.saturating_add(self.step_duration(step));
      }

      let (mut left, mut right) = self.render_frame();
      left = self.dc_left.process(left);
      right = self.dc_right.process(right);
      left = self.limiter.process(left * self.volume);
      right = self.limiter.process(right * self.volume);

      output[frame * 2] = safe_sample(left);
      output[frame * 2 + 1] = safe_sample(right);
      self.current_sample = self.current_sample.saturating_add(1);
      self.metrics.rendered_frames = self.metrics.rendered_frames.saturating_add(1);
    }

    if output.len() % 2 == 1 {
      output[output.len() - 1] = 0.0;
    }

    self.metrics.active_voices = self.voices.iter().filter(|voice| voice.active).count();
    self.metrics.recent_events = self.events.len();
  }

  /// Renders offline stereo audio.
  pub fn render_offline(options: RenderOptions, profile: Profile) -> Result<Vec<f32>, EngineError> {
    let mut engine = Engine::new(options.sample_rate, options.seed, profile)?;
    engine.start();
    let frames = (options.seconds.max(0.0) * options.sample_rate as f32) as usize;
    let mut output = vec![0.0; frames * 2];
    engine.render_interleaved(&mut output);
    Ok(output)
  }

  /// Returns a public snapshot.
  pub fn snapshot(&self) -> Snapshot {
    let tracks = TrackId::ALL
      .iter()
      .map(|track| TrackSnapshot {
        id: track.as_str(),
        muted: self.muted[track.as_index()],
        solo: self.solo[track.as_index()],
        level: self.track_gain[track.as_index()]
      })
      .collect();

    Snapshot {
      seed: self.seed.to_string(),
      sample_rate: self.sample_rate as u32,
      bpm: self.bpm,
      current_bar: self.bar_index,
      current_beat: (self.step_index / 4 + 1) as u8,
      current_section: self.pattern_bank.section.as_str(),
      energy: self.energy,
      intensity: self.intensity,
      is_playing: self.playing,
      tracks
    }
  }

  /// Returns JSON for the current snapshot.
  pub fn snapshot_json(&self) -> Result<String, EngineError> {
    serde_json::to_string(&self.snapshot())
      .map_err(|error| EngineError::new(format!("Snapshot could not be written as JSON: {error}.")))
  }

  /// Returns recent scheduler events.
  pub fn current_events(&self) -> &[MusicEvent] {
    &self.events
  }

  /// Returns recent events as JSON.
  pub fn current_events_json(&self) -> Result<String, EngineError> {
    serde_json::to_string(&self.events)
      .map_err(|error| EngineError::new(format!("Events could not be written as JSON: {error}.")))
  }

  /// Returns current metrics.
  pub fn metrics(&self) -> EngineMetrics {
    self.metrics
  }

  /// Exports the current four-bar phrase as a small MIDI file.
  pub fn export_midi(&self) -> Vec<u8> {
    write_midi(&self.pattern_bank, self.bpm)
  }

  fn trigger_step(&mut self) {
    if self.step_index == 0 {
      self.push_event(MusicEvent::Bar {
        sample: self.current_sample,
        bar: self.bar_index
      });

      if self.section_change_pending {
        self.push_event(MusicEvent::SectionChange {
          sample: self.current_sample,
          section: self.pattern_bank.section,
          bar: self.bar_index
        });
        self.section_change_pending = false;
      }
    }

    if self.step_index % 4 == 0 {
      self.push_event(MusicEvent::Beat {
        sample: self.current_sample,
        bar: self.bar_index,
        beat: (self.step_index / 4 + 1) as u8
      });
    }

    let bar = *self.pattern_bank.bar_at(self.section_bar);
    let chord = self.pattern_bank.chord_at(self.section_bar);
    let step = self.step_index;
    self.update_automation_targets();

    if bar.kick[step] {
      self.start_voice(TrackId::Kick, NoteStep::new(36, 0.92, 2, true, false));
    }

    if bar.snare[step] > 0.0 {
      self.start_voice(TrackId::Snare, NoteStep::new(38, bar.snare[step], 2, false, false));
    }

    if bar.closed_hat[step] > 0.0 {
      self.start_voice(
        TrackId::ClosedHat,
        NoteStep::new(42, bar.closed_hat[step], 1, false, false)
      );
    }

    if bar.open_hat[step] > 0.0 {
      self.start_voice(
        TrackId::OpenHat,
        NoteStep::new(46, bar.open_hat[step], 4, false, false)
      );
    }

    self.start_optional(TrackId::Percussion, bar.percussion[step]);
    self.start_optional(TrackId::Bass, bar.bass[step]);
    self.start_chord_optional(TrackId::Chords, bar.chords[step], chord);
    self.start_chord_optional(TrackId::Pad, bar.pad[step], chord);
    self.start_optional(TrackId::Lead, bar.lead[step]);
    self.start_optional(TrackId::Arp, bar.arp[step]);
    self.start_optional(TrackId::Fx, bar.fx[step]);

    self.advance_step();
  }

  fn start_optional(&mut self, track: TrackId, note: Option<NoteStep>) {
    if let Some(note) = note {
      self.start_voice(track, note);
    }
  }

  fn start_chord_optional(&mut self, track: TrackId, note: Option<NoteStep>, chord: Chord) {
    if let Some(note) = note {
      for index in 0..chord.count {
        let tone = chord.tone(index);
        self.start_voice(
          track,
          NoteStep::new(tone, note.velocity * 0.78, note.length_steps, note.accent, note.slide)
        );
      }
    }
  }

  fn start_voice(&mut self, track: TrackId, note: NoteStep) {
    let note = instrument_note_for_track(track, note);
    let length_samples = self.samples_per_step().saturating_mul(u64::from(note.length_steps)) as u32;
    let mut selected = None;

    for (index, voice) in self.voices.iter().enumerate() {
      if !voice.active {
        selected = Some(index);
        break;
      }
    }

    if selected.is_none() {
      selected = self
        .voices
        .iter()
        .enumerate()
        .max_by_key(|(_, voice)| voice.age_samples)
        .map(|(index, _)| index);
      self.metrics.dropped_voices = self.metrics.dropped_voices.saturating_add(1);
    }

    if let Some(index) = selected {
      self.voices[index].start(
        track,
        note.note,
        note.velocity,
        length_samples,
        self.sample_rate,
        note.accent,
        note.slide
      );
      self.push_event(MusicEvent::NoteOn {
        sample: self.current_sample,
        track,
        note: note.note,
        velocity: note.velocity
      });
    }
  }

  fn advance_step(&mut self) {
    if self.step_index + 1 < STEPS_PER_BAR {
      self.step_index += 1;
      return;
    }

    self.step_index = 0;
    self.bar_index = self.bar_index.saturating_add(1);
    self.section_bar = self.section_bar.saturating_add(1);

    if self.section_bar >= self.pattern_bank.length_bars {
      let current = self.pattern_bank.section;
      let next = ArrangementGenerator::next(
        &mut self.rng,
        current,
        self.energy,
        self.profile.arrangement_rules.reset_chance
      );
      let key = self.genome.key_for_section(&mut self.rng, &self.profile, next);
      self.previous_bank = Some(self.pattern_bank);
      self.pattern_bank = PatternBank::generate(
        &mut self.rng,
        &self.profile,
        &self.genome,
        self.previous_bank.as_ref(),
        next,
        key,
        self.energy
      );
      self.section_bar = 0;
      self.section_change_pending = true;
    }
  }

  fn render_frame(&mut self) -> (f32, f32) {
    let any_solo = self.solo.iter().any(|value| *value);
    let mut left = 0.0;
    let mut right = 0.0;
    let mut send = 0.0;
    let mood_gain = match self.mood {
      Mood::Dark => 1.0,
      Mood::Bright => 1.04,
      Mood::Tense => 1.07,
      Mood::Calm => 0.9
    };

    self.automation.smooth();
    let automation = &self.automation;

    for voice in &mut self.voices {
      if !voice.active {
        continue;
      }

      let index = voice.track.as_index();
      let audible = !self.muted[index] && (!any_solo || self.solo[index]);
      let mut patch = self.patches[index];
      patch.filter_cutoff *= automation.cutoff[index];
      patch.delay_send = (patch.delay_send * automation.send[index]).clamp(0.0, 1.0);
      patch.reverb_send = (patch.reverb_send * automation.send[index]).clamp(0.0, 1.0);
      let mono = voice.render(patch, &self.genome.drums, self.sample_rate);

      if !audible {
        continue;
      }

      let gain = self.track_gain[index] * (0.75 + self.intensity * 0.35) * mood_gain;
      let pan = (self.track_pan[index] + patch.pan + automation.pan[index]).clamp(-1.0, 1.0);
      let (left_gain, right_gain) = pan_gains(pan);
      left += mono * gain * left_gain;
      right += mono * gain * right_gain;
      send += (patch.delay_send + patch.reverb_send * self.profile.mixer.reverb_mix).clamp(0.0, 1.0)
        * mono.abs()
        * 0.15;
    }

    let (delay_left, delay_right) = self.delay.process(left, right, send.clamp(0.0, 0.6));

    (
      (left + delay_left * 0.35) * self.profile.mixer.master_gain,
      (right + delay_right * 0.35) * self.profile.mixer.master_gain
    )
  }

  fn push_event(&mut self, event: MusicEvent) {
    if self.events.len() < self.profile.limits.max_events_per_block {
      self.events.push(event);
    }
  }

  fn samples_per_step(&self) -> u64 {
    let beats_per_second = self.bpm / 60.0;
    let sixteenths_per_second = beats_per_second * 4.0;
    (self.sample_rate / sixteenths_per_second).max(1.0) as u64
  }

  /// Returns the time until the step after `step`. Swing lengthens each
  /// on-beat 16th and shortens the following off-beat 16th by the same amount,
  /// so bars keep their length.
  fn step_duration(&self, step: usize) -> u64 {
    let base = self.samples_per_step() as f32;
    let swing = self.genome.groove.swing;
    let duration = if step % 2 == 0 {
      base * (1.0 + swing)
    } else {
      base * (1.0 - swing)
    };

    (duration.round() as u64).max(1)
  }

  /// Sets automation targets from section shape, energy, mood, and each
  /// track's slow seeded LFO. `automationRules` scale every movement.
  fn update_automation_targets(&mut self) {
    let rules = self.profile.automation_rules;
    let bank = &self.pattern_bank;
    let step = self.step_index as f32;
    let section_steps = (f32::from(bank.length_bars) * STEPS_PER_BAR as f32).max(1.0);
    let elapsed = f32::from(self.section_bar) * STEPS_PER_BAR as f32 + step;
    let progress = (elapsed / section_steps).clamp(0.0, 1.0);
    let section_curve = match bank.section {
      SectionKind::Build => -0.9 + 1.5 * progress,
      SectionKind::Breakdown => -0.8 + 0.4 * progress,
      SectionKind::Intro => -0.7 + 0.7 * progress,
      SectionKind::Drop => 0.25,
      SectionKind::Transition => 0.3 - 0.9 * progress,
      SectionKind::Reset => -0.6,
      _ => 0.0
    } * rules.filter_motion
      * 2.0;
    let tilt = (self.energy - 0.5) * 0.6
      + match self.mood {
        Mood::Dark => 0.0,
        Mood::Bright => 0.35,
        Mood::Tense => 0.15,
        Mood::Calm => -0.3
      };
    let open_space =
      matches!(bank.section, SectionKind::Breakdown | SectionKind::Transition | SectionKind::Intro);
    let wet = if open_space {
      rules.send_motion
    } else {
      0.0
    };
    let position = self.bar_index as f32 + step / STEPS_PER_BAR as f32;

    for (index, shape) in self.genome.automation.iter().enumerate() {
      let cycle = position / shape.period_bars + shape.phase;
      let lfo = (cycle * TAU).sin();
      let slow = ((cycle * 0.5 + 0.25) * TAU).sin();
      let octaves = section_curve + tilt + shape.filter_depth * lfo;
      self.automation.cutoff_target[index] = 2.0_f32.powf(octaves);
      self.automation.send_target[index] = (1.0 + shape.send_depth * slow + wet).max(0.0);
      self.automation.pan_target[index] = shape.pan_depth * ((cycle * 0.5 + 0.5) * TAU).sin();
    }
  }
}

/// Smoothed per-track automation values.
#[derive(Clone, Copy, Debug)]
struct Automation {
  cutoff: [f32; TRACK_COUNT],
  send: [f32; TRACK_COUNT],
  pan: [f32; TRACK_COUNT],
  cutoff_target: [f32; TRACK_COUNT],
  send_target: [f32; TRACK_COUNT],
  pan_target: [f32; TRACK_COUNT]
}

impl Automation {
  const fn new() -> Self {
    Self {
      cutoff: [1.0; TRACK_COUNT],
      send: [1.0; TRACK_COUNT],
      pan: [0.0; TRACK_COUNT],
      cutoff_target: [1.0; TRACK_COUNT],
      send_target: [1.0; TRACK_COUNT],
      pan_target: [0.0; TRACK_COUNT]
    }
  }

  fn smooth(&mut self) {
    for index in 0..TRACK_COUNT {
      self.cutoff[index] += (self.cutoff_target[index] - self.cutoff[index]) * AUTOMATION_SMOOTHING;
      self.send[index] += (self.send_target[index] - self.send[index]) * AUTOMATION_SMOOTHING;
      self.pan[index] += (self.pan_target[index] - self.pan[index]) * AUTOMATION_SMOOTHING;
    }
  }
}

fn patch_array(profile: &Profile) -> Result<[Patch; TRACK_COUNT], EngineError> {
  let first = profile
    .patches
    .first()
    .copied()
    .ok_or_else(|| EngineError::new("Profile must include patches."))?;
  let mut patches = [first; TRACK_COUNT];

  for patch in &profile.patches {
    patches[patch.track.as_index()] = *patch;
  }

  Ok(patches)
}

fn mixer_arrays(profile: &Profile) -> Result<([f32; TRACK_COUNT], [f32; TRACK_COUNT]), EngineError> {
  let mut gain = [1.0; TRACK_COUNT];
  let mut pan = [0.0; TRACK_COUNT];

  for track in &profile.mixer.tracks {
    gain[track.track.as_index()] = track.gain;
    pan[track.track.as_index()] = track.pan;
  }

  Ok((gain, pan))
}

fn pan_gains(pan: f32) -> (f32, f32) {
  let left = (1.0 - pan).clamp(0.0, 2.0) * 0.5;
  let right = (1.0 + pan).clamp(0.0, 2.0) * 0.5;
  (left.sqrt(), right.sqrt())
}

fn safe_sample(value: f32) -> f32 {
  if value.is_finite() {
    value.clamp(-1.0, 1.0)
  } else {
    0.0
  }
}

#[cfg(test)]
mod tests {
  use super::{Engine, EngineError, RenderOptions};
  use crate::profile::{amiga_house_95ish, downtempo_breakbeat, dub_deep_house};

  #[test]
  fn render_outputs_finite_audio() {
    let profile = amiga_house_95ish();
    let mut engine = Engine::new(48_000, 123, profile).expect("engine should create");
    engine.start();
    let mut output = vec![0.0; 128 * 2];
    engine.render_interleaved(&mut output);

    assert!(output.iter().all(|sample| sample.is_finite()));
    assert!(output.iter().any(|sample| sample.abs() > 0.0001));
  }

  #[test]
  fn deterministic_render_matches() {
    let profile = amiga_house_95ish();
    let mut left = Engine::new(48_000, 555, profile.clone()).expect("engine should create");
    let mut right = Engine::new(48_000, 555, profile).expect("engine should create");
    left.start();
    right.start();
    let mut left_output = vec![0.0; 512 * 2];
    let mut right_output = vec![0.0; 512 * 2];
    left.render_interleaved(&mut left_output);
    right.render_interleaved(&mut right_output);

    assert_eq!(left_output, right_output);
  }

  #[test]
  fn offline_render_is_non_silent() {
    let profile = amiga_house_95ish();
    let output = Engine::render_offline(
      RenderOptions {
        sample_rate: 44_100,
        seconds: 0.25,
        seed: 42
      },
      profile
    )
    .expect("offline render should work");

    assert!(output.iter().any(|sample| sample.abs() > 0.0001));
  }

  #[test]
  fn different_seeds_sound_different() -> Result<(), EngineError> {
    let profile = amiga_house_95ish();
    let mut tempos = std::collections::HashSet::new();
    let mut renders = Vec::new();

    for seed in 0..8 {
      let mut engine = Engine::new(22_050, seed, profile.clone())?;
      engine.start();
      let mut output = vec![0.0; 22_050 * 2];
      engine.render_interleaved(&mut output);
      tempos.insert(engine.snapshot().bpm as u32);
      renders.push(output);
    }

    assert!(tempos.len() >= 3, "tempos: {tempos:?}");

    for left in 0..renders.len() {
      for right in left + 1..renders.len() {
        assert_ne!(renders[left], renders[right]);
      }
    }

    Ok(())
  }

  #[test]
  fn long_renders_stay_bounded_for_every_profile() -> Result<(), EngineError> {
    for profile in [amiga_house_95ish(), downtempo_breakbeat(), dub_deep_house()] {
      for (seed, energy) in [(3, 0.1), (11, 0.95)] {
        let mut engine = Engine::new(22_050, seed, profile.clone())?;
        engine.set_energy(energy);
        engine.start();
        let mut output = vec![0.0; 22_050 * 2 * 20];
        engine.render_interleaved(&mut output);
        let power: f32 = output.iter().map(|sample| sample * sample).sum();
        let rms = (power / output.len() as f32).sqrt();

        assert!(output.iter().all(|sample| sample.is_finite() && sample.abs() <= 1.0));
        assert!(rms > 0.01 && rms < 0.5, "{} seed {seed} rms {rms}", profile.id);
      }
    }

    Ok(())
  }

  #[test]
  fn tempo_range_keeps_song_position() -> Result<(), EngineError> {
    let profile = amiga_house_95ish();
    let mut engine = Engine::new(48_000, 9, profile)?;
    let position = engine.genome.tempo_position;
    assert!(engine.set_tempo_range(100.0, 140.0).is_ok());
    let expected = (100.0 + 40.0 * position).round();

    assert_eq!(engine.snapshot().bpm, expected);
    Ok(())
  }

  #[test]
  fn stop_clears_stuck_notes() {
    let profile = amiga_house_95ish();
    let mut engine = Engine::new(48_000, 5, profile).expect("engine should create");
    engine.start();
    let mut output = vec![0.0; 128 * 2];
    engine.render_interleaved(&mut output);
    engine.stop();
    engine.render_interleaved(&mut output);

    assert!(output.iter().all(|sample| *sample == 0.0));
  }
}
