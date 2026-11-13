/// A simple one-pole low-pass filter.
#[derive(Clone, Copy, Debug)]
pub struct OnePole {
  value: f32,
  cutoff: f32,
  alpha: f32
}

impl OnePole {
  /// Creates a cleared filter.
  pub const fn new() -> Self {
    Self {
      value: 0.0,
      cutoff: -1.0,
      alpha: 0.0
    }
  }

  /// Processes a sample.
  pub fn process(&mut self, input: f32, cutoff_hz: f32, sample_rate: f32) -> f32 {
    let cutoff = cutoff_hz.clamp(20.0, sample_rate * 0.45);

    // The coefficient needs `exp`, so it is only recalculated once the cutoff
    // has moved audibly. This form gives the same cutoff at every sample rate.
    if (cutoff - self.cutoff).abs() > self.cutoff * 0.002 {
      self.alpha = 1.0 - (-std::f32::consts::TAU * cutoff / sample_rate).exp();
      self.cutoff = cutoff;
    }

    self.value = denormal_guard(self.value + (input - self.value) * self.alpha);
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

    if !output.is_finite() {
      // Without this reset a single bad sample would leave the filter state
      // non-finite, and the channel silent, for good.
      self.previous_input = 0.0;
      self.previous_output = 0.0;
      return 0.0;
    }

    self.previous_input = input;
    self.previous_output = output;
    output
  }
}

impl Default for DcBlocker {
  fn default() -> Self {
    Self::new()
  }
}

/// Output ceiling of the master limiter, a little under full scale.
const LIMITER_CEILING: f32 = 0.89;

/// Fixed gain ahead of the limiter. The mix peaks a little under full scale
/// on its own, so this brings typical material up to the ceiling.
const LIMITER_MAKE_UP: f32 = 1.25;

/// A stereo look-ahead peak limiter.
///
/// The signal is delayed by a few milliseconds so that the gain can already
/// be down when a peak arrives. Below the ceiling the limiter leaves the
/// signal untouched, unlike a waveshaper, which bends every sample.
#[derive(Clone, Debug)]
pub struct Limiter {
  left: Vec<f32>,
  right: Vec<f32>,
  index: usize,
  drive: f32,
  target: f32,
  hold: usize,
  gain: f32,
  attack: f32,
  release: f32
}

impl Limiter {
  /// Creates a limiter with its look-ahead buffer allocated up front.
  pub fn new(sample_rate: f32, drive: f32) -> Self {
    let len = ((sample_rate * 0.005) as usize).clamp(16, 1_024);

    Self {
      left: vec![0.0; len],
      right: vec![0.0; len],
      index: 0,
      drive: drive.clamp(0.0, 2.0),
      target: 1.0,
      hold: 0,
      gain: 1.0,
      // The gain must settle well inside the look-ahead window, so the
      // attack time constant is a fifth of it.
      attack: 1.0 - (-5.0 / len as f32).exp(),
      release: 1.0 - (-1.0 / (sample_rate * 0.12)).exp()
    }
  }

  /// Updates the input drive.
  pub fn set_drive(&mut self, drive: f32) {
    self.drive = drive.clamp(0.0, 2.0);
  }

  /// Clears the look-ahead buffer and gain state.
  pub fn reset(&mut self) {
    self.left.fill(0.0);
    self.right.fill(0.0);
    self.target = 1.0;
    self.hold = 0;
    self.gain = 1.0;
  }

  /// Processes a stereo sample and returns the delayed, limited sample.
  pub fn process(&mut self, left: f32, right: f32) -> (f32, f32) {
    let left = left * self.drive * LIMITER_MAKE_UP;
    let right = right * self.drive * LIMITER_MAKE_UP;
    let peak = left.abs().max(right.abs());
    let needed = if peak > LIMITER_CEILING { LIMITER_CEILING / peak } else { 1.0 };

    if needed <= self.target {
      // Hold the reduction until this peak has left the look-ahead buffer.
      self.target = needed;
      self.hold = self.left.len();
    } else if self.hold > 0 {
      self.hold -= 1;
    } else {
      self.target += (1.0 - self.target) * self.release;
    }

    let rate = if self.target < self.gain { self.attack } else { self.release };
    self.gain += (self.target - self.gain) * rate;

    let delayed_left = self.left[self.index];
    let delayed_right = self.right[self.index];
    self.left[self.index] = left;
    self.right[self.index] = right;
    self.index = (self.index + 1) % self.left.len();

    (soft_ceiling(delayed_left * self.gain), soft_ceiling(delayed_right * self.gain))
  }
}

/// Rounds off the rare overshoot the gain smoothing lets through, and maps
/// anything non-finite to silence. Samples under the ceiling pass unchanged.
fn soft_ceiling(value: f32) -> f32 {
  if !value.is_finite() {
    return 0.0;
  }

  let magnitude = value.abs();

  if magnitude <= LIMITER_CEILING {
    return value;
  }

  let over = magnitude - LIMITER_CEILING;
  let headroom = 1.0 - LIMITER_CEILING;
  (LIMITER_CEILING + headroom * over / (over + headroom)).copysign(value)
}

/// A tempo-synced stereo ping-pong delay fed by a send bus.
#[derive(Clone, Debug)]
pub struct StereoDelay {
  left: Vec<f32>,
  right: Vec<f32>,
  index: usize,
  length: usize,
  feedback: f32,
  damp_left: f32,
  damp_right: f32
}

impl StereoDelay {
  /// Longest delay time the buffer can hold, in seconds.
  const MAX_SECONDS: f32 = 0.75;

  /// Creates a preallocated delay line.
  pub fn new(sample_rate: f32, feedback: f32) -> Self {
    let len = ((sample_rate * Self::MAX_SECONDS) as usize).max(1_024);

    Self {
      left: vec![0.0; len],
      right: vec![0.0; len],
      index: 0,
      length: len,
      feedback: feedback.clamp(0.0, 0.95),
      damp_left: 0.0,
      damp_right: 0.0
    }
  }

  /// Sets the delay to a dotted eighth note at the given tempo, so echoes
  /// land on the grid. Slow tempos halve the time until it fits the buffer.
  pub fn set_tempo(&mut self, bpm: f32, sample_rate: f32) {
    let mut seconds = 45.0 / bpm.clamp(40.0, 240.0);

    while seconds > Self::MAX_SECONDS {
      seconds *= 0.5;
    }

    let length = ((seconds * sample_rate) as usize).clamp(1, self.left.len());

    // A longer delay starts reading slots last written under an older
    // setting. They are cleared so no stale echo is heard.
    if length > self.length {
      self.left[self.length..length].fill(0.0);
      self.right[self.length..length].fill(0.0);
    }

    self.length = length;
  }

  /// Sets the feedback amount.
  pub fn set_feedback(&mut self, feedback: f32) {
    self.feedback = feedback.clamp(0.0, 0.95);
  }

  /// Clears the delay line.
  pub fn reset(&mut self) {
    self.left.fill(0.0);
    self.right.fill(0.0);
    self.damp_left = 0.0;
    self.damp_right = 0.0;
  }

  /// Writes one stereo sample of the send bus and returns the echo.
  pub fn process(&mut self, send_left: f32, send_right: f32) -> (f32, f32) {
    let delayed_left = self.left[self.index];
    let delayed_right = self.right[self.index];

    // Each repeat is low-passed a little, so echoes sink behind the dry mix
    // instead of piling up at full brightness.
    self.damp_left = denormal_guard(self.damp_left + (delayed_left - self.damp_left) * 0.45);
    self.damp_right = denormal_guard(self.damp_right + (delayed_right - self.damp_right) * 0.45);

    // Feedback crosses channels, which makes the echoes bounce left and right.
    self.left[self.index] = send_left + self.damp_right * self.feedback * 0.82;
    self.right[self.index] = send_right + self.damp_left * self.feedback * 0.82;
    self.index = (self.index + 1) % self.length;

    (delayed_left, delayed_right)
  }
}

/// Comb filter lengths in samples at 44.1 kHz, from the public-domain
/// Freeverb design. They are mutually detuned so the echoes do not line up.
const COMB_LENGTHS: [usize; 6] = [1_116, 1_188, 1_277, 1_356, 1_422, 1_491];

/// All-pass filter lengths in samples at 44.1 kHz.
const ALLPASS_LENGTHS: [usize; 2] = [556, 441];

/// Extra samples added to the right channel's lengths to widen the reverb.
const STEREO_SPREAD: usize = 23;

/// One channel of the reverb: parallel damped combs, then series all-passes.
#[derive(Clone, Debug)]
struct ReverbChannel {
  combs: [Vec<f32>; 6],
  comb_index: [usize; 6],
  comb_damp: [f32; 6],
  allpasses: [Vec<f32>; 2],
  allpass_index: [usize; 2]
}

impl ReverbChannel {
  fn new(sample_rate: f32, spread: usize) -> Self {
    let scale = sample_rate / 44_100.0;
    let sized = |length: usize| vec![0.0; (((length + spread) as f32) * scale) as usize + 1];

    Self {
      combs: COMB_LENGTHS.map(sized),
      comb_index: [0; 6],
      comb_damp: [0.0; 6],
      allpasses: ALLPASS_LENGTHS.map(sized),
      allpass_index: [0; 2]
    }
  }

  fn reset(&mut self) {
    for comb in &mut self.combs {
      comb.fill(0.0);
    }

    for allpass in &mut self.allpasses {
      allpass.fill(0.0);
    }

    self.comb_damp = [0.0; 6];
  }

  fn process(&mut self, input: f32) -> f32 {
    const ROOM: f32 = 0.8;
    const DAMP: f32 = 0.35;
    let mut output = 0.0;

    for ((comb, index), damp) in
      self.combs.iter_mut().zip(&mut self.comb_index).zip(&mut self.comb_damp)
    {
      let delayed = comb[*index];
      *damp = denormal_guard(delayed * (1.0 - DAMP) + *damp * DAMP);
      comb[*index] = input + *damp * ROOM;
      *index = (*index + 1) % comb.len();
      output += delayed;
    }

    for (allpass, index) in self.allpasses.iter_mut().zip(&mut self.allpass_index) {
      let delayed = allpass[*index];
      allpass[*index] = output + delayed * 0.5;
      *index = (*index + 1) % allpass.len();
      output = delayed - output;
    }

    output
  }
}

/// A small stereo reverb fed by a mono send bus.
#[derive(Clone, Debug)]
pub struct Reverb {
  left: ReverbChannel,
  right: ReverbChannel
}

impl Reverb {
  /// Creates a reverb with its buffers allocated up front.
  pub fn new(sample_rate: f32) -> Self {
    Self {
      left: ReverbChannel::new(sample_rate, 0),
      right: ReverbChannel::new(sample_rate, STEREO_SPREAD)
    }
  }

  /// Clears the reverb tail.
  pub fn reset(&mut self) {
    self.left.reset();
    self.right.reset();
  }

  /// Processes one sample of the send bus and returns the stereo tail.
  pub fn process(&mut self, send: f32) -> (f32, f32) {
    // The combs sum to a large gain, so the input is scaled down first.
    let input = send * 0.03;
    (self.left.process(input), self.right.process(input))
  }
}

/// Returns the PolyBLEP correction for a waveform step at phase 0.
///
/// A naive saw or square jumps instantly, which aliases. Subtracting this
/// small polynomial around each jump band-limits it at very low cost.
/// `phase` runs from 0 to 1 and `step` is the phase increment per sample.
pub fn poly_blep(phase: f32, step: f32) -> f32 {
  if step <= 0.0 {
    return 0.0;
  }

  if phase < step {
    let t = phase / step;
    return t + t - t * t - 1.0;
  }

  if phase > 1.0 - step {
    let t = (phase - 1.0) / step;
    return t * t + t + t + 1.0;
  }

  0.0
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
  use super::{Limiter, Svf};

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
  fn limiter_is_transparent_when_quiet_and_bounded_when_loud() {
    let mut limiter = Limiter::new(48_000.0, 1.0);
    let mut quiet_peak: f32 = 0.0;

    for index in 0..4_800 {
      let input = (index as f32 * 0.05).sin() * 0.5;
      quiet_peak = quiet_peak.max(limiter.process(input, input).0.abs());
    }

    assert!((quiet_peak - 0.625).abs() < 0.001, "quiet peak {quiet_peak}");

    for index in 0..48_000 {
      let input = (index as f32 * 0.05).sin() * 8.0;
      let (left, right) = limiter.process(input, f32::NAN);
      // A non-finite input must come out as a bounded sample, never as NaN.
      assert!(left.abs() <= 1.0 && right.abs() <= 1.0);
    }
  }
}
