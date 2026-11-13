use crate::dsp::{denormal_guard, midi_to_hz, noise, poly_blep, OnePole, Svf};
use crate::profile::Patch;
use crate::types::{TrackId, WaveShape};
use std::f32::consts::TAU;

/// Maximum number of simultaneous voices.
pub const MAX_VOICES: usize = 48;

/// Per-song drum synthesis parameters.
///
/// The drum tracks are synthesised rather than sample based, so a seeded kit
/// changes their pitch, punch, and decay from song to song.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DrumKit {
  pub kick_pitch: f32,
  pub kick_sweep: f32,
  pub kick_sweep_rate: f32,
  pub kick_decay: f32,
  pub kick_click: f32,
  pub snare_tone: f32,
  pub snare_decay: f32,
  pub snare_body: f32,
  pub hat_partial_a: f32,
  pub hat_partial_b: f32,
  pub hat_closed_decay: f32,
  pub hat_open_decay: f32,
  pub percussion_decay: f32,
  pub fx_sweep_rate: f32
}

impl DrumKit {
  /// The original fixed kit.
  pub const fn classic() -> Self {
    Self {
      kick_pitch: 42.0,
      kick_sweep: 86.0,
      kick_sweep_rate: 42.0,
      kick_decay: 8.5,
      kick_click: 0.28,
      snare_tone: 185.0,
      snare_decay: 18.0,
      snare_body: 0.38,
      hat_partial_a: 421.0,
      hat_partial_b: 743.0,
      hat_closed_decay: 38.0,
      hat_open_decay: 5.0,
      percussion_decay: 24.0,
      fx_sweep_rate: 2.0
    }
  }
}

impl Default for DrumKit {
  fn default() -> Self {
    Self::classic()
  }
}

/// Samples between updates of each voice's filter modulation. The filter
/// envelope and LFO move slowly, so their `exp` and `sin` calls are kept out
/// of the per-sample path.
pub const CONTROL_INTERVAL: u32 = 16;

/// Song-wide values a voice needs while rendering.
#[derive(Clone, Copy, Debug)]
pub struct RenderContext {
  pub sample_rate: f32,
  /// Beats since the song started, used to keep filter LFOs locked to tempo.
  pub beat: f64
}

/// Values needed to start a voice.
#[derive(Clone, Copy, Debug)]
pub struct VoiceStart {
  pub track: TrackId,
  pub note: i16,
  pub velocity: f32,
  pub length_samples: u32,
  pub accent: bool,
  pub slide: bool,
  /// Varies the noise from hit to hit, so repeated drums are not identical.
  pub seed: u32
}

/// A fixed-pool synth voice.
#[derive(Clone, Copy, Debug)]
pub struct Voice {
  pub active: bool,
  pub track: TrackId,
  pub note: i16,
  pub velocity: f32,
  pub phase: f32,
  pub phase_alt: f32,
  pub age_samples: u32,
  pub duration_samples: u32,
  pub release_samples: u32,
  pub frequency: f32,
  pub accent: bool,
  pub slide: bool,
  /// Where this voice sits in the stereo field, from -1 to 1, before the
  /// patch's `width` scales it.
  pub spread: f32,
  /// Left and right mix gains, refreshed by the engine every few samples.
  pub mix_left: f32,
  pub mix_right: f32,
  /// What is left of the previous note's output when this voice was reused,
  /// already mixed to left and right. The engine fades it out to avoid a click.
  pub tail_left: f32,
  pub tail_right: f32,
  /// Per-sample multiplier that fades the carried tail over a few milliseconds.
  pub tail_decay: f32,
  /// The most recent sample this voice produced, before mixing.
  pub last_output: f32,
  noise_state: u32,
  filter: OnePole,
  tone_filter: Svf,
  attack: f32,
  decay: f32,
  sustain: f32,
  detune_ratio: f32,
  filter_env_decay: f32,
  cutoff_mod: f32
}

impl Voice {
  /// Creates a silent voice.
  pub const fn silent() -> Self {
    Self {
      active: false,
      track: TrackId::Kick,
      note: 0,
      velocity: 0.0,
      phase: 0.0,
      phase_alt: 0.0,
      age_samples: 0,
      duration_samples: 0,
      release_samples: 0,
      frequency: 0.0,
      accent: false,
      slide: false,
      spread: 0.0,
      mix_left: 0.0,
      mix_right: 0.0,
      tail_left: 0.0,
      tail_right: 0.0,
      tail_decay: 0.0,
      last_output: 0.0,
      noise_state: 1,
      filter: OnePole::new(),
      tone_filter: Svf::new(),
      attack: 1.0,
      decay: 1.0,
      sustain: 0.0,
      detune_ratio: 1.0,
      filter_env_decay: 1.0,
      cutoff_mod: 1.0
    }
  }

  /// Returns true for tracks that use the oscillator, envelope, and filter
  /// path rather than a dedicated drum model.
  const fn is_tonal(track: TrackId) -> bool {
    !matches!(
      track,
      TrackId::Kick
        | TrackId::Snare
        | TrackId::ClosedHat
        | TrackId::OpenHat
        | TrackId::Percussion
        | TrackId::Fx
    )
  }

  /// Starts this voice, replacing whatever it was playing.
  pub fn start(&mut self, start: VoiceStart, patch: &Patch, sample_rate: f32) {
    // A reused voice may still be sounding. Its last output is carried
    // forward as a tail and faded over a few milliseconds, which avoids a
    // click at the changeover. The tail keeps the old note's own mix gains,
    // because the new note may belong to a different track.
    if self.active {
      self.tail_left += self.last_output * self.mix_left;
      self.tail_right += self.last_output * self.mix_right;
    }

    self.tail_decay = (-1.0 / (sample_rate * 0.003)).exp();

    let to_samples = |ms: f32| (ms / 1_000.0 * sample_rate).max(1.0);

    self.active = true;
    self.track = start.track;
    self.note = start.note;
    self.velocity = start.velocity.clamp(0.0, 1.0);
    self.phase = 0.0;
    self.phase_alt = 0.25;
    self.age_samples = 0;
    self.duration_samples = start.length_samples.max((sample_rate * 0.025) as u32);
    self.frequency = midi_to_hz(start.note);
    self.accent = start.accent;
    self.slide = start.slide;
    // Pitched notes spread by pitch, so chord tones fan out across the stereo
    // field. Drums have one pitch, so they spread by a hash of the seed.
    self.spread = if Self::is_tonal(start.track) {
      f32::from((start.note * 7).rem_euclid(5) - 2) * 0.5
    } else {
      (start.seed.wrapping_mul(2_654_435_761) >> 29) as f32 / 3.5 - 1.0
    };
    self.noise_state = start.note as u32 ^ start.length_samples ^ start.seed ^ 0xa53c_9e11;
    self.filter = OnePole::new();
    self.tone_filter = Svf::new();
    // A minimum attack of one millisecond stops notes starting with a step.
    self.attack = to_samples(patch.attack_ms.max(1.0));
    self.decay = to_samples(patch.decay_ms);
    self.sustain = patch.sustain;
    self.detune_ratio = 2.0_f32.powf(patch.detune_cents / 1_200.0);
    self.filter_env_decay = to_samples(patch.filter_env_decay_ms);
    self.cutoff_mod = 1.0;
    self.last_output = 0.0;
    // Tonal voices live until their release has finished. Drums keep a short
    // fixed tail, which `tail_fade` brings down to silence.
    self.release_samples = if Self::is_tonal(start.track) {
      to_samples(patch.release_ms.max(5.0)) as u32
    } else {
      (sample_rate * 0.08) as u32
    };
  }

  /// Ends the voice quickly but smoothly. A closed hi-hat uses this to cut
  /// off a ringing open hi-hat, as the two are one instrument.
  pub fn choke(&mut self, sample_rate: f32) {
    // A voice already in its release tail is left alone. Restarting the
    // fade there would jump its level back up before bringing it down.
    if self.active && self.age_samples < self.duration_samples {
      self.duration_samples = self.age_samples;
      self.release_samples = (sample_rate * 0.012) as u32;
    }
  }

  /// Renders one mono sample. `cutoff_scale` multiplies the patch's filter
  /// cutoff, which lets the engine automate it without copying the patch.
  pub fn render(
    &mut self,
    patch: &Patch,
    cutoff_scale: f32,
    kit: &DrumKit,
    context: &RenderContext
  ) -> f32 {
    if !self.active {
      return 0.0;
    }

    let sample_rate = context.sample_rate;
    let value = match self.track {
      TrackId::Kick => self.render_kick(kit, sample_rate),
      TrackId::Snare => self.render_snare(kit, sample_rate),
      TrackId::ClosedHat => self.render_hat(kit, sample_rate, false),
      TrackId::OpenHat => self.render_hat(kit, sample_rate, true),
      TrackId::Percussion => self.render_percussion(kit, sample_rate),
      TrackId::Fx => self.render_fx(kit, sample_rate),
      _ => self.render_tonal(patch, cutoff_scale, context)
    };
    let shaped = if Self::is_tonal(self.track) { value } else { value * self.tail_fade() };

    self.age_samples = self.age_samples.saturating_add(1);

    if self.age_samples > self.duration_samples.saturating_add(self.release_samples) {
      self.active = false;
    }

    self.last_output = denormal_guard(shaped * patch.gain * self.velocity);
    self.last_output
  }

  /// Fades a drum voice to silence over its release tail, so that the voice
  /// can be freed without cutting the decay off mid-swing.
  fn tail_fade(&self) -> f32 {
    if self.age_samples <= self.duration_samples {
      return 1.0;
    }

    let into_tail = (self.age_samples - self.duration_samples) as f32;
    (1.0 - into_tail / self.release_samples.max(1) as f32).clamp(0.0, 1.0)
  }

  fn render_kick(&mut self, kit: &DrumKit, sample_rate: f32) -> f32 {
    let t = self.age_samples as f32 / sample_rate;
    let env = (-t * kit.kick_decay).exp();
    let pitch = kit.kick_pitch + kit.kick_sweep * (-t * kit.kick_sweep_rate).exp();
    self.phase = advance_phase(self.phase, pitch, sample_rate);
    let click = if t < 0.004 {
      noise(&mut self.noise_state) * (1.0 - t / 0.004) * kit.kick_click
    } else {
      0.0
    };

    (self.phase * TAU).sin() * env + click
  }

  fn render_snare(&mut self, kit: &DrumKit, sample_rate: f32) -> f32 {
    let t = self.age_samples as f32 / sample_rate;
    let noise_env = (-t * kit.snare_decay).exp();
    let tone_env = (-t * kit.snare_decay * 0.66).exp();
    self.phase = advance_phase(self.phase, kit.snare_tone, sample_rate);
    let body = (self.phase * TAU).sin() * tone_env * kit.snare_body;
    let snap = noise(&mut self.noise_state) * noise_env;

    body + snap * 0.72
  }

  fn render_hat(&mut self, kit: &DrumKit, sample_rate: f32, open: bool) -> f32 {
    let t = self.age_samples as f32 / sample_rate;
    let decay = if open { kit.hat_open_decay } else { kit.hat_closed_decay };
    let env = (-t * decay).exp();
    let metallic = (self.advance_osc(kit.hat_partial_a, sample_rate).sin()
      + self.advance_alt_osc(kit.hat_partial_b, sample_rate).sin())
      * 0.28;
    let hiss = noise(&mut self.noise_state);

    (hiss * 0.78 + metallic) * env
  }

  fn render_percussion(&mut self, kit: &DrumKit, sample_rate: f32) -> f32 {
    let t = self.age_samples as f32 / sample_rate;
    let env = (-t * kit.percussion_decay).exp();
    let freq = self.frequency.clamp(200.0, 1_800.0);
    self.phase = advance_phase(self.phase, freq, sample_rate);

    ((self.phase * TAU).sin() * 0.7 + noise(&mut self.noise_state) * 0.3) * env
  }

  fn render_fx(&mut self, kit: &DrumKit, sample_rate: f32) -> f32 {
    let t = self.age_samples as f32 / sample_rate;
    let env = (-t * 3.5).exp();
    let sweep = 150.0 + 2_200.0 * (1.0 - (-t * kit.fx_sweep_rate).exp());
    let raw = noise(&mut self.noise_state) * env;
    self.filter.process(raw, sweep, sample_rate)
  }

  fn render_tonal(&mut self, patch: &Patch, cutoff_scale: f32, context: &RenderContext) -> f32 {
    let sample_rate = context.sample_rate;
    let freq = if self.slide {
      let slide_amount = (1.0 - (self.age_samples as f32 / self.duration_samples.max(1) as f32))
        .clamp(0.0, 1.0);
      self.frequency * (1.0 + slide_amount * 0.03)
    } else {
      self.frequency
    };
    let step = freq / sample_rate;
    let mut osc = oscillator(patch.wave, self.phase, step, &mut self.noise_state);
    self.phase = advance_phase(self.phase, freq, sample_rate);

    if patch.detune_cents > 0.0 {
      // A second, slightly sharp oscillator beats against the first, which
      // thickens the sound. The pair is scaled to keep the level similar.
      let detuned = step * self.detune_ratio;
      osc = (osc + oscillator(patch.wave, self.phase_alt, detuned, &mut self.noise_state)) * 0.6;
      self.phase_alt = advance_phase(self.phase_alt, freq * self.detune_ratio, sample_rate);
    }

    if self.age_samples % CONTROL_INTERVAL == 0 {
      self.cutoff_mod = self.filter_modulation(patch, context);
    }

    let env = self.envelope();
    let cutoff =
      patch.filter_cutoff * cutoff_scale * self.cutoff_mod * if self.accent { 1.35 } else { 1.0 };
    let filtered = self.tone_filter.process(osc * env, cutoff, patch.resonance, sample_rate);

    saturate(filtered, if self.accent { 1.7 } else { 1.2 })
  }

  /// Returns the cutoff multiplier from the filter envelope, the tempo-synced
  /// LFO, and velocity.
  fn filter_modulation(&self, patch: &Patch, context: &RenderContext) -> f32 {
    // Louder notes open the filter by up to 0.4 octaves, and quiet notes
    // close it by a little, so dynamics change tone as well as level.
    let mut octaves = self.velocity - 0.6;

    if patch.filter_env_octaves > 0.0 {
      octaves +=
        patch.filter_env_octaves * (-(self.age_samples as f32) / self.filter_env_decay).exp();
    }

    if patch.lfo_rate > 0.0 && patch.lfo_depth > 0.0 {
      let cycle = (context.beat * f64::from(patch.lfo_rate)).fract() as f32;
      octaves += patch.lfo_depth * (cycle * TAU).sin();
    }

    2.0_f32.powf(octaves)
  }

  /// Returns the amplitude envelope for the current sample.
  fn envelope(&self) -> f32 {
    if self.age_samples <= self.duration_samples {
      return self.held_level(self.age_samples as f32);
    }

    // The release starts from wherever the envelope had reached, so a note
    // that ends during its attack or decay does not jump to the sustain
    // level. Squaring the ramp gives a more natural, faster-then-slower fade.
    let level = self.held_level(self.duration_samples as f32);
    let release_age = (self.age_samples - self.duration_samples) as f32;
    let remaining = (1.0 - release_age / self.release_samples.max(1) as f32).clamp(0.0, 1.0);
    level * remaining * remaining
  }

  /// Returns the attack, decay, and sustain level at an age in samples.
  fn held_level(&self, age: f32) -> f32 {
    if age < self.attack {
      return age / self.attack;
    }

    if age < self.attack + self.decay {
      return 1.0 + (self.sustain - 1.0) * (age - self.attack) / self.decay;
    }

    self.sustain
  }

  fn advance_osc(&mut self, freq: f32, sample_rate: f32) -> f32 {
    self.phase = advance_phase(self.phase, freq, sample_rate);
    self.phase * TAU
  }

  fn advance_alt_osc(&mut self, freq: f32, sample_rate: f32) -> f32 {
    self.phase_alt = advance_phase(self.phase_alt, freq, sample_rate);
    self.phase_alt * TAU
  }
}

/// Returns one oscillator sample. `step` is the phase increment per sample,
/// which the band-limiting correction needs.
fn oscillator(wave: WaveShape, phase: f32, step: f32, noise_state: &mut u32) -> f32 {
  match wave {
    WaveShape::Sine => (phase * TAU).sin(),
    WaveShape::Triangle => 4.0 * (phase - 0.5).abs() - 1.0,
    WaveShape::Saw => 2.0 * phase - 1.0 - poly_blep(phase, step),
    WaveShape::Square => pulse(phase, step, 0.5),
    WaveShape::Pulse => pulse(phase, step, 0.34),
    WaveShape::Noise => noise(noise_state)
  }
}

/// Returns a band-limited pulse wave with the given duty cycle.
fn pulse(phase: f32, step: f32, duty: f32) -> f32 {
  let naive = if phase < duty { 1.0 } else { -1.0 };
  // Phase and duty are both inside 0 to 1, so one conditional add wraps the
  // difference without the cost of a general floating-point remainder.
  let offset = phase - duty;
  let falling = if offset < 0.0 { offset + 1.0 } else { offset };

  // The wave steps up at phase 0 and down at the duty point, so each edge
  // gets its own correction.
  naive + poly_blep(phase, step) - poly_blep(falling, step)
}

fn advance_phase(phase: f32, frequency: f32, sample_rate: f32) -> f32 {
  let next = phase + frequency / sample_rate;

  if next >= 1.0 {
    next - next.floor()
  } else {
    next
  }
}

fn saturate(input: f32, drive: f32) -> f32 {
  let driven = input * drive;
  driven / (1.0 + driven.abs())
}

#[cfg(test)]
mod tests {
  use super::{DrumKit, RenderContext, Voice, VoiceStart};
  use crate::builtin::amiga_house_95ish;
  use crate::types::TrackId;

  #[test]
  fn synth_output_is_finite_and_release_ends_without_a_step() {
    let profile = amiga_house_95ish();
    let patch = profile.patches[TrackId::Bass.as_index()];
    let context = RenderContext {
      sample_rate: 48_000.0,
      beat: 0.0
    };
    let mut voice = Voice::silent();
    voice.start(
      VoiceStart {
        track: TrackId::Bass,
        note: 40,
        velocity: 0.8,
        length_samples: 4_800,
        accent: true,
        slide: false,
        seed: 1
      },
      &patch,
      48_000.0
    );
    let mut last = 0.0;

    while voice.active {
      last = voice.render(&patch, 1.0, &DrumKit::classic(), &context);
      assert!(last.is_finite());
    }

    // The voice must have faded out before it was freed.
    assert!(last.abs() < 0.01, "final sample {last}");
  }
}
