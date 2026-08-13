import { useCallback, useState } from 'react'

import type { Theme } from '../lib/accent'
import { saveAppearance, type Appearance, type Settings } from '../lib/settings'
import { applyAppearance } from '../lib/theme'

export interface AppearanceController {
  appearance: Appearance
  setTheme: (theme: Theme) => void
  setAccent: (accent: string) => void
}

/**
 * Holds the current appearance and keeps the document, the React tree and the config file
 * in agreement.
 *
 * The change is painted before it is persisted: DESIGN.md §12 requires a click on the wheel to
 * re-tint the window immediately, with no save button and no reload. Persistence then happens
 * in the background, and if the backend normalizes the value (uppercasing a hex, repairing a
 * malformed one) the corrected value is adopted.
 */
export function useAppearance(initial: Settings): AppearanceController {
  const [appearance, setLocal] = useState<Appearance>(initial.appearance)

  const update = useCallback((next: Appearance) => {
    applyAppearance(document.documentElement, next.theme, next.accent)
    setLocal(next)

    saveAppearance(next)
      .then((stored) => {
        if (stored.appearance.theme === next.theme && stored.appearance.accent === next.accent) {
          return
        }
        applyAppearance(document.documentElement, stored.appearance.theme, stored.appearance.accent)
        setLocal(stored.appearance)
      })
      .catch((error: unknown) => {
        // The change is already on screen; only its persistence failed. Surfacing this to the
        // user belongs to the Settings screen of phase 10.
        console.error("enregistrement de l'apparence impossible", error)
      })
  }, [])

  const setTheme = useCallback(
    (theme: Theme) => {
      update({ ...appearance, theme })
    },
    [appearance, update],
  )

  const setAccent = useCallback(
    (accent: string) => {
      update({ ...appearance, accent })
    },
    [appearance, update],
  )

  return { appearance, setTheme, setAccent }
}
