import { useLocalStorage } from 'foxact/use-local-storage'

const defaultClashLog: IClashLog = {
  enable: true,
  logLevel: 'INFO',
  logFilter: 'all',
  logOrder: 'asc',
}

const normalizeLogLevel = (value: unknown): LogLevel => {
  const normalized = typeof value === 'string' ? value.toUpperCase() : ''

  switch (normalized) {
    case 'DEBUG':
    case 'INFO':
    case 'WARNING':
    case 'ERROR':
    case 'SILENT':
      return normalized
    default:
      return defaultClashLog.logLevel
  }
}

const deserializeClashLog = (value: string): IClashLog => {
  const parsed = JSON.parse(value) as Partial<IClashLog>

  return {
    ...defaultClashLog,
    ...parsed,
    logLevel: normalizeLogLevel(parsed.logLevel),
  }
}

export const useClashLog = () =>
  useLocalStorage<IClashLog>('clash-log', defaultClashLog, {
    serializer: JSON.stringify,
    deserializer: deserializeClashLog,
  })
