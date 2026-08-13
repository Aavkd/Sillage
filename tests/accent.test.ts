import { describe, expect, it } from 'vitest'

import {
  ACCENT_PRESETS,
  accentTokens,
  DEFAULT_ACCENT,
  hsl2rgb,
  lum,
  meshFor,
  mix,
  normalizeAccent,
  ON_ACCENT_THRESHOLD,
  toHex,
  waveformIdleColor,
  wheelColor,
  wheelColorAt,
} from '../src/lib/accent'
import {
  protoHex,
  protoHsl2rgb,
  protoLum,
  protoMesh,
  protoMix,
  SAMPLE_ACCENTS,
} from './prototype'

describe('mix', () => {
  it('matches the prototype for every sampled accent and alpha', () => {
    for (const accent of SAMPLE_ACCENTS) {
      for (const alpha of [0.035, 0.055, 0.08, 0.1, 0.14, 0.16, 0.18, 0.22, 0.26, 0.38, 0.45, 1]) {
        expect(mix(accent, alpha), `${accent} @ ${alpha}`).toBe(protoMix(accent, alpha))
      }
    }
  })

  it('produces the documented rgba() shape', () => {
    // DESIGN.md §2.3: components from the hex, alpha passed through untouched.
    expect(mix('#E08A4B', 0.16)).toBe('rgba(224, 138, 75, 0.16)')
    expect(mix('#000000', 1)).toBe('rgba(0, 0, 0, 1)')
    expect(mix('#FFFFFF', 0.5)).toBe('rgba(255, 255, 255, 0.5)')
  })

  it('is case-insensitive on the hex', () => {
    expect(mix('#e08a4b', 0.2)).toBe(mix('#E08A4B', 0.2))
  })
})

describe('lum', () => {
  it('matches the prototype for every sampled accent', () => {
    for (const accent of SAMPLE_ACCENTS) {
      expect(lum(accent), accent).toBe(protoLum(accent))
    }
  })

  it('follows the 0.299 / 0.587 / 0.114 weighting', () => {
    expect(lum('#000000')).toBe(0)
    expect(lum('#FFFFFF')).toBe(1)
    expect(lum('#FF0000')).toBeCloseTo(0.299, 10)
    expect(lum('#00FF00')).toBeCloseTo(0.587, 10)
    expect(lum('#0000FF')).toBeCloseTo(0.114, 10)
  })
})

describe('hsl2rgb', () => {
  it('matches the prototype across the wheel', () => {
    for (let h = 0; h < 360; h += 3) {
      for (const s of [0, 0.12, 0.35, 0.55, 0.67, 1]) {
        for (const l of [0, 0.54, 0.57, 0.6, 1]) {
          expect(hsl2rgb(h, s, l), `h=${h} s=${s} l=${l}`).toEqual(protoHsl2rgb(h, s, l))
        }
      }
    }
  })

  it('returns grey when saturation is zero', () => {
    expect(hsl2rgb(0, 0, 0.5)).toEqual([128, 128, 128])
    expect(hsl2rgb(210, 0, 0.5)).toEqual([128, 128, 128])
  })

  it('places the primaries at their hues', () => {
    expect(hsl2rgb(0, 1, 0.5)).toEqual([255, 0, 0])
    expect(hsl2rgb(120, 1, 0.5)).toEqual([0, 255, 0])
    expect(hsl2rgb(240, 1, 0.5)).toEqual([0, 0, 255])
  })
})

describe('toHex', () => {
  it('matches the prototype', () => {
    for (const rgb of [
      [0, 0, 0],
      [255, 255, 255],
      [224, 138, 75],
      [1, 2, 3],
      [142, 154, 91],
    ] as [number, number, number][]) {
      expect(toHex(rgb)).toBe(protoHex(rgb))
    }
  })

  it('pads and uppercases', () => {
    expect(toHex([1, 2, 3])).toBe('#010203')
    expect(toHex([224, 138, 75])).toBe('#E08A4B')
  })
})

describe('normalizeAccent', () => {
  it('accepts #RRGGBB in any case and uppercases it', () => {
    expect(normalizeAccent('#e08a4b')).toBe('#E08A4B')
    expect(normalizeAccent('#8E9A5B')).toBe('#8E9A5B')
  })

  it('refuses anything else', () => {
    for (const bad of ['E08A4B', '#E08A4', '#E08A4BB', '#GGGGGG', '', '#', 'rgb(1,2,3)']) {
      expect(normalizeAccent(bad), bad).toBeNull()
    }
  })
})

describe('accentTokens', () => {
  it('derives --accentSoft with the per-theme alpha', () => {
    for (const accent of SAMPLE_ACCENTS) {
      expect(accentTokens(accent, 'dark')['--accentSoft']).toBe(protoMix(accent, 0.16))
      expect(accentTokens(accent, 'light')['--accentSoft']).toBe(protoMix(accent, 0.18))
    }
  })

  it('derives --dashed with the per-theme alpha', () => {
    for (const accent of SAMPLE_ACCENTS) {
      expect(accentTokens(accent, 'dark')['--dashed']).toBe(protoMix(accent, 0.38))
      expect(accentTokens(accent, 'light')['--dashed']).toBe(protoMix(accent, 0.45))
    }
  })

  it('flips --onAccent at the 0.62 threshold', () => {
    for (const accent of SAMPLE_ACCENTS) {
      const expected = protoLum(accent) > ON_ACCENT_THRESHOLD ? '#241811' : '#FFF8F1'
      for (const theme of ['dark', 'light'] as const) {
        expect(accentTokens(accent, theme)['--onAccent'], accent).toBe(expected)
      }
    }
  })

  it('keeps light text on the two accents named in the acceptance criteria', () => {
    /*
     * ROADMAP.md phase 01 asks to check the flip « avec #8E9A5B puis #E08A4B ». Neither
     * crosses the threshold: lum(#8E9A5B) = 0.5617 and lum(#E08A4B) = 0.6139, both ≤ 0.62,
     * so both take light text and switching between them changes nothing. All five presets
     * of DESIGN.md §2.5 are below the threshold. Reported to the user; the prototype's 0.62
     * is authoritative (ROADMAP.md §A) and is what is implemented.
     */
    expect(lum('#8E9A5B')).toBeLessThanOrEqual(ON_ACCENT_THRESHOLD)
    expect(lum('#E08A4B')).toBeLessThanOrEqual(ON_ACCENT_THRESHOLD)

    for (const preset of ACCENT_PRESETS) {
      expect(accentTokens(preset, 'dark')['--onAccent'], preset).toBe('#FFF8F1')
    }
  })

  it('switches to dark text on the light accents the wheel can reach', () => {
    // The threshold is crossed in the yellow-green arc, e.g. hue 60 at full saturation.
    const light = wheelColor(60, 1)
    expect(lum(light)).toBeGreaterThan(ON_ACCENT_THRESHOLD)
    expect(accentTokens(light, 'dark')['--onAccent']).toBe('#241811')

    const dark = wheelColor(240, 1)
    expect(lum(dark)).toBeLessThanOrEqual(ON_ACCENT_THRESHOLD)
    expect(accentTokens(dark, 'dark')['--onAccent']).toBe('#FFF8F1')
  })

  it('flips exactly at 0.62, not at the usual 0.5', () => {
    // DESIGN.md §2.3: the threshold is deliberately high.
    expect(accentTokens('#FFFFFF', 'dark')['--onAccent']).toBe('#241811')
    expect(accentTokens('#000000', 'dark')['--onAccent']).toBe('#FFF8F1')

    // A mid grey sits above 0.5 but below 0.62, and must still take light text.
    const midGrey = '#999999'
    expect(lum(midGrey)).toBeGreaterThan(0.5)
    expect(lum(midGrey)).toBeLessThanOrEqual(ON_ACCENT_THRESHOLD)
    expect(accentTokens(midGrey, 'dark')['--onAccent']).toBe('#FFF8F1')
  })

  it('passes the accent through untouched', () => {
    expect(accentTokens('#E08A4B', 'dark')['--accent']).toBe('#E08A4B')
  })
})

describe('meshFor', () => {
  it('matches the prototype in both themes', () => {
    for (const accent of SAMPLE_ACCENTS) {
      expect(meshFor(accent, 'dark'), accent).toBe(protoMesh(accent, true))
      expect(meshFor(accent, 'light'), accent).toBe(protoMesh(accent, false))
    }
  })

  it('is re-tinted by the accent', () => {
    expect(meshFor('#E08A4B', 'dark')).not.toBe(meshFor('#8E9A5B', 'dark'))
  })

  it('keeps the two fixed gradients out of the accent', () => {
    // The middle gradient of each theme is a constant warm wash, not accent-derived.
    expect(meshFor('#E08A4B', 'dark')).toContain('rgba(120,70,40,.22)')
    expect(meshFor('#8E9A5B', 'light')).toContain('rgba(230,190,150,.45)')
  })
})

describe('wheel', () => {
  it('applies the DESIGN §3 saturation and lightness bounds', () => {
    // saturation 0.55·s + 0.12 → 12 % at the centre, 67 % at the rim
    // lightness  0.60 − 0.06·s → 60 % at the centre, 54 % at the rim
    for (let h = 0; h < 360; h += 7) {
      expect(wheelColor(h, 0)).toBe(protoHex(protoHsl2rgb(h, 0.12, 0.6)))
      expect(wheelColor(h, 1)).toBe(protoHex(protoHsl2rgb(h, 0.67, 0.54)))
    }
  })

  it('is a warm grey at the exact centre', () => {
    // DESIGN.md §3. The centre pixel has d = 0, so atan2(0, 0) forces hue 0, and the
    // saturation floor of 12 % makes it a desaturated warm tone rather than a pure grey.
    const r = 160
    expect(wheelColorAt(r, r, r)).toBe('#A58D8D')
    expect(wheelColorAt(r, r, r)).toBe(wheelColor(0, 0))
  })

  it('keeps saturation off zero everywhere, so no point is fully achromatic', () => {
    // s = 0.55·s + 0.12 never reaches 0; hue still tints the middle of the disc.
    expect(wheelColor(60, 0)).not.toBe(wheelColor(240, 0))
  })

  it('returns null outside the disc', () => {
    const r = 160
    expect(wheelColorAt(0, 0, r)).toBeNull()
    expect(wheelColorAt(2 * r, 2 * r, r)).toBeNull()
    expect(wheelColorAt(r, r, r)).not.toBeNull()
  })

  it('maps the angle to the hue', () => {
    const r = 160
    // Straight right of centre is hue 0; straight down is hue 90 (canvas y grows downward).
    expect(wheelColorAt(2 * r - 1, r, r)).toBe(wheelColor(0, (r - 1) / r))
    expect(wheelColorAt(r, 2 * r - 1, r)).toBe(wheelColor(90, (r - 1) / r))
  })

  it('always produces a value normalizeAccent accepts', () => {
    for (let h = 0; h < 360; h += 11) {
      for (const s of [0, 0.25, 0.5, 0.75, 1]) {
        expect(normalizeAccent(wheelColor(h, s))).toBe(wheelColor(h, s))
      }
    }
  })
})

describe('presets', () => {
  it('are exactly the five of DESIGN.md §2.5, in order', () => {
    expect([...ACCENT_PRESETS]).toEqual([
      '#E08A4B',
      '#D9694E',
      '#C98A2E',
      '#B9755F',
      '#8E9A5B',
    ])
  })

  it('default to #E08A4B', () => {
    expect(DEFAULT_ACCENT).toBe('#E08A4B')
  })
})

describe('waveformIdleColor', () => {
  it('matches the prototype bar colour', () => {
    expect(waveformIdleColor('dark')).toBe(protoMix('#FFF0E2', 0.22))
    expect(waveformIdleColor('light')).toBe(protoMix('#3A2617', 0.22))
  })
})
