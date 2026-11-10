import './configure-android.mjs'
import { spawnSync } from 'node:child_process'
import path from 'node:path'

const android = path.resolve(import.meta.dirname, '../src-tauri/gen/android')
const release = process.argv.includes('--release')
const result = spawnSync(process.platform === 'win32' ? 'gradlew.bat' : './gradlew',
  [release ? ':wear:assembleRelease' : ':wear:assembleDebug'], {
    cwd: android,
    stdio: 'inherit',
    shell: process.platform === 'win32',
  })
if (result.error) throw result.error
process.exit(result.status ?? 1)
