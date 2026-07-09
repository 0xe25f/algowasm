# Performance

## Render path

The real-time path uses a fixed voice pool, preallocated render buffers, and simple DSP. It avoids logging and JSON parsing inside the audio callback.

## Measured checks

Current tests verify:

- deterministic output for the same seed;
- finite audio samples;
- non-silent output for the built-in profile;
- limiter bounds;
- no stuck notes after stop;
- MIDI export header;
- profile validation and JSON round trips.

## Practical targets

At 48 kHz, a 128-frame block should render comfortably below real time on a normal 2025 laptop. Mobile performance depends on browser, CPU, battery state, and active voices.

## Degradation options

Profiles can reduce:

- `maxVoices`;
- track gains and density;
- delay and reverb send;
- melody and arpeggio density;
- section energy.
