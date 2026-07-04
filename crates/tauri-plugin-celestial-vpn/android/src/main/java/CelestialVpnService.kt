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

    // Addresses/routes are placeholders — the embedded core (Milestone C) does its own
    // routing/DNS via TUN once it opens the fd, so these just need to make the interface
    // itself valid. addDisallowedApplication is load-bearing: without it, Android routes
    // this app's OWN sockets (mihomo's DNS bootstrap + proxy-server dials) back into its
    // own TUN interface, so they can never actually reach the internet.
    private fun establishTun(): Int? {
        tunInterface?.let { return it.fd }

        val builder = Builder()
            .setSession("Celestial")
            .addAddress("10.0.0.2", 32)
            .addRoute("0.0.0.0", 0)
            .addDnsServer("1.1.1.1")

        try {
            builder.addDisallowedApplication(packageName)
        } catch (e: android.content.pm.PackageManager.NameNotFoundException) {
            // Can't happen for our own package name, but the API forces us to handle it.
        }

        val pfd = builder.establish() ?: return null

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
