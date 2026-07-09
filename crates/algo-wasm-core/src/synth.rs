use crate::dsp::{denormal_guard, midi_to_hz, noise, OnePole};
use crate::profile::Patch;
use crate::types::{TrackId, WaveShape};
use std::f32::consts::TAU;

/// Maximum number of simultaneous voices.
pub const MAX_VOICES: usize = 48;

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
  noise_state: u32,
  filter: OnePole
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
      noise_state: 1,
      filter: OnePole::new()
    }
  }

  /// Starts this voice.
  ///
  /// Passing scalar values keeps the fixed-pool hot path direct and allocation-free.
  #[allow(clippy::too_many_arguments)]
  pub fn start(
    &mut self,
    track: TrackId,
    note: i16,
    velocity: f32,
    length_samples: u32,
    sample_rate: f32,
    accent: bool,
    slide: bool
  ) {
    self.active = true;
    self.track = track;
    self.note = note;
    self.velocity = velocity.clamp(0.0, 1.0);
    self.phase = 0.0;
    self.phase_alt = 0.25;
    self.age_samples = 0;
    self.duration_samples = length_samples.max((sample_rate * 0.025) as u32);
    self.release_samples = (sample_rate * 0.08) as u32;
    self.frequency = midi_to_hz(note);
    self.accent = accent;
    self.slide = slide;
    self.noise_state = note as u32 ^ length_samples ^ 0xa53c_9e11;
    self.filter = OnePole::new();
  }

  /// Renders one mono sample.
  pub fn render(&mut self, patch: Patch, sample_rate: f32) -> f32 {
    if !self.active {
      return 0.0;
    }

    let value = match self.track {
      TrackId::Kick => self.render_kick(sample_rate),
      TrackId::Snare => self.render_snare(sample_rate),
      TrackId::ClosedHat => self.render_hat(sample_rate, false),
      TrackId::OpenHat => self.render_hat(sample_rate, true),
      TrackId::Percussion => self.render_percussion(sample_rate),
      TrackId::Fx => self.render_fx(sample_rate),
      _ => self.render_tonal(patch, sample_rate)
    };

    self.age_samples = self.age_samples.saturating_add(1);

    if self.age_samples > self.duration_samples.saturating_add(self.release_samples) {
      self.active = false;
    }

    denormal_guard(value * patch.gain * self.velocity)
  }

  fn render_kick(&mut self, sample_rate: f32) -> f32 {
    let t = self.age_samples as f32 / sample_rate;
    let env = (-t * 8.5).exp();
    let pitch = 42.0 + 86.0 * (-t * 42.0).exp();
    self.phase = advance_phase(self.phase, pitch, sample_rate);
    let click = if t < 0.004 {
      noise(&mut self.noise_state) * (1.0 - t / 0.004) * 0.28
    } else {
      0.0
    };

    (self.phase * TAU).sin() * env + click
  }

  fn render_snare(&mut self, sample_rate: f32) -> f32 {
    let t = self.age_samples as f32 / sample_rate;
    let noise_env = (-t * 18.0).exp();
    let tone_env = (-t * 12.0).exp();
    self.phase = advance_phase(self.phase, 185.0, sample_rate);
    let body = (self.phase * TAU).sin() * tone_env * 0.38;
    let snap = noise(&mut self.noise_state) * noise_env;

    body + snap * 0.72
  }

  fn render_hat(&mut self, sample_rate: f32, open: bool) -> f32 {
    let t = self.age_samples as f32 / sample_rate;
    let decay = if open { 5.0 } else { 38.0 };
    let env = (-t * decay).exp();
    let metallic =
      (self.advance_osc(421.0, sample_rate).sin() + self.advance_alt_osc(743.0, sample_rate).sin())
        * 0.28;
    let hiss = noise(&mut self.noise_state);

    (hiss * 0.78 + metallic) * env
  }

  fn render_percussion(&mut self, sample_rate: f32) -> f32 {
    let t = self.age_samples as f32 / sample_rate;
    let env = (-t * 24.0).exp();
    let freq = self.frequency.clamp(200.0, 1_800.0);
    self.phase = advance_phase(self.phase, freq, sample_rate);

    ((self.phase * TAU).sin() * 0.7 + noise(&mut self.noise_state) * 0.3) * env
  }

  fn render_fx(&mut self, sample_rate: f32) -> f32 {
    let t = self.age_samples as f32 / sample_rate;
    let env = (-t * 3.5).exp();
    let sweep = 400.0 + 4_800.0 * (1.0 - (-t * 2.0).exp());
    let raw = noise(&mut self.noise_state) * env;
    self.filter.process(raw, sweep, sample_rate)
  }

  fn render_tonal(&mut self, patch: Patch, sample_rate: f32) -> f32 {
    let freq = if self.slide {
      let slide_amount = (1.0 - (self.age_samples as f32 / self.duration_samples.max(1) as f32))
        .clamp(0.0, 1.0);
      self.frequency * (1.0 + slide_amount * 0.03)
    } else {
      self.frequency
    };
    let osc = oscillator(patch.wave, self.phase, &mut self.noise_state);
    self.phase = advance_phase(self.phase, freq, sample_rate);

    let env = adsr(
      self.age_samples,
      self.duration_samples,
      patch.attack_ms,
      patch.decay_ms,
      patch.sustain,
      patch.release_ms,
      sample_rate
    );
    let cutoff = patch.filter_cutoff * if self.accent { 1.35 } else { 1.0 };
    let filtered = self.filter.process(osc * env, cutoff, sample_rate);

    saturate(filtered, if self.accent { 1.7 } else { 1.2 })
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

fn oscillator(wave: WaveShape, phase: f32, noise_state: &mut u32) -> f32 {
  match wave {
    WaveShape::Sine => (phase * TAU).sin(),
    WaveShape::Triangle => 4.0 * (phase - 0.5).abs() - 1.0,
    WaveShape::Saw => 2.0 * phase - 1.0,
    WaveShape::Square => {
      if phase < 0.5 {
        1.0
      } else {
        -1.0
      }
    }
    WaveShape::Pulse => {
      if phase < 0.34 {
        1.0
      } else {
        -1.0
      }
    }
    WaveShape::Noise => noise(noise_state)
  }
}

fn advance_phase(phase: f32, frequency: f32, sample_rate: f32) -> f32 {
  let next = phase + frequency / sample_rate;

  if next >= 1.0 {
    next - next.floor()
  } else {
    next
  }
}

fn adsr(
  age_samples: u32,
  duration_samples: u32,
  attack_ms: f32,
  decay_ms: f32,
  sustain: f32,
  release_ms: f32,
  sample_rate: f32
) -> f32 {
  let attack = ((attack_ms / 1_000.0) * sample_rate).max(1.0);
  let decay = ((decay_ms / 1_000.0) * sample_rate).max(1.0);
  let release = ((release_ms / 1_000.0) * sample_rate).max(1.0);
  let age = age_samples as f32;

  if age < attack {
    return (age / attack).clamp(0.0, 1.0);
  }

  if age < attack + decay {
    let progress = (age - attack) / decay;
    return 1.0 + (sustain - 1.0) * progress;
  }

  if age_samples <= duration_samples {
    return sustain;
  }

  let release_age = age_samples.saturating_sub(duration_samples) as f32;
  (sustain * (1.0 - release_age / release)).clamp(0.0, 1.0)
}

fn saturate(input: f32, drive: f32) -> f32 {
  let driven = input * drive;
  driven / (1.0 + driven.abs())
}

#[cfg(test)]
mod tests {
  use super::Voice;
  use crate::profile::amiga_house_95ish;
  use crate::types::TrackId;

  #[test]
  fn synth_output_is_finite() {
    let profile = amiga_house_95ish();
    let patch = profile.patches[TrackId::Bass.as_index()];
    let mut voice = Voice::silent();
    voice.start(TrackId::Bass, 40, 0.8, 4_800, 48_000.0, true, false);

    for _ in 0..256 {
      let value = voice.render(patch, 48_000.0);
      assert!(value.is_finite());
    }
  }
}
