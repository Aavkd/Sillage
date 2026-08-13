import { cleanup, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

/**
 * Verifies the title bar of DESIGN.md §6 down to the Tauri window API.
 *
 * What the OS does with `minimize()` is Tauri's business; what this asserts is that the three
 * controls exist, are reachable, and each calls the right method — the part that is ours to
 * get wrong. The drag region is checked by attribute, since jsdom has no window manager.
 */

const minimize = vi.fn()
const toggleMaximize = vi.fn()
const close = vi.fn()
const isMaximized = vi.fn(() => Promise.resolve(false))
const onResized = vi.fn(() => Promise.resolve(() => undefined))

vi.mock('@tauri-apps/api/core', () => ({
  isTauri: () => true,
  invoke: vi.fn(),
}))

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ minimize, toggleMaximize, close, isMaximized, onResized }),
}))

const { TitleBar } = await import('../src/components/TitleBar')

describe('TitleBar', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  // Testing Library only auto-cleans when Vitest globals are enabled; they are not, so each
  // render would otherwise stack another title bar into the document.
  afterEach(cleanup)

  it('shows the app name', () => {
    render(<TitleBar />)
    expect(screen.getByText('Sillage')).toBeTruthy()
  })

  it('is 46px tall with the padding of DESIGN.md §6', () => {
    const { container } = render(<TitleBar />)
    const bar = container.firstElementChild as HTMLElement

    expect(bar.style.height).toBe('46px')
    // jsdom normalizes the shorthand: `0 18px` is serialized back as `0px 18px`.
    expect(bar.style.padding).toBe('0px 18px')
  })

  it('carries the drag region so the window moves by the custom bar', () => {
    const { container } = render(<TitleBar />)
    expect(container.querySelector('[data-tauri-drag-region]')).toBeTruthy()
  })

  it('does not put the drag region on the controls themselves', () => {
    // A control inside the drag region would move the window instead of firing.
    render(<TitleBar />)
    for (const name of ['Réduire', 'Agrandir', 'Fermer']) {
      const button = screen.getByRole('button', { name })
      expect(button.hasAttribute('data-tauri-drag-region')).toBe(false)
    }
  })

  it.each([
    ['Réduire', () => minimize],
    ['Agrandir', () => toggleMaximize],
    ['Fermer', () => close],
  ])('« %s » calls its window method', (name, target) => {
    render(<TitleBar />)
    screen.getByRole('button', { name }).click()

    expect(target()).toHaveBeenCalledTimes(1)
    for (const other of [minimize, toggleMaximize, close]) {
      if (other !== target()) expect(other).not.toHaveBeenCalled()
    }
  })

  it('labels the controls in French', () => {
    render(<TitleBar />)
    expect(screen.getByRole('button', { name: 'Réduire' }).textContent).toBe('—')
    expect(screen.getByRole('button', { name: 'Agrandir' }).textContent).toBe('▢')
    expect(screen.getByRole('button', { name: 'Fermer' }).textContent).toBe('✕')
  })
})
