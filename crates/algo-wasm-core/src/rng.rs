/// Deterministic xoshiro256** PRNG seeded with splitmix64.
#[derive(Clone, Debug)]
pub struct Rng64 {
  state: [u64; 4]
}

impl Rng64 {
  /// Creates a new deterministic RNG.
  pub fn new(seed: u64) -> Self {
    let mut split = SplitMix64 { state: seed };
    Self {
      state: [split.next(), split.next(), split.next(), split.next()]
    }
  }

  /// Returns the next random `u64`.
  pub fn next_u64(&mut self) -> u64 {
    let result = self.state[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
    let t = self.state[1] << 17;

    self.state[2] ^= self.state[0];
    self.state[3] ^= self.state[1];
    self.state[1] ^= self.state[2];
    self.state[0] ^= self.state[3];
    self.state[2] ^= t;
    self.state[3] = self.state[3].rotate_left(45);

    result
  }

  /// Returns a float in the inclusive-exclusive range `[0, 1)`.
  pub fn next_f32(&mut self) -> f32 {
    let value = self.next_u64() >> 40;
    (value as f32) / ((1u32 << 24) as f32)
  }

  /// Returns true with the given probability.
  pub fn chance(&mut self, probability: f32) -> bool {
    self.next_f32() < probability.clamp(0.0, 1.0)
  }

  /// Returns an integer in `0..upper`. Returns 0 for an empty range.
  pub fn range_usize(&mut self, upper: usize) -> usize {
    if upper == 0 {
      return 0;
    }

    (self.next_u64() as usize) % upper
  }

  /// Returns a float in `min..max`.
  pub fn range_f32(&mut self, min: f32, max: f32) -> f32 {
    min + (max - min) * self.next_f32()
  }

  /// Picks one item from a non-empty slice. Returns the default for an empty slice.
  pub fn pick<T: Copy + Default>(&mut self, items: &[T]) -> T {
    if items.is_empty() {
      return T::default();
    }

    items[self.range_usize(items.len())]
  }

  /// Picks an index from positive integer weights.
  pub fn weighted_index(&mut self, weights: &[u32]) -> usize {
    let total = weights.iter().fold(0u64, |acc, weight| acc + u64::from(*weight));

    if total == 0 {
      return 0;
    }

    let mut cursor = self.next_u64() % total;

    for (index, weight) in weights.iter().enumerate() {
      let weight = u64::from(*weight);

      if cursor < weight {
        return index;
      }

      cursor -= weight;
    }

    weights.len().saturating_sub(1)
  }
}

#[derive(Clone, Copy, Debug)]
struct SplitMix64 {
  state: u64
}

impl SplitMix64 {
  fn next(&mut self) -> u64 {
    self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = self.state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
  }
}

#[cfg(test)]
mod tests {
  use super::Rng64;

  #[test]
  fn same_seed_produces_same_sequence() {
    let mut left = Rng64::new(42);
    let mut right = Rng64::new(42);

    for _ in 0..128 {
      assert_eq!(left.next_u64(), right.next_u64());
    }
  }
}
