import { Lightbulb, Tv2 } from 'lucide-react'
import TvPanel from './devices/tv/TvPanel'
import BedsideLampPanel from './devices/bedside-lamp/BedsideLampPanel'

export default function App() {
  return (
    <div className="app-shell px-2.5 py-4">
      <div className="mx-auto max-w-[560px]">
        <header className="mb-6 flex items-center gap-3">
          <div className="flex size-11 items-center justify-center rounded-lg border border-[var(--border)] bg-[var(--secondary)] text-[var(--foreground)]">
            <Lightbulb size={23} strokeWidth={1.8} />
          </div>
          <div>
            <p className="text-[11px] font-bold uppercase tracking-[.22em] text-[var(--muted-foreground)]">Frog</p>
            <h1 className="text-xl font-semibold tracking-tight">Devices</h1>
          </div>
        </header>
        <BedsideLampPanel />
        <div className="mb-3 flex items-center gap-2 px-1 text-sm font-semibold"><Tv2 size={18} /> Television</div>
        <TvPanel />
      </div>
    </div>
  )
}
