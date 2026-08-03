import { PowerSettingsNewRounded } from '@mui/icons-material'
import { Box, CircularProgress } from '@mui/material'

import { useSimpleConnection } from '@/hooks/use-simple-connection'

export const SimpleConnectButton = () => {
  const { connected, pending, toggleConnection } = useSimpleConnection()

  return (
    <Box
      component="button"
      className={`simple-connect-button${connected ? ' is-connected' : ''}`}
      onClick={toggleConnection}
      type="button"
      aria-label={connected ? 'Отключить VPN' : 'Подключить VPN'}
    >
      <span className="simple-connect-button__content">
        {pending ? (
          <CircularProgress size={30} color="inherit" />
        ) : (
          <PowerSettingsNewRounded />
        )}
      </span>
    </Box>
  )
}
