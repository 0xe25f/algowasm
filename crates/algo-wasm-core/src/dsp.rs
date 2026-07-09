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
  use super::SoftLimiter;

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
