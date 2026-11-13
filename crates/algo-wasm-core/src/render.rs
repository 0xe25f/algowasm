use crate::builtin::amiga_house_95ish;
use crate::composition::{
  instrument_note_for_track, ArrangementGenerator, Chord, NoteStep, PatternBank, STEPS_PER_BAR
};
use crate::dsp::{DcBlocker, Limiter, Reverb, StereoDelay};
use crate::export::write_midi;
use crate::genome::SongGenome;
use crate::profile::{Patch, Profile, ProfileError};
use crate::rng::Rng64;
use crate::synth::{RenderContext, Voice, VoiceStart, CONTROL_INTERVAL, MAX_VOICES};
use crate::types::{Mood, MusicEvent, SectionKind, TrackId, EVENT_WIRE_SIZE, TRACK_COUNT};
use std::error::Error;
use std::f32::consts::TAU;
use std::fmt::{Display, Formatter};

/// Per-frame smoothing applied to automation, so that moves never click.
const AUTOMATION_SMOOTHING: f32 = 0.002;

/// Level of the delay return in the mix.
const DELAY_RETURN: f32 = 0.9;

/// Level of the reverb return in the mix, before the profile's `reverbMix`.
const REVERB_RETURN: f32 = 2.4;

/// Number of `f64` values in the binary snapshot.
///
/// The layout is `[seed high, seed low, sample rate, bpm, bar, beat,
/// section, energy, intensity, playing]`, then `[muted, solo, level]` for each
/// track in `TrackId::ALL` order. JavaScript decodes this layout, so it must
/// stay in step with the package's decoder.
pub const SNAPSHOT_WIRE_LEN: usize = 10 + TRACK_COUNT * 3;

/// Rendering options for offline audio generation.
#[derive(Clone, Copy, Debug)]
pub struct RenderOptions {
  pub sample_rate: u32,
  pub seconds: f32,
  pub seed: u64
}

/// Engine metrics suitable for non-realtime reporting.
#[derive(Clone, Copy, Debug, Default)]
pub struct EngineMetrics {
  pub rendered_frames: u64,
  pub active_voices: usize,
  pub dropped_voices: u64,
  /// Events discarded because the buffer was full before it was drained.
  pub dropped_events: u64
}

/// Public track snapshot.
#[derive(Clone, Copy, Debug)]
pub struct TrackSnapshot {
  pub id: &'static str,
  pub muted: bool,
  pub solo: bool,
  pub level: f32
}

/// Public state snapshot.
#[derive(Clone, Copy, Debug)]
pub struct Snapshot {
  pub seed: u64,
  pub sample_rate: u32,
  pub bpm: f32,
  pub current_bar: u64,
  pub current_beat: u8,
  pub current_section: SectionKind,
  pub energy: f32,
  pub intensity: f32,
  pub is_playing: bool,
  pub tracks: [TrackSnapshot; TRACK_COUNT]
}

impl Snapshot {
  /// Encodes the snapshot in the layout described on `SNAPSHOT_WIRE_LEN`.
  pub fn to_wire(&self) -> [f64; SNAPSHOT_WIRE_LEN] {
    let mut wire = [0.0; SNAPSHOT_WIRE_LEN];
    wire[0] = (self.seed >> 32) as f64;
    wire[1] = (self.seed & 0xffff_ffff) as f64;
    wire[2] = f64::from(self.sample_rate);
    wire[3] = f64::from(self.bpm);
    wire[4] = self.current_bar as f64;
    wire[5] = f64::from(self.current_beat);
    wire[6] = f64::from(self.current_section.as_index());
    wire[7] = f64::from(self.energy);
    wire[8] = f64::from(self.intensity);
    wire[9] = f64::from(u8::from(self.is_playing));

    for (index, track) in self.tracks.iter().enumerate() {
      let offset = 10 + index * 3;
      wire[offset] = f64::from(u8::from(track.muted));
      wire[offset + 1] = f64::from(u8::from(track.solo));
      wire[offset + 2] = f64::from(track.level);
    }

    wire
  }
}

/// Engine construction and control errors.
#[derive(Clone, Copy, Debug)]
pub struct EngineError {
  message: &'static str
}

impl EngineError {
  /// Creates an engine error.
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

impl Display for EngineError {
  fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
    formatter.write_str(self.message)
  }
}

impl Error for EngineError {}

impl From<ProfileError> for EngineError {
  fn from(error: ProfileError) -> Self {
    Self::new(error.message())
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
  voice_limit: usize,
  patches: [Patch; TRACK_COUNT],
  automation: Automation,
  track_gain: [f32; TRACK_COUNT],
  track_pan: [f32; TRACK_COUNT],
  muted: [bool; TRACK_COUNT],
  solo: [bool; TRACK_COUNT],
  volume: f32,
  volume_smoothed: f32,
  energy: f32,
  intensity: f32,
  mood: Mood,
  duck: f32,
  duck_target: f32,
  delay: StereoDelay,
  reverb: Reverb,
  dc_left: DcBlocker,
  dc_right: DcBlocker,
  limiter: Limiter,
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
    let bpm = genome.bpm(profile.tempo);
    let rate = sample_rate as f32;
    let mut delay = StereoDelay::new(rate, profile.mixer.delay_feedback);
    delay.set_tempo(bpm, rate);
    let (track_gain, track_pan) = mixer_arrays(&profile);

    Ok(Self {
      bpm,
      sample_rate: rate,
      seed,
      current_sample: 0,
      next_step_sample: 0,
      step_index: 0,
      bar_index: 0,
      section_bar: 0,
      section_change_pending: true,
      playing: false,
      rng,
      patches: genome.patches(&profile.patches),
      genome,
      pattern_bank,
      previous_bank: None,
      voices: [Voice::silent(); MAX_VOICES],
      voice_limit: profile.limits.max_voices.min(MAX_VOICES),
      automation: Automation::new(),
      track_gain,
      track_pan,
      muted: [false; TRACK_COUNT],
      solo: [false; TRACK_COUNT],
      volume: 0.8,
      volume_smoothed: 0.8,
      energy: 0.5,
      intensity: 0.5,
      mood: Mood::Dark,
      duck: 0.0,
      duck_target: 0.0,
      delay,
      reverb: Reverb::new(rate),
      dc_left: DcBlocker::new(),
      dc_right: DcBlocker::new(),
      limiter: Limiter::new(rate, profile.mixer.limiter_drive),
      // Allocated once at the largest size a profile may ask for, so that
      // changing profile never allocates.
      events: Vec::with_capacity(crate::profile::MAX_EVENTS_LIMIT),
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

  /// Stops playback, rewinds the transport, and clears voices and effects.
  pub fn stop(&mut self) {
    self.playing = false;
    self.current_sample = 0;
    self.next_step_sample = 0;
    self.step_index = 0;
    self.bar_index = 0;
    self.section_bar = 0;
    self.section_change_pending = true;
    self.voices = [Voice::silent(); MAX_VOICES];
    self.duck = 0.0;
    self.duck_target = 0.0;
    self.delay.reset();
    self.reverb.reset();
    self.limiter.reset();
    self.dc_left = DcBlocker::new();
    self.dc_right = DcBlocker::new();
  }

  /// Re-seeds the engine and regenerates the song identity and material.
  pub fn set_seed(&mut self, seed: u64) {
    self.seed = seed;
    self.rng = Rng64::new(seed);
    self.genome = SongGenome::generate(seed, &self.profile);
    self.bpm = self.genome.bpm(self.profile.tempo);
    self.delay.set_tempo(self.bpm, self.sample_rate);
    self.patches = self.genome.patches(&self.profile.patches);
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

  /// Applies a new profile and restarts the song from its first bar.
  pub fn set_profile(&mut self, profile: Profile) -> Result<(), EngineError> {
    profile.validate()?;

    if (self.sample_rate as u32) < profile.limits.min_sample_rate
      || (self.sample_rate as u32) > profile.limits.max_sample_rate
    {
      return Err(EngineError::new(
        "Sample rate is outside the safe range declared by the profile."
      ));
    }

    let (gain, pan) = mixer_arrays(&profile);
    self.track_gain = gain;
    self.track_pan = pan;
    self.limiter.set_drive(profile.mixer.limiter_drive);
    self.delay.set_feedback(profile.mixer.delay_feedback);
    self.voice_limit = profile.limits.max_voices.min(MAX_VOICES);
    self.profile = profile;
    self.set_seed(self.seed);
    Ok(())
  }

  /// Sets master volume, clamped to a safe range.
  pub fn set_volume(&mut self, volume: f32) {
    self.volume = unit_or(volume, self.volume);
  }

  /// Sets energy, clamped to a musical range.
  pub fn set_energy(&mut self, value: f32) {
    self.energy = unit_or(value, self.energy);
  }

  /// Sets intensity, clamped to a musical range.
  pub fn set_intensity(&mut self, value: f32) {
    self.intensity = unit_or(value, self.intensity);
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
    // Written as a positive test so that NaN fails it: every comparison with
    // NaN is false, which the earlier "reject if out of range" form let through.
    let valid = min_bpm.is_finite()
      && max_bpm.is_finite()
      && min_bpm <= max_bpm
      && min_bpm >= self.profile.limits.min_bpm
      && max_bpm <= self.profile.limits.max_bpm;

    if !valid {
      return Err(EngineError::new(
        "Tempo range must be finite, ordered, and inside the profile safety limits."
      ));
    }

    self.profile.tempo.min_bpm = min_bpm;
    self.profile.tempo.max_bpm = max_bpm;
    self.profile.tempo.default_bpm = self.profile.tempo.default_bpm.clamp(min_bpm, max_bpm);
    self.bpm = self.genome.bpm_in_range(min_bpm, max_bpm);
    self.delay.set_tempo(self.bpm, self.sample_rate);
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
  ///
  /// Scheduler events accumulate across calls until `drain_events` or
  /// `clear_events` is called, so a caller that reads them less often than it
  /// renders does not lose any.
  pub fn render_interleaved(&mut self, output: &mut [f32]) {
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

      let (left, right) = self.render_frame();
      let (left, right) =
        self.limiter.process(self.dc_left.process(left), self.dc_right.process(right));

      // Volume sits after the limiter, so turning down really is quieter
      // rather than just limiting less. It is smoothed to avoid zipper noise.
      self.volume_smoothed += (self.volume - self.volume_smoothed) * AUTOMATION_SMOOTHING;
      output[frame * 2] = left * self.volume_smoothed;
      output[frame * 2 + 1] = right * self.volume_smoothed;
      self.current_sample = self.current_sample.saturating_add(1);
      self.metrics.rendered_frames = self.metrics.rendered_frames.saturating_add(1);
    }

    if output.len() % 2 == 1 {
      output[output.len() - 1] = 0.0;
    }

    self.metrics.active_voices = self.voices.iter().filter(|voice| voice.active).count();
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
    Snapshot {
      seed: self.seed,
      sample_rate: self.sample_rate as u32,
      bpm: self.bpm,
      current_bar: self.bar_index,
      current_beat: (self.step_index / 4 + 1) as u8,
      current_section: self.pattern_bank.section,
      energy: self.energy,
      intensity: self.intensity,
      is_playing: self.playing,
      tracks: TrackId::ALL.map(|track| TrackSnapshot {
        id: track.as_str(),
        muted: self.muted[track.as_index()],
        solo: self.solo[track.as_index()],
        level: self.track_gain[track.as_index()]
      })
    }
  }

  /// Returns the scheduler events buffered since the last drain.
  pub fn current_events(&self) -> &[MusicEvent] {
    &self.events
  }

  /// Discards the buffered scheduler events.
  pub fn clear_events(&mut self) {
    self.events.clear();
  }

  /// Writes the buffered events into `output` as groups of `EVENT_WIRE_SIZE`
  /// values, clears the buffer, and returns the number of events written.
  pub fn drain_events(&mut self, output: &mut [f64]) -> usize {
    let count = self.events.len().min(output.len() / EVENT_WIRE_SIZE);

    for (event, slot) in self.events.iter().zip(output.chunks_exact_mut(EVENT_WIRE_SIZE)) {
      slot.copy_from_slice(&event.to_wire());
    }

    self.events.clear();
    count
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
      // Each kick pushes the ducking envelope to full depth.
      self.duck_target = 1.0;
    }

    if bar.snare[step] > 0.0 {
      self.start_voice(TrackId::Snare, NoteStep::new(38, bar.snare[step], 2, false, false));
    }

    if bar.closed_hat[step] > 0.0 {
      // A closed hi-hat silences a ringing open one, as on a real kit.
      for voice in &mut self.voices {
        if voice.track == TrackId::OpenHat {
          voice.choke(self.sample_rate);
        }
      }

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
    let pool = &mut self.voices[..self.voice_limit.max(1)];
    let free = pool.iter().position(|voice| !voice.active);
    let index = free.unwrap_or_else(|| {
      // Every voice is busy, so the oldest one is reused. The voice fades its
      // old sound out as the new one starts, so the steal does not click.
      self.metrics.dropped_voices = self.metrics.dropped_voices.saturating_add(1);
      pool
        .iter()
        .enumerate()
        .max_by_key(|(_, voice)| voice.age_samples)
        .map_or(0, |(index, _)| index)
    });

    pool[index].start(
      VoiceStart {
        track,
        note: note.note,
        velocity: note.velocity,
        length_samples,
        accent: note.accent,
        slide: note.slide,
        seed: self.current_sample as u32
      },
      &self.patches[track.as_index()],
      self.sample_rate
    );
    self.push_event(MusicEvent::NoteOn {
      sample: self.current_sample,
      track,
      note: note.note,
      velocity: note.velocity
    });
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
    let mut delay_left = 0.0;
    let mut delay_right = 0.0;
    let mut reverb_send = 0.0;
    let mood_gain = match self.mood {
      Mood::Dark => 1.0,
      Mood::Bright => 1.04,
      Mood::Tense => 1.07,
      Mood::Calm => 0.9
    };

    self.automation.smooth();

    // The ducking envelope jumps to full on each kick and recovers over about
    // a third of a beat. A fast follower smooths the jump so it cannot click.
    let recovery = 1.0 / (self.sample_rate * 20.0 / self.bpm);
    self.duck_target = (self.duck_target - recovery).max(0.0);
    self.duck += (self.duck_target - self.duck) * 0.01;

    let context = RenderContext {
      sample_rate: self.sample_rate,
      beat: self.current_sample as f64 / (self.samples_per_step() * 4) as f64
    };
    let automation = &self.automation;
    let level = (0.75 + self.intensity * 0.35) * mood_gain;

    for voice in &mut self.voices {
      if !voice.active {
        continue;
      }

      let index = voice.track.as_index();
      let audible = !self.muted[index] && (!any_solo || self.solo[index]);
      let patch = &self.patches[index];

      // Pan and level move slowly, so each voice's mix gains are refreshed
      // every few samples rather than paying for two square roots per sample.
      if voice.age_samples % CONTROL_INTERVAL == 0 {
        let pan = (self.track_pan[index]
          + patch.pan
          + automation.pan[index]
          + patch.width * voice.spread)
          .clamp(-1.0, 1.0);
        let (left_gain, right_gain) = pan_gains(pan);
        let gain = self.track_gain[index] * level * (1.0 - patch.duck * self.duck);
        voice.mix_left = gain * left_gain;
        voice.mix_right = gain * right_gain;
      }

      let mono = voice.render(patch, automation.cutoff[index], &self.genome.drums, &context);

      if !audible {
        continue;
      }

      let voice_left = mono * voice.mix_left;
      let voice_right = mono * voice.mix_right;
      left += voice_left;
      right += voice_right;

      // Each voice feeds the effect buses with its own signal, scaled by its
      // own send amounts, so a track with no send stays out of the effects.
      let delay_amount = (patch.delay_send * automation.send[index]).clamp(0.0, 1.0);
      let reverb_amount = (patch.reverb_send * automation.send[index]).clamp(0.0, 1.0);
      delay_left += voice_left * delay_amount;
      delay_right += voice_right * delay_amount;
      reverb_send += (voice_left + voice_right) * 0.5 * reverb_amount;
    }

    let (echo_left, echo_right) = self.delay.process(delay_left, delay_right);
    let (tail_left, tail_right) = self.reverb.process(reverb_send);
    let reverb_level = REVERB_RETURN * self.profile.mixer.reverb_mix;
    let master = self.profile.mixer.master_gain;

    (
      (left + echo_left * DELAY_RETURN + tail_left * reverb_level) * master,
      (right + echo_right * DELAY_RETURN + tail_right * reverb_level) * master
    )
  }

  fn push_event(&mut self, event: MusicEvent) {
    if self.events.len() < self.profile.limits.max_events_per_block {
      self.events.push(event);
    } else {
      self.metrics.dropped_events = self.metrics.dropped_events.saturating_add(1);
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

fn mixer_arrays(profile: &Profile) -> ([f32; TRACK_COUNT], [f32; TRACK_COUNT]) {
  (profile.mixer.tracks.map(|track| track.gain), profile.mixer.tracks.map(|track| track.pan))
}

fn pan_gains(pan: f32) -> (f32, f32) {
  let left = (1.0 - pan).clamp(0.0, 2.0) * 0.5;
  let right = (1.0 + pan).clamp(0.0, 2.0) * 0.5;
  (left.sqrt(), right.sqrt())
}

/// Clamps a control value to 0 to 1, keeping the current value when the new
/// one is not a finite number.
fn unit_or(value: f32, current: f32) -> f32 {
  if value.is_finite() {
    value.clamp(0.0, 1.0)
  } else {
    current
  }
}

#[cfg(test)]
mod tests {
  use super::{Engine, EngineError, RenderOptions};
  use crate::builtin::{all, amiga_house_95ish};
  use crate::types::MusicEvent;

  #[test]
  fn render_outputs_finite_audio() {
    let profile = amiga_house_95ish();
    let mut engine = Engine::new(48_000, 123, profile).expect("engine should create");
    engine.start();
    // Long enough to pass the limiter's look-ahead delay of a few milliseconds.
    let mut output = vec![0.0; 1_024 * 2];
    engine.render_interleaved(&mut output);

    assert!(output.iter().all(|sample| sample.is_finite()));
    assert!(output.iter().any(|sample| sample.abs() > 0.0001));
  }

  #[test]
  fn deterministic_render_matches() {
    let profile = amiga_house_95ish();
    let mut left = Engine::new(48_000, 555, profile).expect("engine should create");
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
      let mut engine = Engine::new(22_050, seed, profile)?;
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
    for profile in all() {
      for (seed, energy) in [(3, 0.1), (11, 0.95)] {
        let mut engine = Engine::new(22_050, seed, profile)?;
        engine.set_energy(energy);
        engine.start();
        let mut output = vec![0.0; 22_050 * 2 * 20];
        engine.render_interleaved(&mut output);
        let power: f32 = output.iter().map(|sample| sample * sample).sum();
        let rms = (power / output.len() as f32).sqrt();

        assert!(output.iter().all(|sample| sample.is_finite() && sample.abs() <= 1.0));
        assert!(rms > 0.01 && rms < 0.5, "seed {seed} rms {rms}");
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

  #[test]
  fn events_survive_until_drained() -> Result<(), EngineError> {
    let mut engine = Engine::new(48_000, 777, amiga_house_95ish())?;
    engine.start();
    let mut block = vec![0.0; 128 * 2];
    let mut wire = vec![0.0; 512 * 5];
    let mut beats = 0;

    // Ten seconds, drained only every sixteenth block as a slow reader would.
    for index in 0..3_750 {
      engine.render_interleaved(&mut block);

      if index % 16 == 15 {
        beats += engine.current_events().iter().filter(|event| matches!(event, MusicEvent::Beat { .. })).count();
        engine.drain_events(&mut wire);
      }
    }

    let expected = (engine.snapshot().bpm / 60.0 * 10.0) as usize;
    assert!(beats + 1 >= expected, "saw {beats} beats, expected about {expected}");
    assert_eq!(engine.metrics().dropped_events, 0);
    Ok(())
  }

  #[test]
  fn non_finite_tempo_range_is_rejected() -> Result<(), EngineError> {
    let mut engine = Engine::new(48_000, 1, amiga_house_95ish())?;

    assert!(engine.set_tempo_range(f32::NAN, f32::NAN).is_err());
    assert!(engine.set_tempo_range(100.0, f32::INFINITY).is_err());
    assert!(engine.snapshot().bpm.is_finite());
    Ok(())
  }
}
