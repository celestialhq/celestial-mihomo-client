import { access, mkdir } from 'node:fs/promises'
import path from 'node:path'

import AdmZip from 'adm-zip'

const target = process.argv[2]
const version = process.argv[3]
const outputDir = path.resolve(process.argv[4] || 'release-assets')

if (!target || !version) {
  throw new Error(
    'Usage: node create-portable.mjs <target> <version> [output-dir]',
  )
}

const archByTarget = {
  'x86_64-pc-windows-msvc': 'x64',
  'aarch64-pc-windows-msvc': 'arm64',
}
const arch = archByTarget[target]

if (!arch) throw new Error(`Unsupported portable target: ${target}`)

const releaseDir = await firstExistingDirectory([
  path.resolve('target', target, 'release'),
  path.resolve('src-tauri', 'target', target, 'release'),
])
// Every core the app ships, under the names Tauri lays them out with beside the binary — the
// target triple is stripped when bundling. A core missing from this list is not a build
// failure, it is a portable archive that quietly ships without it: the installer carried
// `celestial-xray.exe` while the portable zip did not, so the relay there could only fall
// back to running natively.
const requiredFiles = [
  'celestial.exe',
  'celestial-mihomo.exe',
  'celestial-mihomo-alpha.exe',
  'celestial-xray.exe',
]

for (const file of requiredFiles) {
  await access(path.join(releaseDir, file))
}

await access(path.join(releaseDir, 'resources'))
await mkdir(outputDir, { recursive: true })

const zip = new AdmZip()
for (const file of requiredFiles) {
  zip.addLocalFile(path.join(releaseDir, file))
}
zip.addLocalFolder(path.join(releaseDir, 'resources'), 'resources')
zip.addFile('.config/PORTABLE', Buffer.alloc(0))

const outputPath = path.join(
  outputDir,
  `Celestial_${version}_${arch}_portable.zip`,
)
zip.writeZip(outputPath)

console.log(`Created ${outputPath}`)

async function firstExistingDirectory(candidates) {
  for (const candidate of candidates) {
    try {
      await access(candidate)
      return candidate
    } catch {
      // Try the next Cargo target directory layout.
    }
  }
  throw new Error(`Release directory not found: ${candidates.join(', ')}`)
}
