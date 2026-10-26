package com.wgmlgz.frog.tvpython

import android.Manifest
import android.app.Activity
import android.content.pm.PackageManager
import android.content.ClipboardManager
import android.content.Context
import android.os.Build
import android.webkit.WebView
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import com.chaquo.python.Python
import com.chaquo.python.android.AndroidPlatform
import java.util.concurrent.Executors

@InvokeArg
class RequestArgs {
    lateinit var json: String
}

@TauriPlugin
class TvPythonPlugin(private val activity: Activity) : Plugin(activity) {
    private val worker = Executors.newSingleThreadExecutor()

    override fun load(webView: WebView) {
        if (Build.VERSION.SDK_INT >= 37 && activity.applicationInfo.targetSdkVersion >= 37 &&
            activity.checkSelfPermission(Manifest.permission.ACCESS_LOCAL_NETWORK) != PackageManager.PERMISSION_GRANTED) {
            activity.requestPermissions(arrayOf(Manifest.permission.ACCESS_LOCAL_NETWORK), 3701)
        }
    }

    @Command
    fun readClipboardText(invoke: Invoke) {
        try {
            val manager = activity.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
            val clip = manager.primaryClip
            if (clip == null || clip.itemCount == 0) {
                invoke.reject("Clipboard is empty")
                return
            }
            val content = clip.getItemAt(0).coerceToText(activity).toString()
            val result = JSObject()
            result.put("text", content)
            invoke.resolve(result)
        } catch (error: Exception) {
            invoke.reject(error.message ?: "Clipboard read failed")
        }
    }

    @Command
    fun request(invoke: Invoke) {
        val args = invoke.parseArgs(RequestArgs::class.java)
        worker.execute {
            try {
                if (!Python.isStarted()) {
                    Python.start(AndroidPlatform(activity.applicationContext))
                }
                val bridge = Python.getInstance().getModule("remote_bridge").get("mobile_bridge")
                    ?: throw IllegalStateException("Python remote bridge is unavailable")
                val response = bridge.callAttr("request_json", args.json).toString()
                val result = JSObject()
                result.put("json", response)
                invoke.resolve(result)
            } catch (error: Exception) {
                invoke.reject(error.message ?: "TV Python bridge failed")
            }
        }
    }
}
