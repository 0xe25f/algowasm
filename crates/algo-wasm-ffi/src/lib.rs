//! C-compatible WASM boundary for AlgoWASM.
//!
//! JavaScript owns loading and message passing. This crate exposes compact
//! functions for creating an engine, rendering stereo samples, applying live
//! controls, reading snapshots, and exporting MIDI.

use algo_wasm_core::{amiga_house_95ish, Engine, Mood, Profile, TrackId};
use std::cell::RefCell;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::slice;

thread_local! {
  static LAST_ERROR: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
  static LAST_JSON: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
  static LAST_BYTES: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// Returns the packed API version.
#[no_mangle]
pub extern "C" fn algo_wasm_version() -> u32 {
  0x0001_0000
}

/// Allocates a byte buffer owned by WASM.
#[no_mangle]
pub extern "C" fn algo_wasm_alloc(len: usize) -> *mut u8 {
  if len == 0 {
    return std::ptr::null_mut();
  }

  let mut buffer = Vec::<u8>::with_capacity(len);
  let pointer = buffer.as_mut_ptr();
  std::mem::forget(buffer);
  pointer
}

/// Frees a byte buffer allocated by WASM.
///
/// # Safety
///
/// `pointer` and `len` must match a live allocation returned by
/// `algo_wasm_alloc`. Passing any other pointer is undefined behaviour.
#[no_mangle]
pub unsafe extern "C" fn algo_wasm_dealloc(pointer: *mut u8, len: usize) {
  if pointer.is_null() || len == 0 {
    return;
  }

  let _ = catch_unwind(AssertUnwindSafe(|| {
    // JavaScript passes back the exact pointer and capacity returned by
    // `algo_wasm_alloc`, so reconstructing the Vec lets Rust free it.
    let buffer = unsafe { Vec::from_raw_parts(pointer, 0, len) };
    drop(buffer);
  }));
}

/// Creates an engine. Returns a null pointer on error.
#[no_mangle]
pub extern "C" fn algo_wasm_engine_create(
  sample_rate: u32,
  seed_high: u32,
  seed_low: u32,
  profile_ptr: *const u8,
  profile_len: usize
) -> *mut Engine {
  clear_error();
  let seed = (u64::from(seed_high) << 32) | u64::from(seed_low);
  let result = catch_unwind(AssertUnwindSafe(|| {
    let profile = if profile_ptr.is_null() || profile_len == 0 {
      amiga_house_95ish()
    } else {
      let json = utf8_from_raw(profile_ptr, profile_len)?;
      Profile::from_json(&json).map_err(|error| error.message().to_string())?
    };

    Engine::new(sample_rate, seed, profile).map_err(|error| error.message().to_string())
  }));

  match result {
    Ok(Ok(engine)) => Box::into_raw(Box::new(engine)),
    Ok(Err(error)) => {
      set_error(error);
      std::ptr::null_mut()
    }
    Err(_) => {
      set_error("Engine creation panicked and was stopped at the WASM boundary.");
      std::ptr::null_mut()
    }
  }
}

/// Destroys an engine. Repeated calls with null are safe.
///
/// # Safety
///
/// `engine` must be null or a pointer returned by `algo_wasm_engine_create`
/// that has not already been destroyed.
#[no_mangle]
pub unsafe extern "C" fn algo_wasm_engine_destroy(engine: *mut Engine) {
  if engine.is_null() {
    return;
  }

  let _ = catch_unwind(AssertUnwindSafe(|| {
    // Engine pointers are created by `Box::into_raw` in this crate and must be
    // returned exactly once for destruction.
    let engine = unsafe { Box::from_raw(engine) };
    drop(engine);
  }));
}

/// Starts playback.
#[no_mangle]
pub extern "C" fn algo_wasm_engine_start(engine: *mut Engine) -> i32 {
  with_engine(engine, -1, |engine| {
    engine.start();
    0
  })
}

/// Pauses playback.
#[no_mangle]
pub extern "C" fn algo_wasm_engine_pause(engine: *mut Engine) -> i32 {
  with_engine(engine, -1, |engine| {
    engine.pause();
    0
  })
}

/// Resumes playback.
#[no_mangle]
pub extern "C" fn algo_wasm_engine_resume(engine: *mut Engine) -> i32 {
  with_engine(engine, -1, |engine| {
    engine.resume();
    0
  })
}

/// Stops playback and clears voices.
#[no_mangle]
pub extern "C" fn algo_wasm_engine_stop(engine: *mut Engine) -> i32 {
  with_engine(engine, -1, |engine| {
    engine.stop();
    0
  })
}

/// Renders interleaved stereo f32 samples into WASM memory.
///
/// # Safety
///
/// `engine` must be a live engine pointer. `output_ptr` must point to a valid
/// mutable `f32` buffer with at least `output_len` elements.
#[no_mangle]
pub unsafe extern "C" fn algo_wasm_engine_render(
  engine: *mut Engine,
  output_ptr: *mut f32,
  output_len: usize
) -> i32 {
  if output_ptr.is_null() {
    set_error("Render output pointer was null.");
    return -1;
  }

  with_engine(engine, -1, |engine| {
    // The caller provides an f32 buffer in WASM memory with `output_len`
    // elements. The worklet allocates it once and reuses it.
    let output = unsafe { slice::from_raw_parts_mut(output_ptr, output_len) };
    engine.render_interleaved(output);
    0
  })
}

/// Sets the deterministic seed.
#[no_mangle]
pub extern "C" fn algo_wasm_engine_set_seed(
  engine: *mut Engine,
  seed_high: u32,
  seed_low: u32
) -> i32 {
  with_engine(engine, -1, |engine| {
    engine.set_seed((u64::from(seed_high) << 32) | u64::from(seed_low));
    0
  })
}

/// Sets the profile from JSON.
#[no_mangle]
pub extern "C" fn algo_wasm_engine_set_profile(
  engine: *mut Engine,
  profile_ptr: *const u8,
  profile_len: usize
) -> i32 {
  if profile_ptr.is_null() || profile_len == 0 {
    set_error("Profile JSON is required.");
    return -1;
  }

  with_engine(engine, -1, |engine| {
    let json = match utf8_from_raw(profile_ptr, profile_len) {
      Ok(json) => json,
      Err(error) => {
        set_error(error);
        return -1;
      }
    };
    let profile = match Profile::from_json(&json) {
      Ok(profile) => profile,
      Err(error) => {
        set_error(error.message());
        return -1;
      }
    };

    match engine.set_profile(profile) {
      Ok(()) => 0,
      Err(error) => {
        set_error(error.message());
        -1
      }
    }
  })
}

/// Sets master volume.
#[no_mangle]
pub extern "C" fn algo_wasm_engine_set_volume(engine: *mut Engine, value: f32) -> i32 {
  with_engine(engine, -1, |engine| {
    engine.set_volume(value);
    0
  })
}

/// Sets energy.
#[no_mangle]
pub extern "C" fn algo_wasm_engine_set_energy(engine: *mut Engine, value: f32) -> i32 {
  with_engine(engine, -1, |engine| {
    engine.set_energy(value);
    0
  })
}

/// Sets intensity.
#[no_mangle]
pub extern "C" fn algo_wasm_engine_set_intensity(engine: *mut Engine, value: f32) -> i32 {
  with_engine(engine, -1, |engine| {
    engine.set_intensity(value);
    0
  })
}

/// Sets mood from a UTF-8 id.
#[no_mangle]
pub extern "C" fn algo_wasm_engine_set_mood(
  engine: *mut Engine,
  mood_ptr: *const u8,
  mood_len: usize
) -> i32 {
  if mood_ptr.is_null() || mood_len == 0 {
    set_error("Mood id is required.");
    return -1;
  }

  with_engine(engine, -1, |engine| {
    let id = match utf8_from_raw(mood_ptr, mood_len) {
      Ok(id) => id,
      Err(error) => {
        set_error(error);
        return -1;
      }
    };
    let mood = match Mood::from_id(&id) {
      Some(mood) => mood,
      None => {
        set_error("Mood must be dark, bright, tense, or calm.");
        return -1;
      }
    };
    engine.set_mood(mood);
    0
  })
}

/// Sets tempo range.
#[no_mangle]
pub extern "C" fn algo_wasm_engine_set_tempo_range(
  engine: *mut Engine,
  min_bpm: f32,
  max_bpm: f32
) -> i32 {
  with_engine(engine, -1, |engine| match engine.set_tempo_range(min_bpm, max_bpm) {
    Ok(()) => 0,
    Err(error) => {
      set_error(error.message());
      -1
    }
  })
}

/// Sets track mute state.
#[no_mangle]
pub extern "C" fn algo_wasm_engine_set_muted(
  engine: *mut Engine,
  track_ptr: *const u8,
  track_len: usize,
  muted: i32
) -> i32 {
  set_track_state(engine, track_ptr, track_len, muted, true)
}

/// Sets track solo state.
#[no_mangle]
pub extern "C" fn algo_wasm_engine_set_solo(
  engine: *mut Engine,
  track_ptr: *const u8,
  track_len: usize,
  solo: i32
) -> i32 {
  set_track_state(engine, track_ptr, track_len, solo, false)
}

/// Writes snapshot JSON into an internal buffer and returns its length.
#[no_mangle]
pub extern "C" fn algo_wasm_engine_snapshot_json(engine: *mut Engine) -> usize {
  with_engine(engine, 0usize, |engine| match engine.snapshot_json() {
    Ok(json) => store_json(json),
    Err(error) => {
      set_error(error.message());
      0
    }
  })
}

/// Writes current events JSON into an internal buffer and returns its length.
#[no_mangle]
pub extern "C" fn algo_wasm_engine_events_json(engine: *mut Engine) -> usize {
  with_engine(engine, 0usize, |engine| match engine.current_events_json() {
    Ok(json) => store_json(json),
    Err(error) => {
      set_error(error.message());
      0
    }
  })
}

/// Exports MIDI bytes into an internal buffer and returns its length.
#[no_mangle]
pub extern "C" fn algo_wasm_engine_export_midi(engine: *mut Engine) -> usize {
  with_engine(engine, 0usize, |engine| {
    LAST_BYTES.with(|bytes| {
      let mut bytes = bytes.borrow_mut();
      *bytes = engine.export_midi();
      bytes.len()
    })
  })
}

/// Returns a pointer to the internal JSON buffer.
#[no_mangle]
pub extern "C" fn algo_wasm_json_ptr() -> *const u8 {
  LAST_JSON.with(|json| json.borrow().as_ptr())
}

/// Returns a pointer to the internal byte buffer.
#[no_mangle]
pub extern "C" fn algo_wasm_bytes_ptr() -> *const u8 {
  LAST_BYTES.with(|bytes| bytes.borrow().as_ptr())
}

/// Returns last error length.
#[no_mangle]
pub extern "C" fn algo_wasm_last_error_len() -> usize {
  LAST_ERROR.with(|error| error.borrow().len())
}

/// Returns last error pointer.
#[no_mangle]
pub extern "C" fn algo_wasm_last_error_ptr() -> *const u8 {
  LAST_ERROR.with(|error| error.borrow().as_ptr())
}

fn set_track_state(
  engine: *mut Engine,
  track_ptr: *const u8,
  track_len: usize,
  value: i32,
  muted: bool
) -> i32 {
  if track_ptr.is_null() || track_len == 0 {
    set_error("Track id is required.");
    return -1;
  }

  with_engine(engine, -1, |engine| {
    let id = match utf8_from_raw(track_ptr, track_len) {
      Ok(id) => id,
      Err(error) => {
        set_error(error);
        return -1;
      }
    };
    let track = match TrackId::from_id(&id) {
      Some(track) => track,
      None => {
        set_error("Track id is not recognised.");
        return -1;
      }
    };

    if muted {
      engine.set_muted(track, value != 0);
    } else {
      engine.set_solo(track, value != 0);
    }

    0
  })
}

fn with_engine<R>(engine: *mut Engine, fallback: R, action: impl FnOnce(&mut Engine) -> R) -> R
where
  R: Copy
{
  clear_error();

  if engine.is_null() {
    set_error("Engine pointer was null.");
    return fallback;
  }

  match catch_unwind(AssertUnwindSafe(|| {
    // Engine pointers are only created by `algo_wasm_engine_create` and remain
    // valid until `algo_wasm_engine_destroy`.
    let engine = unsafe { &mut *engine };
    action(engine)
  })) {
    Ok(value) => value,
    Err(_) => {
      set_error("Engine call panicked and was stopped at the WASM boundary.");
      fallback
    }
  }
}

fn utf8_from_raw(pointer: *const u8, len: usize) -> Result<String, String> {
  if pointer.is_null() {
    return Err("Pointer was null.".to_string());
  }

  // The caller provides an immutable byte slice in WASM memory for this call.
  let bytes = unsafe { slice::from_raw_parts(pointer, len) };
  std::str::from_utf8(bytes)
    .map(str::to_owned)
    .map_err(|_| "Input bytes were not valid UTF-8.".to_string())
}

fn store_json(json: String) -> usize {
  LAST_JSON.with(|buffer| {
    let mut buffer = buffer.borrow_mut();
    buffer.clear();
    buffer.extend_from_slice(json.as_bytes());
    buffer.len()
  })
}

fn set_error(message: impl AsRef<str>) {
  LAST_ERROR.with(|error| {
    let mut error = error.borrow_mut();
    error.clear();
    error.extend_from_slice(message.as_ref().as_bytes());
  });
}

fn clear_error() {
  LAST_ERROR.with(|error| error.borrow_mut().clear());
}
