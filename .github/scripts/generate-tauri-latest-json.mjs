import fs from 'node:fs/promises'

const assetsJsonPath = process.argv[2] || 'release-assets.json'
const outputPath = process.argv[3] || 'latest.json'
const version = requiredEnv('VERSION')
const updateVersion = process.env.UPDATE_VERSION || version
const buildCommit = process.env.BUILD_COMMIT || ''
const notes = process.env.NOTES_FILE
  ? await fs.readFile(process.env.NOTES_FILE, 'utf8')
  : ''

const payload = JSON.parse(
  (await fs.readFile(assetsJsonPath, 'utf8')).replace(/^\uFEFF/, ''),
)
const assets = Array.isArray(payload.assets) ? payload.assets : payload
const assetsByName = new Map(assets.map((asset) => [asset.name, asset]))
const platforms = {}

for (const asset of assets) {
  const name = asset.name
  if (!name.includes(version) || name.endsWith('.sig')) continue

  if (name.endsWith('_x64-setup.exe')) {
    await setPlatform(['win64', 'windows-x86_64'], name)
  } else if (name.endsWith('_arm64-setup.exe')) {
    await setPlatform(['windows-aarch64'], name)
  } else if (name.endsWith('_x64.app.tar.gz')) {
    await setPlatform(['darwin', 'darwin-intel', 'darwin-x86_64'], name)
  } else if (name.endsWith('_aarch64.app.tar.gz')) {
    await setPlatform(['darwin-aarch64'], name)
  } else if (name.endsWith('_amd64.AppImage.tar.gz')) {
    await setPlatform(['linux', 'linux-x86_64', 'linux-x86_64-appimage'], name)
  } else if (name.endsWith('_amd64.deb')) {
    await setPlatform(['linux-x86_64-deb'], name)
  } else if (name.endsWith('x86_64.rpm')) {
    await setPlatform(['linux-x86_64-rpm'], name)
  }
}

if (Object.keys(platforms).length === 0) {
  console.log(
    `No signed updater assets found for ${version}; latest.json was not created.`,
  )
  process.exit(2)
}

await fs.writeFile(
  outputPath,
  `${JSON.stringify(
    {
      version: updateVersion,
      ...(buildCommit ? { build_commit: buildCommit } : {}),
      notes,
      pub_date: new Date().toISOString(),
      platforms,
    },
    null,
    2,
  )}\n`,
)

async function setPlatform(keys, assetName) {
  const asset = assetsByName.get(assetName)
  const signature = assetsByName.get(`${assetName}.sig`)
  if (!asset?.browser_download_url || !signature?.browser_download_url) return

  const response = await fetch(signature.browser_download_url)
  if (!response.ok) {
    throw new Error(`Failed to download ${signature.name}: ${response.status}`)
  }
  const signatureText = (await response.text()).trim()

  for (const key of keys) {
    platforms[key] = {
      url: asset.browser_download_url,
      signature: signatureText,
    }
  }
}

function requiredEnv(name) {
  const value = process.env[name]
  if (!value) throw new Error(`${name} is required`)
  return value
}
