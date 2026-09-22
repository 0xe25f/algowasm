#![forbid(unsafe_code)]
//! Core deterministic composition and rendering engine for AlgoWASM.
//!
//! This crate has no browser dependencies. It owns the musical state, profile
//! validation, event generation, procedural synthesis, DSP, offline rendering,
//! and deterministic seed behaviour.

pub mod composition;
pub mod dsp;
pub mod export;
pub mod genome;
pub mod profile;
pub mod render;
pub mod rng;
pub mod synth;
pub mod types;

pub use profile::{
  amiga_house_95ish, downtempo_breakbeat, dub_deep_house, ArrangementRules, AutomationRules,
  BassRules, ChordRules, MelodyRules, MixerProfile, MixerTrackProfile, Patch, Profile,
  ProfileError, RhythmRules, SafetyLimits, ScaleRule, TempoRules
};
pub use render::{Engine, EngineError, EngineMetrics, RenderOptions, Snapshot, TrackSnapshot};
pub use genome::SongGenome;
pub use rng::Rng64;
pub use types::{Mood, MusicEvent, SectionKind, TrackId, WaveShape};
