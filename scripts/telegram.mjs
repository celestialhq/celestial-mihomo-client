import { readFileSync } from 'fs'

import axios from 'axios'

import { log_error, log_info, log_success } from './utils.mjs'

const CHAT_ID_RELEASE =
  process.env.TELEGRAM_RELEASE_CHAT_ID || '@celestial_releases'
const CHAT_ID_TEST = process.env.TELEGRAM_TEST_CHAT_ID || '@celestial_ddev'

async function sendTelegramNotification() {
  if (!process.env.TELEGRAM_BOT_TOKEN) {
    throw new Error('TELEGRAM_BOT_TOKEN is required')
  }

  const version =
    process.env.VERSION ||
    (() => {
      const pkg = readFileSync('package.json', 'utf-8')
      return JSON.parse(pkg).version
    })()

  const downloadUrl =
    process.env.DOWNLOAD_URL ||
    `https://github.com/${process.env.PUBLIC_RELEASE_REPO || 'pius-pp/celestial-mihomo-client-public'}/releases/download/v${version}`

  const isAutobuild =
    process.env.BUILD_TYPE === 'autobuild' || version.includes('autobuild')
  const chatId = isAutobuild ? CHAT_ID_TEST : CHAT_ID_RELEASE
  const buildType = isAutobuild ? 'автосборка' : 'стабильный релиз'

  log_info(`Preparing Telegram notification for ${buildType} ${version}`)
  log_info(`Target channel: ${chatId}`)
  log_info(`Download URL: ${downloadUrl}`)

  // Чтение описания релиза
  let releaseContent
  try {
    releaseContent = readFileSync('release.txt', 'utf-8')
    log_info('Successfully read release.txt file')
  } catch (error) {
    log_error('Failed to read release.txt, using default release notes', error)
    releaseContent =
      'В этой версии добавлены новые возможности и улучшения. Подробный список изменений доступен на странице релиза.'
  }

  // Преобразование Markdown в HTML для Telegram
  function convertMarkdownToTelegramHTML(content) {
    const cleanHeading = (text) =>
      text
        .replace(/<\/?[^>]+>/g, '')
        .replace(/\*\*/g, '')
        .trim()

    return content
      .split('\n')
      .map((line) => {
        if (line.trim().length === 0) {
          return ''
        } else if (line.startsWith('## ')) {
          return `<b>${cleanHeading(line.replace('## ', ''))}</b>`
        } else if (line.startsWith('### ')) {
          return `<b>${cleanHeading(line.replace('### ', ''))}</b>`
        } else if (line.startsWith('#### ')) {
          return `<b>${cleanHeading(line.replace('#### ', ''))}</b>`
        } else {
          let processedLine = line.replace(
            /\[([^\]]+)\]\(([^)]+)\)/g,
            (match, text, url) => {
              const encodedUrl = encodeURI(url)
              return `<a href="${encodedUrl}">${text}</a>`
            },
          )
          processedLine = processedLine.replace(/\*\*([^*]+)\*\*/g, '<b>$1</b>')
          return processedLine
        }
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

  // Удаление HTML-тегов, не поддерживаемых Telegram
  function sanitizeTelegramHTML(content) {
    // Telegram supports: b, strong, i, em, u, ins, s, strike, del,
    // a, code, pre, blockquote, tg-spoiler, tg-emoji
    const allowedTags =
      /^\/?(b|strong|i|em|u|ins|s|strike|del|a|code|pre|blockquote|tg-spoiler|tg-emoji)(\s|>|$)/i

    return content.replace(/<\/?[^>]*>/g, (tag) => {
      const inner = tag.replace(/^<\/?/, '').replace(/>$/, '')
      if (allowedTags.test(inner) || allowedTags.test(tag.slice(1))) {
        return tag
      }
      return tag.replace(/</g, '&lt;').replace(/>/g, '&gt;')
    })
  }

  releaseContent = normalizeDetailsTags(releaseContent)
  const formattedContent = sanitizeTelegramHTML(
    convertMarkdownToTelegramHTML(releaseContent),
  )

  const releaseTitle = isAutobuild
    ? 'вышла новая автосборка'
    : 'вышел новый стабильный релиз'
  const encodedVersion = encodeURIComponent(version)
  const releaseTag = isAutobuild ? 'autobuild' : `v${version}`
  const publicReleaseRepo =
    process.env.PUBLIC_RELEASE_REPO || 'pius-pp/celestial-mihomo-client-public'
  const releasePageUrl = `https://github.com/${publicReleaseRepo}/releases/tag/${releaseTag}`

  const content = `<b>🎉 <a href="${releasePageUrl}">Celestial v${version}</a> — ${releaseTitle}</b>\n\n${formattedContent}`

  // Отправка сообщения в Telegram
  try {
    await axios.post(
      `https://api.telegram.org/bot${process.env.TELEGRAM_BOT_TOKEN}/sendMessage`,
      {
        chat_id: chatId,
        text: content,
        link_preview_options: {
          is_disabled: false,
          url: `https://github.com/${publicReleaseRepo}/releases/tag/v${encodedVersion}`,
          prefer_large_media: true,
        },
        parse_mode: 'HTML',
      },
    )
    log_success(`Telegram notification sent successfully to ${chatId}`)
  } catch (error) {
    log_error(
      `Telegram notification failed for ${chatId}:`,
      error.response?.data || error.message,
      error,
    )
    process.exit(1)
  }
}

// Запуск
sendTelegramNotification().catch((error) => {
  log_error('Script execution failed:', error)
  process.exit(1)
})
