import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { invoke } from '@tauri-apps/api/core'
import { Power, RefreshCw, Trash2 } from 'lucide-react'
import { useState } from 'react'
import { toast } from 'sonner'

type LampConfig = { name: string; ip: string; device_id: number; has_token: boolean; is_color: boolean }
type LampState = { power: boolean; brightness: number; temperature: number; rgb: number | null }
type LampCommand =
  | { name: 'set_lamp_power'; args: { on: boolean } }
  | { name: 'set_lamp_brightness'; args: { brightness: number } }
  | { name: 'set_lamp_temperature'; args: { temperature: number } }
  | { name: 'set_lamp_rgb'; args: { rgb: number } }

const message = (error: unknown) => error instanceof Error ? error.message : String(error)

function LampSettings({ deviceId, config, onSaved, onRenamed }: {
  deviceId: string; config: LampConfig; onSaved: () => void; onRenamed: () => void
}) {
  const [name, setName] = useState(config.name)
  const [ip, setIp] = useState(config.ip)
  const [lampDeviceId, setLampDeviceId] = useState(String(config.device_id || ''))
  const [token, setToken] = useState('')
  const save = useMutation({
    mutationFn: () => invoke<LampConfig>('save_lamp_config', { deviceId, config: { name, ip, device_id: Number(lampDeviceId), token } }),
    onSuccess: () => { setToken(''); toast.success('Lamp settings saved'); onSaved() },
    onError: error => toast.error(message(error)),
  })
  const rename = useMutation({
    mutationFn: () => invoke<LampConfig>('save_lamp_config', {
      deviceId, config: { name: name.trim(), ip: config.ip, device_id: config.device_id, token: '' },
    }),
    onSuccess: () => { toast.success('Device renamed'); onRenamed() },
    onError: error => toast.error(message(error)),
  })

  return <div className="panel space-y-4 p-5">
    <label className="block space-y-2">
      <span className="text-xs font-medium">Device name</span>
      <div className="flex gap-2">
        <input className="field min-w-0 flex-1 text-sm" value={name} onChange={event => setName(event.target.value)} />
        <button className="soft-button px-3 text-sm" disabled={rename.isPending || !name.trim() || name.trim() === config.name}
          onClick={() => rename.mutate()}>Rename</button>
      </div>
    </label>
    <label className="block space-y-2">
      <span className="text-xs font-medium">IP address</span>
      <input className="field mono text-sm" value={ip} onChange={event => setIp(event.target.value)} />
    </label>
    <label className="block space-y-2">
      <span className="text-xs font-medium">Xiaomi device ID</span>
      <input className="field mono text-sm" inputMode="numeric" value={lampDeviceId}
        onChange={event => setLampDeviceId(event.target.value)} />
    </label>
    <label className="block space-y-2">
      <span className="text-xs font-medium">Device token</span>
      <input className="field mono text-sm" type="password" autoComplete="off" value={token}
        placeholder={config.has_token ? 'Saved — leave blank to keep it' : '32 hexadecimal characters'}
        onChange={event => setToken(event.target.value)} />
    </label>
    <button className="primary-button w-full px-4 py-3 text-sm" disabled={save.isPending}
      onClick={() => save.mutate()}>Save lamp settings</button>
    <p className="text-xs leading-5 text-[var(--muted-foreground)]">
      The token is stored in Frog’s config and included when you export it.
    </p>
  </div>
}

export default function BedsideLampPanel({ deviceId, onUpdated, onRemove, isRemoving }: {
  deviceId: string; onUpdated: () => void; onRemove: () => void; isRemoving: boolean
}) {
  const queryClient = useQueryClient()
  const [tab, setTab] = useState<'controls' | 'settings'>('controls')
  const [brightnessDraft, setBrightnessDraft] = useState<number | null>(null)
  const [temperatureDraft, setTemperatureDraft] = useState<number | null>(null)
  const config = useQuery({ queryKey: ['lamp-config', deviceId], queryFn: () => invoke<LampConfig>('get_lamp_config', { deviceId }) })
  const state = useQuery({
    queryKey: ['lamp-state', deviceId], queryFn: () => invoke<LampState>('get_lamp_state', { deviceId }),
    enabled: config.data?.has_token === true, refetchInterval: 10000,
  })
  const action = useMutation({
    mutationFn: (command: LampCommand) => invoke<LampState>(command.name, { deviceId, ...command.args }),
    onSuccess: data => {
      queryClient.setQueryData(['lamp-state', deviceId], data)
      setBrightnessDraft(null)
      setTemperatureDraft(null)
    },
    onError: error => toast.error(message(error)),
  })
  const run = (command: LampCommand) => action.mutate(command)
  const brightness = brightnessDraft ?? state.data?.brightness ?? 100
  const temperature = temperatureDraft ?? state.data?.temperature ?? 1700
  const color = `#${(state.data?.rgb ?? 0xffffff).toString(16).padStart(6, '0')}`

  return <section>
    <div className="mb-3 grid grid-cols-2 gap-2 rounded-xl border border-[var(--border)] bg-[var(--surface)] p-1.5">
      {(['controls', 'settings'] as const).map(item => <button key={item}
        className={`rounded-lg px-3 py-2 text-sm font-medium capitalize ${tab === item ? 'bg-[var(--secondary)] text-[var(--foreground)]' : 'text-[var(--muted-foreground)] hover:text-[var(--foreground)]'}`}
        onClick={() => setTab(item)}>{item}</button>)}
    </div>
    {tab === 'controls' && <div className="panel p-4">
      {!config.data?.has_token && !config.isLoading && <p className="mb-4 text-sm text-[var(--muted-foreground)]">
        Add the lamp’s local device token in Settings to connect.
      </p>}
      {config.error && <p className="mb-4 text-sm text-[var(--destructive)]">{message(config.error)}</p>}
      {state.error && <p className="mb-4 text-sm text-[var(--destructive)]">{message(state.error)}</p>}

      <div className="mb-4 flex items-center justify-between rounded-lg bg-[var(--muted)] px-3 py-2">
        <div>
          <p className="text-sm font-medium">Power</p>
          <p className="text-xs text-[var(--muted-foreground)]">{state.data ? state.data.power ? 'On' : 'Off' : 'Unavailable'}</p>
        </div>
        <button className={`soft-button p-2.5 ${state.data?.power ? 'active' : ''}`} aria-label={state.data?.power ? 'Turn lamp off' : 'Turn lamp on'}
          disabled={!state.data || action.isPending}
          onClick={() => run({ name: 'set_lamp_power', args: { on: !state.data!.power } })}><Power size={19} /></button>
      </div>

      <div className="space-y-3">
        <div>
          <div className="mb-1 flex items-center justify-between text-sm"><span>Brightness</span><span className="mono">{brightness}%</span></div>
          <input aria-label="Lamp brightness" className="brightness-range w-full" type="range" min="1" max="100"
            value={brightness} disabled={!state.data?.power || action.isPending}
            onChange={event => setBrightnessDraft(Number(event.target.value))}
            onPointerUp={() => { if (brightnessDraft !== null) run({ name: 'set_lamp_brightness', args: { brightness: brightnessDraft } }) }}
            onKeyUp={() => { if (brightnessDraft !== null) run({ name: 'set_lamp_brightness', args: { brightness: brightnessDraft } }) }} />
        </div>
        <div>
          <div className="mb-1 flex items-center justify-between text-sm"><span>Color temperature</span><span className="mono">{temperature} K</span></div>
          <input aria-label="Lamp color temperature" className="temperature-range w-full" type="range" min="1700" max="6500" step="100"
            value={temperature} disabled={!state.data?.power || action.isPending}
            onChange={event => setTemperatureDraft(Number(event.target.value))}
            onPointerUp={() => { if (temperatureDraft !== null) run({ name: 'set_lamp_temperature', args: { temperature: temperatureDraft } }) }}
            onKeyUp={() => { if (temperatureDraft !== null) run({ name: 'set_lamp_temperature', args: { temperature: temperatureDraft } }) }} />
        </div>
        {config.data?.is_color && <div className="flex items-center justify-between gap-4">
          <span className="text-sm">Color</span>
          <input aria-label="Lamp color" type="color" value={color} disabled={!state.data?.power || action.isPending}
            onChange={event => run({ name: 'set_lamp_rgb', args: { rgb: Number.parseInt(event.target.value.slice(1), 16) } })} />
        </div>}
      </div>
    </div>}
    {tab === 'settings' && config.data && <>
      <LampSettings deviceId={deviceId} config={config.data} onSaved={() => {
        void queryClient.invalidateQueries({ queryKey: ['lamp-config', deviceId] })
        void queryClient.invalidateQueries({ queryKey: ['lamp-state', deviceId] })
        onUpdated()
        setTab('controls')
      }} onRenamed={() => {
        void queryClient.invalidateQueries({ queryKey: ['lamp-config', deviceId] })
        onUpdated()
      }} />
    </>}
    {tab === 'settings' && <div className="mt-3 flex flex-wrap items-center justify-end gap-2">
      <button className="soft-button flex items-center gap-2 px-3 py-2 text-xs"
        onClick={() => void state.refetch()} disabled={!config.data?.has_token || state.isFetching}>
        <RefreshCw size={15} /> Refresh lamp
      </button>
      <button className="soft-button flex items-center gap-2 px-3 py-2 text-xs text-[var(--destructive)]"
        disabled={isRemoving} onClick={onRemove}><Trash2 size={15} /> Remove device</button>
    </div>}
  </section>
}
