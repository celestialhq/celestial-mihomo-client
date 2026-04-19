import { readFileSync } from 'fs'

import axios from 'axios'

import { log_error, log_info, log_success } from './utils.mjs'

// Set these to your own Telegram channel usernames or chat IDs.
// Stable releases go to CHANNEL_RELEASE, autobuild (dev) builds go to CHANNEL_DEV.
const CHANNEL_RELEASE = process.env.TG_CHANNEL_RELEASE || '@celestial_releases'
const CHANNEL_DEV = process.env.TG_CHANNEL_DEV || '@celestial_ddev'

async function sendTelegramNotification() {
  if (!process.env.TELEGRAM_BOT_TOKEN) {
    throw new Error('TELEGRAM_BOT_TOKEN environment variable is required')
  }

  const version =
    process.env.VERSION ||
    (() => {
      const pkg = readFileSync('package.json', 'utf-8')
      return JSON.parse(pkg).version
    })()

  const downloadUrl =
    process.env.DOWNLOAD_URL ||
    `https://github.com/pius-pp/celestial-mihomo-client/releases/download/v${version}`

  const isAutobuild =
    process.env.BUILD_TYPE === 'autobuild' || version.includes('autobuild')

  const chatId = isAutobuild ? CHANNEL_DEV : CHANNEL_RELEASE
  const buildLabel = isAutobuild ? 'Development Build' : 'Stable Release'
  const releaseTag = isAutobuild ? 'autobuild' : `v${version}`

  log_info(`Build type   : ${buildLabel}`)
  log_info(`Version      : ${version}`)
  log_info(`Target channel: ${chatId}`)
  log_info(`Download URL : ${downloadUrl}`)

  // Read the pre-generated release body produced by the workflow
  let releaseContent = ''
  try {
    releaseContent = readFileSync('release.txt', 'utf-8')
    log_info('release.txt loaded successfully')
  } catch {
    log_error('Could not read release.txt — using fallback message')
    releaseContent = 'New features and fixes are available. See the release page for details.'
  }

  // ── Markdown → Telegram HTML ─────────────────────────────────────────────
  function cleanHeading(text) {
    return text
      .replace(/<\/?[^>]+>/g, '')
      .replace(/\*\*/g, '')
      .trim()
  }

  function convertMarkdownToTelegramHTML(content) {
    return content
      .split('\n')
      .map((line) => {
        if (line.trim().length === 0) return ''

        if (line.startsWith('#### ')) return `<b>${cleanHeading(line.slice(5))}</b>`
        if (line.startsWith('### '))  return `<b>${cleanHeading(line.slice(4))}</b>`
        if (line.startsWith('## '))   return `<b>${cleanHeading(line.slice(3))}</b>`

        // Convert [text](url) links
        let out = line.replace(
          /\[([^\]]+)\]\(([^)]+)\)/g,
          (_, text, url) => `<a href="${encodeURI(url)}">${text}</a>`,
        )
        // Convert **bold**
        out = out.replace(/\*\*([^*]+)\*\*/g, '<b>$1</b>')
        // Strip blockquote markers (GitHub > [!NOTE] etc.)
        out = out.replace(/^>\s*\[!.*?\]\s*/i, '').replace(/^>\s*/, '')
        return out
      })
      .join('\n')
  }

  function normalizeDetailsTags(content) {
    return content
      .replace(
        /<summary>\s*<strong>\s*(.*?)\s*<\/strong>\s*<\/summary>/g,
        '\n<b>$1</b>\n',
      )
      .replace(/<summary>\s*(.*?)\s*<\/summary>/g, '\n<b>$1</b>\n')
      .replace(/<\/?details>/g, '')
      .replace(/<\/?strong>/g, (m) => (m === '</strong>' ? '</b>' : '<b>'))
      .replace(/<br\s*\/?>/g, '\n')
  }

  // Strip HTML tags Telegram does not support, escape everything else
  function sanitizeTelegramHTML(content) {
    const allowed =
      /^\/?(b|strong|i|em|u|ins|s|strike|del|a|code|pre|blockquote|tg-spoiler|tg-emoji)(\s|>|$)/i
    return content.replace(/<\/?[^>]*>/g, (tag) => {
      const inner = tag.replace(/^<\/?/, '').replace(/>$/, '')
      if (allowed.test(inner) || allowed.test(tag.slice(1))) return tag
      return tag.replace(/</g, '&lt;').replace(/>/g, '&gt;')
    })
  }

  releaseContent = normalizeDetailsTags(releaseContent)
  const body = sanitizeTelegramHTML(convertMarkdownToTelegramHTML(releaseContent))

  // ── Build the full message ────────────────────────────────────────────────
  const releaseUrl = `https://github.com/pius-pp/celestial-mihomo-client/releases/tag/${releaseTag}`
  const header = isAutobuild
    ? `🔧 <b><a href="${releaseUrl}">Celestial Dev Build</a> — ${version}</b>`
    : `🎉 <b><a href="${releaseUrl}">Celestial ${releaseTag}</a> — ${buildLabel}</b>`

  const message = `${header}\n\n${body}`

  // ── Send ─────────────────────────────────────────────────────────────────
  try {
    await axios.post(
      `https://api.telegram.org/bot${process.env.TELEGRAM_BOT_TOKEN}/sendMessage`,
      {
        chat_id: chatId,
        text: message,
        parse_mode: 'HTML',
        link_preview_options: {
          is_disabled: false,
          url: releaseUrl,
          prefer_large_media: true,
        },
      },
    )
    log_success(`Telegram notification sent to ${chatId}`)
  } catch (error) {
    log_error(
      `Failed to send Telegram notification to ${chatId}:`,
      error.response?.data || error.message,
      error,
    )
    process.exit(1)
  }
}

sendTelegramNotification().catch((error) => {
  log_error('Script failed:', error)
  process.exit(1)
})
