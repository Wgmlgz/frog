import { cpSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs'
import path from 'node:path'

const settings = path.resolve(import.meta.dirname, '../src-tauri/gen/android/settings.gradle')
const root = path.resolve(import.meta.dirname, '..')
const iconRoot = path.join(root, 'src-tauri/icons/android')
const androidRes = path.join(root, 'src-tauri/gen/android/app/src/main/res')
for (const density of ['mdpi', 'hdpi', 'xhdpi', 'xxhdpi', 'xxxhdpi']) {
  const sourceDir = path.join(iconRoot, `mipmap-${density}`)
  const targetDir = path.join(androidRes, `mipmap-${density}`)
  mkdirSync(targetDir, { recursive: true })
  for (const name of readdirSync(sourceDir).filter(name => name.endsWith('.png'))) {
    cpSync(path.join(sourceDir, name), path.join(targetDir, name))
  }
}
const pythonSources = path.join(root, 'tauri-plugin-tv-python/android/src/main/python')
if (!pythonSources.startsWith(root + path.sep)) throw new Error('Python stage must stay inside the workspace')
rmSync(pythonSources, { recursive: true, force: true })
mkdirSync(pythonSources, { recursive: true })
cpSync(path.join(root, 'tv/remote_bridge.py'), path.join(pythonSources, 'remote_bridge.py'))
cpSync(path.join(root, 'tv/vendor/bscpylgtv/bscpylgtv'), path.join(pythonSources, 'bscpylgtv'), {
  recursive: true,
  force: true,
  filter: source => path.basename(source) !== '__pycache__' && !source.endsWith('.pyc'),
})
// The Android bridge always passes MemoryStorage, so bscpylgtv never opens SQLite.
// Defer this optional import in the staged copy: Chaquopy cannot install sqlitedict's source-only distribution.
const storageModule = path.join(pythonSources, 'bscpylgtv/storage_sqlitedict.py')
const storage = readFileSync(storageModule, 'utf8')
const eagerImport = /from sqlitedict import SqliteDict\r?\n/
if (!eagerImport.test(storage)) throw new Error('bscpylgtv storage import changed')
writeFileSync(storageModule, storage.replace(eagerImport, '').replace(
  '        self.db = SqliteDict(self.db_path, self.table)',
  '        from sqlitedict import SqliteDict\n        self.db = SqliteDict(self.db_path, self.table)',
))
let source = readFileSync(settings, 'utf8')
if (!source.includes('pluginManagement {')) {
  source = `pluginManagement {
    repositories {
        mavenCentral()
        gradlePluginPortal()
        google()
    }
}

${source}`
  writeFileSync(settings, source)
}

const wrapperProperties = path.join(root, 'src-tauri/gen/android/gradle/wrapper/gradle-wrapper.properties')
let wrapper = readFileSync(wrapperProperties, 'utf8')
if (!wrapper.includes('networkTimeout=')) {
  wrapper += '\nnetworkTimeout=120000\n'
  writeFileSync(wrapperProperties, wrapper)
}

const appGradle = path.join(root, 'src-tauri/gen/android/app/build.gradle.kts')
let appBuild = readFileSync(appGradle, 'utf8')
if (!appBuild.includes('// Frog release signing')) {
  const buildTypes = '    buildTypes {'
  const releaseType = '        getByName("release") {'
  if (!appBuild.includes(buildTypes) || !appBuild.includes(releaseType)) {
    throw new Error('Tauri Android signing insertion points changed')
  }
  appBuild = appBuild.replace(buildTypes, `    // Frog release signing: keystore.properties stays outside Git.
    signingConfigs {
        val keystoreFile = rootProject.file("keystore.properties")
        if (keystoreFile.exists()) {
            create("release") {
                val credentials = Properties().apply { keystoreFile.inputStream().use { load(it) } }
                keyAlias = credentials.getProperty("keyAlias")
                keyPassword = credentials.getProperty("keyPassword", credentials.getProperty("password"))
                storeFile = file(credentials.getProperty("storeFile"))
                storePassword = credentials.getProperty("storePassword", credentials.getProperty("password"))
            }
        }
    }
    buildTypes {`)
  appBuild = appBuild.replace(releaseType, `${releaseType}
            if (rootProject.file("keystore.properties").exists()) {
                signingConfig = signingConfigs.getByName("release")
            }`)
  writeFileSync(appGradle, appBuild)
}
