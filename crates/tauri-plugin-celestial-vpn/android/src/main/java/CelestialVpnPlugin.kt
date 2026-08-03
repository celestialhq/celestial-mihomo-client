package app.tauri.celestialvpn

import android.app.Activity
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.ServiceConnection
import android.net.VpnService
import android.os.IBinder
import androidx.activity.result.ActivityResult
import app.tauri.annotation.ActivityCallback
import app.tauri.annotation.Command
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin

@TauriPlugin
class CelestialVpnPlugin(private val activity: Activity) : Plugin(activity) {
    private var pendingStartInvoke: Invoke? = null
    private var serviceBinder: CelestialVpnService.LocalBinder? = null
    private var bound = false

    private val connection = object : ServiceConnection {
        override fun onServiceConnected(name: ComponentName?, binder: IBinder) {
            serviceBinder = binder as CelestialVpnService.LocalBinder
            bound = true

            val invoke = pendingStartInvoke
            pendingStartInvoke = null
            if (invoke == null) return

            val fd = serviceBinder?.establish()
            if (fd == null || fd < 0) {
                invoke.reject("Failed to establish VPN interface")
            } else {
                val result = JSObject()
                result.put("fd", fd)
                invoke.resolve(result)
            }
        }

        override fun onServiceDisconnected(name: ComponentName?) {
            serviceBinder = null
            bound = false
        }
    }

    @Command
    fun startVpn(invoke: Invoke) {
        val prepareIntent = VpnService.prepare(activity)
        if (prepareIntent != null) {
            pendingStartInvoke = invoke
            startActivityForResult(invoke, prepareIntent, "onVpnPermissionResult")
        } else {
            bindAndEstablish(invoke)
        }
    }

    @ActivityCallback
    private fun onVpnPermissionResult(invoke: Invoke, result: ActivityResult) {
        if (result.resultCode == Activity.RESULT_OK) {
            bindAndEstablish(invoke)
        } else {
            invoke.reject("VPN permission denied")
        }
    }

    private fun bindAndEstablish(invoke: Invoke) {
        pendingStartInvoke = invoke
        val intent = Intent(activity, CelestialVpnService::class.java)
        activity.startService(intent)
        activity.bindService(intent, connection, Context.BIND_AUTO_CREATE)
    }

    @Command
    fun stopVpn(invoke: Invoke) {
        if (bound) {
            activity.unbindService(connection)
            bound = false
            serviceBinder = null
        }
        activity.stopService(Intent(activity, CelestialVpnService::class.java))
        invoke.resolve()
    }
}
