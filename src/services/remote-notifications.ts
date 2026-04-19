import { invoke } from '@tauri-apps/api/core'
import { fetch } from '@tauri-apps/plugin-http'

export type RemoteNotificationStatus = 'info' | 'success' | 'warning' | 'urgent'

export interface RemoteNotificationContent {
  title: string
  body: string
}

export interface RemoteNotificationItem extends RemoteNotificationContent {
  id: string
  status: RemoteNotificationStatus
  createdAt: string
  updatedAt?: string
  expiresAt?: string
  link?: string
}

export interface RemoteNotificationSnapshot {
  available: boolean
  loading: boolean
  notifications: RemoteNotificationItem[]
  generatedAt?: string
  lastCheckedAt?: string
}

type RawNotificationPayload = {
  schemaVersion?: unknown
  generatedAt?: unknown
  notifications?: unknown
}

type RawNotificationItem = {
  id?: unknown
  status?: unknown
  title?: unknown
  body?: unknown
  createdAt?: unknown
  updatedAt?: unknown
  expiresAt?: unknown
  link?: unknown
  locale?: unknown
}

type Subscriber = () => void

const NOTIFY_URL =
  'https://celestialreleases-555951992191-us-east-1-an.s3.us-east-1.amazonaws.com/notify.json'
const POLL_INTERVAL_MS = 60_000
const URGENT_STORAGE_KEY = 'celestial.remoteNotifications.urgentNotified'

let snapshot: RemoteNotificationSnapshot = {
  available: false,
  loading: true,
  notifications: [],
}
let pollingStarted = false
let pollingInFlight = false
let intervalId: ReturnType<typeof setInterval> | undefined

const subscribers = new Set<Subscriber>()

const notifySubscribers = () => {
  subscribers.forEach((subscriber) => {
    subscriber()
  })
}

const setSnapshot = (next: RemoteNotificationSnapshot) => {
  snapshot = next
  notifySubscribers()
}

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value)

const readString = (value: unknown): string | undefined =>
  typeof value === 'string' && value.trim() ? value.trim() : undefined

const normalizeStatus = (status: unknown): RemoteNotificationStatus => {
  if (status === 'urgent') return 'urgent'
  if (status === 'warning') return 'warning'
  if (status === 'success') return 'success'
  return 'info'
}

const currentLocaleCandidates = () => {
  const locale = navigator.language.toLowerCase()
  const short = locale.split('-')[0]
  return [locale, short, 'en', 'ru']
}

const resolveLocalizedContent = (
  item: RawNotificationItem,
): RemoteNotificationContent => {
  if (isRecord(item.locale)) {
    for (const locale of currentLocaleCandidates()) {
      const localized = item.locale[locale]
      if (!isRecord(localized)) continue

      const title = readString(localized.title)
      const body = readString(localized.body)
      if (title || body) {
        return {
          title: title ?? readString(item.title) ?? 'Celestial',
          body: body ?? readString(item.body) ?? '',
        }
      }
    }
  }

  return {
    title: readString(item.title) ?? 'Celestial',
    body: readString(item.body) ?? '',
  }
}

const normalizeNotification = (
  item: unknown,
  index: number,
  generatedAt?: string,
): RemoteNotificationItem | null => {
  if (!isRecord(item)) return null

  const raw = item as RawNotificationItem
  const content = resolveLocalizedContent(raw)
  const createdAt =
    readString(raw.createdAt) ??
    readString(raw.updatedAt) ??
    generatedAt ??
    new Date().toISOString()
  const id = readString(raw.id) ?? `${createdAt}-${index}`
  const expiresAt = readString(raw.expiresAt)

  if (expiresAt && Number.isFinite(Date.parse(expiresAt))) {
    if (Date.parse(expiresAt) <= Date.now()) return null
  }

  return {
    id,
    status: normalizeStatus(raw.status),
    title: content.title,
    body: content.body,
    createdAt,
    updatedAt: readString(raw.updatedAt),
    expiresAt,
    link: readString(raw.link),
  }
}

const normalizePayload = (
  rawPayload: unknown,
): {
  generatedAt?: string
  notifications: RemoteNotificationItem[]
} => {
  const payload: RawNotificationPayload = Array.isArray(rawPayload)
    ? { notifications: rawPayload }
    : isRecord(rawPayload)
      ? rawPayload
      : {}

  const generatedAt = readString(payload.generatedAt)
  const rawNotifications = Array.isArray(payload.notifications)
    ? payload.notifications
    : []

  return {
    generatedAt,
    notifications: rawNotifications
      .map((item, index) => normalizeNotification(item, index, generatedAt))
      .filter((item): item is RemoteNotificationItem => Boolean(item))
      .sort((a, b) => Date.parse(b.createdAt) - Date.parse(a.createdAt)),
  }
}

const readNotifiedUrgentKeys = () => {
  try {
    const parsed = JSON.parse(localStorage.getItem(URGENT_STORAGE_KEY) ?? '[]')
    const keys = Array.isArray(parsed)
      ? parsed.filter((item): item is string => typeof item === 'string')
      : []
    return new Set(keys)
  } catch {
    return new Set<string>()
  }
}

const writeNotifiedUrgentKeys = (keys: Set<string>) => {
  localStorage.setItem(
    URGENT_STORAGE_KEY,
    JSON.stringify([...keys].slice(-200)),
  )
}

const urgentNotificationKey = (item: RemoteNotificationItem) =>
  `${item.id}:${item.updatedAt ?? item.createdAt}`

const notifyUrgentItems = async (items: RemoteNotificationItem[]) => {
  const notifiedKeys = readNotifiedUrgentKeys()
  let changed = false

  for (const item of items) {
    if (item.status !== 'urgent') continue

    const key = urgentNotificationKey(item)
    if (notifiedKeys.has(key)) continue

    notifiedKeys.add(key)
    changed = true

    try {
      await invoke('show_remote_notification', {
        title: item.title,
        body: item.body,
      })
    } catch (error) {
      console.warn('[RemoteNotifications] urgent push failed:', error)
    }
  }

  if (changed) {
    writeNotifiedUrgentKeys(notifiedKeys)
  }
}

export const refreshRemoteNotifications = async () => {
  if (pollingInFlight) return
  pollingInFlight = true

  setSnapshot({
    ...snapshot,
    loading: true,
    lastCheckedAt: new Date().toISOString(),
  })

  try {
    const response = await fetch(NOTIFY_URL, {
      method: 'GET',
      cache: 'no-store',
    })

    if (!response.ok) {
      throw new Error(`notify.json returned ${response.status}`)
    }

    const text = await response.text()
    const rawPayload = text.trim() ? JSON.parse(text) : {}
    const payload = normalizePayload(rawPayload)
    const nextSnapshot: RemoteNotificationSnapshot = {
      available: true,
      loading: false,
      lastCheckedAt: new Date().toISOString(),
      ...payload,
    }

    setSnapshot(nextSnapshot)
    await notifyUrgentItems(nextSnapshot.notifications)
  } catch (error) {
    console.warn('[RemoteNotifications] waiting for notify.json:', error)
    setSnapshot({
      ...snapshot,
      available: false,
      loading: true,
      lastCheckedAt: new Date().toISOString(),
    })
  } finally {
    pollingInFlight = false
  }
}

export const ensureRemoteNotificationsPolling = () => {
  if (pollingStarted) return
  pollingStarted = true
  void refreshRemoteNotifications()
  intervalId = setInterval(() => {
    void refreshRemoteNotifications()
  }, POLL_INTERVAL_MS)
}

export const stopRemoteNotificationsPolling = () => {
  if (intervalId) clearInterval(intervalId)
  intervalId = undefined
  pollingStarted = false
}

export const subscribeRemoteNotifications = (subscriber: Subscriber) => {
  subscribers.add(subscriber)
  return () => {
    subscribers.delete(subscriber)
  }
}

export const getRemoteNotificationsSnapshot = () => snapshot
