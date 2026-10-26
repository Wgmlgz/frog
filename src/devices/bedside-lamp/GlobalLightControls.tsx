import { useMutation, useQueries, useQueryClient } from '@tanstack/react-query'
import { invoke } from '@tauri-apps/api/core'
import { Power } from 'lucide-react'
import { useState } from 'react'
import { toast } from 'sonner'

type Lamp = { id: string; name: string }
type LampState = { power: boolean; brightness: number; temperature: number; rgb: number | null }
type LampCommand =
  | { name: 'set_lamp_power'; args: { on: boolean } }
  | { name: 'set_lamp_brightness'; args: { brightness: number } }
  | { name: 'set_lamp_temperature'; args: { temperature: number } }
  | { name: 'set_lamp_rgb'; args: { rgb: number } }

const message = (error: unknown) => error instanceof Error ? error.message : String(error)

export default function GlobalLightControls({ lamps, loading }: { lamps: Lamp[]; loading: boolean }) {
  const queryClient = useQueryClient()
  const [brightnessDraft, setBrightnessDraft] = useState<number | null>(null)
  const [temperatureDraft, setTemperatureDraft] = useState<number | null>(null)
  const states = useQueries({ queries: lamps.map(lamp => ({
    queryKey: ['lamp-state', lamp.id],
    queryFn: () => invoke<LampState>('get_lamp_state', { deviceId: lamp.id }),
    refetchInterval: 10000,
  })) })
  const available = states.flatMap(state => state.data ? [state.data] : [])
  const onCount = available.filter(state => state.power).length
  const allOn = available.length === lamps.length && onCount === lamps.length
  const average = (values: number[], fallback: number) => values.length
    ? Math.round(values.reduce((sum, value) => sum + value, 0) / values.length)
    : fallback
  const brightness = brightnessDraft ?? average(available.map(state => state.brightness), 100)
  const temperature = temperatureDraft ?? average(available.map(state => state.temperature), 1700)
  const color = `#${(available.find(state => state.rgb !== null)?.rgb ?? 0xffffff).toString(16).padStart(6, '0')}`

  const action = useMutation({
    mutationFn: async (command: LampCommand) => {
      const results = await Promise.allSettled(lamps.map(lamp =>
        invoke<LampState>(command.name, { deviceId: lamp.id, ...command.args })))
      const failures = results.flatMap((result, index) => result.status === 'rejected'
        ? [`${lamps[index].name}: ${message(result.reason)}`] : [])
      if (failures.length) throw new Error(failures.join('; '))
    },
    onSuccess: () => toast.success(`Updated ${lamps.length} ${lamps.length === 1 ? 'lamp' : 'lamps'}`),
    onError: error => toast.error(message(error)),
    onSettled: () => {
      setBrightnessDraft(null)
      setTemperatureDraft(null)
      void Promise.all(lamps.map(lamp => queryClient.invalidateQueries({ queryKey: ['lamp-state', lamp.id] })))
    },
  })
  const disabled = lamps.length === 0 || action.isPending

  return <section className="panel p-4">
    <h2 className="mb-1 text-base font-semibold">All lights</h2>
    <p className="mb-4 text-xs text-[var(--muted-foreground)]">
      {loading ? 'Loading lamps…' : lamps.length === 0 ? 'Add a lamp on the Devices page to control it here.'
        : `${lamps.length} ${lamps.length === 1 ? 'lamp' : 'lamps'} · ${onCount} on${available.length < lamps.length ? ` · ${lamps.length - available.length} unavailable` : ''}`}
    </p>
    <div className="mb-4 flex items-center justify-between rounded-lg bg-[var(--muted)] px-3 py-2">
      <span className="text-sm font-medium">Power</span>
      <button className={`soft-button p-2.5 ${allOn ? 'active' : ''}`} aria-label={allOn ? 'Turn all lamps off' : 'Turn all lamps on'}
        disabled={disabled} onClick={() => action.mutate({ name: 'set_lamp_power', args: { on: !allOn } })}>
        <Power size={19} />
      </button>
    </div>
    <div className="space-y-3">
      <div>
        <div className="mb-1 flex items-center justify-between text-sm"><span>Brightness</span><span className="mono">{brightness}%</span></div>
        <input aria-label="All lamps brightness" className="brightness-range w-full" type="range" min="1" max="100"
          value={brightness} disabled={disabled} onChange={event => setBrightnessDraft(Number(event.target.value))}
          onPointerUp={event => action.mutate({ name: 'set_lamp_brightness', args: { brightness: Number(event.currentTarget.value) } })}
          onKeyUp={event => { if (brightnessDraft !== null) action.mutate({ name: 'set_lamp_brightness', args: { brightness: Number(event.currentTarget.value) } }) }} />
      </div>
      <div>
        <div className="mb-1 flex items-center justify-between text-sm"><span>Color temperature</span><span className="mono">{temperature} K</span></div>
        <input aria-label="All lamps color temperature" className="temperature-range w-full" type="range" min="1700" max="6500" step="100"
          value={temperature} disabled={disabled} onChange={event => setTemperatureDraft(Number(event.target.value))}
          onPointerUp={event => action.mutate({ name: 'set_lamp_temperature', args: { temperature: Number(event.currentTarget.value) } })}
          onKeyUp={event => { if (temperatureDraft !== null) action.mutate({ name: 'set_lamp_temperature', args: { temperature: Number(event.currentTarget.value) } }) }} />
      </div>
      <div className="flex items-center justify-between gap-4">
        <span className="text-sm">Color</span>
        <input aria-label="All lamps color" type="color" value={color} disabled={disabled}
          onChange={event => action.mutate({ name: 'set_lamp_rgb', args: { rgb: Number.parseInt(event.target.value.slice(1), 16) } })} />
      </div>
    </div>
  </section>
}
