import fs from 'fs'
import fsp from 'fs/promises'
import { createRequire } from 'module'
import path from 'path'

import AdmZip from 'adm-zip'

const target = process.argv.slice(2)[0]
const ARCH_MAP = {
  'x86_64-pc-windows-msvc': 'x64',
  'aarch64-pc-windows-msvc': 'arm64',
}

const PROCESS_MAP = {
  x64: 'x64',
  arm64: 'arm64',
}
const arch = target ? ARCH_MAP[target] : PROCESS_MAP[process.arch]

/// The cores the app cannot run without, as Tauri lays them out beside the binary — the
/// target triple is stripped when bundling, so these are the names on disk.
///
/// Listed rather than globbed so that adding a sidecar without adding it here is a build
/// failure instead of a portable build that silently ships without a core. That is not
/// hypothetical: the xray core was bundled by the installer and missing from the portable
/// zip, where the relay could only fall back to running natively.
const SIDECARS = [
  'celestial-mihomo.exe',
  'celestial-mihomo-alpha.exe',
  'celestial-xray.exe',
]

function addSidecars(zip, releaseDir) {
  const missing = SIDECARS.filter(
    (name) => !fs.existsSync(path.join(releaseDir, name)),
  )
  if (missing.length > 0) {
    throw new Error(
      `missing sidecar(s) in ${releaseDir}: ${missing.join(', ')}. ` +
        'Run `pnpm prebuild` before packaging.',
    )
  }
  for (const name of SIDECARS) {
    zip.addLocalFile(path.join(releaseDir, name))
  }
}
/// Script for ci
/// 打包绿色版/便携版 (only Windows)
async function resolvePortable() {
  if (process.platform !== 'win32') return

  const releaseDir = target
    ? `./src-tauri/target/${target}/release`
    : `./src-tauri/target/release`
  const configDir = path.join(releaseDir, '.config')

  if (!fs.existsSync(releaseDir)) {
    throw new Error('could not found the release dir')
  }

  await fsp.mkdir(configDir, { recursive: true })
  if (!fs.existsSync(path.join(configDir, 'PORTABLE'))) {
    await fsp.writeFile(path.join(configDir, 'PORTABLE'), '')
  }
  const zip = new AdmZip()

  zip.addLocalFile(path.join(releaseDir, 'celestial.exe'))
  addSidecars(zip, releaseDir)
  zip.addLocalFolder(path.join(releaseDir, 'resources'), 'resources')
  zip.addLocalFolder(configDir, '.config')

  const require = createRequire(import.meta.url)
  const packageJson = require('../package.json')
  const { version } = packageJson
  const zipFile = `Celestial_${version}_${arch}_portable.zip`
  zip.writeZip(zipFile)
  console.log('[INFO]: create portable zip successfully')
}

resolvePortable().catch(console.error)
