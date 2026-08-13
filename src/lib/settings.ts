import { invoke, isTauri } from '@tauri-apps/api/core'

import { DEFAULT_ACCENT, normalizeAccent, type Theme } from './accent'

export interface Appearance {
  theme: Theme
  accent: string
}

export interface Settings {
  appearance: Appearance
}

export const DEFAULT_SETTINGS: Settings = {
  appearance: { theme: 'dark', accent: DEFAULT_ACCENT },
}

/**
 * Repairs anything the backend could not, so a malformed value can never reach the renderer.
 *
 * The Rust side already sanitizes on read and on write; this is the same guarantee restated
 * on the frontend so that running outside Tauri (plain `vite dev`, tests) behaves identically.
 */
function sanitize(settings: Settings): Settings {
  return {
    appearance: {
      theme: settings.appearance?.theme === 'light' ? 'light' : 'dark',
      accent: normalizeAccent(settings.appearance?.accent ?? '') ?? DEFAULT_ACCENT,
    },
  }
}

export async function loadSettings(): Promise<Settings> {
  // Outside Tauri there is no config file; the defaults keep the UI usable in a plain browser.
  if (!isTauri()) return DEFAULT_SETTINGS

  try {
    return sanitize(await invoke<Settings>('get_settings'))
  } catch (error) {
    /*
     * Never let this reject. main.tsx awaits it at module scope, so a rejection would leave
     * the module unevaluated: nothing rendered, an empty window, and no way for the user to
     * repair it from inside the app. Same reasoning as the Rust side, which falls back to
     * defaults on an unreadable file rather than refusing to launch.
     */
    console.error('lecture des réglages impossible, retour aux valeurs par défaut', error)
    return DEFAULT_SETTINGS
  }
}

/**
 * Persists the appearance and returns what was actually stored.
 *
 * The caller renders the returned value rather than the one it sent, so the frontend and the
 * config file can never disagree about a normalized accent.
 */
export async function saveAppearance(appearance: Appearance): Promise<Settings> {
  if (!isTauri()) return sanitize({ appearance })

  return sanitize(await invoke<Settings>('set_appearance', { appearance }))
}
