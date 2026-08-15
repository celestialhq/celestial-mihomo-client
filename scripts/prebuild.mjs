import { execSync } from 'child_process'
import { createHash } from 'crypto'
import fs from 'fs'
import fsp from 'fs/promises'
import path from 'path'
import zlib from 'zlib'

import AdmZip from 'adm-zip'
import { glob } from 'glob'
import { HttpsProxyAgent } from 'https-proxy-agent'
import fetch from 'node-fetch'
import { extract } from 'tar'

import { log_debug, log_error, log_info, log_success } from './utils.mjs'

/**
 * Prebuild script with optimization features:
 * 1. Skip downloading mihomo core if it already exists (unless --force is used)
 * 2. Cache version information for 1 hour to avoid repeated version checks
 * 3. Use file hash to detect changes and skip unnecessary chmod/copy operations
 * 4. Use --force or -f flag to force re-download and update all resources
 *
 */

const cwd = process.cwd()
const TEMP_DIR = path.join(cwd, 'node_modules/.verge')
const FORCE = process.argv.includes('--force') || process.argv.includes('-f')
const VERSION_CACHE_FILE = path.join(TEMP_DIR, '.version_cache.json')
const HASH_CACHE_FILE = path.join(TEMP_DIR, '.hash_cache.json')

const PLATFORM_MAP = {
  'x86_64-pc-windows-msvc': 'win32',
  'i686-pc-windows-msvc': 'win32',
  'aarch64-pc-windows-msvc': 'win32',
  'x86_64-apple-darwin': 'darwin',
  'aarch64-apple-darwin': 'darwin',
  'x86_64-unknown-linux-gnu': 'linux',
  'i686-unknown-linux-gnu': 'linux',
  'aarch64-unknown-linux-gnu': 'linux',
  'armv7-unknown-linux-gnueabihf': 'linux',
  'riscv64gc-unknown-linux-gnu': 'linux',
  'loongarch64-unknown-linux-gnu': 'linux',
}
const ARCH_MAP = {
  'x86_64-pc-windows-msvc': 'x64',
  'i686-pc-windows-msvc': 'ia32',
  'aarch64-pc-windows-msvc': 'arm64',
  'x86_64-apple-darwin': 'x64',
  'aarch64-apple-darwin': 'arm64',
  'x86_64-unknown-linux-gnu': 'x64',
  'i686-unknown-linux-gnu': 'ia32',
  'aarch64-unknown-linux-gnu': 'arm64',
  'armv7-unknown-linux-gnueabihf': 'arm',
  'riscv64gc-unknown-linux-gnu': 'riscv64',
  'loongarch64-unknown-linux-gnu': 'loong64',
}

const arg1 = process.argv.slice(2)[0]
const arg2 = process.argv.slice(2)[1]
const target = arg1 === '--force' || arg1 === '-f' ? arg2 : arg1
const { platform, arch } = target
  ? { platform: PLATFORM_MAP[target], arch: ARCH_MAP[target] }
  : process

const SIDECAR_HOST = target
  ? target
  : execSync('rustc -vV')
      .toString()
      .match(/(?<=host: ).+(?=\s*)/g)[0]

const RESOURCES_DIR = path.join(cwd, 'src-tauri', 'resources')
const SIDECAR_DIR = path.join(cwd, 'src-tauri', 'sidecar')
// Linux service binaries are bundled as externalBin sidecars (see tauri.linux.conf.json)
const SERVICE_DIR = platform === 'linux' ? SIDECAR_DIR : RESOURCES_DIR

// =======================
// Version Cache
// =======================
async function loadVersionCache() {
  try {
    if (fs.existsSync(VERSION_CACHE_FILE)) {
      const data = await fsp.readFile(VERSION_CACHE_FILE, 'utf-8')
      return JSON.parse(data)
    }
  } catch (err) {
    log_debug('Failed to load version cache:', err.message)
  }
  return {}
}
async function saveVersionCache(cache) {
  try {
    await fsp.mkdir(TEMP_DIR, { recursive: true })
    await fsp.writeFile(VERSION_CACHE_FILE, JSON.stringify(cache, null, 2))
    log_debug('Version cache saved')
  } catch (err) {
    log_debug('Failed to save version cache:', err.message)
  }
}
async function getCachedVersion(key) {
  const cache = await loadVersionCache()
  const cached = cache[key]
  if (cached && Date.now() - cached.timestamp < 3600000) {
    log_info(`Using cached version for ${key}: ${cached.version}`)
    return cached.version
  }
  return null
}
async function setCachedVersion(key, version) {
  const cache = await loadVersionCache()
  cache[key] = { version, timestamp: Date.now() }
  await saveVersionCache(cache)
}

// =======================
// Hash Cache & File Hash
// =======================
async function calculateFileHash(filePath) {
  try {
    const fileBuffer = await fsp.readFile(filePath)
    const hashSum = createHash('sha256')
    hashSum.update(fileBuffer)
    return hashSum.digest('hex')
  } catch (ignoreErr) {
    return null
  }
}
async function loadHashCache() {
  try {
    if (fs.existsSync(HASH_CACHE_FILE)) {
      const data = await fsp.readFile(HASH_CACHE_FILE, 'utf-8')
      return JSON.parse(data)
    }
  } catch (err) {
    log_debug('Failed to load hash cache:', err.message)
  }
  return {}
}
async function saveHashCache(cache) {
  try {
    await fsp.mkdir(TEMP_DIR, { recursive: true })
    await fsp.writeFile(HASH_CACHE_FILE, JSON.stringify(cache, null, 2))
    log_debug('Hash cache saved')
  } catch (err) {
    log_debug('Failed to save hash cache:', err.message)
  }
}
async function hasFileChanged(filePath, targetPath) {
  if (FORCE) return true
  if (!fs.existsSync(targetPath)) return true
  const hashCache = await loadHashCache()
  const sourceHash = await calculateFileHash(filePath)
  const targetHash = await calculateFileHash(targetPath)
  if (!sourceHash || !targetHash) return true
  const cacheKey = targetPath
  const cachedHash = hashCache[cacheKey]
  if (cachedHash === sourceHash && sourceHash === targetHash) {
    return false
  }
  return true
}
async function updateHashCache(targetPath) {
  const hashCache = await loadHashCache()
  const hash = await calculateFileHash(targetPath)
  if (hash) {
    hashCache[targetPath] = hash
    await saveHashCache(hashCache)
  }
}

// =======================
// Meta maps (stable & alpha)
// =======================
const META_ALPHA_VERSION_URL =
  'https://github.com/MetaCubeX/mihomo/releases/download/Prerelease-Alpha/version.txt'
const META_ALPHA_URL_PREFIX = `https://github.com/MetaCubeX/mihomo/releases/download/Prerelease-Alpha`
let META_ALPHA_VERSION

const META_VERSION_URL =
  'https://github.com/MetaCubeX/mihomo/releases/latest/download/version.txt'
const META_URL_PREFIX = `https://github.com/MetaCubeX/mihomo/releases/download`
let META_VERSION

const META_ALPHA_MAP = {
  'win32-x64': 'mihomo-windows-amd64-v2',
  'win32-ia32': 'mihomo-windows-386',
  'win32-arm64': 'mihomo-windows-arm64',
  'darwin-x64': 'mihomo-darwin-amd64-v1-go122',
  'darwin-arm64': 'mihomo-darwin-arm64-go122',
  'linux-x64': 'mihomo-linux-amd64-v2',
  'linux-ia32': 'mihomo-linux-386',
  'linux-arm64': 'mihomo-linux-arm64',
  'linux-arm': 'mihomo-linux-armv7',
  'linux-riscv64': 'mihomo-linux-riscv64',
  'linux-loong64': 'mihomo-linux-loong64',
}

const META_MAP = {
  'win32-x64': 'mihomo-windows-amd64-v2',
  'win32-ia32': 'mihomo-windows-386',
  'win32-arm64': 'mihomo-windows-arm64',
  'darwin-x64': 'mihomo-darwin-amd64-v2-go122',
  'darwin-arm64': 'mihomo-darwin-arm64-go122',
  'linux-x64': 'mihomo-linux-amd64-v2',
  'linux-ia32': 'mihomo-linux-386',
  'linux-arm64': 'mihomo-linux-arm64',
  'linux-arm': 'mihomo-linux-armv7',
  'linux-riscv64': 'mihomo-linux-riscv64',
  'linux-loong64': 'mihomo-linux-loong64',
}

// =======================
// Xray maps (release & pre-release)
// =======================
// The binary is renamed to `celestial-xray`. That is what the anti-loop rule matches, and
// naming it after ourselves means the rule cannot catch an xray belonging to some other
// client the user is running and divert its traffic.
//
// The channel is chosen here rather than shipped as two binaries, because the rule matches
// on process name and both channels would have to answer to the same one. Laying the
// pre-release build out under its own directory would keep the name and allow switching at
// runtime — the rule looks at the name, not the path — but that needs Tauri to preserve the
// subdirectory for an `externalBin` entry rather than flattening it next to the app binary,
// which is unverified. Until then: one binary, chosen at build time.
const XRAY_REPO = 'https://api.github.com/repos/XTLS/Xray-core'
const XRAY_URL_PREFIX = 'https://github.com/XTLS/Xray-core/releases/download'
const XRAY_PRERELEASE =
  process.argv.includes('--xray-prerelease') ||
  process.env.XRAY_CHANNEL === 'prerelease'
let XRAY_VERSION

const XRAY_MAP = {
  'win32-x64': 'Xray-windows-64',
  'win32-ia32': 'Xray-windows-32',
  'win32-arm64': 'Xray-windows-arm64-v8a',
  'darwin-x64': 'Xray-macos-64',
  'darwin-arm64': 'Xray-macos-arm64-v8a',
  'linux-x64': 'Xray-linux-64',
  'linux-ia32': 'Xray-linux-32',
  'linux-arm64': 'Xray-linux-arm64-v8a',
  'linux-arm': 'Xray-linux-arm32-v7a',
  'linux-riscv64': 'Xray-linux-riscv64',
  'linux-loong64': 'Xray-linux-loong64',
}

/// Resolves the tag to download.
///
/// Xray publishes no `version.txt` the way mihomo does, so the tag comes from the API.
/// The stable channel uses `releases/latest`, which GitHub already defines as the newest
/// non-prerelease; the pre-release channel walks the list and takes the first entry marked
/// as one, falling back to the newest release of any kind rather than failing when there is
/// no open pre-release.
async function getLatestXrayVersion() {
  const cacheKey = XRAY_PRERELEASE ? 'XRAY_PRERELEASE_VERSION' : 'XRAY_VERSION'
  if (!FORCE) {
    const cached = await getCachedVersion(cacheKey)
    if (cached) {
      XRAY_VERSION = cached
      return
    }
  }

  const options = {}
  const httpProxy =
    process.env.HTTP_PROXY ||
    process.env.http_proxy ||
    process.env.HTTPS_PROXY ||
    process.env.https_proxy
  if (httpProxy) options.agent = new HttpsProxyAgent(httpProxy)

  const headers = { Accept: 'application/vnd.github+json' }
  // Unauthenticated API calls are rate limited to 60/hour per IP, which a busy CI runner
  // can exhaust. Use the token when one is around.
  if (process.env.GITHUB_TOKEN) {
    headers.Authorization = `Bearer ${process.env.GITHUB_TOKEN}`
  }

  const url = XRAY_PRERELEASE
    ? `${XRAY_REPO}/releases?per_page=20`
    : `${XRAY_REPO}/releases/latest`

  try {
    const response = await fetch(url, { ...options, method: 'GET', headers })
    if (!response.ok) {
      throw new Error(`Failed to fetch ${url}: ${response.status}`)
    }
    const body = await response.json()
    const tag = XRAY_PRERELEASE
      ? (body.find((it) => it.prerelease) ?? body[0])?.tag_name
      : body.tag_name
    if (!tag) throw new Error('no xray release tag in the response')

    XRAY_VERSION = tag
    log_info(
      `Latest xray ${XRAY_PRERELEASE ? 'pre-release' : 'release'} version: ${XRAY_VERSION}`,
    )
    await setCachedVersion(cacheKey, XRAY_VERSION)
  } catch (err) {
    log_error('Error fetching latest xray version:', err.message)
    throw err
  }
}

function xrayCore() {
  const name = XRAY_MAP[`${platform}-${arch}`]
  const isWin = platform === 'win32'
  return {
    name: 'celestial-xray',
    targetFile: `celestial-xray-${SIDECAR_HOST}${isWin ? '.exe' : ''}`,
    // The name inside the archive is still xray's own.
    exeFile: `xray${isWin ? '.exe' : ''}`,
    zipFile: `${name}-${XRAY_VERSION}.zip`,
    downloadURL: `${XRAY_URL_PREFIX}/${XRAY_VERSION}/${name}.zip`,
  }
}

/// Fails the build if the downloaded xray renames the XHTTP masking fields.
///
/// `crates/celestial-xray-relay` converts mihomo's `xhttp-opts` onto these names, and xray
/// parses `extra` into a struct that **ignores keys it does not know** — a renamed field
/// produces a config that validates, starts, and quietly drops the obfuscation it was
/// written for. Nothing downstream can catch that, so it is caught here, where the binary
/// changes.
///
/// The session family exists under two spellings, and which one a build gets depends on the
/// channel: release 26.3.27 declares `sessionKey` / `sessionPlacement`, while the pre-release
/// channel (26.7.28) renamed them to `sessionIDKey` / `sessionIDPlacement`. The converter
/// writes both, so either core finds the one it reads — this only has to confirm that at
/// least one of them is still there, and that the rest of the family has not moved too.
async function checkXrayXhttpFieldNames() {
  const target = path.join(SIDECAR_DIR, xrayCore().targetFile)
  const binary = await fsp.readFile(target)
  const text = binary.toString('latin1')

  const required = [
    'json:"xPaddingObfsMode"',
    'json:"uplinkHTTPMethod"',
    'json:"noGRPCHeader"',
    'json:"seqPlacement"',
    'json:"seqKey"',
  ]
  const missing = required.filter((tag) => !text.includes(tag))

  const sessionSpellings = ['json:"sessionKey"', 'json:"sessionIDKey"'].filter(
    (tag) => text.includes(tag),
  )
  if (sessionSpellings.length === 0) {
    missing.push('json:"sessionKey" or json:"sessionIDKey"')
  }

  if (missing.length > 0) {
    throw new Error(
      `xray ${XRAY_VERSION} does not declare ${missing.join(', ')}. ` +
        'The XHTTP masking field names moved again; the relay converter would emit names ' +
        'this core silently ignores, and no config test can object because xray drops ' +
        'unknown keys in "extra". Update IRREGULAR and add_session_aliases in ' +
        'crates/celestial-xray-relay/src/parse.rs, and the round-trip test beside them.',
    )
  }
  log_success(
    `"celestial-xray" XHTTP field names match the relay converter (${sessionSpellings.join(', ')})`,
  )
}

// =======================
// Fetch latest versions
// =======================
async function getLatestAlphaVersion() {
  if (!FORCE) {
    const cached = await getCachedVersion('META_ALPHA_VERSION')
    if (cached) {
      META_ALPHA_VERSION = cached
      return
    }
  }
  const options = {}
  const httpProxy =
    process.env.HTTP_PROXY ||
    process.env.http_proxy ||
    process.env.HTTPS_PROXY ||
    process.env.https_proxy
  if (httpProxy) options.agent = new HttpsProxyAgent(httpProxy)

  try {
    const response = await fetch(META_ALPHA_VERSION_URL, {
      ...options,
      method: 'GET',
    })
    if (!response.ok)
      throw new Error(
        `Failed to fetch ${META_ALPHA_VERSION_URL}: ${response.status}`,
      )
    META_ALPHA_VERSION = (await response.text()).trim()
    log_info(`Latest alpha version: ${META_ALPHA_VERSION}`)
    await setCachedVersion('META_ALPHA_VERSION', META_ALPHA_VERSION)
  } catch (err) {
    log_error('Error fetching latest alpha version:', err.message)
    process.exit(1)
  }
}

async function getLatestReleaseVersion() {
  if (!FORCE) {
    const cached = await getCachedVersion('META_VERSION')
    if (cached) {
      META_VERSION = cached
      return
    }
  }
  const options = {}
  const httpProxy =
    process.env.HTTP_PROXY ||
    process.env.http_proxy ||
    process.env.HTTPS_PROXY ||
    process.env.https_proxy
  if (httpProxy) options.agent = new HttpsProxyAgent(httpProxy)

  try {
    const response = await fetch(META_VERSION_URL, {
      ...options,
      method: 'GET',
    })
    if (!response.ok)
      throw new Error(`Failed to fetch ${META_VERSION_URL}: ${response.status}`)
    META_VERSION = (await response.text()).trim()
    log_info(`Latest release version: ${META_VERSION}`)
    await setCachedVersion('META_VERSION', META_VERSION)
  } catch (err) {
    log_error('Error fetching latest release version:', err.message)
    process.exit(1)
  }
}

// =======================
// Validate availability
// =======================
if (!META_MAP[`${platform}-${arch}`]) {
  throw new Error(`clash meta unsupported platform "${platform}-${arch}"`)
}
if (!META_ALPHA_MAP[`${platform}-${arch}`]) {
  throw new Error(`clash meta alpha unsupported platform "${platform}-${arch}"`)
}
if (!XRAY_MAP[`${platform}-${arch}`]) {
  throw new Error(`xray unsupported platform "${platform}-${arch}"`)
}

// =======================
// Build meta objects
// =======================
function clashMetaAlpha() {
  const name = META_ALPHA_MAP[`${platform}-${arch}`]
  const isWin = platform === 'win32'
  const urlExt = isWin ? 'zip' : 'gz'
  return {
    name: 'celestial-mihomo-alpha',
    targetFile: `celestial-mihomo-alpha-${SIDECAR_HOST}${isWin ? '.exe' : ''}`,
    exeFile: `${name}${isWin ? '.exe' : ''}`,
    zipFile: `${name}-${META_ALPHA_VERSION}.${urlExt}`,
    downloadURL: `${META_ALPHA_URL_PREFIX}/${name}-${META_ALPHA_VERSION}.${urlExt}`,
  }
}

function clashMeta() {
  const name = META_MAP[`${platform}-${arch}`]
  const isWin = platform === 'win32'
  const urlExt = isWin ? 'zip' : 'gz'
  return {
    name: 'celestial-mihomo',
    targetFile: `celestial-mihomo-${SIDECAR_HOST}${isWin ? '.exe' : ''}`,
    exeFile: `${name}${isWin ? '.exe' : ''}`,
    zipFile: `${name}-${META_VERSION}.${urlExt}`,
    downloadURL: `${META_URL_PREFIX}/${META_VERSION}/${name}-${META_VERSION}.${urlExt}`,
  }
}

// =======================
// download helper (增强：status + magic bytes)
// =======================
async function downloadFile(url, outPath) {
  const options = {}
  const httpProxy =
    process.env.HTTP_PROXY ||
    process.env.http_proxy ||
    process.env.HTTPS_PROXY ||
    process.env.https_proxy
  if (httpProxy) options.agent = new HttpsProxyAgent(httpProxy)

  const response = await fetch(url, {
    ...options,
    method: 'GET',
    headers: { 'Content-Type': 'application/octet-stream' },
  })
  if (!response.ok) {
    const body = await response.text().catch(() => '')
    // 将 body 写到文件以便排查（可通过临时目录查看）
    await fsp.mkdir(path.dirname(outPath), { recursive: true })
    await fsp.writeFile(outPath, body)
    throw new Error(`Failed to download ${url}: status ${response.status}`)
  }

  const buf = Buffer.from(await response.arrayBuffer())
  await fsp.mkdir(path.dirname(outPath), { recursive: true })

  // 简单 magic 字节检查
  if (url.endsWith('.gz') || url.endsWith('.tgz')) {
    if (!(buf[0] === 0x1f && buf[1] === 0x8b)) {
      await fsp.writeFile(outPath, buf)
      throw new Error(
        `Downloaded file for ${url} is not a valid gzip (magic mismatch).`,
      )
    }
  } else if (url.endsWith('.zip')) {
    if (!(buf[0] === 0x50 && buf[1] === 0x4b)) {
      await fsp.writeFile(outPath, buf)
      throw new Error(
        `Downloaded file for ${url} is not a valid zip (magic mismatch).`,
      )
    }
  }

  await fsp.writeFile(outPath, buf)
  log_success(`download finished: ${url}`)
}

// =======================
// resolveSidecar (支持 zip / tgz / gz)
// =======================
async function resolveSidecar(binInfo) {
  const { name, targetFile, zipFile, exeFile, downloadURL } = binInfo
  const sidecarPath = path.join(SIDECAR_DIR, targetFile)
  await fsp.mkdir(SIDECAR_DIR, { recursive: true })

  if (!FORCE && fs.existsSync(sidecarPath)) {
    log_success(`"${name}" already exists, skipping download`)
    return
  }

  const tempDir = path.join(TEMP_DIR, name)
  const tempZip = path.join(tempDir, zipFile)
  const tempExe = path.join(tempDir, exeFile)
  await fsp.mkdir(tempDir, { recursive: true })

  try {
    if (!fs.existsSync(tempZip)) {
      await downloadFile(downloadURL, tempZip)
    }

    if (zipFile.endsWith('.zip')) {
      const zip = new AdmZip(tempZip)
      zip.getEntries().forEach((entry) => {
        log_debug(`"${name}" entry: ${entry.entryName}`)
      })
      zip.extractAllTo(tempDir, true)
      // 尝试按 exeFile 重命名，否则找第一个可执行文件
      if (fs.existsSync(tempExe)) {
        await fsp.rename(tempExe, sidecarPath)
      } else {
        // 搜索候选
        const files = await fsp.readdir(tempDir)
        const candidate = files.find(
          (f) =>
            f === path.basename(exeFile) ||
            f.endsWith('.exe') ||
            !f.includes('.'),
        )
        if (!candidate)
          throw new Error(`Expected binary not found in ${tempDir}`)
        await fsp.rename(path.join(tempDir, candidate), sidecarPath)
      }
      if (platform !== 'win32') execSync(`chmod 755 ${sidecarPath}`)
      log_success(`unzip finished: "${name}"`)
    } else if (zipFile.endsWith('.tgz')) {
      await extract({ cwd: tempDir, file: tempZip })
      const files = await fsp.readdir(tempDir)
      log_debug(`"${name}" extracted files:`, files)
      // 优先寻找给定 exeFile 或已知前缀
      let extracted = files.find(
        (f) =>
          f === path.basename(exeFile) ||
          f.startsWith('虚空终端-') ||
          !f.includes('.'),
      )
      if (!extracted) extracted = files[0]
      if (!extracted) throw new Error(`Expected file not found in ${tempDir}`)
      await fsp.rename(path.join(tempDir, extracted), sidecarPath)
      execSync(`chmod 755 ${sidecarPath}`)
      log_success(`tgz processed: "${name}"`)
    } else {
      // .gz
      const readStream = fs.createReadStream(tempZip)
      const writeStream = fs.createWriteStream(sidecarPath)
      await new Promise((resolve, reject) => {
        readStream
          .pipe(zlib.createGunzip())
          .on('error', (e) => {
            log_error(`gunzip error for ${name}:`, e.message)
            reject(e)
          })
          .pipe(writeStream)
          .on('finish', () => {
            if (platform !== 'win32') execSync(`chmod 755 ${sidecarPath}`)
            resolve()
          })
          .on('error', (e) => {
            log_error(`write stream error for ${name}:`, e.message)
            reject(e)
          })
      })
      log_success(`gz binary processed: "${name}"`)
    }
  } catch (err) {
    await fsp.rm(sidecarPath, { recursive: true, force: true })
    throw err
  } finally {
    await fsp.rm(tempDir, { recursive: true, force: true })
  }
}

async function resolveResource(binInfo) {
  const { file, downloadURL, localPath, dir } = binInfo
  const baseDir = dir ?? RESOURCES_DIR
  const targetPath = path.join(baseDir, file)

  if (!FORCE && fs.existsSync(targetPath) && !downloadURL && !localPath) {
    log_success(`"${file}" already exists, skipping`)
    return
  }

  if (downloadURL) {
    if (!FORCE && fs.existsSync(targetPath)) {
      log_success(`"${file}" already exists, skipping download`)
      return
    }
    await fsp.mkdir(baseDir, { recursive: true })
    await downloadFile(downloadURL, targetPath)
    await updateHashCache(targetPath)
  }

  if (localPath) {
    if (!(await hasFileChanged(localPath, targetPath))) {
      return
    }
    await fsp.mkdir(baseDir, { recursive: true })
    await fsp.copyFile(localPath, targetPath)
    await updateHashCache(targetPath)
    log_success(`Copied file: ${file}`)
  }

  log_success(`${file} finished`)
}

// SimpleSC.dll (win plugin)
const resolvePlugin = async () => {
  const url =
    'https://nsis.sourceforge.io/mediawiki/images/e/ef/NSIS_Simple_Service_Plugin_Unicode_1.30.zip'
  const tempDir = path.join(TEMP_DIR, 'SimpleSC')
  const tempZip = path.join(
    tempDir,
    'NSIS_Simple_Service_Plugin_Unicode_1.30.zip',
  )
  const tempDll = path.join(tempDir, 'SimpleSC.dll')
  const pluginDir = path.join(process.env.APPDATA || '', 'Local/NSIS')
  const pluginPath = path.join(pluginDir, 'SimpleSC.dll')
  await fsp.mkdir(pluginDir, { recursive: true })
  await fsp.mkdir(tempDir, { recursive: true })
  if (!FORCE && fs.existsSync(pluginPath)) return
  try {
    if (!fs.existsSync(tempZip)) {
      await downloadFile(url, tempZip)
    }
    const zip = new AdmZip(tempZip)
    zip
      .getEntries()
      .forEach((entry) => log_debug(`"SimpleSC" entry`, entry.entryName))
    zip.extractAllTo(tempDir, true)
    if (fs.existsSync(tempDll)) {
      await fsp.cp(tempDll, pluginPath, { recursive: true, force: true })
      log_success(`unzip finished: "SimpleSC"`)
    } else {
      // 如果 dll 名称不同，尝试找到 dll
      const files = await fsp.readdir(tempDir)
      const dll = files.find((f) => f.toLowerCase().endsWith('.dll'))
      if (dll) {
        await fsp.cp(path.join(tempDir, dll), pluginPath, {
          recursive: true,
          force: true,
        })
        log_success(`unzip finished: "SimpleSC" (found ${dll})`)
      } else {
        throw new Error('SimpleSC.dll not found in zip')
      }
    }
  } finally {
    await fsp.rm(tempDir, { recursive: true, force: true })
  }
}

// service chmod (保留并使用 glob)
const resolveServicePermission = async () => {
  const serviceExecutables = [
    'celestial-service*',
    'celestial-service-install*',
    'celestial-service-uninstall*',
  ]
  const hashCache = await loadHashCache()
  let hasChanges = false

  for (const f of serviceExecutables) {
    const files = glob.sync(path.join(SERVICE_DIR, f))
    for (const filePath of files) {
      if (fs.existsSync(filePath)) {
        const currentHash = await calculateFileHash(filePath)
        const cacheKey = `${filePath}_chmod`
        if (!FORCE && hashCache[cacheKey] === currentHash) {
          continue
        }
        try {
          execSync(`chmod 755 ${filePath}`)
          log_success(`chmod finished: "${filePath}"`)
        } catch (e) {
          log_error(`chmod failed for ${filePath}:`, e.message)
        }
        hashCache[cacheKey] = currentHash
        hasChanges = true
      }
    }
  }

  if (hasChanges) {
    await saveHashCache(hashCache)
  }
}

// =======================
// Other resource resolvers (service, mmdb, geosite, geoip, enableLoopback)
// =======================
const SERVICE_URL_PREFIX =
  'https://github.com/celestialhq/celestial-service-ipc/releases/download'
let SERVICE_VERSION

const SERVICE_BINARIES = [
  'celestial-service',
  'celestial-service-install',
  'celestial-service-uninstall',
]

function serviceFileInfo(name) {
  const ext = platform === 'win32' ? '.exe' : ''
  const suffix = platform === 'linux' ? '-' + SIDECAR_HOST : ''
  return {
    sourceFile: `${name}${ext}`,
    targetFile: `${name}${suffix}${ext}`,
  }
}

/**
 * The service version this client is built against, taken from Cargo.lock.
 *
 * Deliberately not `releases/latest`. The bundled helper and the client speak a
 * versioned protocol to each other, and resolving the helper independently of the code
 * let the two drift apart in silence: a client built against service-ipc 2.5.3 shipped
 * a 2.3.0 helper, whose reply the client rejected as an incompatible protocol. There
 * was no way out from inside the app either, because reinstalling installed the same
 * old helper again.
 *
 * Cargo.lock is the one place that records which service this client expects, so it
 * decides. A missing release now fails the build, where it used to ship.
 */
async function getServiceVersion() {
  const lockPath = path.join(cwd, 'Cargo.lock')
  const lock = await fsp.readFile(lockPath, 'utf8')
  const match = lock.match(
    /\[\[package\]\]\s*\r?\nname = "celestial_service_ipc"\s*\r?\nversion = "([^"]+)"/,
  )

  if (!match) {
    log_error(
      'Unable to read the celestial_service_ipc version from Cargo.lock',
    )
    process.exit(1)
  }

  SERVICE_VERSION = `v${match[1]}`
  log_info(`Service version pinned by Cargo.lock: ${SERVICE_VERSION}`)
}

async function findExtractedFile(dir, fileName) {
  const entries = await fsp.readdir(dir, { withFileTypes: true })
  for (const entry of entries) {
    const entryPath = path.join(dir, entry.name)
    if (entry.isFile() && entry.name === fileName) return entryPath
    if (entry.isDirectory()) {
      const found = await findExtractedFile(entryPath, fileName)
      if (found) return found
    }
  }
  return null
}

async function resolveServiceBundle() {
  const files = SERVICE_BINARIES.map((name) => {
    const info = serviceFileInfo(name)
    return {
      ...info,
      targetPath: path.join(SERVICE_DIR, info.targetFile),
    }
  })

  if (!FORCE && files.every(({ targetPath }) => fs.existsSync(targetPath))) {
    log_success('"celestial-service-ipc" already exists, skipping download')
    return
  }

  await getServiceVersion()

  const archiveExt = platform === 'win32' ? 'zip' : 'tar.gz'
  const archiveFile = `celestial-service-ipc-${SERVICE_VERSION}-${SIDECAR_HOST}.${archiveExt}`
  const downloadURL = `${SERVICE_URL_PREFIX}/${SERVICE_VERSION}/${archiveFile}`
  const tempDir = path.join(TEMP_DIR, 'celestial-service-ipc')
  const tempArchive = path.join(tempDir, archiveFile)

  await fsp.mkdir(tempDir, { recursive: true })
  await fsp.mkdir(SERVICE_DIR, { recursive: true })

  try {
    await downloadFile(downloadURL, tempArchive)

    if (platform === 'win32') {
      const zip = new AdmZip(tempArchive)
      zip
        .getEntries()
        .forEach((entry) =>
          log_debug('"celestial-service-ipc" entry:', entry.entryName),
        )
      zip.extractAllTo(tempDir, true)
    } else {
      await extract({ cwd: tempDir, file: tempArchive })
    }

    for (const { sourceFile, targetFile, targetPath } of files) {
      const extractedFile = await findExtractedFile(tempDir, sourceFile)
      if (!extractedFile) {
        throw new Error(`Expected binary ${sourceFile} not found in archive`)
      }

      await fsp.copyFile(extractedFile, targetPath)
      if (platform !== 'win32') await fsp.chmod(targetPath, 0o755)
      await updateHashCache(targetPath)
      log_success(`Extracted service file: ${targetFile}`)
    }

    log_success(`service bundle finished: ${archiveFile}`)
  } finally {
    await fsp.rm(tempDir, { recursive: true, force: true })
  }
}

const resolveMmdb = () =>
  resolveResource({
    file: 'Country.mmdb',
    downloadURL: `https://github.com/MetaCubeX/meta-rules-dat/releases/download/latest/country.mmdb`,
  })
const resolveGeosite = () =>
  resolveResource({
    file: 'geosite.dat',
    downloadURL: `https://github.com/MetaCubeX/meta-rules-dat/releases/download/latest/geosite.dat`,
  })
const resolveGeoIP = () =>
  resolveResource({
    file: 'geoip.dat',
    downloadURL: `https://github.com/MetaCubeX/meta-rules-dat/releases/download/latest/geoip.dat`,
  })
const resolveEnableLoopback = () =>
  resolveResource({
    file: 'enableLoopback.exe',
    downloadURL: `https://github.com/Kuingsmile/uwp-tool/releases/download/latest/enableLoopback.exe`,
  })

const resolveSetDnsScript = () =>
  resolveResource({
    file: 'set_dns.sh',
    localPath: path.join(cwd, 'scripts/set_dns.sh'),
  })
const resolveUnSetDnsScript = () =>
  resolveResource({
    file: 'unset_dns.sh',
    localPath: path.join(cwd, 'scripts/unset_dns.sh'),
  })

// =======================
// Tasks
// =======================
const tasks = [
  {
    name: 'celestial-mihomo-alpha',
    func: () =>
      getLatestAlphaVersion().then(() => resolveSidecar(clashMetaAlpha())),
    retry: 5,
  },
  {
    name: 'celestial-mihomo',
    func: () =>
      getLatestReleaseVersion().then(() => resolveSidecar(clashMeta())),
    retry: 5,
  },
  {
    name: 'celestial-xray',
    func: () =>
      getLatestXrayVersion()
        .then(() => resolveSidecar(xrayCore()))
        .then(() => checkXrayXhttpFieldNames()),
    retry: 5,
  },
  { name: 'plugin', func: resolvePlugin, retry: 5, winOnly: true },
  { name: 'service', func: resolveServiceBundle, retry: 5 },
  { name: 'mmdb', func: resolveMmdb, retry: 5 },
  { name: 'geosite', func: resolveGeosite, retry: 5 },
  { name: 'geoip', func: resolveGeoIP, retry: 5 },
  {
    name: 'enableLoopback',
    func: resolveEnableLoopback,
    retry: 5,
    winOnly: true,
  },
  {
    name: 'service_chmod',
    func: resolveServicePermission,
    retry: 5,
    unixOnly: platform === 'linux' || platform === 'darwin',
  },
  {
    name: 'set_dns_script',
    func: resolveSetDnsScript,
    retry: 5,
    macosOnly: true,
  },
  {
    name: 'unset_dns_script',
    func: resolveUnSetDnsScript,
    retry: 5,
    macosOnly: true,
  },
]

async function runTask() {
  const task = tasks.shift()
  if (!task) return
  if (task.unixOnly && platform === 'win32') return runTask()
  if (task.winOnly && platform !== 'win32') return runTask()
  if (task.macosOnly && platform !== 'darwin') return runTask()
  if (task.linuxOnly && platform !== 'linux') return runTask()

  for (let i = 0; i < task.retry; i++) {
    try {
      await task.func()
      break
    } catch (err) {
      log_error(`task::${task.name} try ${i} ==`, err.message)
      if (i === task.retry - 1) throw err
    }
  }
  return runTask()
}

runTask()
