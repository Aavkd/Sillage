// `vitest/config` rather than `vite`: it is the one that types the `test` block below.
import { defineConfig } from 'vitest/config'
import react from '@vitejs/plugin-react'

// Tauri fixes the dev server port so the Rust side knows where to point the webview.
const DEV_PORT = 1420

export default defineConfig({
  plugins: [react()],

  // `src-tauri` is a Cargo project; Vite must never try to watch or bundle it.
  clearScreen: false,
  server: {
    port: DEV_PORT,
    strictPort: true,
    watch: { ignored: ['**/src-tauri/**', '**/spike/**', '**/vendor/**'] },
  },

  build: {
    // Tauri v2 ships a Chromium-based webview on Windows (WebView2).
    target: 'chrome120',
    sourcemap: true,
    // Fonts are inlined only if tiny; woff2 files must stay separate assets.
    assetsInlineLimit: 0,
  },

  test: {
    environment: 'jsdom',
    include: ['src/**/*.test.{ts,tsx}', 'tests/**/*.test.{ts,tsx}'],
    // Required: tests/tokens.test.ts reads the stylesheets through `?raw` to compare every
    // token against the prototype. With `css: false` Vitest stubs CSS imports to an empty
    // string and the comparison silently passes over nothing.
    css: true,
  },
})
