import { createHash } from 'node:crypto'
import { readdir, readFile, writeFile } from 'node:fs/promises'
import path from 'node:path'

const assetsDir = path.resolve(process.argv[2] || 'release-assets')
const entries = await readdir(assetsDir, { withFileTypes: true })

for (const entry of entries) {
  if (!entry.isFile() || entry.name.endsWith('.sha256')) continue

  const filePath = path.join(assetsDir, entry.name)
  const digest = createHash('sha256')
    .update(await readFile(filePath))
    .digest('hex')

  await writeFile(`${filePath}.sha256`, `${digest}  ${entry.name}\n`, 'utf8')
}
