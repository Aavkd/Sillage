import { describe, expect, it } from 'vitest'

import figtreeCss from '@fontsource-variable/figtree/wght.css?raw'
import jetbrainsCss from '@fontsource-variable/jetbrains-mono/wght.css?raw'
import newsreaderCss from '@fontsource-variable/newsreader/opsz.css?raw'

import fontsCss from '../src/styles/fonts.css?raw'
import tokensCss from '../src/styles/tokens.css?raw'
import { prototypeThemes } from './prototype'

/**
 * Verifies styles/tokens.css against the prototype, token by token.
 *
 * ROADMAP.md phase 01, first acceptance criterion: « Chaque jeton de DESIGN.md §2.1 et §2.2
 * existe et vaut exactement la valeur indiquée. » DESIGN.md itself transcribes the prototype,
 * so the comparison is made against the prototype's own theme table — the authority of last
 * resort — which also catches any drift that may have crept into DESIGN.md.
 */

/** Strips CSS comments; a family named in prose is not a reference to it. */
const stripComments = (css: string) => css.replace(/\/\*[\s\S]*?\*\//g, '')

/** The custom properties declared in one `:root[data-theme='…']` block of tokens.css. */
function cssTokens(theme: string): Record<string, string> {
  const pattern = new RegExp(`:root\\[data-theme='${theme}'\\]\\s*\\{([\\s\\S]*?)\\}`)
  const match = pattern.exec(stripComments(tokensCss))
  if (!match) throw new Error(`bloc « ${theme} » introuvable dans tokens.css`)

  const entries: Record<string, string> = {}
  for (const [, key, value] of match[1].matchAll(/--([\w-]+):\s*([^;]+);/g)) {
    entries[key] = value.trim()
  }
  return entries
}

const [protoDark, protoLight] = prototypeThemes()

describe.each([
  ['dark', protoDark],
  ['light', protoLight],
])('tokens.css — thème %s', (theme, expected) => {
  const actual = cssTokens(theme)

  it('declares every token of the prototype and nothing more', () => {
    expect(Object.keys(actual).sort()).toEqual(Object.keys(expected).sort())
  })

  it.each(Object.entries(expected))('--%s is %s', (name, value) => {
    expect(actual[name]).toBe(value)
  })
})

describe('tokens.css — structure', () => {
  it('declares the 18 theme tokens per theme', () => {
    expect(Object.keys(cssTokens('dark'))).toHaveLength(18)
    expect(Object.keys(cssTokens('light'))).toHaveLength(18)
  })

  it('leaves the accent-derived tokens to lib/accent.ts', () => {
    // --accent, --accentSoft, --dashed, --onAccent and --mesh depend on the accent as well as
    // the theme, so a static stylesheet cannot hold them.
    const css = stripComments(tokensCss)
    for (const derived of ['--accent:', '--accentSoft:', '--dashed:', '--onAccent:', '--mesh:']) {
      expect(css).not.toContain(derived)
    }
  })

  it('names the three font stacks of DESIGN.md §1', () => {
    for (const stack of ['--font-ui:', '--font-read:', '--font-mono:']) {
      expect(tokensCss).toContain(stack)
    }
  })
})

describe('fonts — aucun accès réseau', () => {
  it('imports the three families from local packages', () => {
    for (const pkg of [
      '@fontsource-variable/figtree/wght.css',
      '@fontsource-variable/newsreader/opsz.css',
      '@fontsource-variable/jetbrains-mono/wght.css',
    ]) {
      expect(stripComments(fontsCss)).toContain(pkg)
    }
  })

  it('never references Google Fonts', () => {
    for (const css of [fontsCss, tokensCss]) {
      expect(stripComments(css)).not.toMatch(/fonts\.googleapis\.com|fonts\.gstatic\.com/)
    }
  })

  it('resolves every woff2 to a relative path', () => {
    // A remote `src: url(https://…)` would be fetched at runtime and break offline use.
    for (const css of [figtreeCss, newsreaderCss, jetbrainsCss]) {
      expect(css).not.toMatch(/url\(\s*['"]?https?:/)
      expect(css).toMatch(/url\(\.\/files\//)
    }
  })

  it('keeps the opsz axis Newsreader is specified with', () => {
    // DESIGN.md §1: Newsreader `opsz` 6..72. The wght-only file would drop the axis.
    expect(newsreaderCss).toContain('opsz')
    expect(stripComments(fontsCss)).not.toContain('newsreader/wght.css')
  })
})
