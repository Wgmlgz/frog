package com.wgmlgz.frog.tvpython

import android.content.Context
import android.content.ComponentName
import android.appwidget.AppWidgetManager
import android.util.Log
import androidx.work.Data
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.Worker
import androidx.work.WorkerParameters
import com.google.android.gms.tasks.Tasks
import com.google.android.gms.wearable.MessageEvent
import com.google.android.gms.wearable.Wearable
import com.google.android.gms.wearable.WearableListenerService
import org.json.JSONObject
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.InetAddress
import java.net.InetSocketAddress
import java.net.Socket
import java.util.concurrent.TimeUnit

/** The phone owns all network access and credentials; the watch only sends commands. */
class FrogWearListener : WearableListenerService() {
    override fun onMessageReceived(event: MessageEvent) {
        if (event.path != REQUEST_PATH || event.data.size > 4096) return
        val request = try { JSONObject(String(event.data, Charsets.UTF_8)) } catch (_: Exception) { return }
        val id = request.optString("id")
        val action = request.optString("action")
        val device = request.optString("device")
        if (id.length !in 1..80 || action !in ACTIONS || device.length > 160) return
        val work = OneTimeWorkRequestBuilder<FrogWearWorker>()
            .setInputData(Data.Builder().putString("id", id).putString("action", action)
                .putString("device", device).putString("node", event.sourceNodeId).build())
            .build()
        WorkManager.getInstance(this).enqueue(work)
    }

    companion object {
        const val REQUEST_PATH = "/frog/request"
        const val RESPONSE_PATH = "/frog/response"
        val ACTIONS = setOf("status", "lights", "lamp", "power", "speaker", "headphones", "up", "down")
    }
}

class FrogWearWorker(context: Context, params: WorkerParameters) : Worker(context, params) {
    override fun doWork(): Result {
        val id = inputData.getString("id") ?: return Result.failure()
        val action = inputData.getString("action") ?: return Result.failure()
        val device = inputData.getString("device") ?: ""
        val node = inputData.getString("node") ?: return Result.failure()
        val response = JSONObject().put("id", id).put("action", action)
        try {
            val path = WidgetUi.config(applicationContext).absolutePath
            val devices = checked(LampWidgetNative.devices(path)).getJSONArray("devices")
            val tvId = (0 until devices.length()).mapNotNull { devices.optJSONObject(it) }
                .firstOrNull { it.optString("type") == "tv" && (device.isEmpty() || it.optString("id") == device) }
                ?.optString("id") ?: ""
            response.put("devices", devices)
            response.put("device", if (action == "lamp") device else tvId)
            when (action) {
                "status" -> {
                    response.put("lights", checked(LampWidgetNative.run(path, false)).optString("state"))
                    if (tvId.isNotEmpty()) {
                        val config = checked(LampWidgetNative.tvConfig(path, tvId))
                        val on = tvIsOn(config.getString("ip"))
                        response.put("power", if (on) "on" else "off")
                        if (on) try {
                            val audio = checked(LampWidgetNative.tvAudio(path, tvId, "refresh"))
                            response.put("output", audio.optString("output"))
                            response.put("volume", audio.optInt("volume", -1))
                        } catch (error: Exception) { Log.w("FrogWear", "Audio status unavailable", error) }
                    }
                }
                "lights" -> {
                    response.put("lights", checked(LampWidgetNative.run(path, true)).getString("state"))
                    AllLightsWidgetProvider.setState(applicationContext, response.getString("lights"))
                }
                "lamp" -> {
                    require(device.isNotEmpty()) { "Choose a lamp" }
                    response.put("lamp", checked(LampWidgetNative.lamp(path, device, true)).getString("state"))
                }
                "power" -> {
                    require(tvId.isNotEmpty()) { "Add a TV in Frog on your phone" }
                    val config = checked(LampWidgetNative.tvConfig(path, tvId))
                    if (tvIsOn(config.getString("ip"))) {
                        val command = JSONObject().put("id", 1).put("action", "button")
                            .put("button", "POWER").put("tv_ip", config.getString("ip"))
                            .put("client_key", config.getString("clientKey"))
                        val result = JSONObject(TvPythonBridge.request(applicationContext, command.toString()))
                        require(result.optBoolean("ok")) { result.optString("error", "TV power failed") }
                        response.put("power", "off")
                    } else {
                        wake(config)
                        response.put("power", "starting")
                    }
                }
                "speaker", "headphones", "up", "down" -> {
                    require(tvId.isNotEmpty()) { "Add a TV in Frog on your phone" }
                    val audio = checked(LampWidgetNative.tvAudio(path, tvId, action))
                    response.put("output", audio.getString("output"))
                    response.put("volume", audio.getInt("volume"))
                    response.put("power", "on")
                }
            }
            if (action != "status") {
                WidgetUi.refreshAll(applicationContext)
                if (action == "power") {
                    val manager = AppWidgetManager.getInstance(applicationContext)
                    listOf(TvPowerWidgetProvider::class.java, TvQuickWidgetProvider::class.java)
                        .forEach { provider ->
                            manager.getAppWidgetIds(ComponentName(applicationContext, provider))
                                .forEach { widget -> WidgetUi.enqueue(applicationContext, widget, "refresh", 15) }
                        }
                }
            }
            response.put("ok", true)
        } catch (error: Throwable) {
            Log.w("FrogWear", "Watch action $action failed", error)
            response.put("ok", false).put("error", error.message ?: "Command failed")
        }
        return try {
            Tasks.await(Wearable.getMessageClient(applicationContext).sendMessage(
                node, FrogWearListener.RESPONSE_PATH, response.toString().toByteArray(Charsets.UTF_8)),
                10, TimeUnit.SECONDS)
            Result.success()
        } catch (error: Exception) {
            Log.w("FrogWear", "Could not send watch response", error)
            Result.failure()
        }
    }

    private fun checked(value: String): JSONObject = JSONObject(value).also {
        it.optString("error").takeIf(String::isNotEmpty)?.let { message -> throw IllegalStateException(message) }
    }

    private fun tvIsOn(ip: String) = try {
        Socket().use { it.connect(InetSocketAddress(ip, 3000), 1200); true }
    } catch (_: Exception) { false }

    private fun wake(config: JSONObject) {
        val mac = config.getString("mac").replace("-", "").replace(":", "")
        require(mac.length == 12) { "Invalid TV MAC address" }
        val bytes = ByteArray(6) { mac.substring(it * 2, it * 2 + 2).toInt(16).toByte() }
        val packet = ByteArray(102) { if (it < 6) 0xff.toByte() else bytes[(it - 6) % 6] }
        DatagramSocket().use {
            it.broadcast = true
            it.send(DatagramPacket(packet, packet.size, InetAddress.getByName(config.getString("broadcast")), 9))
        }
    }
}
