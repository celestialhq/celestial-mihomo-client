import fs from 'node:fs/promises'

// Declared up here, not beside `fetchSignature` below: the top-level loop calls
// it before execution ever reaches that point. Function declarations hoist,
// `const` does not, so leaving these next to their user is a
// use-before-initialization error that only appears at runtime.
const SIGNATURE_TIMEOUT_MS = 30_000
const SIGNATURE_ATTEMPTS = 4

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
  } else if (name.endsWith('_amd64.AppImage')) {
    // Tauri v2 signs the AppImage itself and emits `.AppImage.sig` beside it.
    // The `.AppImage.tar.gz` this used to look for is a v1 artifact that no
    // build here has ever produced, so Linux never got an updater entry.
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

/**
 * Fetch a signature, refusing to wait forever for it.
 *
 * Node's `fetch` has no default timeout, so a connection that stalls rather
 * than failing hangs this script — and with it the publish step, and with that
 * the whole concurrency group, until someone notices and cancels by hand. One
 * such stall held a release job for over an hour with every asset already
 * uploaded. A bounded wait turns that into a retry, and a bounded number of
 * retries turns a genuinely unreachable asset into a failure that says so.
 */
async function fetchSignature(url, name) {
  let lastError
  for (let attempt = 1; attempt <= SIGNATURE_ATTEMPTS; attempt += 1) {
    try {
      const response = await fetch(url, {
        signal: AbortSignal.timeout(SIGNATURE_TIMEOUT_MS),
      })
      if (!response.ok) {
        throw new Error(`status ${response.status}`)
      }
      return (await response.text()).trim()
    } catch (error) {
      lastError = error
      if (attempt < SIGNATURE_ATTEMPTS) {
        const delay = 2 ** (attempt - 1) * 1000
        console.log(
          `Failed to download ${name} (attempt ${attempt}/${SIGNATURE_ATTEMPTS}): ${error.message}; retrying in ${delay}ms`,
        )
        await new Promise((resolve) => setTimeout(resolve, delay))
      }
    }
  }
  throw new Error(
    `Failed to download ${name} after ${SIGNATURE_ATTEMPTS} attempts: ${lastError?.message}`,
  )
}

async function setPlatform(keys, assetName) {
  const asset = assetsByName.get(assetName)
  const signature = assetsByName.get(`${assetName}.sig`)
  if (!asset?.browser_download_url || !signature?.browser_download_url) return

  const signatureText = await fetchSignature(
    signature.browser_download_url,
    signature.name,
  )

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
