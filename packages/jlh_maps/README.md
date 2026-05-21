# jlh_maps

Vue 3 + Vite frontend for the JLH maps application. It renders the MapLibre map UI, routing controls, custom layers, and the Rust WASM map integration from `crates/jlh_maps_app`.

## Commands

```sh
npm install
npm run dev
npm run build
npm run test:unit
npm run test:e2e
npm run lint
```

## Environment

Vite loads `.env` for every command and `.env.[mode]` for the active mode.
`npm run dev` runs `vite --mode dev`, so dev-server overrides live in
`.env.dev`. `npm run build` and `npm run build:prod` use the production-domain
defaults from `.env`. `npm run build:local` uses Vite mode `local-docker` and
`.env.local-docker` for the static bundle served by `infra/compose.local.yaml`.
Vite reserves `local` as the `.env.local` machine-override suffix, so it cannot
be used directly as a mode name.

The `VITE_*` values are embedded in the frontend bundle at build time.

## Structure

```text
├── e2e/       Playwright end-to-end tests.
├── public/    Static assets served by Vite.
└── src/       Vue application source.
```

## Source Layout

```text
src/
├── assets/            App styles and bundled assets.
├── components/        Reusable Vue UI components.
├── composables/       Shared Vue composition functions.
├── external/          Boundaries for external libraries and generated clients.
├── maplibre-layers/   Custom MapLibre layers and overlays.
├── router/            Vue Router setup.
├── runtime/           Runtime configuration and app wiring.
├── shaders/           GLSL shader sources.
├── stores/            Pinia stores.
├── types/             Shared TypeScript types.
├── utils/             General frontend utilities.
├── views/             Route-level Vue views.
└── wasm/              WASM integration code.
```
