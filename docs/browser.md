# Browser Notes

## AudioWorklet

AlgoWASM requires AudioWorklet for real-time playback. It does not provide a ScriptProcessorNode fallback.

## User gesture

Most browsers lock audio until a user gesture. Call `start()` from a click, tap, key press, or another trusted interaction.

## WASM MIME type

Serve `.wasm` as:

```text
application/wasm
```

The loader falls back to ArrayBuffer compilation when local servers use a wrong MIME type. If a server claims `application/wasm` and streaming still fails, the error is reported.

## CSP

Strict Content Security Policy can block WebAssembly compilation or worklet loading. Keep the WASM and worklet assets same-origin where possible and check browser console messages during deployment.

## SharedArrayBuffer

SharedArrayBuffer is not required in this version. Future versions may add it as an optional transport with COOP and COEP documentation.
