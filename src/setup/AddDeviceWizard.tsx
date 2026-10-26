import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { invoke } from '@tauri-apps/api/core'
import { Lightbulb, RefreshCw, Search, Tv2 } from 'lucide-react'
import { useState } from 'react'
import { toast } from 'sonner'

type Candidate = { kind: 'tv' | 'xiaomi_miio'; ip: string; device_id: number | null; name: string }
type Device = { id: string; name: string; type: 'tv' | 'bedside_lamp'; address: string }
type CloudDevice = { did: string; name: string; model: string; local_ip: string; has_token: boolean }
type XiaomiAccount = { user_id: string; username: string; region: string }
type LoginStep =
  | { status: 'captcha'; image: string; wrong: boolean }
  | { status: 'email_code' }
  | { status: 'ready'; devices: CloudDevice[] }

const message = (error: unknown) => error instanceof Error ? error.message : String(error)

export default function AddDeviceWizard({ onDone, onCancel }: { onDone: (device: Device) => void; onCancel: () => void }) {
  const queryClient = useQueryClient()
  const [selected, setSelected] = useState<Candidate | null>(null)
  const [name, setName] = useState('')
  const [manualIp, setManualIp] = useState('')
  const [token, setToken] = useState('')
  const [rootKey, setRootKey] = useState('')
  const [paired, setPaired] = useState(false)
  const [username, setUsername] = useState('')
  const [savedAccountId, setSavedAccountId] = useState('')
  const [password, setPassword] = useState('')
  const [region, setRegion] = useState('all')
  const [captcha, setCaptcha] = useState('')
  const [emailCode, setEmailCode] = useState('')
  const [cloudStep, setCloudStep] = useState<LoginStep | null>(null)
  const [manualTokenOpen, setManualTokenOpen] = useState(false)
  const discovered = useQuery({ queryKey: ['setup-discovery'], queryFn: () => invoke<Candidate[]>('discover_devices'), staleTime: 0 })
  const connected = useQuery({ queryKey: ['devices'], queryFn: () => invoke<Device[]>('list_devices') })
  const accounts = useQuery({ queryKey: ['xiaomi-accounts'], queryFn: () => invoke<XiaomiAccount[]>('list_xiaomi_accounts') })
  const choose = (candidate: Candidate) => { setSelected(candidate); setName(candidate.kind === 'tv' ? 'LG TV' : candidate.name); setPaired(false) }
  const probe = useMutation({
    mutationFn: () => invoke<Candidate>('probe_device', { ip: manualIp }),
    onSuccess: candidate => choose(candidate),
    onError: error => toast.error(message(error)),
  })
  const probeAccountDevice = useMutation({
    mutationFn: async (device: CloudDevice) => {
      if (!device.local_ip) throw new Error('Xiaomi did not provide this device’s local IP address')
      const candidate = await invoke<Candidate>('probe_device', { ip: device.local_ip })
      if (String(candidate.device_id) !== device.did) throw new Error('The address belongs to a different device')
      return candidate
    },
    onSuccess: candidate => choose(candidate),
    onError: error => toast.error(message(error)),
  })
  const pair = useMutation({
    mutationFn: () => invoke<void>('pair_new_tv', { ip: selected?.ip }),
    onSuccess: () => { setPaired(true); toast.success('TV paired') },
    onError: error => toast.error(message(error)),
  })
  const begin = useMutation({
    mutationFn: () => invoke<LoginStep>('begin_xiaomi_setup', { username, password, region }),
    onSuccess: step => { setPassword(''); setCloudStep(step); void queryClient.invalidateQueries({ queryKey: ['xiaomi-accounts'] }) },
    onError: error => toast.error(message(error)),
  })
  const answerCaptcha = useMutation({
    mutationFn: () => invoke<LoginStep>('submit_xiaomi_captcha', { code: captcha }),
    onSuccess: step => { setCaptcha(''); setCloudStep(step); void queryClient.invalidateQueries({ queryKey: ['xiaomi-accounts'] }) },
    onError: error => toast.error(message(error)),
  })
  const answerEmail = useMutation({
    mutationFn: () => invoke<LoginStep>('submit_xiaomi_email', { code: emailCode }),
    onSuccess: step => { setEmailCode(''); setCloudStep(step); void queryClient.invalidateQueries({ queryKey: ['xiaomi-accounts'] }) },
    onError: error => toast.error(message(error)),
  })
  const useAccount = useMutation({
    mutationFn: (userId: string) => invoke<LoginStep>('use_xiaomi_account', { userId }),
    onSuccess: step => setCloudStep(step),
    onError: error => toast.error(message(error)),
  })
  const finish = useMutation({
    mutationFn: async (mode: 'tv' | 'cloud' | 'manual') => {
      if (!selected) throw new Error('Select a device first')
      if (mode === 'tv') return invoke<Device>('finish_new_tv', { name, ip: selected.ip, rootPrivateKey: rootKey })
      if (selected.device_id === null) throw new Error('Xiaomi device ID is missing')
      if (mode === 'cloud') return invoke<Device>('finish_cloud_lamp', { name, ip: selected.ip, deviceId: selected.device_id })
      return invoke<Device>('finish_manual_lamp', { name, ip: selected.ip, deviceId: selected.device_id, token })
    },
    onSuccess: device => { toast.success(`${device.name} connected`); onDone(device) },
    onError: error => toast.error(message(error)),
  })
  const cloudMatch = cloudStep?.status === 'ready' ? cloudStep.devices.find(device => device.did === String(selected?.device_id)) : null

  return <div className="space-y-4">
    {!selected ? <>
      {!!accounts.data?.length && <div className="panel space-y-3 p-5">
        <div><h2 className="font-semibold">Saved Xiaomi account</h2>
          <p className="mt-1 text-xs text-[var(--muted-foreground)]">Choose an account to see its lamps, then select one to set up locally.</p></div>
        {accounts.data.map(account => <button key={account.user_id} className="soft-button w-full px-3 py-3 text-left text-sm"
          disabled={useAccount.isPending} onClick={() => useAccount.mutate(account.user_id)}>
          {account.username} <span className="text-xs text-[var(--muted-foreground)]">({account.region === 'all' ? 'all regions' : account.region})</span>
        </button>)}
        {cloudStep?.status === 'ready' && <div className="space-y-2 border-t border-[var(--border)] pt-3">
          {cloudStep.devices.filter(device => ['yeelink.light.bslamp1', 'yeelink.light.color4'].includes(device.model) && device.has_token &&
            !connected.data?.some(item => item.address === device.local_ip)).map(device => {
            const local = discovered.data?.find(candidate => candidate.kind === 'xiaomi_miio' && String(candidate.device_id) === device.did)
            return <button key={device.did} className="soft-button w-full px-3 py-3 text-left text-sm"
              disabled={probeAccountDevice.isPending || !local && !device.local_ip}
              onClick={() => local ? choose({ ...local, name: device.name }) : probeAccountDevice.mutate(device)}>
              <span className="block font-medium">{device.name}</span>
              <span className="mono text-xs text-[var(--muted-foreground)]">{local?.ip ?? device.local_ip ?? 'No local IP'} · {device.did}</span>
            </button>
          })}
          {!cloudStep.devices.some(device => ['yeelink.light.bslamp1', 'yeelink.light.color4'].includes(device.model) && device.has_token &&
            !connected.data?.some(item => item.address === device.local_ip)) &&
            <p className="text-xs text-[var(--muted-foreground)]">No unconnected supported lamps with tokens were found in this account.</p>}
        </div>}
      </div>}
      <div className="panel p-5">
        <div className="mb-4 flex items-start justify-between gap-3">
          <div><h2 className="font-semibold">Devices on this network</h2>
            <p className="mt-1 text-xs text-[var(--muted-foreground)]">Select a device that Frog has found but has not connected.</p></div>
          <button className="soft-button p-2.5" title="Scan again" aria-label="Scan again" disabled={discovered.isFetching}
            onClick={() => void discovered.refetch()}><RefreshCw size={17} /></button>
        </div>
        {discovered.isFetching && <p className="text-sm text-[var(--muted-foreground)]">Scanning the local network…</p>}
        {discovered.error && <p className="text-sm text-[var(--destructive)]">{message(discovered.error)}</p>}
        <div className="space-y-2">
          {discovered.data?.map(candidate => <button key={`${candidate.kind}:${candidate.ip}`}
            className="soft-button flex w-full items-center gap-3 p-3 text-left" onClick={() => choose(candidate)}>
            {candidate.kind === 'tv' ? <Tv2 size={21} /> : <Lightbulb size={21} />}
            <span className="min-w-0 flex-1"><span className="block truncate text-sm font-semibold">{candidate.name}</span>
              <span className="mono text-xs text-[var(--muted-foreground)]">{candidate.ip}{candidate.device_id ? ` · ID ${candidate.device_id}` : ''}</span></span>
          </button>)}
        </div>
        {discovered.data?.length === 0 && !discovered.isFetching && <p className="text-sm text-[var(--muted-foreground)]">No unconnected devices found. You can retry or enter an address below.</p>}
      </div>
      <div className="panel p-5">
        <h3 className="mb-2 text-sm font-semibold">Device on another subnet?</h3>
        <div className="flex gap-2"><input className="field mono min-w-0 flex-1 text-sm" placeholder="IP address" value={manualIp}
          onChange={event => setManualIp(event.target.value)} />
          <button className="soft-button flex items-center gap-2 px-3 text-sm" disabled={probe.isPending}
            onClick={() => probe.mutate()}><Search size={17} /> Find</button></div>
      </div>
    </> : <>
      <button className="text-sm text-[var(--muted-foreground)] hover:text-[var(--foreground)]" onClick={() => setSelected(null)}>← Discovered devices</button>
      <div className="panel space-y-4 p-5">
        <div className="flex items-center gap-3">{selected.kind === 'tv' ? <Tv2 size={22} /> : <Lightbulb size={22} />}
          <div><h2 className="font-semibold">{selected.name}</h2><p className="mono text-xs text-[var(--muted-foreground)]">{selected.ip}</p></div></div>
        <label className="block space-y-2"><span className="text-xs font-medium">Name in Frog</span>
          <input className="field text-sm" value={name} onChange={event => setName(event.target.value)} /></label>
      </div>

      {selected.kind === 'tv' ? <div className="panel space-y-4 p-5">
        <div><h3 className="font-semibold">Pair this TV</h3><p className="mt-1 text-xs leading-5 text-[var(--muted-foreground)]">Press Pair, then accept the prompt on the TV. Frog saves the TV only after pairing succeeds.</p></div>
        <button className="primary-button w-full px-4 py-3 text-sm" disabled={pair.isPending} onClick={() => pair.mutate()}>
          {pair.isPending ? 'Waiting for TV approval…' : paired ? 'Pair again' : 'Pair TV'}</button>
        <label className="block space-y-2"><span className="text-xs font-medium">Root SSH private key (optional)</span>
          <textarea className="field mono min-h-24 text-xs" value={rootKey} onChange={event => setRootKey(event.target.value)}
            placeholder="Paste the private key for audio and Bluetooth controls" /></label>
        <p className="text-xs leading-5 text-[var(--muted-foreground)]">Without a root key, remote buttons work; audio and Bluetooth controls need one.</p>
        <button className="primary-button w-full px-4 py-3 text-sm" disabled={!paired || finish.isPending}
          onClick={() => finish.mutate('tv')}>Add paired TV</button>
      </div> : <div className="panel space-y-4 p-5">
        <div><h3 className="font-semibold">Get this lamp’s local token</h3>
          <p className="mt-1 text-xs leading-5 text-[var(--muted-foreground)]">Sign in to Xiaomi to retrieve the token. Frog saves a reusable account session, but never the password. Lamp control remains local afterward.</p></div>
        {!cloudStep && <>
          <div className="space-y-2 border-b border-[var(--border)] pb-4">
            <label className="block space-y-2"><span className="text-xs font-medium">Existing Xiaomi account in Frog</span>
              <select className="field text-sm" value={savedAccountId} disabled={!accounts.data?.length || useAccount.isPending}
                onChange={event => setSavedAccountId(event.target.value)}>
                <option value="">{accounts.isLoading ? 'Checking saved accounts…' : accounts.data?.length ? 'Select an account' : 'No saved account yet'}</option>
                {accounts.data?.map(account => <option key={account.user_id} value={account.user_id}>
                  {account.username} ({account.region === 'all' ? 'all regions' : account.region})
                </option>)}
              </select></label>
            {accounts.error && <p className="text-xs text-[var(--destructive)]">Could not load saved accounts: {message(accounts.error)}</p>}
            {accounts.data?.length === 0 && <p className="text-xs leading-5 text-[var(--muted-foreground)]">
              No Xiaomi account session is saved on this controller. Sign in below once; Frog will offer that account here for later devices.
            </p>}
            <button className="soft-button w-full px-4 py-3 text-sm" disabled={!savedAccountId || useAccount.isPending}
              onClick={() => useAccount.mutate(savedAccountId)}>{useAccount.isPending ? 'Loading account…' : 'Use saved account'}</button>
          </div>
          <p className="text-xs font-medium">{accounts.data?.length ? 'Sign in to another Xiaomi account' : 'Sign in to Xiaomi'}</p>
          <input className="field text-sm" autoComplete="username" placeholder="Xiaomi account email or phone" value={username}
            onChange={event => setUsername(event.target.value)} />
          <input className="field text-sm" type="password" autoComplete="current-password" placeholder="Password" value={password}
            onChange={event => setPassword(event.target.value)} />
          <label className="block space-y-2"><span className="text-xs font-medium">Xiaomi Home region</span>
            <select className="field text-sm" value={region} onChange={event => setRegion(event.target.value)}>
              <option value="all">Search all regions</option>
              <option value="ru">Russia</option><option value="de">Europe</option><option value="cn">China</option>
              <option value="us">United States</option><option value="sg">Singapore</option>
              <option value="in">India</option><option value="i2">India 2</option><option value="tw">Taiwan</option>
            </select></label>
          <button className="primary-button w-full px-4 py-3 text-sm" disabled={begin.isPending || !username || !password}
            onClick={() => begin.mutate()}>{begin.isPending ? 'Signing in…' : 'Retrieve token'}</button>
        </>}
        {cloudStep?.status === 'captcha' && <>
          <p className="text-sm">{cloudStep.wrong ? 'That CAPTCHA was rejected. Try the new image.' : 'Type the characters shown below.'}</p>
          <img className="max-w-full rounded-lg bg-white p-2" src={cloudStep.image} alt="Xiaomi sign-in CAPTCHA" />
          <input className="field text-sm" value={captcha} onChange={event => setCaptcha(event.target.value)} placeholder="CAPTCHA characters" />
          <button className="primary-button w-full px-4 py-3 text-sm" disabled={answerCaptcha.isPending || !captcha.trim()}
            onClick={() => answerCaptcha.mutate()}>{answerCaptcha.isPending ? 'Checking…' : 'Continue'}</button>
        </>}
        {cloudStep?.status === 'email_code' && <>
          <p className="text-sm">Enter the verification code Xiaomi sent to your account email.</p>
          <input className="field mono text-sm" inputMode="numeric" autoComplete="one-time-code" value={emailCode}
            onChange={event => setEmailCode(event.target.value)} placeholder="Email code" />
          <button className="primary-button w-full px-4 py-3 text-sm" disabled={answerEmail.isPending || !emailCode.trim()}
            onClick={() => answerEmail.mutate()}>{answerEmail.isPending ? 'Checking…' : 'Verify and find token'}</button>
        </>}
        {cloudStep?.status === 'ready' && <>
          {cloudMatch ? <p className="text-sm">Xiaomi account match: <strong>{cloudMatch.name}</strong> ({cloudMatch.model}).
            {cloudMatch.has_token ? ' Token found.' : ' Xiaomi did not provide a token.'}</p>
            : <p className="text-sm text-[var(--destructive)]">This device ID was not found in the selected Xiaomi region. Check the region or use a token you already have.</p>}
          {cloudMatch && ['yeelink.light.bslamp1', 'yeelink.light.color4'].includes(cloudMatch.model) && cloudMatch.has_token &&
            <button className="primary-button w-full px-4 py-3 text-sm" disabled={finish.isPending}
              onClick={() => finish.mutate('cloud')}>{finish.isPending ? 'Testing local control…' : 'Test and add lamp'}</button>}
          {cloudMatch && !['yeelink.light.bslamp1', 'yeelink.light.color4'].includes(cloudMatch.model) &&
            <p className="text-sm text-[var(--destructive)]">Frog does not yet have controls for this Xiaomi model.</p>}
          <button className="text-xs underline" onClick={() => setCloudStep(null)}>Try another account or region</button>
        </>}
        <button className="text-xs underline" onClick={() => setManualTokenOpen(!manualTokenOpen)}>
          {manualTokenOpen ? 'Hide manual token entry' : 'I already have a device token'}</button>
        {manualTokenOpen && <div className="space-y-3 border-t border-[var(--border)] pt-4">
          <input className="field mono text-sm" type="password" autoComplete="off" placeholder="32-character local token"
            value={token} onChange={event => setToken(event.target.value)} />
          <button className="soft-button w-full px-4 py-3 text-sm" disabled={finish.isPending || token.length !== 32}
            onClick={() => finish.mutate('manual')}>Test and add with this token</button>
        </div>}
      </div>}
    </>}
    <button className="text-sm text-[var(--muted-foreground)] hover:text-[var(--foreground)]" onClick={onCancel}>Cancel setup</button>
  </div>
}
