import { useCallback, useEffect, useState } from 'react'

import { isTauri } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'

export interface WindowControls {
  isMaximized: boolean
  minimize: () => void
  toggleMaximize: () => void
  close: () => void
}

/**
 * The three window controls of the custom title bar (DESIGN.md §6).
 *
 * Outside Tauri the callbacks are inert so the interface can still be opened in a plain
 * browser during development.
 */
export function useWindowControls(): WindowControls {
  const [isMaximized, setIsMaximized] = useState(false)

  useEffect(() => {
    if (!isTauri()) return

    const window = getCurrentWindow()
    let cancelled = false

    const sync = () => {
      void window.isMaximized().then((value) => {
        if (!cancelled) setIsMaximized(value)
      })
    }

    sync()
    // Maximizing by double-clicking the title bar or by snapping bypasses our own button.
    const unlisten = window.onResized(sync)

    return () => {
      cancelled = true
      void unlisten.then((off) => {
        off()
      })
    }
  }, [])

  // `data-maximized` drops the 22px frame radius when the window is flush with the screen.
  useEffect(() => {
    document.documentElement.dataset.maximized = String(isMaximized)
  }, [isMaximized])

  const minimize = useCallback(() => {
    if (isTauri()) void getCurrentWindow().minimize()
  }, [])

  const toggleMaximize = useCallback(() => {
    if (isTauri()) void getCurrentWindow().toggleMaximize()
  }, [])

  const close = useCallback(() => {
    if (isTauri()) void getCurrentWindow().close()
  }, [])

  return { isMaximized, minimize, toggleMaximize, close }
}
