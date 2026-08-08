import { useQuery } from '@tanstack/react-query'

import { getRunState } from '@/services/cmds'
import getSystem from '@/utils/get-system'

// TUN mode on Android goes through VpnService's one-time permission dialog
// (requested on demand when the user toggles it on), not an installable
// privileged helper — there's no "is it installed" check to make in advance
// the way desktop's admin/service checks do.
const IS_MOBILE_PLATFORM = getSystem() === 'android'

/** Shared so the `verge://run-state-changed` listener can push straight into it. */
export const RUN_STATE_QUERY_KEY = ['getRunState'] as const

const defaultRunState: IRunState = {
  mode: 'NotRunning',
  service: 'unknown',
  serviceUnavailableReason: null,
  pendingAction: null,
  sidecarAllowed: false,
  isAdmin: false,
  opInFlight: false,
  serviceUsable: false,
  tunCapable: false,
  serviceNeedsAttention: false,
}

/**
 * 自定义 hook 用于获取系统运行状态
 * 包括运行模式、管理员状态、系统服务是否可用
 *
 * The backend owns this state and pushes every transition, so this hook only
 * reads. It used to poll every two seconds, wait out a fixed ten-second startup
 * grace period, and turn TUN off itself when it decided the service was missing
 * — three separate guesses at a question the backend can answer exactly, and
 * which it now answers via `tunCapable`.
 */
export function useSystemState() {
  const {
    data: runState = defaultRunState,
    refetch: mutateSystemState,
    isLoading,
  } = useQuery({
    queryKey: RUN_STATE_QUERY_KEY,
    queryFn: getRunState,
    // A safety net for a dropped event, not the way state normally arrives.
    refetchInterval: 60_000,
  })

  return {
    runState,
    runningMode: runState.mode,
    isAdminMode: runState.isAdmin,
    isServiceOk: runState.serviceUsable,
    isSidecarMode: runState.mode === 'Sidecar',
    isServiceMode: runState.mode === 'Service',
    // Android's TUN *is* VpnService, granted by a permission dialog rather than
    // backed by a privileged helper, so the service question never applies.
    isTunModeAvailable: IS_MOBILE_PLATFORM || runState.tunCapable,
    serviceNeedsAttention: runState.serviceNeedsAttention,
    mutateSystemState,
    isLoading,
  }
}
