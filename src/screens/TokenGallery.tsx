import { useMemo, useState } from 'react'

import {
  ACCENT_PRESETS,
  lum,
  normalizeAccent,
  ON_ACCENT_THRESHOLD,
  type Theme,
} from '../lib/accent'
import type { AppearanceController } from '../hooks/useAppearance'

/**
 * Temporary verification page — ROADMAP.md phase 01, task 8.
 *
 * Shows every token so each value can be checked in the inspector against DESIGN.md §2, in
 * both themes and with any accent. It is scaffolding: phase 05 replaces it with the Library
 * screen and phase 10 turns the controls below into the real Appearance tab.
 */

/** DESIGN.md §2.1 and §2.2 — the theme-only tokens, in document order. */
const THEME_TOKENS = [
  '--page',
  '--frame',
  '--text',
  '--dim',
  '--faint',
  '--panel',
  '--panelStrong',
  '--border',
  '--hair',
  '--sunken',
  '--sel',
  '--dropBg',
  '--warn',
  '--warnBorder',
  '--err',
  '--errBorder',
  '--errSoft',
  '--ok',
] as const

/** DESIGN.md §2.3 and §2.4 — recomputed on every accent or theme change. */
const DERIVED_TOKENS = ['--accent', '--accentSoft', '--dashed', '--onAccent'] as const

function readTokens(names: readonly string[]): Record<string, string> {
  const computed = getComputedStyle(document.documentElement)
  return Object.fromEntries(names.map((name) => [name, computed.getPropertyValue(name).trim()]))
}

export function TokenGallery({ appearance, setTheme, setAccent }: AppearanceController) {
  /*
   * Read back what the browser actually computed, rather than what we believe we set — that is
   * the point of this page. Reading during render is correct here: applyAppearance() writes the
   * tokens onto the document synchronously, before the state update that triggers this render,
   * so the DOM is already up to date. The read is idempotent, so StrictMode's double render is
   * harmless.
   */
  const values = useMemo(
    () => readTokens([...THEME_TOKENS, ...DERIVED_TOKENS]),
    // eslint-disable-next-line react-hooks/exhaustive-deps -- re-read whenever appearance changes
    [appearance],
  )

  return (
    <div
      style={{
        flex: 1,
        minHeight: 0,
        overflowY: 'auto',
        padding: '8px 26px 32px',
        display: 'flex',
        flexDirection: 'column',
        gap: 26,
      }}
    >
      <AppearanceControls appearance={appearance} setTheme={setTheme} setAccent={setAccent} />

      <Section
        title="Jetons de thème"
        note="DESIGN.md §2.1 (sombre) et §2.2 (clair) — valeurs calculées, lues sur l'élément racine"
      >
        <TokenGrid names={THEME_TOKENS} values={values} />
      </Section>

      <Section
        title="Jetons dérivés de l'accent"
        note="DESIGN.md §2.3 — recalculés à chaque changement d'accent ou de thème"
      >
        <TokenGrid names={DERIVED_TOKENS} values={values} />
        <OnAccentProof accent={appearance.accent} />
      </Section>

      <Section title="Polices" note="DESIGN.md §1 — fichiers woff2 locaux, aucune requête réseau">
        <Specimens />
      </Section>
    </div>
  )
}

function AppearanceControls({ appearance, setTheme, setAccent }: AppearanceController) {
  const [draft, setDraft] = useState(appearance.accent)
  const [invalid, setInvalid] = useState(false)
  const [lastAccent, setLastAccent] = useState(appearance.accent)

  // Adjusting state during render — the documented way to resync a field when the value it
  // mirrors changes elsewhere (here: a preset click, or a hex the backend normalized).
  if (lastAccent !== appearance.accent) {
    setLastAccent(appearance.accent)
    setDraft(appearance.accent)
    setInvalid(false)
  }

  const commit = (raw: string) => {
    const normalized = normalizeAccent(raw.trim())
    if (normalized) {
      setInvalid(false)
      setAccent(normalized)
    } else {
      // A malformed entry is refused without disturbing the accent in use (DESIGN.md §12).
      setInvalid(true)
    }
  }

  return (
    <div
      style={{
        background: 'var(--panel)',
        border: '1px solid var(--border)',
        borderRadius: 16,
        backdropFilter: 'blur(20px)',
        padding: '20px 22px',
        display: 'flex',
        gap: 26,
        alignItems: 'flex-start',
        flexWrap: 'wrap',
      }}
    >
      <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
        <div>
          <Label>Accent</Label>
          <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
            <div
              style={{
                width: 30,
                height: 30,
                borderRadius: 9,
                background: 'var(--accent)',
                boxShadow: '0 0 0 1px var(--border), 0 6px 18px -6px var(--accent)',
              }}
            />
            <input
              value={draft}
              spellCheck={false}
              onChange={(event) => {
                setDraft(event.target.value)
              }}
              onBlur={(event) => {
                commit(event.target.value)
              }}
              onKeyDown={(event) => {
                if (event.key === 'Enter') commit(event.currentTarget.value)
              }}
              style={{
                fontFamily: 'var(--font-mono)',
                fontSize: 14,
                width: 100,
                padding: '5px 8px',
                borderRadius: 8,
                background: 'var(--sunken)',
                border: `1px solid ${invalid ? 'var(--errBorder)' : 'var(--border)'}`,
                color: invalid ? 'var(--err)' : 'var(--text)',
                userSelect: 'text',
              }}
            />
          </div>
        </div>

        <div style={{ display: 'flex', gap: 8 }}>
          {ACCENT_PRESETS.map((preset) => (
            <button
              key={preset}
              type="button"
              aria-label={preset}
              title={preset}
              onClick={() => {
                setAccent(preset)
              }}
              style={{
                width: 24,
                height: 24,
                borderRadius: 8,
                border: '1px solid var(--border)',
                background: preset,
              }}
            />
          ))}
        </div>

        <div>
          <Label>Thème</Label>
          <div
            style={{
              display: 'inline-flex',
              gap: 6,
              background: 'var(--sunken)',
              borderRadius: 10,
              padding: 3,
            }}
          >
            <ThemeButton current={appearance.theme} value="dark" onPick={setTheme}>
              Sombre
            </ThemeButton>
            <ThemeButton current={appearance.theme} value="light" onPick={setTheme}>
              Clair
            </ThemeButton>
          </div>
        </div>
      </div>

      <div style={{ flex: 1, minWidth: 280 }}>
        <Label>Fond maillé</Label>
        <div
          style={{
            height: 132,
            borderRadius: 14,
            border: '1px solid var(--border)',
            background: 'var(--frame)',
            position: 'relative',
            overflow: 'hidden',
          }}
        >
          <div style={{ position: 'absolute', inset: 0, background: 'var(--mesh)' }} />
        </div>
        <div
          style={{
            fontFamily: 'var(--font-mono)',
            fontSize: 11.5,
            color: 'var(--faint)',
            marginTop: 8,
          }}
        >
          les trois dégradés radiaux de DESIGN.md §2.4 · le maillage de la fenêtre est le même
        </div>
      </div>
    </div>
  )
}

function ThemeButton({
  current,
  value,
  onPick,
  children,
}: {
  current: Theme
  value: Theme
  onPick: (theme: Theme) => void
  children: string
}) {
  return (
    <button
      type="button"
      onClick={() => {
        onPick(value)
      }}
      style={{
        padding: '7px 14px',
        borderRadius: 8,
        fontFamily: 'var(--font-ui)',
        fontSize: 13,
        color: 'var(--text)',
        background: current === value ? 'var(--sel)' : 'transparent',
      }}
    >
      {children}
    </button>
  )
}

/** Makes the 0.62 threshold of DESIGN.md §2.3 checkable at a glance. */
function OnAccentProof({ accent }: { accent: string }) {
  const luminance = lum(accent)

  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 12, marginTop: 12, flexWrap: 'wrap' }}>
      <div
        style={{
          padding: '8px 14px',
          borderRadius: 10,
          background: 'var(--accent)',
          color: 'var(--onAccent)',
          fontSize: 14,
          fontWeight: 600,
          boxShadow: '0 10px 26px -12px var(--accent)',
        }}
      >
        Texte sur accent
      </div>
      <span style={{ fontFamily: 'var(--font-mono)', fontSize: 12.5, color: 'var(--dim)' }}>
        lum({accent}) = {luminance.toFixed(4)} {luminance > ON_ACCENT_THRESHOLD ? '>' : '≤'}{' '}
        {ON_ACCENT_THRESHOLD} → texte {luminance > ON_ACCENT_THRESHOLD ? 'sombre' : 'clair'}
      </span>
    </div>
  )
}

function TokenGrid({
  names,
  values,
}: {
  names: readonly string[]
  values: Record<string, string>
}) {
  return (
    <div
      style={{
        display: 'grid',
        gridTemplateColumns: 'repeat(auto-fill, minmax(232px, 1fr))',
        gap: 10,
      }}
    >
      {names.map((name) => (
        <div
          key={name}
          style={{
            display: 'flex',
            alignItems: 'center',
            gap: 10,
            padding: '9px 12px',
            borderRadius: 11,
            border: '1px solid var(--hair)',
            background: 'var(--panel)',
          }}
        >
          <div
            style={{
              width: 26,
              height: 26,
              borderRadius: 8,
              flexShrink: 0,
              // Checkerboard so the translucent tokens read as translucent.
              backgroundImage: `linear-gradient(var(--${name.slice(2)}), var(--${name.slice(2)})), repeating-conic-gradient(var(--sunken) 0% 25%, transparent 0% 50%)`,
              backgroundSize: '100% 100%, 10px 10px',
              boxShadow: 'inset 0 0 0 1px var(--border)',
            }}
          />
          <div style={{ minWidth: 0 }}>
            <div style={{ fontFamily: 'var(--font-mono)', fontSize: 12, color: 'var(--text)' }}>
              {name}
            </div>
            <div
              style={{
                fontFamily: 'var(--font-mono)',
                fontSize: 11.5,
                color: 'var(--faint)',
                whiteSpace: 'nowrap',
                overflow: 'hidden',
                textOverflow: 'ellipsis',
              }}
            >
              {values[name] ?? '—'}
            </div>
          </div>
        </div>
      ))}
    </div>
  )
}

function Specimens() {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 14 }}>
      <div>
        <Caption>Figtree · interface · 400 500 600 700</Caption>
        <div style={{ fontFamily: 'var(--font-ui)', fontSize: 16 }}>
          {[400, 500, 600, 700].map((weight) => (
            <div key={weight} style={{ fontWeight: weight }}>
              Déposez un fichier audio ou vidéo — {weight}
            </div>
          ))}
        </div>
      </div>
      <div>
        <Caption>Newsreader · lecture · 19.5px / 1.78 · opsz 6..72</Caption>
        <div
          style={{
            fontFamily: 'var(--font-read)',
            fontSize: 19.5,
            lineHeight: 1.78,
            textWrap: 'pretty',
          }}
        >
          Le verbatim reste immuable. Les corrections vivent dans une couche au-dessus, chaque mot
          garde son identifiant, et on peut revenir en arrière à tout moment.
        </div>
      </div>
      <div>
        <Caption>JetBrains Mono · technique · 400 500</Caption>
        <div style={{ fontFamily: 'var(--font-mono)', fontSize: 13, color: 'var(--dim)' }}>
          12 août · 16:40 42:18 fr turbo Ctrl+Shift+F #E08A4B
        </div>
      </div>
    </div>
  )
}

function Section({
  title,
  note,
  children,
}: {
  title: string
  note: string
  children: React.ReactNode
}) {
  return (
    <section style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
        <div
          style={{
            fontSize: 13,
            letterSpacing: '.16em',
            textTransform: 'uppercase',
            color: 'var(--dim)',
            whiteSpace: 'nowrap',
          }}
        >
          {title}
        </div>
        <div style={{ flex: 1, height: 1, background: 'var(--border)' }} />
      </div>
      <div style={{ fontSize: 12.5, color: 'var(--faint)', marginTop: -4 }}>{note}</div>
      {children}
    </section>
  )
}

function Label({ children }: { children: string }) {
  return (
    <div
      style={{
        fontSize: 11,
        letterSpacing: '.16em',
        textTransform: 'uppercase',
        color: 'var(--dim)',
        marginBottom: 8,
      }}
    >
      {children}
    </div>
  )
}

function Caption({ children }: { children: string }) {
  return (
    <div
      style={{
        fontFamily: 'var(--font-mono)',
        fontSize: 11.5,
        color: 'var(--faint)',
        marginBottom: 6,
      }}
    >
      {children}
    </div>
  )
}
