import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { invoke } from '@tauri-apps/api/core'
import {
  ArrowDown, ArrowLeft, ArrowRight, ArrowUp, Bluetooth, CircleHelp,
  Home, Info, ListVideo, Minus, Plus, Power, Settings2, Tv2,
  Volume2, VolumeX,
} from 'lucide-react'
import { useState } from 'react'
import { toast } from 'sonner'

type TvConfig = {
  name: string
  tv_ip: string
  tv_mac: string
  broadcast_ip: string
  preferred_bluetooth_address: string
  has_root_private_key: boolean
  has_webos_client_key: boolean
}
type ConfigField = 'name' | 'tv_ip' | 'tv_mac' | 'broadcast_ip' | 'preferred_bluetooth_address'
type AudioState = { volume: number; sound_output: string }
type Device = { name: string; address: string; state: boolean }
type Tab = 'audio' | 'remote' | 'settings'

const message = (error: unknown) => error instanceof Error ? error.message : String(error)

function Settings({ deviceId, initial, onSaved }: { deviceId: string; initial: TvConfig; onSaved: () => void }) {
  const [form, setForm] = useState(initial)
  const [rootPrivateKey, setRootPrivateKey] = useState('')
  const save = useMutation({
    mutationFn: () => invoke<TvConfig>('save_config', { deviceId, config: { ...form, root_private_key: rootPrivateKey } }),
    onSuccess: () => { setRootPrivateKey(''); toast.success('TV settings saved'); onSaved() },
    onError: error => toast.error(message(error)),
  })
  const field = (key: ConfigField, label: string, placeholder: string) => (
    <label className="block space-y-2">
      <span className="text-xs font-medium text-[var(--foreground)]">{label}</span>
      <input className="field mono text-sm" value={form[key]} placeholder={placeholder}
        onChange={event => setForm({ ...form, [key]: event.target.value })} />
    </label>
  )
  return (
    <div className="panel space-y-5 p-5">
      <div>
        <h3 className="font-semibold">TV connection</h3>
        <p className="mt-1 text-xs leading-5 text-[var(--muted-foreground)]">The SSH key is stored in Frog’s config and included when you export it.</p>
      </div>
      {field('name', 'Device name', 'Living room TV')}
      {field('tv_ip', 'TV IP address', '192.168.31.89')}
      {field('tv_mac', 'TV MAC address', '20-28-BC-73-B2-7A')}
      {field('broadcast_ip', 'Wake broadcast address', '192.168.31.255')}
      {field('preferred_bluetooth_address', 'Headphones Bluetooth address', 'AA:BB:CC:DD:EE:FF')}
      <label className="block space-y-2">
        <span className="text-xs font-medium">Root SSH private key</span>
        <textarea className="field mono min-h-24 text-xs" value={rootPrivateKey}
          placeholder={initial.has_root_private_key ? 'Saved — leave blank to keep it' : 'Paste the OpenSSH private key'}
          onChange={event => setRootPrivateKey(event.target.value)} />
      </label>
      <button className="primary-button w-full px-4 py-3 text-sm" disabled={save.isPending}
        onClick={() => save.mutate()}>Save settings</button>
      <p className="text-xs leading-5 text-[var(--muted-foreground)]">The webOS pairing key is {initial.has_webos_client_key ? 'saved' : 'not yet saved'}. On first remote use, accept the TV pairing prompt once.</p>
    </div>
  )
}

export default function TvPanel({ deviceId, name, onUpdated }: { deviceId: string; name: string; onUpdated: () => void }) {
  const queryClient = useQueryClient()
  const [tab, setTab] = useState<Tab>('audio')
  const [draft, setDraft] = useState<number | null>(null)
  const config = useQuery({ queryKey: ['tv-config', deviceId], queryFn: () => invoke<TvConfig>('get_config', { deviceId }) })
  const state = useQuery({
    queryKey: ['tv-state', deviceId], queryFn: () => invoke<AudioState>('get_state', { deviceId }),
    refetchInterval: tab === 'audio' ? 10000 : false,
  })
  const devices = useQuery({
    queryKey: ['tv-devices', deviceId], queryFn: () => invoke<Device[]>('get_paired_devices', { deviceId }),
    enabled: tab === 'audio', refetchInterval: tab === 'audio' ? 15000 : false,
  })
  const action = useMutation({
    mutationFn: ({ command, args }: { command: string; args?: Record<string, unknown> }) =>
      invoke<AudioState>(command, { deviceId, ...args }),
    onSuccess: data => { queryClient.setQueryData(['tv-state', deviceId], data); setDraft(null) },
    onError: error => toast.error(message(error)),
  })
  const remote = useMutation({
    mutationFn: (button: string) => invoke<void>('remote_button', { deviceId, button }),
    onError: error => toast.error(message(error)),
  })
  const run = (command: string, args?: Record<string, unknown>) => action.mutate({ command, args })
  const send = (button: string) => remote.mutate(button)
  const volume = draft ?? state.data?.volume ?? 0
  const connectedHeadphones = devices.data?.find(device =>
    device.address.toLowerCase() === config.data?.preferred_bluetooth_address.toLowerCase())

  return (
    <div>
      <div className="panel mb-4 flex items-center gap-3 px-4 py-3">
        <div className="flex size-10 items-center justify-center rounded-xl bg-[var(--secondary)] text-[var(--foreground)]"><Tv2 size={20} /></div>
        <div className="min-w-0 flex-1">
          <div className="font-semibold">{name}</div>
          <div className="mono text-[11px] text-[var(--muted-foreground)]">{config.data?.tv_ip ?? 'Connecting…'}</div>
        </div>
        <span className={`size-2.5 rounded-full ${state.isSuccess ? 'bg-[var(--foreground)]' : 'bg-[var(--muted-foreground)]'}`} />
      </div>

      <div className="mb-4 grid grid-cols-3 gap-2 rounded-xl border border-[var(--border)] bg-[var(--surface)] p-1.5">
        {(['audio', 'remote', 'settings'] as const).map(item => (
          <button key={item} className={`rounded-lg px-3 py-2 text-sm font-medium capitalize ${tab === item ? 'bg-[var(--secondary)] text-[var(--foreground)]' : 'text-[var(--muted-foreground)] hover:text-[var(--foreground)]'}`}
            onClick={() => setTab(item)}>{item}</button>
        ))}
      </div>

      {tab === 'audio' && <div className="space-y-4">
        <div className="panel p-5">
          <div className="mb-4 flex items-center justify-between">
            <div>
              <h2 className="font-semibold">Volume</h2>
              <p className="text-xs text-[var(--muted-foreground)]">TV master volume</p>
            </div>
            <span className="mono text-4xl font-medium tabular-nums text-[var(--foreground)]">{state.data ? volume : '--'}<span className="ml-1 text-sm">%</span></span>
          </div>
          <input aria-label="Volume" className="range mb-4" type="range" min="0" max="100" value={volume}
            disabled={!state.data || action.isPending} onChange={event => setDraft(Number(event.target.value))}
            onPointerUp={() => { if (draft !== null) run('set_volume', { volume: draft }) }}
            onKeyUp={() => { if (draft !== null) run('set_volume', { volume: draft }) }} />
          <div className="grid grid-cols-2 gap-2">
            <button className="soft-button flex items-center justify-center py-3" aria-label="Volume down" disabled={!state.data || action.isPending}
              onClick={() => run('set_volume', { volume: Math.max(0, volume - 1) })}><Minus size={19} /></button>
            <button className="soft-button flex items-center justify-center py-3" aria-label="Volume up" disabled={!state.data || action.isPending}
              onClick={() => run('set_volume', { volume: Math.min(100, volume + 1) })}><Plus size={19} /></button>
          </div>
        </div>
        <div className="panel p-5">
          <h2 className="mb-4 font-semibold">Sound output</h2>
          <div className="grid grid-cols-2 gap-2">
            <button className={`soft-button flex min-h-20 flex-col items-start justify-center gap-1 px-4 text-left text-sm ${state.data?.sound_output === 'tv_speaker' ? 'active' : ''}`}
              disabled={action.isPending} onClick={() => run('set_output', { output: 'tv_speaker' })}>
              <Volume2 size={20} /><span>TV speakers</span>
            </button>
            <button className={`soft-button flex min-h-20 flex-col items-start justify-center gap-1 px-4 text-left text-sm ${state.data?.sound_output === 'bt_soundbar' ? 'active' : ''}`}
              disabled={action.isPending} onClick={() => run('connect_headphones')}>
              <Bluetooth size={20} /><span className="truncate">{connectedHeadphones?.name ?? 'Bluetooth headphones'}</span>
            </button>
          </div>
          <div className="mt-3 flex items-center gap-2 text-xs text-[var(--muted-foreground)]">
            <span className={`size-1.5 rounded-full ${connectedHeadphones?.state ? 'bg-[var(--foreground)]' : 'bg-[var(--muted-foreground)]'}`} />
            {connectedHeadphones ? connectedHeadphones.state ? 'Headphones connected' : 'Headphones paired, ready to connect' : 'Checking paired headphones…'}
          </div>
        </div>
      </div>}

      {tab === 'remote' && <div className="panel p-5">
        <div className="mb-5 flex items-start justify-between">
          <div><h2 className="font-semibold">Remote control</h2><p className="mt-1 text-xs text-[var(--muted-foreground)]">Manual buttons for the TV</p></div>
          <button className="soft-button p-2.5 text-[var(--destructive)]" title="Power" aria-label="Power" onClick={() => send('POWER')}><Power size={19} /></button>
        </div>
        <div className="mb-5 grid grid-cols-3 gap-2">
          <RemoteButton label="Home" icon={<Home size={18} />} onClick={() => send('HOME')} />
          <RemoteButton label="Inputs" icon={<ListVideo size={18} />} onClick={() => send('INPUT_HUB')} />
          <RemoteButton label="Settings" icon={<Settings2 size={18} />} onClick={() => send('MENU')} />
        </div>
        <div className="mx-auto mb-5 grid w-52 grid-cols-3 gap-2">
          <span /><RemoteButton label="Up" icon={<ArrowUp size={21} />} onClick={() => send('UP')} /><span />
          <RemoteButton label="Left" icon={<ArrowLeft size={21} />} onClick={() => send('LEFT')} />
          <button className="primary-button min-h-14" onClick={() => send('ENTER')}>OK</button>
          <RemoteButton label="Right" icon={<ArrowRight size={21} />} onClick={() => send('RIGHT')} />
          <span /><RemoteButton label="Down" icon={<ArrowDown size={21} />} onClick={() => send('DOWN')} /><span />
        </div>
        <div className="mb-5 grid grid-cols-3 gap-2">
          <RemoteButton label="Back" onClick={() => send('BACK')} />
          <RemoteButton label="Exit" onClick={() => send('EXIT')} />
          <RemoteButton label="Info" icon={<Info size={18} />} onClick={() => send('INFO')} />
        </div>
        <div className="grid grid-cols-2 gap-3 border-t border-[var(--border)] pt-5">
          <div className="grid grid-cols-3 gap-1.5">
            <RemoteButton label="−" title="Volume down" onClick={() => send('VOLUMEDOWN')} />
            <RemoteButton label="Mute" title="Mute" icon={<VolumeX size={18} />} onClick={() => send('MUTE')} />
            <RemoteButton label="+" title="Volume up" onClick={() => send('VOLUMEUP')} />
          </div>
          <div className="grid grid-cols-2 gap-1.5">
            <RemoteButton label="Ch −" onClick={() => send('CHANNELDOWN')} />
            <RemoteButton label="Ch +" onClick={() => send('CHANNELUP')} />
          </div>
        </div>
        <div className="mt-3 grid grid-cols-3 gap-2">
          <RemoteButton label="Play" onClick={() => send('PLAY')} />
          <RemoteButton label="Pause" onClick={() => send('PAUSE')} />
          <RemoteButton label="Stop" onClick={() => send('STOP')} />
        </div>
        <p className="mt-5 flex items-center gap-2 text-xs text-[var(--muted-foreground)]"><CircleHelp size={15} /> First press may show a one-time TV pairing prompt.</p>
      </div>}

      {tab === 'settings' && config.data && <Settings deviceId={deviceId} initial={config.data} onSaved={() => {
        void queryClient.invalidateQueries({ queryKey: ['tv-config', deviceId] })
        void queryClient.invalidateQueries({ queryKey: ['tv-state', deviceId] })
        void queryClient.invalidateQueries({ queryKey: ['tv-devices', deviceId] })
        onUpdated()
      }} />}
      {tab === 'settings' && config.error && <p className="text-sm text-[var(--destructive)]">{message(config.error)}</p>}
      {tab === 'audio' && state.error && <p className="flex items-center gap-2 text-sm text-[var(--destructive)]"><CircleHelp size={16} /> {message(state.error)}. Check Settings.</p>}
      <div className="mt-5 flex justify-center gap-1 text-[11px] text-[var(--muted-foreground)]"><Tv2 size={13} /> LG webOS controller</div>
    </div>
  )
}

function RemoteButton({ label, title, icon, onClick }: { label: string; title?: string; icon?: React.ReactNode; onClick: () => void }) {
  return <button title={title ?? label} aria-label={title ?? label} className="soft-button flex min-h-12 items-center justify-center gap-1 text-xs font-medium" onClick={onClick}>
    {icon}{icon ? <span className="sr-only">{label}</span> : label}
  </button>
}
