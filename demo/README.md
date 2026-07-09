# AlgoWASM Demo

A static, dependency-free demo of AlgoWASM for GitHub Pages. It has no build
step: open `index.html` through any static file server and it runs.

This folder is a GitHub-only artefact. It is not part of the
`@algo-wasm/algo-wasm` npm package (see the `files` allowlist in
[`packages/algo-wasm/package.json`](../packages/algo-wasm/package.json)) and is
not one of the npm workspaces.

It mirrors [`examples/vanilla`](../examples/vanilla), minus Vite and the
`@algo-wasm/algo-wasm` package dependency: it imports a vendored copy of the
plain ES module build directly with relative paths, so no bundler or
`npm install` is required to run it.

## Regenerating the vendored library

`demo/vendor/algo-wasm/` is generated, not hand-written, and is git-ignored.
Rebuild it from the repository root whenever the library changes:

```sh
npm run build
npm run build:demo
```

The first command compiles the Rust crate to WASM and builds the TypeScript
package into `packages/algo-wasm/dist`. The second copies the plain JavaScript
modules, the `.wasm` binary, and the worklet from `dist` into
`demo/vendor/algo-wasm/`.

## Running locally

```sh
npx serve demo
```

Then open the printed URL. A real HTTP server is required (not `file://`) so
that the WASM binary is served with the correct MIME type and the
`AudioWorklet` module loads correctly.

## Deploying

The `.github/workflows/pages.yml` workflow builds the library, regenerates
`demo/vendor/`, and publishes this folder to GitHub Pages on every push to
`main`. Nothing under `demo/vendor/` needs to be committed for that to work.
