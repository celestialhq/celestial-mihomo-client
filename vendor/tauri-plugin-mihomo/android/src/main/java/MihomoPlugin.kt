package app.tauri.mihomo

import android.app.Activity
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Plugin

// The Rust side of this plugin talks to the mihomo core over a
// LocalSocket/HTTP transport of its own — it does not dispatch any
// commands through the Kotlin WebView bridge, so this class only needs
// to exist as a valid Tauri Android plugin module.
@TauriPlugin
class MihomoPlugin(private val activity: Activity) : Plugin(activity)
