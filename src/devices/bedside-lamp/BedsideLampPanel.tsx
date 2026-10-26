import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { invoke } from '@tauri-apps/api/core'
import { Lightbulb, Power, RefreshCw, Settings2 } from 'lucide-react'
import { useState } from 'react'
import { toast } from 'sonner'

type LampConfig = { ip: string; device_id: number; has_token: boolean }
type LampState = { power: boolean; brightness: number; temperature: number }
type LampCommand =
  | { name: 'set_lamp_power'; args: { on: boolean } }
  | { name: 'set_lamp_brightness'; args: { brightness: number } }
  | { name: 'set_lamp_temperature'; args: { temperature: number } }

const message = (error: unknown) => error instanceof Error ? error.message : String(error)

function LampSettings({ config, onSaved }: { config: LampConfig; onSaved: () => void }) {
  const [ip, setIp] = useState(config.ip)
  const [token, setToken] = useState('')
  const save = useMutation({
    mutationFn: () => invoke<LampConfig>('save_lamp_config', { config: { ip, token } }),
    onSuccess: () => { setToken(''); toast.success('Lamp settings saved'); onSaved() },
    onError: error => toast.error(message(error)),
  })

  return <div className="panel mt-3 space-y-4 p-5">
    <div>
      <h3 className="font-semibold">Bedside lamp connection</h3>
      <p className="mt-1 text-xs leading-5 text-[var(--muted-foreground)]">
        Direct local connection over Xiaomi miIO. Device ID {config.device_id}.
      </p>
    </div>
    <label className="block space-y-2">
      <span className="text-xs font-medium">IP address</span>
      <input className="field mono text-sm" value={ip} onChange={event => setIp(event.target.value)} />
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
      The token stays in this computer’s app data and is never included in app builds.
    </p>
  </div>
}

export default function BedsideLampPanel() {
  const queryClient = useQueryClient()
  const [settingsOpen, setSettingsOpen] = useState(false)
  const [brightnessDraft, setBrightnessDraft] = useState<number | null>(null)
  const [temperatureDraft, setTemperatureDraft] = useState<number | null>(null)
  const config = useQuery({ queryKey: ['lamp-config'], queryFn: () => invoke<LampConfig>('get_lamp_config') })
  const state = useQuery({
    queryKey: ['lamp-state'], queryFn: () => invoke<LampState>('get_lamp_state'),
    enabled: config.data?.has_token === true, refetchInterval: 10000,
  })
  const action = useMutation({
    mutationFn: (command: LampCommand) => invoke<LampState>(command.name, command.args),
    onSuccess: data => {
      queryClient.setQueryData(['lamp-state'], data)
      setBrightnessDraft(null)
      setTemperatureDraft(null)
    },
    onError: error => toast.error(message(error)),
  })
  const run = (command: LampCommand) => action.mutate(command)
  const brightness = brightnessDraft ?? state.data?.brightness ?? 100
  const temperature = temperatureDraft ?? state.data?.temperature ?? 1700

  return <section className="mb-5">
    <div className="panel p-5">
      <div className="mb-5 flex items-center gap-3">
        <div className="flex size-10 items-center justify-center rounded-xl bg-[var(--secondary)]"><Lightbulb size={20} /></div>
        <div className="min-w-0 flex-1">
          <h2 className="font-semibold">Mi Bedside Lamp</h2>
          <p className="mono text-[11px] text-[var(--muted-foreground)]">{config.data?.ip ?? 'Connecting…'}</p>
        </div>
        <button className="soft-button p-2.5" title="Refresh lamp" aria-label="Refresh lamp"
          onClick={() => void state.refetch()} disabled={!config.data?.has_token || state.isFetching}><RefreshCw size={17} /></button>
        <button className="soft-button p-2.5" title="Lamp settings" aria-label="Lamp settings"
          onClick={() => setSettingsOpen(!settingsOpen)}><Settings2 size={17} /></button>
      </div>

      {!config.data?.has_token && !config.isLoading && <p className="mb-4 text-sm text-[var(--muted-foreground)]">
        Add the lamp’s local device token in Settings to connect.
      </p>}
      {config.error && <p className="mb-4 text-sm text-[var(--destructive)]">{message(config.error)}</p>}
      {state.error && <p className="mb-4 text-sm text-[var(--destructive)]">{message(state.error)}</p>}

      <div className="mb-5 flex items-center justify-between rounded-lg bg-[var(--muted)] px-4 py-3">
        <div>
          <p className="text-sm font-medium">Power</p>
          <p className="text-xs text-[var(--muted-foreground)]">{state.data ? state.data.power ? 'On' : 'Off' : 'Unavailable'}</p>
        </div>
        <button className={`soft-button p-3 ${state.data?.power ? 'active' : ''}`} aria-label={state.data?.power ? 'Turn lamp off' : 'Turn lamp on'}
          disabled={!state.data || action.isPending}
          onClick={() => run({ name: 'set_lamp_power', args: { on: !state.data!.power } })}><Power size={20} /></button>
      </div>

      <div className="space-y-5">
        <div>
          <div className="mb-2 flex items-center justify-between text-sm"><span>Brightness</span><span className="mono">{brightness}%</span></div>
          <input aria-label="Lamp brightness" className="range w-full" type="range" min="1" max="100"
            value={brightness} disabled={!state.data?.power || action.isPending}
            onChange={event => setBrightnessDraft(Number(event.target.value))}
            onPointerUp={() => { if (brightnessDraft !== null) run({ name: 'set_lamp_brightness', args: { brightness: brightnessDraft } }) }}
            onKeyUp={() => { if (brightnessDraft !== null) run({ name: 'set_lamp_brightness', args: { brightness: brightnessDraft } }) }} />
        </div>
        <div>
          <div className="mb-2 flex items-center justify-between text-sm"><span>Color temperature</span><span className="mono">{temperature} K</span></div>
          <input aria-label="Lamp color temperature" className="range w-full" type="range" min="1700" max="6500" step="100"
            value={temperature} disabled={!state.data?.power || action.isPending}
            onChange={event => setTemperatureDraft(Number(event.target.value))}
            onPointerUp={() => { if (temperatureDraft !== null) run({ name: 'set_lamp_temperature', args: { temperature: temperatureDraft } }) }}
            onKeyUp={() => { if (temperatureDraft !== null) run({ name: 'set_lamp_temperature', args: { temperature: temperatureDraft } }) }} />
          <div className="mt-1 flex justify-between text-[11px] text-[var(--muted-foreground)]"><span>Warm</span><span>Cool</span></div>
        </div>
      </div>
    </div>
    {settingsOpen && config.data && <LampSettings config={config.data} onSaved={() => {
      void queryClient.invalidateQueries({ queryKey: ['lamp-config'] })
      void queryClient.invalidateQueries({ queryKey: ['lamp-state'] })
      setSettingsOpen(false)
    }} />}
  </section>
}
