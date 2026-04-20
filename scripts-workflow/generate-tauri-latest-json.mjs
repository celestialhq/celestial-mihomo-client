import fs from 'node:fs/promises'

const assetsJsonPath = process.argv[2] || 'release-assets.json'
const outputPath = process.argv[3] || 'latest.json'

const version = requiredEnv('VERSION')
const notes = process.env.NOTES_FILE
  ? await fs.readFile(process.env.NOTES_FILE, 'utf8')
  : process.env.RELEASE_NOTES || ''

const assetsPayload = JSON.parse(
  (await fs.readFile(assetsJsonPath, 'utf8')).replace(/^\uFEFF/, ''),
)
const assets = Array.isArray(assetsPayload.assets)
  ? assetsPayload.assets
  : assetsPayload
const assetsByName = new Map(assets.map((asset) => [asset.name, asset]))
const platforms = {}

for (const asset of assets) {
  const name = asset.name
  if (!name.includes(version) || name.endsWith('.sig')) {
    continue
  }

  if (name.includes('fixed_webview2')) {
    continue
  }

  if (name.endsWith('_x64-setup.exe')) {
    await setPlatform(['win64', 'windows-x86_64'], name)
    continue
  }

  if (name.endsWith('.app.tar.gz') && !name.includes('aarch')) {
    await setPlatform(['darwin', 'darwin-intel', 'darwin-x86_64'], name)
    continue
  }

  if (name.endsWith('amd64.AppImage.tar.gz')) {
    await setPlatform(['linux', 'linux-x86_64', 'linux-x86_64-appimage'], name)
    continue
  }

  if (name.endsWith('amd64.deb')) {
    await setPlatform(['linux-x86_64-deb'], name)
    continue
  }

  if (name.endsWith('x86_64.rpm')) {
    await setPlatform(['linux-x86_64-rpm'], name)
  }
}

if (Object.keys(platforms).length === 0) {
  throw new Error(`No complete updater platforms found for ${version}`)
}

const latest = {
  version,
  notes,
  pub_date: new Date().toISOString(),
  platforms,
}

await fs.writeFile(outputPath, `${JSON.stringify(latest, null, 2)}\n`)

function requiredEnv(name) {
  const value = process.env[name]
  if (!value) {
    throw new Error(`${name} is required`)
  }
  return value
}

async function setPlatform(keys, assetName) {
  const asset = assetsByName.get(assetName)
  const signatureAsset = assetsByName.get(`${assetName}.sig`)

  if (!asset || !signatureAsset) {
    console.log(`[Warning]: skipped incomplete updater asset: ${assetName}`)
    return
  }

  if (!asset.browser_download_url || !signatureAsset.browser_download_url) {
    throw new Error(
      `Missing browser_download_url for ${assetName}. ` +
        `Expected GitHub REST release payload, got incompatible assets JSON.`,
    )
  }

  const signature = await fetch(signatureAsset.browser_download_url).then(
    (response) => {
      if (!response.ok) {
        throw new Error(
          `Failed to download signature ${signatureAsset.name}: ${response.status}`,
        )
      }
      return response.text()
    },
  )

  for (const key of keys) {
    platforms[key] = {
      url: asset.browser_download_url,
      signature: signature.trim(),
    }
  }
}
