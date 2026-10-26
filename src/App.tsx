import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { invoke } from '@tauri-apps/api/core'
import { ArrowLeft, ChevronRight, ClipboardCopy, ClipboardPaste, Lightbulb, Plus, Trash2, Tv2 } from 'lucide-react'
import { useState } from 'react'
import { toast } from 'sonner'
import BedsideLampPanel from './devices/bedside-lamp/BedsideLampPanel'
import TvPanel from './devices/tv/TvPanel'

type Device = { id: string; name: string; type: 'tv' | 'bedside_lamp'; address: string }
const message = (error: unknown) => error instanceof Error ? error.message : String(error)

export default function App() {
  const queryClient = useQueryClient()
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [adding, setAdding] = useState(false)
  const devices = useQuery({ queryKey: ['devices'], queryFn: () => invoke<Device[]>('list_devices') })
  const refresh = () => void queryClient.invalidateQueries({ queryKey: ['devices'] })
  const add = useMutation({
    mutationFn: (deviceType: Device['type']) => invoke<Device>('add_device', { deviceType }),
    onSuccess: device => { refresh(); setAdding(false); setSelectedId(device.id); toast.success(`${device.name} added`) },
    onError: error => toast.error(message(error)),
  })
  const remove = useMutation({
    mutationFn: (deviceId: string) => invoke<void>('remove_device', { deviceId }),
    onSuccess: () => { setSelectedId(null); refresh(); toast.success('Device removed') },
    onError: error => toast.error(message(error)),
  })
  const exportConfig = useMutation({
    mutationFn: () => invoke<void>('export_config_to_clipboard'),
    onSuccess: () => toast.success('Complete config copied to clipboard'),
    onError: error => toast.error(message(error)),
  })
  const importConfig = useMutation({
    mutationFn: () => invoke<Device[]>('import_config_from_clipboard'),
    onSuccess: () => {
      setSelectedId(null)
      setAdding(false)
      void queryClient.invalidateQueries()
      toast.success('Config imported from clipboard')
    },
    onError: error => toast.error(message(error)),
  })
  const selected = devices.data?.find(device => device.id === selectedId)

  return <div className="app-shell px-2.5 py-4">
    <div className="mx-auto max-w-[560px]">
      <header className="mb-6 flex items-center gap-3">
        {selected && <button className="soft-button p-2.5" aria-label="Back to devices" onClick={() => setSelectedId(null)}><ArrowLeft size={20} /></button>}
        <div className="flex size-11 items-center justify-center rounded-lg border border-[var(--border)] bg-[var(--secondary)] text-[var(--foreground)]"><Lightbulb size={23} strokeWidth={1.8} /></div>
        <div className="min-w-0 flex-1">
          <p className="text-[11px] font-bold uppercase tracking-[.22em] text-[var(--muted-foreground)]">Frog</p>
          <h1 className="truncate text-xl font-semibold tracking-tight">{selected ? selected.name : 'Devices'}</h1>
        </div>
      </header>

      {selected ? <>
        {selected.type === 'tv' ? <TvPanel key={selected.id} deviceId={selected.id} name={selected.name} onUpdated={refresh} />
          : <BedsideLampPanel key={selected.id} deviceId={selected.id} name={selected.name} onUpdated={refresh} />}
        <button className="mt-8 flex items-center gap-2 text-xs text-[var(--destructive)]"
          disabled={remove.isPending} onClick={() => {
            if (window.confirm(`Remove ${selected.name} from Frog?`)) remove.mutate(selected.id)
          }}><Trash2 size={15} /> Remove device</button>
      </> : <>
        {devices.error && <p className="mb-4 text-sm text-[var(--destructive)]">{message(devices.error)}</p>}
        <div className="grid grid-cols-2 gap-3">
          {devices.data?.map(device => <button key={device.id}
            className="panel flex min-h-36 flex-col items-start justify-between p-4 text-left transition hover:bg-[var(--accent)]"
            onClick={() => setSelectedId(device.id)}>
            <span className="flex w-full items-start justify-between"><span className="flex size-10 items-center justify-center rounded-lg bg-[var(--secondary)]">
              {device.type === 'tv' ? <Tv2 size={22} /> : <Lightbulb size={22} />}
            </span><ChevronRight size={18} className="text-[var(--muted-foreground)]" /></span>
            <span className="min-w-0"><span className="block truncate font-semibold">{device.name}</span>
              <span className="mono block truncate text-[11px] text-[var(--muted-foreground)]">{device.address}</span></span>
          </button>)}
          <button className="panel flex min-h-36 flex-col items-center justify-center gap-2 border-dashed text-sm font-medium transition hover:bg-[var(--accent)]"
            onClick={() => setAdding(!adding)}><Plus size={24} /> Add device</button>
        </div>
        {adding && <div className="panel mt-3 space-y-2 p-4">
          <p className="mb-3 text-sm font-semibold">Choose a device type</p>
          <button className="soft-button flex w-full items-center gap-3 p-3 text-left" disabled={add.isPending}
            onClick={() => add.mutate('tv')}><Tv2 size={20} /> LG webOS TV</button>
          <button className="soft-button flex w-full items-center gap-3 p-3 text-left" disabled={add.isPending}
            onClick={() => add.mutate('bedside_lamp')}><Lightbulb size={20} /> Mi Bedside Lamp</button>
        </div>}
        <div className="mt-8 border-t border-[var(--border)] pt-5">
          <h2 className="mb-3 text-sm font-semibold">Move to another controller</h2>
          <div className="grid grid-cols-2 gap-2">
            <button className="soft-button flex items-center justify-center gap-2 px-3 py-3 text-sm" disabled={exportConfig.isPending}
              onClick={() => exportConfig.mutate()}><ClipboardCopy size={17} /> Export to clipboard</button>
            <button className="soft-button flex items-center justify-center gap-2 px-3 py-3 text-sm" disabled={importConfig.isPending}
              onClick={() => {
                if (window.confirm('Replace all devices and credentials with the config on your clipboard?')) importConfig.mutate()
              }}><ClipboardPaste size={17} /> Import from clipboard</button>
          </div>
          <p className="mt-3 text-xs leading-5 text-[var(--muted-foreground)]">The exported TOML includes device tokens, pairing keys, and SSH private keys. Clear your clipboard after transferring it.</p>
        </div>
      </>}
    </div>
  </div>
}
