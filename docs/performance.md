# Performance

## Render path

The real-time path uses a fixed voice pool, preallocated render buffers, and simple DSP. The audio callback does no logging, parses no text, and allocates nothing in WASM. This includes section changes: generating a new section takes about 6 microseconds and uses no heap memory.

When events are pending, the worklet copies two small buffers and posts them to the main thread, about every 21 ms. That is the only allocation on the audio thread.

## Measured checks

Current tests verify:

- deterministic output for the same seed;
- finite audio samples;
- non-silent output for the built-in profile;
- limiter bounds;
- no stuck notes after stop;
- MIDI export header;
- profile validation, including hostile values;
- that no beat event is lost when events are read less often than audio is rendered;
- size budgets for the WASM binary and the JavaScript bundles.

## Benchmarks

Measured on 2026-11-13 with Node.js 22 (V8) on a cloud x86-64 virtual machine, rendering 60 seconds at 48 kHz in 128-frame blocks, energy 0.87, seed 7158349371545587579.

| Profile | Speed |
| --- | --- |
| Amiga House 95ish | 31 times real time |
| Downtempo Breakbeat | 39 times real time |
| Dub Deep House | 33 times real time |
| Dubstep | 31 times real time |

Version 1.1.0 rendered the Amiga House profile natively at about 88 times real time on the same machine, against about 54 times for 1.2.0. The extra cost is the reverb, the look-ahead limiter, band-limited oscillators, and notes that now ring for their full release time. Browser figures were not measured.

The release build uses `opt-level = "s"`. Against `opt-level = 3` it is about 10% slower and a third smaller (103 KB against 154 KB).

## Release sizes

| File | 1.1.0 | 1.2.0 | 1.2.0 gzipped |
| --- | --- | --- | --- |
| `algo_wasm.wasm` | 347,263 bytes | 103,323 bytes | 46,881 bytes |
| `index.js` | not bundled | 35,057 bytes | 9,396 bytes |
| `worklet/algo-worklet.js` | not bundled | 7,494 bytes | 2,370 bytes |

## Practical targets

At 48 kHz, a 128-frame block should render comfortably below real time on a normal 2025 laptop. Mobile performance depends on browser, CPU, battery state, and active voices.

## Degradation options

Profiles can reduce:

- `maxVoices`;
- track gains and density;
- delay and reverb send;
- melody and arpeggio density;
- section energy.
