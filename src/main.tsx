import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'

import App from './App'
import { loadSettings } from './lib/settings'
import { applyAppearance } from './lib/theme'

import './styles/fonts.css'
import './styles/tokens.css'
import './styles/base.css'

/*
 * The stored theme goes onto the document *before* the first render, so the first frame the
 * user sees is already the right one — no default-themed flash to correct afterwards.
 *
 * The window is created visible (tauri.conf.json). The usual `visible: false` + `show()` from
 * JS does not work here: WebView2 does not run scripts while the window is hidden, so the call
 * that would reveal it never executes and the window stays hidden for good. Nothing is painted
 * before React commits anyway — the window is transparent — so starting visible costs no flash.
 */
const settings = await loadSettings()
applyAppearance(document.documentElement, settings.appearance.theme, settings.appearance.accent)

const root = document.getElementById('root')
if (!root) throw new Error('élément #root introuvable')

createRoot(root).render(
  <StrictMode>
    <App initialSettings={settings} />
  </StrictMode>,
)
