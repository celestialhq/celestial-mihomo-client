// get the system os
// according to UA
export default function getSystem() {
  const ua = navigator.userAgent
  const platform = OS_PLATFORM

  // Runtime UA checks must come before OS_PLATFORM: that constant is baked
  // in at bundle-build time from the *build machine's* Node.js process, not
  // the device the app actually runs on — a Windows dev box cross-compiling
  // an Android APK still bakes in "win32", so android detection can only
  // come from the (correctly runtime-reported) user agent.
  if (/android/i.test(ua)) return 'android'

  if (ua.includes('Mac OS X') || platform === 'darwin') return 'macos'

  if (/win64|win32/i.test(ua) || platform === 'win32') return 'windows'

  if (/linux/i.test(ua)) return 'linux'

  return 'unknown'
}
