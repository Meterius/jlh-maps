import { fileURLToPath, URL } from 'node:url'
import { readFileSync } from 'node:fs'
import { relative } from 'node:path'

import { defineConfig, normalizePath, type Plugin } from 'vite'
import vue from '@vitejs/plugin-vue'
import vueJsx from '@vitejs/plugin-vue-jsx'
import ui from '@nuxt/ui/vite'
import vueDevTools from 'vite-plugin-vue-devtools'
import wasm from 'vite-plugin-wasm'
import { viteStaticCopy } from 'vite-plugin-static-copy'

const outlineSolidBase =
  'relative isolate overflow-hidden bg-default before:absolute before:inset-0 before:pointer-events-none before:content-[""] before:opacity-0 disabled:before:opacity-0 aria-disabled:before:opacity-0 focus:outline-none'

const crossOriginIsolationHeaders = {
  'Cross-Origin-Opener-Policy': 'same-origin',
  'Cross-Origin-Embedder-Policy': 'credentialless',
}

const coiServiceWorkerPath = fileURLToPath(
  new URL('./node_modules/coi-serviceworker/coi-serviceworker.js', import.meta.url),
)

const bevyAssetsPath = fileURLToPath(new URL('../../crates/jlh_maps_app/assets', import.meta.url))
const bevyAssetsGlob = `${normalizePath(bevyAssetsPath)}/**/*`

// injects https://github.com/gzuidhof/coi-serviceworker into demo build which is required
// to allow a static headless file server to run with COEP and COOP headers required for wasm threading
function demoCoopCoepPlugin(): Plugin {
  return {
    name: 'jlh-maps-demo-coop-coep',
    apply: 'build' as const,
    transformIndexHtml() {
      return [
        {
          tag: 'script',
          children: 'window.coi = { coepCredentialless: () => true, quiet: () => true }',
          injectTo: 'head' as const,
        },
        {
          tag: 'script',
          attrs: {
            src: './coi-serviceworker.js',
          },
          injectTo: 'head' as const,
        },
      ]
    },
    generateBundle() {
      this.emitFile({
        type: 'asset',
        fileName: 'coi-serviceworker.js',
        source: readFileSync(coiServiceWorkerPath, 'utf8'),
      })
    },
  }
}

// https://vite.dev/config/
export default defineConfig(({ mode }) => ({
  ...(mode === 'demo' ? { base: './' } : {}),
  server: {
    headers: crossOriginIsolationHeaders,
    fs: {
      allow: [
        './',
        '../../crates/jlh_maps_frontend/pkg',
        '../../crates/jlh_maps_app/pkg',
        '../../crates/jlh_maps_app/pkg_threaded',
      ],
    },
  },
  preview: {
    headers: crossOriginIsolationHeaders,
  },
  plugins: [
    viteStaticCopy({
      targets: [
        {
          src: bevyAssetsGlob,
          dest: 'bevy-assets',
          rename: (_fileName, _fileExtension, fullPath) =>
            normalizePath(relative(bevyAssetsPath, fullPath)),
        },
      ],
    }),
    ...(mode === 'demo' ? [demoCoopCoepPlugin()] : []),
    wasm(),
    vue(),
    ui({
      ui: {
        colors: {
          primary: 'fuchsia',
          secondary: 'sky',
          success: 'green',
          info: 'blue',
          warning: 'yellow',
          error: 'red',
          neutral: 'slatish',
        },
        button: {
          variants: {
            variant: {
              'outline-solid': '',
            },
          },
          compoundVariants: [
            {
              color: 'primary',
              variant: 'outline-solid',
              class: `${outlineSolidBase} ring ring-inset ring-primary/50 text-primary before:bg-primary hover:before:opacity-10 active:before:opacity-10 focus-visible:ring-2 focus-visible:ring-primary`,
            },
            {
              color: 'secondary',
              variant: 'outline-solid',
              class: `${outlineSolidBase} ring ring-inset ring-secondary/50 text-secondary before:bg-secondary hover:before:opacity-10 active:before:opacity-10 focus-visible:ring-2 focus-visible:ring-secondary`,
            },
            {
              color: 'success',
              variant: 'outline-solid',
              class: `${outlineSolidBase} ring ring-inset ring-success/50 text-success before:bg-success hover:before:opacity-10 active:before:opacity-10 focus-visible:ring-2 focus-visible:ring-success`,
            },
            {
              color: 'info',
              variant: 'outline-solid',
              class: `${outlineSolidBase} ring ring-inset ring-info/50 text-info before:bg-info hover:before:opacity-10 active:before:opacity-10 focus-visible:ring-2 focus-visible:ring-info`,
            },
            {
              color: 'warning',
              variant: 'outline-solid',
              class: `${outlineSolidBase} ring ring-inset ring-warning/50 text-warning before:bg-warning hover:before:opacity-10 active:before:opacity-10 focus-visible:ring-2 focus-visible:ring-warning`,
            },
            {
              color: 'error',
              variant: 'outline-solid',
              class: `${outlineSolidBase} ring ring-inset ring-error/50 text-error before:bg-error hover:before:opacity-10 active:before:opacity-10 focus-visible:ring-2 focus-visible:ring-error`,
            },
            {
              color: 'neutral',
              variant: 'outline-solid',
              class:
                'ring ring-inset ring-accented text-default bg-default hover:bg-elevated active:bg-elevated disabled:bg-default aria-disabled:bg-default focus:outline-none focus-visible:ring-2 focus-visible:ring-inverted',
            },
          ],
        },
      },
    }),
    vueJsx(),
    vueDevTools(),
  ],
  resolve: {
    dedupe: ['maplibre-gl'],
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
  },
}))
