import { fetch } from '@tauri-apps/plugin-http'

const SOCLESTIAL_ORIGIN = 'https://socelestial.com'
const MANAGEMENT_API_PATH = '/api/celestial/subscription-management'

export interface SubscriptionManagementEligibility {
  origin: string
  shortUuid: string
}

export interface CelestialSubscriptionUser {
  uuid: string
  shortUuid: string
  username: string
  status: 'ACTIVE' | 'DISABLED' | 'LIMITED' | 'EXPIRED' | string
  expireAt?: string | null
  expiresAt?: string | null
  daysLeft?: number
  trafficUsed?: string
  trafficLimit?: string
  lifetimeTrafficUsed?: string
  trafficUsedBytes?: string | number
  trafficLimitBytes?: string | number
  lifetimeTrafficUsedBytes?: string | number
  trafficLimitStrategy?: string
  hwidDeviceLimit?: number | null
  userTraffic?: {
    usedTrafficBytes: number
    lifetimeUsedTrafficBytes: number
    onlineAt: string | null
    firstConnectedAt: string | null
    lastConnectedNodeUuid: string | null
  }
}

export interface CelestialSubscriptionDevice {
  hwid: string
  userUuid: string
  platform: string | null
  osVersion: string | null
  deviceModel: string | null
  userAgent: string | null
  createdAt: string
  updatedAt: string
}

export interface CelestialSubscriptionManagement {
  user: CelestialSubscriptionUser
  subscription: {
    isFound: boolean
    subscriptionUrl: string
    links: string[]
    ssConfLinks: Record<string, string>
  }
  devices: {
    total: number
    items: CelestialSubscriptionDevice[]
  }
}

export const getSubscriptionManagementEligibility = (
  profile?: IProfileItem,
): SubscriptionManagementEligibility | null => {
  if (!profile || profile.type !== 'remote' || !profile.url) {
    return null
  }

  let parsedUrl: URL
  try {
    parsedUrl = new URL(profile.url)
  } catch {
    return null
  }

  if (parsedUrl.origin !== SOCLESTIAL_ORIGIN) {
    return null
  }

  const shortUuid = extractShortUuid(parsedUrl)

  if (!shortUuid) {
    return null
  }

  return {
    origin: parsedUrl.origin,
    shortUuid,
  }
}

export const getSubscriptionManagement = async ({
  origin,
  shortUuid,
}: SubscriptionManagementEligibility) => {
  return requestSubscriptionManagement<CelestialSubscriptionManagement>(
    origin,
    `/${encodeURIComponent(shortUuid)}`,
  )
}

export const deleteSubscriptionDevice = async (
  { origin, shortUuid }: SubscriptionManagementEligibility,
  hwid: string,
) => {
  return requestSubscriptionManagement<{ isDeleted: boolean }>(
    origin,
    `/${encodeURIComponent(shortUuid)}/devices/delete`,
    {
      method: 'POST',
      body: JSON.stringify({ hwid }),
    },
  )
}

export const deleteAllSubscriptionDevices = async ({
  origin,
  shortUuid,
}: SubscriptionManagementEligibility) => {
  return requestSubscriptionManagement<{ isDeleted: boolean }>(
    origin,
    `/${encodeURIComponent(shortUuid)}/devices/delete-all`,
    {
      method: 'POST',
    },
  )
}

const requestSubscriptionManagement = async <T>(
  origin: string,
  path: string,
  init?: RequestInit,
) => {
  const response = await fetch(`${origin}${MANAGEMENT_API_PATH}${path}`, {
    method: init?.method ?? 'GET',
    headers: {
      Accept: 'application/json',
      ...(init?.body ? { 'Content-Type': 'application/json' } : {}),
    },
    body: init?.body,
  })

  if (!response.ok) {
    const message = await readErrorMessage(response)
    throw new Error(message || `HTTP ${response.status}`)
  }

  return response.json() as Promise<T>
}

const readErrorMessage = async (response: Response) => {
  try {
    const payload = await response.json()
    return (
      payload?.message ||
      payload?.error ||
      payload?.errorCode ||
      response.statusText
    )
  } catch {
    return response.statusText
  }
}

const extractShortUuid = (url: URL) => {
  const queryKeys = ['shortUuid', 'short_uuid', 'token', 'sub']

  for (const key of queryKeys) {
    const value = url.searchParams.get(key)
    if (value) {
      return sanitizeShortUuid(value)
    }
  }

  const segments = url.pathname.split('/').filter(Boolean)

  for (let index = segments.length - 1; index >= 0; index -= 1) {
    const segment = sanitizeShortUuid(segments[index])
    if (
      segment &&
      !['sub', 'subscription', 'api', 'mihomo', 'clash'].includes(segment)
    ) {
      return segment
    }
  }

  return null
}

const sanitizeShortUuid = (value: string) => {
  const trimmed = decodeURIComponent(value).trim()
  const match = trimmed.match(/[a-zA-Z0-9_-]{6,}/)
  return match?.[0] ?? null
}
