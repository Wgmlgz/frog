import { execFileSync } from 'node:child_process'
import { existsSync, mkdirSync } from 'node:fs'
import path from 'node:path'

const root = path.resolve(import.meta.dirname, '..')
const triple = execFileSync('rustc', ['--print', 'host-tuple'], { encoding: 'utf8' }).trim()
const venvPython = path.join(root, 'tv', '.venv', process.platform === 'win32' ? 'Scripts/python.exe' : 'bin/python')
const python = process.env.PYTHON || (existsSync(venvPython) ? venvPython : process.platform === 'win32' ? 'python' : 'python3')
const outDir = path.join(root, 'src-tauri', 'binaries')
mkdirSync(outDir, { recursive: true })

const args = [
  '-m', 'PyInstaller', '--noconfirm', '--clean', '--onefile',
  '--name', `tv-python-${triple}`,
  '--distpath', outDir,
  '--workpath', path.join(root, 'tv', 'build'),
  '--specpath', path.join(root, 'tv'),
  '--paths', path.join(root, 'tv', 'vendor', 'bscpylgtv'),
  path.join(root, 'tv', 'python_sidecar.py'),
]
console.log(`Building Python sidecar for ${triple}`)
execFileSync(python, args, { cwd: root, stdio: 'inherit' })
