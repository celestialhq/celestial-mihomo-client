import { readdir, readFile, writeFile } from 'node:fs/promises'
import path from 'node:path'

const assetsDir = path.resolve(process.argv[2] || 'release-assets')
const outputPath = path.resolve(process.argv[3] || 'release.txt')
const downloadUrl = requiredArg(4, 'download URL').replace(/\/$/, '')
const version = requiredArg(5, 'version')
const releaseKind = process.argv[6] || 'dev'

const assets = new Set(
  (await readdir(assetsDir, { withFileTypes: true }))
    .filter((entry) => entry.isFile())
    .map((entry) => entry.name),
)
const changelog = await changelogEntry(version)
const rows = [
  buildRow('Windows', [
    findBadge(/_x64-setup\.exe$/, 'x64 installer', '0078D6'),
    findBadge(/_x64_portable\.zip$/, 'x64 portable', '0078D6'),
    findBadge(/_arm64-setup\.exe$/, 'ARM64 installer', '0078D6'),
    findBadge(/_arm64_portable\.zip$/, 'ARM64 portable', '0078D6'),
  ]),
  buildRow('macOS', [
    findBadge(/_aarch64\.dmg$/, 'Apple Silicon DMG', '000000'),
    findBadge(/_x64\.dmg$/, 'Intel x64 DMG', '000000'),
  ]),
  buildRow('Linux', [
    findBadge(/_amd64\.AppImage$/, 'AppImage x64', 'FCC624', '000000'),
    findBadge(/_amd64\.deb$/, 'DEB x64', 'A81D33'),
    findBadge(/x86_64\.rpm$/, 'RPM x64', '294172'),
  ]),
].filter(Boolean)

if (rows.length === 0) {
  throw new Error('No downloadable release packages were found')
}

const body = [
  ...releaseNotice(),
  '## Changelog',
  '',
  changelog,
  '',
  '## Downloads',
  '',
  '| OS | Download |',
  '|:--|:--|',
  ...rows,
  '',
  `Version: \`${version}\`  `,
  `Built: ${new Date()
    .toISOString()
    .replace('T', ' ')
    .replace(/\.\d{3}Z$/, ' UTC')}`,
  '',
].join('\n')

await writeFile(outputPath, body, 'utf8')

function buildRow(os, badges) {
  const available = badges.filter(Boolean)
  return available.length > 0 ? `| ${os} | ${available.join('<br>')} |` : null
}

function findBadge(pattern, label, color, labelColor = '555') {
  const name = [...assets].find(
    (asset) =>
      pattern.test(asset) &&
      !asset.endsWith('.sha256') &&
      !asset.endsWith('.sig'),
  )
  if (!name) return null

  const badgeLabel = encodeURIComponent(label).replace(/-/g, '--')
  const badge = `https://img.shields.io/badge/download-${badgeLabel}-${color}?style=for-the-badge&logoColor=white&labelColor=${labelColor}`
  const assetUrl = `${downloadUrl}/${encodeURIComponent(name)}`
  const checksum = assets.has(`${name}.sha256`)
    ? ` [SHA-256](${downloadUrl}/${encodeURIComponent(`${name}.sha256`)})`
    : ''

  return `[![${label}](${badge})](${assetUrl})${checksum}`
}

function releaseNotice() {
  if (releaseKind === 'stable') return []
  if (releaseKind === 'rc') {
    return [
      '> [!WARNING]',
      '> This is a release candidate. It may still contain regressions and is',
      '> intended for final validation before the stable release.',
      '',
    ]
  }
  return [
    '> [!WARNING]',
    '> This is an automated build from the `dev` branch. It may be unstable,',
    '> contain unfinished changes, or behave differently from stable releases.',
    '',
  ]
}

async function changelogEntry(targetVersion) {
  try {
    const content = await readFile('Changelog.md', 'utf8')
    const heading = `## v${targetVersion.replace(/^v/, '')}`
    const start = content
      .split(/\r?\n/)
      .reduce(
        (offset, line) =>
          offset.found
            ? offset
            : line.trim() === heading
              ? { found: true, value: offset.value }
              : { found: false, value: offset.value + line.length + 1 },
        { found: false, value: 0 },
      )
    if (!start.found) return 'No changelog entry was found for this release.'

    const next = content.indexOf('\n## v', start.value + heading.length)
    const entry = content
      .slice(start.value, next === -1 ? undefined : next)
      .trim()
    return entry.replace(/^## /, '### ')
  } catch {
    return 'No changelog entry was found for this release.'
  }
}

function requiredArg(index, name) {
  const value = process.argv[index]
  if (!value) throw new Error(`Missing ${name}`)
  return value
}
