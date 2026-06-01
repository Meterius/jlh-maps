import { fileURLToPath } from 'node:url'
import type { ConfigEnv, UserConfig } from 'vite'
import { mergeConfig, defineConfig, configDefaults } from 'vitest/config'
import viteConfig from './vite.config'

function resolveViteConfig(env: ConfigEnv): UserConfig {
  if (typeof viteConfig === 'function') return viteConfig(env) as UserConfig

  return viteConfig as UserConfig
}

export default defineConfig((env) =>
  mergeConfig(
    resolveViteConfig(env),
    defineConfig({
      test: {
        environment: 'jsdom',
        exclude: [...configDefaults.exclude, 'e2e/**'],
        root: fileURLToPath(new URL('./', import.meta.url)),
      },
    }),
  ),
)
