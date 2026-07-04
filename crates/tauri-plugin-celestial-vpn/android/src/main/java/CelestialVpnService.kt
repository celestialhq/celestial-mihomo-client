package app.tauri.celestialvpn

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.content.Intent
import android.net.VpnService
import android.os.Binder
import android.os.Build
import android.os.IBinder
import android.os.ParcelFileDescriptor

private const val NOTIFICATION_ID = 1
private const val CHANNEL_ID = "celestial_vpn"

class CelestialVpnService : VpnService() {
    private var tunInterface: ParcelFileDescriptor? = null

    inner class LocalBinder : Binder() {
        fun establish(): Int? = this@CelestialVpnService.establishTun()
    }

    private val binder = LocalBinder()

    override fun onBind(intent: Intent?): IBinder = binder

    override fun onCreate() {
        super.onCreate()
        startForeground(NOTIFICATION_ID, buildNotification())
    }

    // TEMPORARY: minimal addressing just to prove the permission/fd-handoff plumbing
    // (Milestone B1) — real routes/DNS come from the profile's mihomo config once the
    // embedded core is wired in (Milestone C).
    private fun establishTun(): Int? {
        tunInterface?.let { return it.fd }

        val pfd = Builder()
            .setSession("Celestial")
            .addAddress("10.0.0.2", 32)
            .addRoute("0.0.0.0", 0)
            .addDnsServer("1.1.1.1")
            .establish() ?: return null

        tunInterface = pfd
        return pfd.fd
    }

    private fun buildNotification(): Notification {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val channel = NotificationChannel(
                CHANNEL_ID,
                "Celestial VPN",
                NotificationManager.IMPORTANCE_LOW,
            )
            getSystemService(NotificationManager::class.java).createNotificationChannel(channel)
        }
        return Notification.Builder(this, CHANNEL_ID)
            .setContentTitle("Celestial")
            .setContentText("VPN is active")
            .setSmallIcon(android.R.drawable.ic_lock_lock)
            .build()
    }

    override fun onRevoke() {
        closeTun()
        stopForeground(STOP_FOREGROUND_REMOVE)
        stopSelf()
        super.onRevoke()
    }

    override fun onDestroy() {
        closeTun()
        super.onDestroy()
    }

    private fun closeTun() {
        tunInterface?.close()
        tunInterface = null
    }
}
