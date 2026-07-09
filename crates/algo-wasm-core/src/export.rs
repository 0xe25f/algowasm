use crate::composition::{PatternBank, STEPS_PER_BAR};
use crate::types::TrackId;

/// Writes the current four-bar pattern as a type-0 MIDI file.
pub fn write_midi(bank: &PatternBank, bpm: f32) -> Vec<u8> {
  let ticks_per_quarter = 480u16;
  let mut track = Vec::with_capacity(4096);
  let tempo_us = (60_000_000.0 / bpm.clamp(40.0, 240.0)) as u32;

  write_delta(&mut track, 0);
  track.extend_from_slice(&[0xff, 0x51, 0x03]);
  track.push(((tempo_us >> 16) & 0xff) as u8);
  track.push(((tempo_us >> 8) & 0xff) as u8);
  track.push((tempo_us & 0xff) as u8);

  let ticks_per_step = u32::from(ticks_per_quarter) / 4;
  let mut absolute_events = Vec::with_capacity(256);

  for (bar_index, bar) in bank.bars.iter().enumerate() {
    let bar_tick = bar_index as u32 * ticks_per_step * STEPS_PER_BAR as u32;

    for step in 0..STEPS_PER_BAR {
      let tick = bar_tick + step as u32 * ticks_per_step;
      collect_note(&mut absolute_events, tick, TrackId::Kick, bar.kick[step], 36, 88, ticks_per_step);
      collect_note(&mut absolute_events, tick, TrackId::Snare, bar.snare[step], 38, 78, ticks_per_step);

      if bar.closed_hat[step] > 0.0 {
        absolute_events.push(MidiNote {
          tick,
          track: TrackId::ClosedHat,
          note: 42,
          velocity: (bar.closed_hat[step] * 100.0) as u8,
          length: ticks_per_step / 2
        });
      }

      for (track, note) in [
        (TrackId::Bass, bar.bass[step]),
        (TrackId::Chords, bar.chords[step]),
        (TrackId::Lead, bar.lead[step]),
        (TrackId::Arp, bar.arp[step])
      ] {
        if let Some(note) = note {
          absolute_events.push(MidiNote {
            tick,
            track,
            note: note.note.clamp(0, 127) as u8,
            velocity: (note.velocity.clamp(0.0, 1.0) * 110.0) as u8,
            length: ticks_per_step * u32::from(note.length_steps)
          });
        }
      }
    }
  }

  absolute_events.sort_by_key(|event| event.tick);
  let mut last_tick = 0;

  for event in absolute_events {
    write_delta(&mut track, event.tick.saturating_sub(last_tick));
    track.push(0x90 | channel_for_track(event.track));
    track.push(event.note);
    track.push(event.velocity.max(1));
    last_tick = event.tick;

    write_delta(&mut track, event.length);
    track.push(0x80 | channel_for_track(event.track));
    track.push(event.note);
    track.push(0);
    last_tick = last_tick.saturating_add(event.length);
  }

  write_delta(&mut track, 0);
  track.extend_from_slice(&[0xff, 0x2f, 0x00]);

  let mut file = Vec::with_capacity(track.len() + 32);
  file.extend_from_slice(b"MThd");
  file.extend_from_slice(&6u32.to_be_bytes());
  file.extend_from_slice(&0u16.to_be_bytes());
  file.extend_from_slice(&1u16.to_be_bytes());
  file.extend_from_slice(&ticks_per_quarter.to_be_bytes());
  file.extend_from_slice(b"MTrk");
  file.extend_from_slice(&(track.len() as u32).to_be_bytes());
  file.extend_from_slice(&track);
  file
}

#[derive(Clone, Copy, Debug)]
struct MidiNote {
  tick: u32,
  track: TrackId,
  note: u8,
  velocity: u8,
  length: u32
}

fn collect_note(
  events: &mut Vec<MidiNote>,
  tick: u32,
  track: TrackId,
  active: bool,
  note: u8,
  velocity: u8,
  length: u32
) {
  if active {
    events.push(MidiNote {
      tick,
      track,
      note,
      velocity,
      length
    });
  }
}

fn channel_for_track(track: TrackId) -> u8 {
  match track {
    TrackId::Kick | TrackId::Snare | TrackId::ClosedHat | TrackId::OpenHat | TrackId::Percussion => 9,
    TrackId::Bass => 1,
    TrackId::Chords | TrackId::Pad => 2,
    TrackId::Lead => 3,
    TrackId::Arp => 4,
    TrackId::Fx => 5
  }
}

fn write_delta(output: &mut Vec<u8>, value: u32) {
  let mut buffer = [0u8; 5];
  let mut value = value;
  let mut index = 4;
  buffer[index] = (value & 0x7f) as u8;

  while {
    value >>= 7;
    value > 0
  } {
    index = index.saturating_sub(1);
    buffer[index] = ((value & 0x7f) as u8) | 0x80;
  }

  output.extend_from_slice(&buffer[index..]);
}

#[cfg(test)]
mod tests {
  use super::write_midi;
  use crate::composition::PatternBank;
  use crate::profile::amiga_house_95ish;
  use crate::rng::Rng64;
  use crate::types::SectionKind;

  #[test]
  fn midi_export_has_header() {
    let profile = amiga_house_95ish();
    let mut rng = Rng64::new(1);
    let bank = PatternBank::generate(&mut rng, &profile, None, SectionKind::Intro, 0.5);
    let midi = write_midi(&bank, 126.0);

    assert_eq!(&midi[0..4], b"MThd");
  }
}
