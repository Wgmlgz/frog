import { Tv2 } from 'lucide-react'
import TvPanel from './devices/tv/TvPanel'

export default function App() {
  return (
    <div className="app-shell px-2.5 py-4">
      <div className="mx-auto max-w-[560px]">
        <header className="mb-6 flex items-center gap-3">
          <div className="flex size-11 items-center justify-center rounded-lg border border-[var(--border)] bg-[var(--secondary)] text-[var(--foreground)]">
            <Tv2 size={23} strokeWidth={1.8} />
          </div>
          <div>
            <p className="text-[11px] font-bold uppercase tracking-[.22em] text-[var(--muted-foreground)]">Frog</p>
            <h1 className="text-xl font-semibold tracking-tight">Devices</h1>
          </div>
        </header>
        <TvPanel />
      </div>
    </div>
  )
}
