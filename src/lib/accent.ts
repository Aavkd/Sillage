/**
 * Accent derivation — DESIGN.md §2.3, §2.4 and §3.
 *
 * Every function here is a direct port of `Sillage.dc.html` (the prototype is the final
 * authority on appearance). The formulas are reproduced literally, including the exact output
 * string shape of `mix`, because the accent re-tints the whole window and any drift here shows
 * up in twelve components at once.
 */

export type Theme = 'dark' | 'light'

/** DESIGN.md §2.5 — the five preset swatches, in this order. `#E08A4B` is the default. */
export const ACCENT_PRESETS = [
  '#E08A4B',
  '#D9694E',
  '#C98A2E',
  '#B9755F',
  '#8E9A5B',
] as const

export const DEFAULT_ACCENT = ACCENT_PRESETS[0]

/** Text colour laid *on* the accent, above and below the luminance threshold. */
const ON_ACCENT_DARK_TEXT = '#241811'
const ON_ACCENT_LIGHT_TEXT = '#FFF8F1'

/**
 * The threshold is deliberately high (DESIGN.md §2.3): it flips to dark text as soon as the
 * accent gets light, rather than waiting for the usual 0.5.
 */
export const ON_ACCENT_THRESHOLD = 0.62

const HEX_PATTERN = /^#[0-9A-Fa-f]{6}$/

/** Validates `#RRGGBB` and normalizes to uppercase; `null` when malformed. */
export function normalizeAccent(raw: string): string | null {
  return HEX_PATTERN.test(raw) ? raw.toUpperCase() : null
}

function channels(hex: string): [number, number, number] {
  const n = parseInt(hex.slice(1), 16)
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255]
}

/**
 * `mix(hex, alpha)` → `rgba(r, g, b, alpha)`.
 *
 * The components come from `hex`, the alpha is passed through untouched.
 */
export function mix(hex: string, alpha: number): string {
  const [r, g, b] = channels(hex)
  return `rgba(${r}, ${g}, ${b}, ${alpha})`
}

/** Perceived luminance, `(0.299·R + 0.587·G + 0.114·B) / 255`, in `0..1`. */
export function lum(hex: string): number {
  const [r, g, b] = channels(hex)
  return (0.299 * r + 0.587 * g + 0.114 * b) / 255
}

/**
 * HSL → RGB, the division-free variant used by the prototype.
 *
 * `h` in degrees, `s` and `l` in `0..1`. Returns three `0..255` integers.
 */
export function hsl2rgb(h: number, s: number, l: number): [number, number, number] {
  const k = (n: number) => (n + h / 30) % 12
  const a = s * Math.min(l, 1 - l)
  const f = (n: number) => l - a * Math.max(-1, Math.min(k(n) - 3, Math.min(9 - k(n), 1)))
  return [f(0), f(8), f(4)].map((v) => Math.round(v * 255)) as [number, number, number]
}

/** Three `0..255` channels → `#RRGGBB`, uppercase. */
export function toHex(rgb: readonly [number, number, number]): string {
  return '#' + rgb.map((v) => v.toString(16).padStart(2, '0')).join('').toUpperCase()
}

/**
 * The accent wheel colour at a normalized saturation (DESIGN.md §3).
 *
 * `hue` in degrees, `sat` the distance to the centre normalized to `0..1`. The bounded
 * lightness and saturation are what keep the wheel from producing neon or muddy accents:
 * saturation stays within 12–67 %, lightness within 54–60 %.
 */
export function wheelColor(hue: number, sat: number): string {
  return toHex(hsl2rgb(hue, 0.55 * sat + 0.12, 0.6 - 0.06 * sat))
}

/**
 * The accent for a point on the wheel canvas, or `null` outside the disc.
 *
 * `x`/`y` are canvas-internal coordinates, `radius` is half the canvas width.
 */
export function wheelColorAt(x: number, y: number, radius: number): string | null {
  const dx = x - radius
  const dy = y - radius
  const d = Math.sqrt(dx * dx + dy * dy)
  if (d > radius) return null

  const hue = ((Math.atan2(dy, dx) * 180) / Math.PI + 360) % 360
  return wheelColor(hue, Math.min(1, d / radius))
}

/** DESIGN.md §2.4 — three stacked radial gradients, re-tinted by the accent. */
export function meshFor(accent: string, theme: Theme): string {
  return theme === 'dark'
    ? `radial-gradient(900px 520px at 12% -8%, ${mix(accent, 0.2)}, transparent 68%), ` +
        `radial-gradient(760px 500px at 96% 6%, rgba(120,70,40,.22), transparent 70%), ` +
        `radial-gradient(700px 600px at 60% 110%, ${mix(accent, 0.1)}, transparent 72%)`
    : `radial-gradient(900px 520px at 10% -10%, ${mix(accent, 0.26)}, transparent 66%), ` +
        `radial-gradient(760px 520px at 98% 4%, rgba(230,190,150,.45), transparent 70%), ` +
        `radial-gradient(700px 600px at 55% 112%, ${mix(accent, 0.14)}, transparent 72%)`
}

/**
 * The custom properties that depend on the accent. The rest live in `styles/tokens.css`.
 *
 * Declared as a type alias rather than an interface so it carries an implicit index signature
 * and can be iterated with `Object.entries` without losing its `string` values.
 */
export type AccentTokens = {
  '--accent': string
  '--accentSoft': string
  '--dashed': string
  '--onAccent': string
  '--mesh': string
}

export function accentTokens(accent: string, theme: Theme): AccentTokens {
  const dark = theme === 'dark'
  return {
    '--accent': accent,
    '--accentSoft': mix(accent, dark ? 0.16 : 0.18),
    '--dashed': mix(accent, dark ? 0.38 : 0.45),
    '--onAccent': lum(accent) > ON_ACCENT_THRESHOLD ? ON_ACCENT_DARK_TEXT : ON_ACCENT_LIGHT_TEXT,
    '--mesh': meshFor(accent, theme),
  }
}

/**
 * Colour of the unplayed waveform bars — DESIGN.md §8.
 *
 * Not accent-derived, but it is the one colour that depends on the theme without being a
 * static token, so it belongs with the other computed values.
 */
export function waveformIdleColor(theme: Theme): string {
  return mix(theme === 'dark' ? '#FFF0E2' : '#3A2617', 0.22)
}
