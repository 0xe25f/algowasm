//! C-compatible WASM boundary for AlgoWASM.
//!
//! JavaScript owns loading and message passing. This crate exposes compact
//! functions for creating an engine, rendering stereo samples, applying live
//! controls, reading snapshots and events, and exporting MIDI.
//!
//! Everything crosses the boundary as numbers or as buffers of numbers: there
//! is no JSON and no text apart from error messages. Ids are small integers
//! in the same order as the package's `WIRE_*` tables.
//!
//! Panics are not caught here. On `wasm32-unknown-unknown` a panic is a trap,
//! which JavaScript sees as a `WebAssembly.RuntimeError`. The package treats
//! that as fatal for the instance and stops using it.

use algo_wasm_core::{
  Engine, Mood, Profile, TrackId, EVENT_WIRE_SIZE, PROFILE_WIRE_LEN, SNAPSHOT_WIRE_LEN
};
use std::alloc::{alloc, dealloc, Layout};
use std::cell::Cell;
use std::slice;

/// Most events a single drain can return, matching the profile limit.
const EVENT_CAPACITY: usize = 512;

/// Alignment of buffers handed to JavaScript, enough for `f32` and `f64`.
const BUFFER_ALIGN: usize = 8;

/// Largest buffer JavaScript may request, as a guard against a bad length.
const MAX_ALLOC_BYTES: usize = 1 << 26;

thread_local! {
  static LAST_ERROR: Cell<&'static str> = const { Cell::new("") };
}

/// An engine together with the scratch buffers JavaScript reads from.
///
/// Keeping the buffers beside the engine means reads never allocate, and the
/// pointers stay valid until the engine is destroyed.
pub struct Handle {
  engine: Engine,
  snapshot: [f64; SNAPSHOT_WIRE_LEN],
  events: [f64; EVENT_CAPACITY * EVENT_WIRE_SIZE],
  midi: Vec<u8>
}

/// Returns the packed API version, as `0x00MMmmpp`.
#[no_mangle]
pub extern "C" fn algo_wasm_version() -> u32 {
  0x0001_0200
}

/// Allocates an uninitialised buffer of `len` bytes owned by WASM, aligned for
/// `f32` and `f64`. Returns null when `len` is zero or unreasonably large.
/// Free it with `algo_wasm_dealloc` and the same length.
#[no_mangle]
pub extern "C" fn algo_wasm_alloc(len: usize) -> *mut u8 {
  match buffer_layout(len) {
    // The layout has a non-zero size, which is what `alloc` requires.
    Some(layout) => unsafe { alloc(layout) },
    None => std::ptr::null_mut()
  }
}

/// Frees a buffer allocated by `algo_wasm_alloc`.
///
/// # Safety
///
/// `pointer` and `len` must match a live allocation returned by
/// `algo_wasm_alloc`. Passing any other pointer is undefined behaviour.
#[no_mangle]
pub unsafe extern "C" fn algo_wasm_dealloc(pointer: *mut u8, len: usize) {
  if pointer.is_null() {
    return;
  }

  if let Some(layout) = buffer_layout(len) {
    // JavaScript passes back the exact pointer and length it was given, so
    // the layout matches the one used to allocate.
    unsafe { dealloc(pointer, layout) };
  }
}

/// Creates an engine from a flat profile. Returns a null pointer on error.
///
/// # Safety
///
/// `profile_ptr` must be null or point to `profile_len` readable `f32` values.
#[no_mangle]
pub unsafe extern "C" fn algo_wasm_engine_create(
  sample_rate: u32,
  seed_high: u32,
  seed_low: u32,
  profile_ptr: *const f32,
  profile_len: usize
) -> *mut Handle {
  clear_error();
  let seed = (u64::from(seed_high) << 32) | u64::from(seed_low);
  let profile = match unsafe { read_profile(profile_ptr, profile_len) } {
    Ok(profile) => profile,
    Err(message) => return fail(message, std::ptr::null_mut())
  };

  match Engine::new(sample_rate, seed, profile) {
    Ok(engine) => Box::into_raw(Box::new(Handle {
      engine,
      snapshot: [0.0; SNAPSHOT_WIRE_LEN],
      events: [0.0; EVENT_CAPACITY * EVENT_WIRE_SIZE],
      midi: Vec::new()
    })),
    Err(error) => fail(error.message(), std::ptr::null_mut())
  }
}

/// Destroys an engine. Calling with null is safe.
///
/// # Safety
///
/// `handle` must be null or a pointer returned by `algo_wasm_engine_create`
/// that has not already been destroyed.
#[no_mangle]
pub unsafe extern "C" fn algo_wasm_engine_destroy(handle: *mut Handle) {
  if !handle.is_null() {
    // Handles are created by `Box::into_raw` in this crate and are returned
    // exactly once for destruction.
    drop(unsafe { Box::from_raw(handle) });
  }
}

/// Starts playback.
///
/// # Safety
///
/// `handle` must be null or a live handle.
#[no_mangle]
pub unsafe extern "C" fn algo_wasm_engine_start(handle: *mut Handle) -> i32 {
  unsafe { with_engine(handle, |engine| engine.start()) }
}

/// Pauses playback.
///
/// # Safety
///
/// `handle` must be null or a live handle.
#[no_mangle]
pub unsafe extern "C" fn algo_wasm_engine_pause(handle: *mut Handle) -> i32 {
  unsafe { with_engine(handle, |engine| engine.pause()) }
}

/// Resumes playback.
///
/// # Safety
///
/// `handle` must be null or a live handle.
#[no_mangle]
pub unsafe extern "C" fn algo_wasm_engine_resume(handle: *mut Handle) -> i32 {
  unsafe { with_engine(handle, |engine| engine.resume()) }
}

/// Stops playback and clears voices.
///
/// # Safety
///
/// `handle` must be null or a live handle.
#[no_mangle]
pub unsafe extern "C" fn algo_wasm_engine_stop(handle: *mut Handle) -> i32 {
  unsafe { with_engine(handle, |engine| engine.stop()) }
}

/// Renders interleaved stereo `f32` samples into WASM memory.
///
/// # Safety
///
/// `handle` must be null or a live handle. `output_ptr` must be null or point
/// to a mutable `f32` buffer with at least `output_len` elements.
#[no_mangle]
pub unsafe extern "C" fn algo_wasm_engine_render(
  handle: *mut Handle,
  output_ptr: *mut f32,
  output_len: usize
) -> i32 {
  if output_ptr.is_null() || output_len > MAX_ALLOC_BYTES / 4 {
    return fail("Render output buffer was missing or too large.", -1);
  }

  unsafe {
    with_engine(handle, |engine| {
      // The caller provides an `f32` buffer in WASM memory with `output_len`
      // elements. The package allocates it once and reuses it.
      let output = slice::from_raw_parts_mut(output_ptr, output_len);
      engine.render_interleaved(output);
    })
  }
}

/// Sets the deterministic seed.
///
/// # Safety
///
/// `handle` must be null or a live handle.
#[no_mangle]
pub unsafe extern "C" fn algo_wasm_engine_set_seed(
  handle: *mut Handle,
  seed_high: u32,
  seed_low: u32
) -> i32 {
  unsafe {
    with_engine(handle, |engine| {
      engine.set_seed((u64::from(seed_high) << 32) | u64::from(seed_low));
    })
  }
}

/// Sets the profile from the flat layout.
///
/// # Safety
///
/// `handle` must be null or a live handle. `profile_ptr` must be null or
/// point to `profile_len` readable `f32` values.
#[no_mangle]
pub unsafe extern "C" fn algo_wasm_engine_set_profile(
  handle: *mut Handle,
  profile_ptr: *const f32,
  profile_len: usize
) -> i32 {
  let profile = match unsafe { read_profile(profile_ptr, profile_len) } {
    Ok(profile) => profile,
    Err(message) => return fail(message, -1)
  };

  unsafe { try_with_engine(handle, |engine| engine.set_profile(profile).map_err(|error| error.message())) }
}

/// Sets master volume.
///
/// # Safety
///
/// `handle` must be null or a live handle.
#[no_mangle]
pub unsafe extern "C" fn algo_wasm_engine_set_volume(handle: *mut Handle, value: f32) -> i32 {
  unsafe { with_engine(handle, |engine| engine.set_volume(value)) }
}

/// Sets energy.
///
/// # Safety
///
/// `handle` must be null or a live handle.
#[no_mangle]
pub unsafe extern "C" fn algo_wasm_engine_set_energy(handle: *mut Handle, value: f32) -> i32 {
  unsafe { with_engine(handle, |engine| engine.set_energy(value)) }
}

/// Sets intensity.
///
/// # Safety
///
/// `handle` must be null or a live handle.
#[no_mangle]
pub unsafe extern "C" fn algo_wasm_engine_set_intensity(handle: *mut Handle, value: f32) -> i32 {
  unsafe { with_engine(handle, |engine| engine.set_intensity(value)) }
}

/// Sets mood from its numeric id: 0 dark, 1 bright, 2 tense, 3 calm.
///
/// # Safety
///
/// `handle` must be null or a live handle.
#[no_mangle]
pub unsafe extern "C" fn algo_wasm_engine_set_mood(handle: *mut Handle, mood: u32) -> i32 {
  unsafe {
    try_with_engine(handle, |engine| {
      let mood = Mood::from_index(mood).ok_or("Mood must be dark, bright, tense, or calm.")?;
      engine.set_mood(mood);
      Ok(())
    })
  }
}

/// Sets tempo range.
///
/// # Safety
///
/// `handle` must be null or a live handle.
#[no_mangle]
pub unsafe extern "C" fn algo_wasm_engine_set_tempo_range(
  handle: *mut Handle,
  min_bpm: f32,
  max_bpm: f32
) -> i32 {
  unsafe {
    try_with_engine(handle, |engine| {
      engine.set_tempo_range(min_bpm, max_bpm).map_err(|error| error.message())
    })
  }
}

/// Sets track mute state. `track` is the track's index in built-in order.
///
/// # Safety
///
/// `handle` must be null or a live handle.
#[no_mangle]
pub unsafe extern "C" fn algo_wasm_engine_set_muted(
  handle: *mut Handle,
  track: u32,
  muted: i32
) -> i32 {
  unsafe {
    try_with_engine(handle, |engine| {
      engine.set_muted(track_from(track)?, muted != 0);
      Ok(())
    })
  }
}

/// Sets track solo state. `track` is the track's index in built-in order.
///
/// # Safety
///
/// `handle` must be null or a live handle.
#[no_mangle]
pub unsafe extern "C" fn algo_wasm_engine_set_solo(handle: *mut Handle, track: u32, solo: i32) -> i32 {
  unsafe {
    try_with_engine(handle, |engine| {
      engine.set_solo(track_from(track)?, solo != 0);
      Ok(())
    })
  }
}

/// Writes the snapshot into the handle's buffer and returns a pointer to its
/// `SNAPSHOT_WIRE_LEN` `f64` values, or null on error. The pointer stays valid
/// until the engine is destroyed.
///
/// # Safety
///
/// `handle` must be null or a live handle.
#[no_mangle]
pub unsafe extern "C" fn algo_wasm_engine_snapshot(handle: *mut Handle) -> *const f64 {
  clear_error();

  // Handles are only created by `algo_wasm_engine_create` and stay valid
  // until `algo_wasm_engine_destroy`.
  match unsafe { handle.as_mut() } {
    Some(handle) => {
      handle.snapshot = handle.engine.snapshot().to_wire();
      handle.snapshot.as_ptr()
    }
    None => fail("Engine pointer was null.", std::ptr::null())
  }
}

/// Moves the buffered events into the handle's event buffer and returns how
/// many there are. Read them from `algo_wasm_engine_events_ptr`.
///
/// # Safety
///
/// `handle` must be null or a live handle.
#[no_mangle]
pub unsafe extern "C" fn algo_wasm_engine_drain_events(handle: *mut Handle) -> usize {
  clear_error();

  match unsafe { handle.as_mut() } {
    Some(handle) => handle.engine.drain_events(&mut handle.events),
    None => fail("Engine pointer was null.", 0)
  }
}

/// Returns a pointer to the handle's event buffer of `f64` values, five per
/// event, or null when the handle is null.
///
/// # Safety
///
/// `handle` must be null or a live handle.
#[no_mangle]
pub unsafe extern "C" fn algo_wasm_engine_events_ptr(handle: *mut Handle) -> *const f64 {
  match unsafe { handle.as_ref() } {
    Some(handle) => handle.events.as_ptr(),
    None => std::ptr::null()
  }
}

/// Exports MIDI bytes into the handle's buffer and returns their length.
/// Read them from `algo_wasm_engine_midi_ptr` before the next export.
///
/// # Safety
///
/// `handle` must be null or a live handle.
#[no_mangle]
pub unsafe extern "C" fn algo_wasm_engine_export_midi(handle: *mut Handle) -> usize {
  clear_error();

  match unsafe { handle.as_mut() } {
    Some(handle) => {
      handle.midi = handle.engine.export_midi();
      handle.midi.len()
    }
    None => fail("Engine pointer was null.", 0)
  }
}

/// Returns a pointer to the bytes of the last MIDI export, or null when the
/// handle is null.
///
/// # Safety
///
/// `handle` must be null or a live handle.
#[no_mangle]
pub unsafe extern "C" fn algo_wasm_engine_midi_ptr(handle: *mut Handle) -> *const u8 {
  match unsafe { handle.as_ref() } {
    Some(handle) => handle.midi.as_ptr(),
    None => std::ptr::null()
  }
}

/// Returns the length in bytes of the last error message, or zero.
#[no_mangle]
pub extern "C" fn algo_wasm_last_error_len() -> usize {
  LAST_ERROR.with(|error| error.get().len())
}

/// Returns a pointer to the last error message. Messages are ASCII.
#[no_mangle]
pub extern "C" fn algo_wasm_last_error_ptr() -> *const u8 {
  LAST_ERROR.with(|error| error.get().as_ptr())
}

/// Runs an action that cannot fail against the engine. Returns 0, or -1 when
/// the handle is null.
unsafe fn with_engine(handle: *mut Handle, action: impl FnOnce(&mut Engine)) -> i32 {
  unsafe {
    try_with_engine(handle, |engine| {
      action(engine);
      Ok(())
    })
  }
}

/// Runs an action against the engine. Returns 0 on success, or -1 after
/// recording the error message.
unsafe fn try_with_engine(
  handle: *mut Handle,
  action: impl FnOnce(&mut Engine) -> Result<(), &'static str>
) -> i32 {
  clear_error();

  // Handles are only created by `algo_wasm_engine_create` and stay valid
  // until `algo_wasm_engine_destroy`.
  let Some(handle) = (unsafe { handle.as_mut() }) else {
    return fail("Engine pointer was null.", -1);
  };

  match action(&mut handle.engine) {
    Ok(()) => 0,
    Err(message) => fail(message, -1)
  }
}

/// Reads and validates a flat profile from WASM memory.
unsafe fn read_profile(pointer: *const f32, len: usize) -> Result<Profile, &'static str> {
  if pointer.is_null() || len != PROFILE_WIRE_LEN {
    return Err(
      "Profile data is missing or the wrong size. Use the WASM file and JavaScript from the same release."
    );
  }

  // The length was checked against the fixed layout size above, and the
  // caller provides that many readable values for the duration of this call.
  let values = unsafe { slice::from_raw_parts(pointer, len) };
  Profile::from_wire(values).map_err(|error| error.message())
}

fn track_from(index: u32) -> Result<TrackId, &'static str> {
  TrackId::from_index(index as usize).ok_or("Track id is not recognised.")
}

fn buffer_layout(len: usize) -> Option<Layout> {
  if len == 0 || len > MAX_ALLOC_BYTES {
    return None;
  }

  Layout::from_size_align(len, BUFFER_ALIGN).ok()
}

/// Records an error message and returns the given failure value.
fn fail<T>(message: &'static str, value: T) -> T {
  LAST_ERROR.with(|error| error.set(message));
  value
}

fn clear_error() {
  LAST_ERROR.with(|error| error.set(""));
}

#[cfg(test)]
mod tests {
  use super::*;
  use algo_wasm_core::builtin::AMIGA_HOUSE_95ISH_WIRE;

  #[test]
  fn engine_round_trip_through_the_boundary() {
    let bytes = PROFILE_WIRE_LEN * 4;
    let profile = algo_wasm_alloc(bytes).cast::<f32>();
    let frames = 4_096;
    let output = algo_wasm_alloc(frames * 2 * 4).cast::<f32>();
    assert!(!profile.is_null() && !output.is_null());

    unsafe {
      slice::from_raw_parts_mut(profile, PROFILE_WIRE_LEN).copy_from_slice(&AMIGA_HOUSE_95ISH_WIRE);

      // A corrupt profile is refused with a message rather than a crash.
      *profile.add(40) = 4.0e9;
      assert!(algo_wasm_engine_create(48_000, 0, 7, profile, PROFILE_WIRE_LEN).is_null());
      assert!(algo_wasm_last_error_len() > 0);
      *profile.add(40) = AMIGA_HOUSE_95ISH_WIRE[40];

      let handle = algo_wasm_engine_create(48_000, 0, 7, profile, PROFILE_WIRE_LEN);
      assert!(!handle.is_null());
      assert_eq!(algo_wasm_engine_start(handle), 0);
      assert_eq!(algo_wasm_engine_render(handle, output, frames * 2), 0);
      assert!(slice::from_raw_parts(output, frames * 2).iter().any(|sample| sample.abs() > 0.001));

      let snapshot = slice::from_raw_parts(algo_wasm_engine_snapshot(handle), SNAPSHOT_WIRE_LEN);
      assert_eq!((snapshot[1], snapshot[2], snapshot[9]), (7.0, 48_000.0, 1.0));

      let events = algo_wasm_engine_drain_events(handle);
      assert!(events > 0);
      assert_eq!(algo_wasm_engine_drain_events(handle), 0);

      assert_eq!(algo_wasm_engine_set_tempo_range(handle, f32::NAN, f32::NAN), -1);
      assert_eq!(algo_wasm_engine_set_mood(handle, 9), -1);
      assert_eq!(algo_wasm_engine_set_muted(handle, 99, 1), -1);
      assert!(algo_wasm_engine_export_midi(handle) > 14);

      algo_wasm_engine_destroy(handle);
      assert_eq!(algo_wasm_engine_start(std::ptr::null_mut()), -1);
      algo_wasm_dealloc(output.cast(), frames * 2 * 4);
      algo_wasm_dealloc(profile.cast(), bytes);
    }
  }
}
