import { accentTokens, type Theme } from './accent'

/**
 * Writes the appearance onto the document root.
 *
 * This is a single synchronous pass: the theme becomes a `data-theme` attribute (which swaps
 * the static token block in styles/tokens.css) and the five accent-derived properties are set
 * inline. Nothing reloads, nothing remounts, so switching theme or accent cannot flash —
 * the browser simply recomputes the custom properties.
 */
export function applyAppearance(root: HTMLElement, theme: Theme, accent: string): void {
  root.dataset.theme = theme

  // Widened to an index signature so Object.entries yields `string` values rather than `any`.
  const tokens: Record<string, string> = accentTokens(accent, theme)
  for (const [name, value] of Object.entries(tokens)) {
    root.style.setProperty(name, value)
  }
}
