# Contributing

## Development

You need Node.js 22 or newer, and Rust with the WASM target:

```sh
rustup target add wasm32-unknown-unknown
```

Then build and test everything with three commands:

```sh
npm install
cargo test --workspace
npm test
```

`npm test` builds the package first, so there is no separate build step to remember. A successful build prints `Built package JavaScript, declarations, schema, and WASM.` before the tests run.

Run `npm install` again after pulling changes, in case the development dependencies have changed.

Other commands:

- `npm run build` builds without testing. It writes the package to `packages/algo-wasm/dist`.
- `npm run build:demo` copies the built package into `demo/vendor` for the demo site.
- `npm run test:browser` runs the Playwright browser tests. Run `npx playwright install` once first. They use an HTTP server so fetch, MIME types, modules, and AudioWorklet behave like production.
- `npm run lint` checks indentation, spelling, and that the generated profile files are up to date.
- `npm run generate:profiles` rewrites the Rust and TypeScript built-in profiles from the JSON files in `profiles/`. `npm run build` does this for you.

## Release

1. Run all tests.
2. Run `npm run build`.
3. Check `packages/algo-wasm/dist`.
4. Confirm package contents include JavaScript, declarations, WASM, worklet, README, and licence, and no source maps. `npm pack --dry-run` in `packages/algo-wasm` lists them.
5. Publish from `packages/algo-wasm`.
