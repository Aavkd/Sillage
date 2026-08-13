import { useWindowControls } from '../hooks/useWindowControls'

/**
 * Custom title bar — DESIGN.md §6.
 *
 * 46px tall, `padding: 0 18px`. Left: a 9×9 accent dot then « Sillage » at 13px/600 with
 * `letter-spacing: .02em`, `gap: 10px`. Right: `— ▢ ✕` in JetBrains Mono 15px, `--dim`,
 * `gap: 18px`.
 */
export function TitleBar() {
  const { minimize, toggleMaximize, close } = useWindowControls()

  return (
    <div
      data-tauri-drag-region
      style={{
        height: 46,
        flexShrink: 0,
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'space-between',
        padding: '0 18px',
      }}
    >
      <div
        data-tauri-drag-region
        style={{ display: 'flex', alignItems: 'center', gap: 10, pointerEvents: 'none' }}
      >
        <div style={{ width: 9, height: 9, borderRadius: '50%', background: 'var(--accent)' }} />
        <span style={{ fontSize: 13, fontWeight: 600, letterSpacing: '.02em' }}>Sillage</span>
      </div>

      <div
        style={{
          display: 'flex',
          gap: 18,
          color: 'var(--dim)',
          fontSize: 15,
          fontFamily: 'var(--font-mono)',
        }}
      >
        <WindowButton label="Réduire" glyph="—" onClick={minimize} />
        <WindowButton label="Agrandir" glyph="▢" onClick={toggleMaximize} />
        <WindowButton label="Fermer" glyph="✕" onClick={close} />
      </div>
    </div>
  )
}

/**
 * The prototype shows these glyphs with no interactive state, having no behaviour to show.
 * The only affordance added is the colour going from `--dim` to `--text` on hover — existing
 * tokens, no new value. See PROGRESS.md — écarts au design.
 */
function WindowButton({
  label,
  glyph,
  onClick,
}: {
  label: string
  glyph: string
  onClick: () => void
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      onClick={onClick}
      onMouseEnter={(event) => {
        event.currentTarget.style.color = 'var(--text)'
      }}
      onMouseLeave={(event) => {
        event.currentTarget.style.color = 'inherit'
      }}
      style={{ fontFamily: 'inherit', fontSize: 'inherit', lineHeight: 1 }}
    >
      {glyph}
    </button>
  )
}
