import { TitleBar } from './components/TitleBar'
import { useAppearance } from './hooks/useAppearance'
import type { Settings } from './lib/settings'
import { TokenGallery } from './screens/TokenGallery'

export default function App({ initialSettings }: { initialSettings: Settings }) {
  const controller = useAppearance(initialSettings)

  return (
    <div className="frame">
      {/* DESIGN.md §2.4 — above --frame, below the content, never intercepting a click. */}
      <div className="mesh" />
      <TitleBar />
      <TokenGallery {...controller} />
    </div>
  )
}
