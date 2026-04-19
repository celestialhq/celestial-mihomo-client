import {
  CalendarMonthRounded,
  DeleteSweepRounded,
  DevicesRounded,
  EventAvailableRounded,
  InfoRounded,
  RefreshRounded,
  RouterRounded,
  SpeedRounded,
} from '@mui/icons-material'
import { LoadingButton } from '@mui/lab'
import {
  Alert,
  Box,
  Chip,
  type ChipProps,
  CircularProgress,
  Divider,
  Grid,
  IconButton,
  LinearProgress,
  Paper,
  Stack,
  Tooltip,
  Typography,
} from '@mui/material'
import { useMutation, useQuery } from '@tanstack/react-query'
import dayjs from 'dayjs'
import type { ReactNode } from 'react'
import { useEffect, useMemo } from 'react'
import { useNavigate } from 'react-router'

import { BasePage } from '@/components/base'
import { useProfiles } from '@/hooks/use-profiles'
import { showNotice } from '@/services/notice-service'
import {
  deleteAllSubscriptionDevices,
  deleteSubscriptionDevice,
  getSubscriptionManagement,
  getSubscriptionManagementEligibility,
  type CelestialSubscriptionDevice,
} from '@/services/subscription-management'

const SubscriptionManagementPage = () => {
  const navigate = useNavigate()
  const { current } = useProfiles()
  const eligibility = useMemo(
    () => getSubscriptionManagementEligibility(current),
    [current],
  )

  const { data, isFetching, isPending, refetch } = useQuery({
    queryKey: ['subscriptionManagement', eligibility?.shortUuid],
    queryFn: () => getSubscriptionManagement(eligibility!),
    enabled: Boolean(eligibility),
    refetchOnWindowFocus: false,
    retry: true,
    retryDelay: 3000,
    refetchInterval: (query) => (query.state.data ? false : 5000),
  })

  const deleteDevice = useMutation({
    mutationFn: (deviceId: string) =>
      deleteSubscriptionDevice(eligibility!, deviceId),
    onSuccess: async () => {
      showNotice.success('Устройство удалено')
      await refetch()
    },
    onError: (mutationError) => showNotice.error(String(mutationError)),
  })

  const deleteAllDevices = useMutation({
    mutationFn: () => deleteAllSubscriptionDevices(eligibility!),
    onSuccess: async () => {
      showNotice.success('Все устройства удалены')
      await refetch()
    },
    onError: (mutationError) => showNotice.error(String(mutationError)),
  })

  useEffect(() => {
    if (current && !eligibility) {
      navigate('/profile', { replace: true })
    }
  }, [current, eligibility, navigate])

  if (!eligibility) {
    return (
      <BasePage title="Управление подпиской">
        <Alert severity="info">
          Страница доступна только для активного Remote-профиля с подпиской
          https://socelestial.com.
        </Alert>
      </BasePage>
    )
  }

  if (isPending || !data) {
    return (
      <BasePage
        full
        title="Управление подпиской"
        contentStyle={{ height: '100%' }}
      >
        <Box
          sx={{
            height: '100%',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            p: 2,
          }}
        >
          <Stack spacing={1.5} sx={{ alignItems: 'center', maxWidth: 360 }}>
            <CircularProgress size={32} />
            <Typography sx={{ fontWeight: 800 }}>Данные загружаются</Typography>
            <Typography
              variant="body2"
              color="text.secondary"
              sx={{ textAlign: 'center' }}
            >
              Проверяем доступность API управления подпиской. Страница откроется
              автоматически, когда сервис ответит.
            </Typography>
          </Stack>
        </Box>
      </BasePage>
    )
  }

  const user = data.user
  const devices = data.devices.items
  const deviceLimit = user.hwidDeviceLimit ?? null
  const trafficUsed = toNumber(user.trafficUsedBytes)
  const trafficLimit = toNumber(user.trafficLimitBytes)
  const trafficProgress =
    trafficLimit > 0 ? Math.min((trafficUsed / trafficLimit) * 100, 100) : 0

  return (
    <BasePage
      full
      title="Управление подпиской"
      contentStyle={{ height: '100%', overflowY: 'auto' }}
      header={
        <Tooltip title="Обновить" arrow>
          <span>
            <IconButton
              size="small"
              color="primary"
              disabled={isFetching}
              onClick={() => void refetch()}
            >
              <RefreshRounded fontSize="small" />
            </IconButton>
          </span>
        </Tooltip>
      }
    >
      <Box sx={{ p: 1.25, pb: 2 }}>
        <Grid container spacing={1.25}>
          <Grid size={{ xs: 12, md: 8 }}>
            <Paper variant="outlined" sx={{ p: 1.5, borderRadius: 2 }}>
              <Stack
                direction="row"
                spacing={1}
                sx={{
                  alignItems: 'flex-start',
                  justifyContent: 'space-between',
                }}
              >
                <Box sx={{ minWidth: 0 }}>
                  <Typography variant="overline" color="text.secondary">
                    Подписка
                  </Typography>
                  <Typography variant="h5" sx={{ fontWeight: 800 }} noWrap>
                    {user.username ?? current?.name ?? 'Celestial'}
                  </Typography>
                </Box>
                <Chip
                  size="small"
                  color={getStatusColor(user.status)}
                  label={translateStatus(user.status)}
                />
              </Stack>

              <Grid container spacing={1} sx={{ mt: 1 }}>
                <Metric
                  icon={<EventAvailableRounded />}
                  label="Осталось"
                  value={formatDaysLeft(user.daysLeft)}
                />
                <Metric
                  icon={<CalendarMonthRounded />}
                  label="Действует до"
                  value={formatDate(user.expireAt ?? user.expiresAt)}
                />
                <Metric
                  icon={<SpeedRounded />}
                  label="Лимит трафика"
                  value={formatBytes(trafficLimit)}
                />
                <Metric
                  icon={<RouterRounded />}
                  label="Устройства"
                  value={`${devices.length}${deviceLimit ? ` / ${deviceLimit}` : ''}`}
                />
              </Grid>

              <Box sx={{ mt: 1.5 }}>
                <Stack
                  direction="row"
                  sx={{
                    alignItems: 'center',
                    justifyContent: 'space-between',
                    mb: 0.5,
                  }}
                >
                  <Typography sx={{ fontWeight: 700 }}>Трафик</Typography>
                  <Typography variant="body2" color="text.secondary">
                    {formatBytes(trafficUsed)} / {formatBytes(trafficLimit)}
                  </Typography>
                </Stack>
                <LinearProgress
                  variant={trafficLimit > 0 ? 'determinate' : 'indeterminate'}
                  value={trafficProgress}
                  sx={{ height: 8, borderRadius: 1 }}
                />
              </Box>
            </Paper>
          </Grid>

          <Grid size={{ xs: 12, md: 4 }}>
            <Paper variant="outlined" sx={{ p: 1.5, borderRadius: 2 }}>
              <Stack direction="row" spacing={1} sx={{ alignItems: 'center' }}>
                <InfoRounded color="primary" />
                <Typography sx={{ fontWeight: 800 }}>Информация</Typography>
              </Stack>
              <Divider sx={{ my: 1 }} />
              <InfoRow label="Short UUID" value={maskSecret(user.shortUuid)} />
              <InfoRow
                label="Стратегия лимита"
                value={user.trafficLimitStrategy ?? '-'}
              />
              <InfoRow
                label="Всего использовано"
                value={formatBytes(toNumber(user.lifetimeTrafficUsedBytes))}
              />
            </Paper>
          </Grid>

          <Grid size={{ xs: 12 }}>
            <Paper variant="outlined" sx={{ p: 1.5, borderRadius: 2 }}>
              <Stack
                direction="row"
                spacing={1}
                sx={{
                  alignItems: 'center',
                  justifyContent: 'space-between',
                  mb: 1,
                }}
              >
                <Stack
                  direction="row"
                  spacing={1}
                  sx={{ alignItems: 'center' }}
                >
                  <DevicesRounded color="primary" />
                  <Box>
                    <Typography sx={{ fontWeight: 800 }}>Устройства</Typography>
                    <Typography variant="body2" color="text.secondary">
                      Управление привязанными устройствами
                    </Typography>
                  </Box>
                </Stack>
                <LoadingButton
                  size="small"
                  color="error"
                  variant="outlined"
                  startIcon={<DeleteSweepRounded />}
                  loading={deleteAllDevices.isPending}
                  disabled={devices.length === 0}
                  onClick={() => deleteAllDevices.mutate()}
                >
                  Очистить
                </LoadingButton>
              </Stack>

              <Stack spacing={1}>
                {devices.length === 0 && (
                  <Alert severity="info">Привязанных устройств пока нет.</Alert>
                )}
                {devices.map((device) => (
                  <DeviceCard
                    key={device.id}
                    device={device}
                    deleting={deleteDevice.isPending}
                    onDelete={() => deleteDevice.mutate(device.id)}
                  />
                ))}
              </Stack>
            </Paper>
          </Grid>
        </Grid>
      </Box>
    </BasePage>
  )
}

interface MetricProps {
  icon: ReactNode
  label: string
  value: string
}

const Metric = ({ icon, label, value }: MetricProps) => (
  <Grid size={{ xs: 6, sm: 3 }}>
    <Box
      sx={(theme) => ({
        p: 1,
        minHeight: 86,
        borderRadius: 1.5,
        border: `1px solid ${theme.palette.divider}`,
      })}
    >
      <Stack direction="row" spacing={0.75} sx={{ alignItems: 'center' }}>
        <Box sx={{ display: 'flex', color: 'primary.main' }}>{icon}</Box>
        <Typography variant="body2" color="text.secondary" noWrap>
          {label}
        </Typography>
      </Stack>
      <Typography sx={{ mt: 0.75, fontWeight: 800 }} noWrap title={value}>
        {value}
      </Typography>
    </Box>
  </Grid>
)

const InfoRow = ({ label, value }: { label: string; value: string }) => (
  <Stack
    direction="row"
    spacing={1}
    sx={{ justifyContent: 'space-between', py: 0.5 }}
  >
    <Typography variant="body2" color="text.secondary">
      {label}
    </Typography>
    <Typography variant="body2" sx={{ fontWeight: 700 }} noWrap title={value}>
      {value}
    </Typography>
  </Stack>
)

interface DeviceCardProps {
  device: CelestialSubscriptionDevice
  deleting: boolean
  onDelete: () => void
}

const DeviceCard = ({ device, deleting, onDelete }: DeviceCardProps) => {
  const title = [device.platform, device.deviceModel]
    .filter(Boolean)
    .join(' / ')

  return (
    <Box
      sx={(theme) => ({
        p: 1,
        borderRadius: 1.5,
        border: `1px solid ${theme.palette.divider}`,
      })}
    >
      <Stack
        direction="row"
        spacing={1}
        sx={{ justifyContent: 'space-between' }}
      >
        <Box sx={{ minWidth: 0 }}>
          <Typography
            sx={{ fontWeight: 800 }}
            noWrap
            title={title || device.hwidMasked}
          >
            {title || 'Неизвестное устройство'}
          </Typography>
          <Typography
            variant="body2"
            color="text.secondary"
            noWrap
            title={device.hwidMasked}
          >
            {device.hwidMasked}
          </Typography>
          <Typography variant="caption" color="text.secondary">
            Последняя активность: {formatDateTime(device.updatedAt)}
          </Typography>
        </Box>
        <LoadingButton
          size="small"
          color="error"
          variant="text"
          loading={deleting}
          onClick={onDelete}
        >
          Удалить
        </LoadingButton>
      </Stack>
    </Box>
  )
}

const getStatusColor = (status?: string): ChipProps['color'] => {
  if (status === 'ACTIVE') return 'success'
  if (status === 'LIMITED') return 'warning'
  if (status === 'DISABLED' || status === 'EXPIRED') return 'error'
  return 'default'
}

const translateStatus = (status?: string) => {
  const statuses: Record<string, string> = {
    ACTIVE: 'Активна',
    LIMITED: 'Лимит',
    DISABLED: 'Отключена',
    EXPIRED: 'Истекла',
  }

  return statuses[status ?? ''] ?? 'Неизвестно'
}

const formatDaysLeft = (daysLeft?: number) => {
  if (daysLeft == null) return '-'
  if (daysLeft <= 0) return '0 дней'
  return `${Math.floor(daysLeft)} дней`
}

const formatDate = (value?: string | null) => {
  if (!value) return '-'
  return dayjs(value).format('DD.MM.YYYY')
}

const formatDateTime = (value?: string | null) => {
  if (!value) return '-'
  return dayjs(value).format('DD.MM.YYYY HH:mm')
}

const toNumber = (value?: string | number | null) => {
  if (typeof value === 'number') return value
  if (!value) return 0
  const parsed = Number(value)
  return Number.isFinite(parsed) ? parsed : 0
}

const formatBytes = (value: number) => {
  if (!value) return '-'

  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  let size = value
  let unitIndex = 0

  while (size >= 1024 && unitIndex < units.length - 1) {
    size /= 1024
    unitIndex += 1
  }

  return `${size.toFixed(size >= 10 || unitIndex === 0 ? 0 : 1)} ${units[unitIndex]}`
}

const maskSecret = (value: string) => {
  if (value.length <= 8) return '••••'
  return `${value.slice(0, 3)}••••${value.slice(-3)}`
}

export default SubscriptionManagementPage
