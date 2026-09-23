/// A simple one-pole low-pass filter.
#[derive(Clone, Copy, Debug)]
pub struct OnePole {
  value: f32
}

impl OnePole {
  /// Creates a cleared filter.
  pub const fn new() -> Self {
    Self {
      value: 0.0
    }
  }

  /// Processes a sample.
  pub fn process(&mut self, input: f32, cutoff_hz: f32, sample_rate: f32) -> f32 {
    let cutoff = cutoff_hz.clamp(20.0, sample_rate * 0.45);
    let alpha = (cutoff / sample_rate).clamp(0.001, 0.95);
    self.value += (input - self.value) * alpha;
    self.value
  }
}

impl Default for OnePole {
  fn default() -> Self {
    Self::new()
  }
}

/// A resonant two-pole state-variable low-pass filter.
///
/// This uses the trapezoidal (zero-delay feedback) form, which stays stable
/// while the cutoff moves. Coefficients are cached and only recalculated when
/// the cutoff or resonance changes noticeably, which keeps `tan` out of the
/// per-sample hot path for held notes.
#[derive(Clone, Copy, Debug)]
pub struct Svf {
  ic1: f32,
  ic2: f32,
  cutoff: f32,
  resonance: f32,
  a1: f32,
  a2: f32,
  a3: f32
}

impl Svf {
  /// Creates a cleared filter.
  pub const fn new() -> Self {
    Self {
      ic1: 0.0,
      ic2: 0.0,
      cutoff: -1.0,
      resonance: -1.0,
      a1: 0.0,
      a2: 0.0,
      a3: 0.0
    }
  }

  /// Processes a sample. `resonance` runs from 0 (gentle) to 1 (sharp peak).
  pub fn process(&mut self, input: f32, cutoff_hz: f32, resonance: f32, sample_rate: f32) -> f32 {
    let cutoff = cutoff_hz.clamp(20.0, sample_rate * 0.45);
    let resonance = resonance.clamp(0.0, 1.0);

    if (cutoff - self.cutoff).abs() > self.cutoff * 0.002 || resonance != self.resonance {
      let g = (std::f32::consts::PI * cutoff / sample_rate).tan();
      let k = 2.0 - 1.9 * resonance;
      self.a1 = 1.0 / (1.0 + g * (g + k));
      self.a2 = g * self.a1;
      self.a3 = g * self.a2;
      self.cutoff = cutoff;
      self.resonance = resonance;
    }

    let v3 = input - self.ic2;
    let v1 = self.a1 * self.ic1 + self.a2 * v3;
    let v2 = self.ic2 + self.a2 * self.ic1 + self.a3 * v3;
    self.ic1 = denormal_guard(2.0 * v1 - self.ic1);
    self.ic2 = denormal_guard(2.0 * v2 - self.ic2);

    if v2.is_finite() {
      v2
    } else {
      self.ic1 = 0.0;
      self.ic2 = 0.0;
      0.0
    }
  }
}

impl Default for Svf {
  fn default() -> Self {
    Self::new()
  }
}

/// DC blocking high-pass filter.
#[derive(Clone, Copy, Debug)]
pub struct DcBlocker {
  previous_input: f32,
  previous_output: f32
}

impl DcBlocker {
  /// Creates a cleared DC blocker.
  pub const fn new() -> Self {
    Self {
      previous_input: 0.0,
      previous_output: 0.0
    }
  }

  /// Processes a sample.
  pub fn process(&mut self, input: f32) -> f32 {
    let output = input - self.previous_input + 0.995 * self.previous_output;
    self.previous_input = input;
    self.previous_output = output;
    if output.is_finite() { output } else { 0.0 }
  }
}

impl Default for DcBlocker {
  fn default() -> Self {
    Self::new()
  }
}

/// A small soft limiter.
#[derive(Clone, Copy, Debug)]
pub struct SoftLimiter {
  drive: f32
}

impl SoftLimiter {
  /// Creates a limiter.
  pub const fn new(drive: f32) -> Self {
    Self {
      drive
    }
  }

  /// Updates the limiter drive.
  pub fn set_drive(&mut self, drive: f32) {
    self.drive = drive.clamp(0.1, 2.0);
  }

  /// Processes a sample.
  pub fn process(&self, input: f32) -> f32 {
    let driven = input * self.drive;
    let clipped = driven / (1.0 + driven.abs());
    if clipped.is_finite() { clipped.clamp(-1.0, 1.0) } else { 0.0 }
  }
}

/// A bounded stereo delay used outside the hot allocation path.
#[derive(Clone, Debug)]
pub struct StereoDelay {
  left: Vec<f32>,
  right: Vec<f32>,
  index: usize,
  feedback: f32
}

impl StereoDelay {
  /// Creates a preallocated delay line.
  pub fn new(sample_rate: f32, feedback: f32) -> Self {
    let len = ((sample_rate * 0.28) as usize).clamp(1_024, 32_768);

    Self {
      left: vec![0.0; len],
      right: vec![0.0; len],
      index: 0,
      feedback: feedback.clamp(0.0, 0.95)
    }
  }

  /// Processes a stereo sample.
  pub fn process(&mut self, left: f32, right: f32, send: f32) -> (f32, f32) {
    let delayed_left = self.left[self.index];
    let delayed_right = self.right[self.index];
    let feedback = self.feedback;
    let send = send.clamp(0.0, 1.0);

    self.left[self.index] = left * send + delayed_right * feedback * 0.82;
    self.right[self.index] = right * send + delayed_left * feedback * 0.82;
    self.index = (self.index + 1) % self.left.len();

    (delayed_left, delayed_right)
  }
}

/// Prevents denormal floats from building up in filters.
pub fn denormal_guard(value: f32) -> f32 {
  if value.abs() < 1.0e-20 { 0.0 } else { value }
}

/// Converts MIDI note number to frequency in Hz.
pub fn midi_to_hz(note: i16) -> f32 {
  440.0 * 2.0_f32.powf((f32::from(note) - 69.0) / 12.0)
}

/// Produces a compact deterministic noise sample.
pub fn noise(state: &mut u32) -> f32 {
  *state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
  let value = ((*state >> 8) as f32) / 16_777_216.0;
  value * 2.0 - 1.0
}

#[cfg(test)]
mod tests {
  use super::{SoftLimiter, Svf};

  #[test]
  fn svf_stays_bounded_at_high_resonance() {
    let mut filter = Svf::new();
    let mut peak: f32 = 0.0;

    for index in 0..48_000 {
      let input = if (index / 60) % 2 == 0 { 1.0 } else { -1.0 };
      let cutoff = 200.0 + (index as f32 * 0.1) % 8_000.0;
      let output = filter.process(input, cutoff, 1.0, 48_000.0);
      assert!(output.is_finite());
      peak = peak.max(output.abs());
    }

    assert!(peak < 20.0);
  }

  #[test]
  fn limiter_stays_bounded() {
    let limiter = SoftLimiter::new(1.0);

    for input in [-10.0, -1.0, 0.0, 1.0, 10.0] {
      let output = limiter.process(input);
      assert!(output.is_finite());
      assert!((-1.0..=1.0).contains(&output));
    }
  }
}
