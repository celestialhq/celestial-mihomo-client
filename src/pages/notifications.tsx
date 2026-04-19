import {
  CheckCircleRounded,
  InfoRounded,
  NotificationsActiveRounded,
  OpenInNewRounded,
  PriorityHighRounded,
  RefreshRounded,
  WarningAmberRounded,
} from '@mui/icons-material'
import {
  Alert,
  Box,
  Button,
  Chip,
  CircularProgress,
  IconButton,
  Paper,
  Stack,
  Tooltip,
  Typography,
  alpha,
} from '@mui/material'
import dayjs from 'dayjs'
import { useSyncExternalStore } from 'react'
import type { ReactElement } from 'react'
import { useTranslation } from 'react-i18next'

import { BasePage } from '@/components/base'
import { openWebUrl } from '@/services/cmds'
import {
  getRemoteNotificationsSnapshot,
  refreshRemoteNotifications,
  subscribeRemoteNotifications,
  type RemoteNotificationItem,
  type RemoteNotificationStatus,
} from '@/services/remote-notifications'

const statusIcon: Record<RemoteNotificationStatus, ReactElement> = {
  urgent: <PriorityHighRounded fontSize="small" />,
  warning: <WarningAmberRounded fontSize="small" />,
  success: <CheckCircleRounded fontSize="small" />,
  info: <InfoRounded fontSize="small" />,
}

const statusColor: Record<
  RemoteNotificationStatus,
  'default' | 'primary' | 'success' | 'warning' | 'error'
> = {
  urgent: 'error',
  warning: 'warning',
  success: 'success',
  info: 'primary',
}

const statusLabelKey: Record<RemoteNotificationStatus, string> = {
  urgent: 'notifications.status.urgent',
  warning: 'notifications.status.warning',
  success: 'notifications.status.success',
  info: 'notifications.status.info',
}

const NotificationCard = ({ item }: { item: RemoteNotificationItem }) => {
  const { t } = useTranslation()

  return (
    <Paper
      elevation={0}
      sx={(theme) => ({
        p: 2,
        borderRadius: 2,
        border: `1px solid ${alpha(theme.palette.divider, 0.8)}`,
        bgcolor:
          item.status === 'urgent'
            ? alpha(theme.palette.error.main, 0.08)
            : theme.palette.background.paper,
      })}
    >
      <Stack spacing={1.25}>
        <Stack
          direction="row"
          spacing={1}
          sx={{ alignItems: 'center', justifyContent: 'space-between' }}
        >
          <Chip
            size="small"
            icon={statusIcon[item.status]}
            color={statusColor[item.status]}
            label={t(statusLabelKey[item.status])}
            sx={{ fontWeight: 700 }}
          />
          <Typography variant="caption" color="text.secondary">
            {dayjs(item.createdAt).format('YYYY-MM-DD HH:mm')}
          </Typography>
        </Stack>

        <Typography variant="h6" sx={{ fontSize: 17, fontWeight: 800 }}>
          {item.title}
        </Typography>
        {item.body && (
          <Typography
            variant="body2"
            color="text.secondary"
            sx={{ whiteSpace: 'pre-wrap', lineHeight: 1.6 }}
          >
            {item.body}
          </Typography>
        )}

        {item.link && (
          <Box>
            <Button
              size="small"
              startIcon={<OpenInNewRounded />}
              onClick={() => {
                if (item.link) void openWebUrl(item.link)
              }}
            >
              {t('notifications.actions.openLink')}
            </Button>
          </Box>
        )}
      </Stack>
    </Paper>
  )
}

const NotificationsPage = () => {
  const { t } = useTranslation()
  const snapshot = useSyncExternalStore(
    subscribeRemoteNotifications,
    getRemoteNotificationsSnapshot,
  )

  const hasNotifications = snapshot.notifications.length > 0

  return (
    <BasePage
      title={t('notifications.page.title')}
      contentStyle={{ padding: 16 }}
      header={
        <Stack direction="row" spacing={1} sx={{ alignItems: 'center' }}>
          {snapshot.loading && <CircularProgress size={18} />}
          <Tooltip title={t('shared.actions.refresh')} arrow>
            <IconButton
              size="small"
              color="inherit"
              onClick={() => void refreshRemoteNotifications()}
            >
              <RefreshRounded />
            </IconButton>
          </Tooltip>
        </Stack>
      }
    >
      <Stack spacing={2}>
        {!snapshot.available && (
          <Alert icon={<NotificationsActiveRounded />} severity="info">
            {t('notifications.page.loading')}
          </Alert>
        )}

        {snapshot.available && !hasNotifications && (
          <Paper
            elevation={0}
            sx={(theme) => ({
              p: 4,
              borderRadius: 2,
              textAlign: 'center',
              border: `1px dashed ${theme.palette.divider}`,
            })}
          >
            <NotificationsActiveRounded
              color="disabled"
              sx={{ fontSize: 42 }}
            />
            <Typography sx={{ mt: 1, fontWeight: 700 }}>
              {t('notifications.page.empty')}
            </Typography>
          </Paper>
        )}

        {snapshot.generatedAt && (
          <Typography variant="caption" color="text.secondary">
            {t('notifications.page.generatedAt', {
              time: dayjs(snapshot.generatedAt).format('YYYY-MM-DD HH:mm'),
            })}
          </Typography>
        )}

        {snapshot.notifications.map((item) => (
          <NotificationCard
            key={`${item.id}-${item.updatedAt ?? item.createdAt}`}
            item={item}
          />
        ))}
      </Stack>
    </BasePage>
  )
}

export default NotificationsPage
