// `?raw` lets Vite resolve the path at transform time, so these helpers work in any test
// environment rather than depending on `import.meta.url` being a real file URL.
import source from '../app-design-with-glassmorphism/project/Sillage.dc.html?raw'

/**
 * Loads the colour functions straight out of the prototype.
 *
 * `Sillage.dc.html` is the final authority on appearance (ROADMAP.md §A), so the tests compare
 * our port against the prototype's own code rather than against constants transcribed by hand.
 * Transcribed constants would only prove that two copies of the same typo agree; this proves
 * equivalence with the source of truth, and it breaks loudly if the prototype ever changes.
 */

/** Pulls one class method out of the prototype's `<script>` block and rebuilds it. */
function extractMethod(name: string): (...args: never[]) => unknown {
  const patterns = [
    // Multi-line body, closing brace at method indentation: mix, lum, hsl2rgb.
    new RegExp(`\\n {2}${name}\\(([^)]*)\\) \\{\\r?\\n([\\s\\S]*?)\\r?\\n {2}\\}`),
    // Single-line body: hex.
    new RegExp(`\\n {2}${name}\\(([^)]*)\\) \\{ (.*?) \\}\\r?\\n`),
  ]

  const match = patterns.map((pattern) => pattern.exec(source)).find((result) => result !== null)
  if (!match) throw new Error(`méthode « ${name} » introuvable dans le prototype`)

  const [, params, body] = match
  const args = params
    .split(',')
    .map((p) => p.trim())
    .filter(Boolean)

  // eslint-disable-next-line @typescript-eslint/no-implied-eval
  return new Function(...args, body) as (...args: never[]) => unknown
}

export const protoMix = extractMethod('mix') as unknown as (hex: string, alpha: number) => string
export const protoLum = extractMethod('lum') as unknown as (hex: string) => number
export const protoHsl2rgb = extractMethod('hsl2rgb') as unknown as (
  h: number,
  s: number,
  l: number,
) => number[]
export const protoHex = extractMethod('hex') as unknown as (rgb: number[]) => string

/** The two theme tables of `renderVals()`, dark first. */
export function prototypeThemes(): [Record<string, string>, Record<string, string>] {
  const match = /const t = dark \? \{([\s\S]*?)\} : \{([\s\S]*?)\};/.exec(source)
  if (!match) throw new Error('table des thèmes introuvable dans le prototype')

  const parse = (block: string) => {
    const entries: Record<string, string> = {}
    // Only quoted literals: `dashed` is a call to mix() and is derived at runtime instead.
    for (const [, key, value] of block.matchAll(/(\w+):\s*"([^"]+)"/g)) {
      entries[key] = value
    }
    return entries
  }

  return [parse(match[1]), parse(match[2])]
}

/** The literal mesh expressions of `renderVals()`, for the §2.4 comparison. */
export function protoMesh(accent: string, dark: boolean): string {
  return dark
    ? `radial-gradient(900px 520px at 12% -8%, ${protoMix(accent, 0.2)}, transparent 68%), radial-gradient(760px 500px at 96% 6%, rgba(120,70,40,.22), transparent 70%), radial-gradient(700px 600px at 60% 110%, ${protoMix(accent, 0.1)}, transparent 72%)`
    : `radial-gradient(900px 520px at 10% -10%, ${protoMix(accent, 0.26)}, transparent 66%), radial-gradient(760px 520px at 98% 4%, rgba(230,190,150,.45), transparent 70%), radial-gradient(700px 600px at 55% 112%, ${protoMix(accent, 0.14)}, transparent 72%)`
}

/** A spread of accents covering both sides of the 0.62 luminance threshold. */
export const SAMPLE_ACCENTS = [
  '#E08A4B',
  '#D9694E',
  '#C98A2E',
  '#B9755F',
  '#8E9A5B',
  '#000000',
  '#FFFFFF',
  '#7F7F7F',
  '#123456',
  '#ABCDEF',
  '#0E0906',
  '#F4EAE0',
]
