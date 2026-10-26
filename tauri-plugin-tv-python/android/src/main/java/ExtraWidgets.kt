package com.wgmlgz.frog.tvpython

import android.app.Activity
import android.app.AlertDialog
import android.app.PendingIntent
import android.appwidget.AppWidgetManager
import android.appwidget.AppWidgetProvider
import android.content.BroadcastReceiver
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.graphics.Color
import android.net.Uri
import android.os.Bundle
import android.util.Log
import android.widget.RemoteViews
import androidx.work.Data
import androidx.work.ExistingWorkPolicy
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.Worker
import androidx.work.WorkerParameters
import org.json.JSONObject
import java.io.File
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.InetAddress
import java.net.InetSocketAddress
import java.net.Socket
import java.util.concurrent.TimeUnit

class LampToggleWidgetProvider : ConfiguredWidgetProvider()
class TvPowerWidgetProvider : ConfiguredWidgetProvider()
class TvVolumeWidgetProvider : ConfiguredWidgetProvider()
class TvAudioWidgetProvider : ConfiguredWidgetProvider()
class TvOutputWidgetProvider : ConfiguredWidgetProvider()
class TvQuickWidgetProvider : ConfiguredWidgetProvider()

open class ConfiguredWidgetProvider : AppWidgetProvider() {
    override fun onUpdate(context: Context, manager: AppWidgetManager, widgetIds: IntArray) {
        widgetIds.forEach { WidgetUi.render(context, it); WidgetUi.enqueue(context, it, "refresh") }
    }

    override fun onDeleted(context: Context, widgetIds: IntArray) {
        widgetIds.forEach { WidgetUi.clear(context, it) }
    }
}

class WidgetConfigActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val id = intent.getIntExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, AppWidgetManager.INVALID_APPWIDGET_ID)
        setResult(RESULT_CANCELED)
        if (id == AppWidgetManager.INVALID_APPWIDGET_ID) { finish(); return }
        val type = WidgetUi.type(this, id)
        if (type == null) { finish(); return }
        val wanted = if (type == "lamp") "lamp" else "tv"
        val response = try {
            JSONObject(LampWidgetNative.devices(WidgetUi.config(this).absolutePath))
        } catch (error: Throwable) {
            AlertDialog.Builder(this).setMessage(error.message ?: "Cannot read devices")
                .setPositiveButton("OK") { _, _ -> finish() }.show()
            return
        }
        val devices = response.optJSONArray("devices")
        val choices = (0 until (devices?.length() ?: 0)).mapNotNull { index ->
            devices?.optJSONObject(index)?.takeIf { it.optString("type") == wanted }
        }
        if (choices.isEmpty()) {
            AlertDialog.Builder(this).setMessage("Add a ${if (wanted == "tv") "TV" else "lamp"} in Frog first")
                .setPositiveButton("Open Frog") { _, _ ->
                    packageManager.getLaunchIntentForPackage(packageName)?.let(::startActivity)
                    finish()
                }.setNegativeButton("Cancel") { _, _ -> finish() }.show()
            return
        }
        AlertDialog.Builder(this).setTitle(if (wanted == "tv") "Choose TV" else "Choose lamp")
            .setItems(choices.map { it.optString("name") }.toTypedArray()) { _, index ->
                WidgetUi.setDevice(this, id, choices[index].optString("id"), choices[index].optString("name"))
                WidgetUi.render(this, id)
                WidgetUi.enqueue(this, id, "refresh")
                setResult(RESULT_OK, Intent().putExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, id))
                finish()
            }.setOnCancelListener { finish() }.show()
    }
}

internal object WidgetUi {
    private const val PREFS = "frog_extra_widgets"
    private fun prefs(context: Context) = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
    fun config(context: Context) = File(context.dataDir, "frog.toml")
    fun type(context: Context, id: Int): String? = when (
        AppWidgetManager.getInstance(context).getAppWidgetInfo(id)?.provider?.className
    ) {
        LampToggleWidgetProvider::class.java.name -> "lamp"
        TvPowerWidgetProvider::class.java.name -> "power"
        TvVolumeWidgetProvider::class.java.name -> "volume"
        TvAudioWidgetProvider::class.java.name -> "audio"
        TvOutputWidgetProvider::class.java.name -> "output"
        TvQuickWidgetProvider::class.java.name -> "quick"
        else -> null
    }
    fun device(context: Context, id: Int) = prefs(context).getString("device_$id", "") ?: ""
    fun name(context: Context, id: Int) = prefs(context).getString("name_$id", "") ?: ""
    fun setDevice(context: Context, id: Int, device: String, name: String) {
        prefs(context).edit().putString("device_$id", device).putString("name_$id", name).apply()
    }
    fun clear(context: Context, id: Int) {
        prefs(context).edit().remove("device_$id").remove("name_$id")
            .remove("state_$id").remove("volume_$id").remove("output_$id").apply()
    }
    fun state(context: Context, id: Int) = prefs(context).getString("state_$id", "unknown") ?: "unknown"
    fun setState(context: Context, id: Int, state: String) {
        if (state(context, id) == state) return
        prefs(context).edit().putString("state_$id", state).apply()
        render(context, id)
    }
    fun setAudio(context: Context, id: Int, volume: Int, output: String) {
        val preferences = prefs(context)
        if (preferences.getInt("volume_$id", -1) == volume && preferences.getString("output_$id", "") == output) return
        preferences.edit().putInt("volume_$id", volume).putString("output_$id", output).apply()
        render(context, id)
    }
    fun enqueue(context: Context, id: Int, action: String, delaySeconds: Long = 0) {
        if (type(context, id) == null || device(context, id).isEmpty()) return
        val request = OneTimeWorkRequestBuilder<ExtraWidgetWorker>()
            .setInputData(Data.Builder().putInt("id", id).putString("action", action).build())
            .setInitialDelay(delaySeconds, TimeUnit.SECONDS).build()
        WorkManager.getInstance(context).enqueueUniqueWork(
            if (delaySeconds > 0) "frog_widget_${id}_scheduled" else "frog_widget_$id",
            ExistingWorkPolicy.APPEND_OR_REPLACE, request
        )
    }
    fun refreshAll(context: Context) {
        val manager = AppWidgetManager.getInstance(context)
        listOf(LampToggleWidgetProvider::class.java, TvPowerWidgetProvider::class.java,
            TvVolumeWidgetProvider::class.java, TvAudioWidgetProvider::class.java,
            TvOutputWidgetProvider::class.java, TvQuickWidgetProvider::class.java).forEach { provider ->
            manager.getAppWidgetIds(ComponentName(context, provider)).forEach { enqueue(context, it, "refresh") }
        }
    }
    private fun click(context: Context, id: Int, action: String): PendingIntent {
        val intent = Intent(context, ExtraWidgetTapReceiver::class.java).apply {
            this.action = "com.wgmlgz.frog.WIDGET_ACTION"
            data = Uri.parse("frog-widget://$id/$action")
            putExtra("id", id)
            putExtra("widgetAction", action)
        }
        return PendingIntent.getBroadcast(context, 0, intent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
    }
    private fun openApp(context: Context, id: Int): PendingIntent? {
        val intent = context.packageManager.getLaunchIntentForPackage(context.packageName) ?: return null
        return PendingIntent.getActivity(context, id, intent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
    }
    fun render(context: Context, id: Int) {
        val kind = type(context, id) ?: return
        Log.d("FrogWidget", "Render widget $id ($kind)")
        val state = state(context, id)
        val views = when (kind) {
            "lamp" -> RemoteViews(context.packageName, R.layout.all_lights_widget).apply {
                setInt(R.id.all_lights_widget, "setBackgroundResource", if (state == "on") R.drawable.widget_on else R.drawable.widget_off)
                setTextViewText(R.id.widget_label, name(context, id).ifEmpty { "Lamp" }.take(12))
                setTextColor(R.id.widget_label, if (state == "on") Color.rgb(30, 41, 59) else Color.WHITE)
                setImageViewResource(R.id.widget_power_icon, if (state == "on") R.drawable.widget_power_dark else R.drawable.widget_power)
                setOnClickPendingIntent(R.id.all_lights_widget, click(context, id, "toggle"))
            }
            "power" -> RemoteViews(context.packageName, R.layout.tv_power_widget).apply {
                setInt(R.id.tv_power_widget, "setBackgroundResource", if (state == "on") R.drawable.widget_on else R.drawable.widget_off)
                val foreground = if (state == "on") Color.rgb(30, 41, 59) else Color.WHITE
                setTextColor(R.id.tv_power_title, foreground)
                setTextColor(R.id.tv_power_status, foreground)
                setTextViewText(R.id.tv_power_title, name(context, id).ifEmpty { "TV" }.take(18))
                setTextViewText(R.id.tv_power_status, when (state) {
                    "on" -> "On"; "off" -> "Off"; "starting" -> "Starting…"; "unavailable" -> "Offline"; else -> "Checking…"
                })
                setImageViewResource(R.id.tv_power_button_icon, if (state == "on") R.drawable.widget_power_dark else R.drawable.widget_power)
                openApp(context, id)?.let { setOnClickPendingIntent(R.id.tv_power_open, it) }
                setOnClickPendingIntent(R.id.tv_power_button, click(context, id, "toggle"))
            }
            "volume" -> RemoteViews(context.packageName, R.layout.tv_volume_widget).apply {
                val volume = prefs(context).getInt("volume_$id", -1)
                setTextViewText(R.id.tv_volume_value, if (volume < 0) "--" else "$volume%")
                setOnClickPendingIntent(R.id.tv_volume_minus, click(context, id, "down"))
                setOnClickPendingIntent(R.id.tv_volume_plus, click(context, id, "up"))
                openApp(context, id)?.let { setOnClickPendingIntent(R.id.tv_volume_open, it) }
            }
            "output" -> RemoteViews(context.packageName, R.layout.tv_output_widget).apply {
                val output = prefs(context).getString("output_$id", "")
                setInt(R.id.tv_output_speaker, "setBackgroundResource",
                    if (output == "tv_speaker") R.drawable.widget_selected else android.R.color.transparent)
                setInt(R.id.tv_output_headphones, "setBackgroundResource",
                    if (output == "bt_soundbar") R.drawable.widget_selected else android.R.color.transparent)
                setImageViewResource(R.id.tv_output_speaker,
                    if (output == "tv_speaker") R.drawable.widget_speaker_dark else R.drawable.widget_speaker)
                setImageViewResource(R.id.tv_output_headphones,
                    if (output == "bt_soundbar") R.drawable.widget_headphones_dark else R.drawable.widget_headphones)
                setOnClickPendingIntent(R.id.tv_output_speaker, click(context, id, "speaker"))
                setOnClickPendingIntent(R.id.tv_output_headphones, click(context, id, "headphones"))
            }
            "quick" -> RemoteViews(context.packageName, R.layout.tv_quick_widget).apply {
                val on = state == "on"
                val foreground = if (on) Color.rgb(30, 41, 59) else Color.WHITE
                val output = prefs(context).getString("output_$id", "")
                setInt(R.id.tv_quick_widget, "setBackgroundResource", if (on) R.drawable.widget_on else R.drawable.widget_off)
                setTextViewText(R.id.tv_quick_title, name(context, id).ifEmpty { "TV" }.take(18))
                setTextViewText(R.id.tv_quick_status, when (state) {
                    "on" -> "On"; "off" -> "Off"; "starting" -> "Starting…"; "unavailable" -> "Offline"; else -> "Checking…"
                })
                setTextColor(R.id.tv_quick_title, foreground)
                setTextColor(R.id.tv_quick_status, foreground)
                setImageViewResource(R.id.tv_quick_power_icon, if (on) R.drawable.widget_power_dark else R.drawable.widget_power)
                val selectedBackground = if (on) R.drawable.widget_selected_dark else R.drawable.widget_selected
                setInt(R.id.tv_quick_speaker, "setBackgroundResource",
                    if (output == "tv_speaker") selectedBackground else android.R.color.transparent)
                setInt(R.id.tv_quick_headphones, "setBackgroundResource",
                    if (output == "bt_soundbar") selectedBackground else android.R.color.transparent)
                setImageViewResource(R.id.tv_quick_speaker,
                    if (on != (output == "tv_speaker")) R.drawable.widget_speaker_dark else R.drawable.widget_speaker)
                setImageViewResource(R.id.tv_quick_headphones,
                    if (on != (output == "bt_soundbar")) R.drawable.widget_headphones_dark else R.drawable.widget_headphones)
                openApp(context, id)?.let { setOnClickPendingIntent(R.id.tv_quick_open, it) }
                setOnClickPendingIntent(R.id.tv_quick_power, click(context, id, "toggle"))
                setOnClickPendingIntent(R.id.tv_quick_speaker, click(context, id, "speaker"))
                setOnClickPendingIntent(R.id.tv_quick_headphones, click(context, id, "headphones"))
            }
            else -> RemoteViews(context.packageName, R.layout.tv_audio_widget).apply {
                val volume = prefs(context).getInt("volume_$id", -1)
                val output = prefs(context).getString("output_$id", "")
                setTextViewText(R.id.tv_audio_value, if (volume < 0) "--" else "$volume%")
                setTextViewText(R.id.tv_audio_output, when (output) {
                    "tv_speaker" -> "TV speakers"; "bt_soundbar" -> "Headphones"; else -> "Audio offline"
                })
                setInt(R.id.tv_audio_speaker, "setBackgroundResource",
                    if (output == "tv_speaker") R.drawable.widget_selected else android.R.color.transparent)
                setInt(R.id.tv_audio_headphones, "setBackgroundResource",
                    if (output == "bt_soundbar") R.drawable.widget_selected else android.R.color.transparent)
                setImageViewResource(R.id.tv_audio_speaker,
                    if (output == "tv_speaker") R.drawable.widget_speaker_dark else R.drawable.widget_speaker)
                setImageViewResource(R.id.tv_audio_headphones,
                    if (output == "bt_soundbar") R.drawable.widget_headphones_dark else R.drawable.widget_headphones)
                setOnClickPendingIntent(R.id.tv_audio_minus, click(context, id, "down"))
                setOnClickPendingIntent(R.id.tv_audio_plus, click(context, id, "up"))
                setOnClickPendingIntent(R.id.tv_audio_speaker, click(context, id, "speaker"))
                setOnClickPendingIntent(R.id.tv_audio_headphones, click(context, id, "headphones"))
                openApp(context, id)?.let { setOnClickPendingIntent(R.id.tv_audio_open, it) }
            }
        }
        AppWidgetManager.getInstance(context).updateAppWidget(id, views)
    }
}

class ExtraWidgetTapReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action != "com.wgmlgz.frog.WIDGET_ACTION") return
        WidgetUi.enqueue(context, intent.getIntExtra("id", -1), intent.getStringExtra("widgetAction") ?: return)
    }
}

class ExtraWidgetWorker(context: Context, params: WorkerParameters) : Worker(context, params) {
    override fun doWork(): Result {
        val context = applicationContext
        val id = inputData.getInt("id", -1)
        val kind = WidgetUi.type(context, id) ?: return Result.failure()
        val device = WidgetUi.device(context, id)
        val action = inputData.getString("action") ?: "refresh"
        val path = WidgetUi.config(context).absolutePath
        return try {
            when (kind) {
                "lamp" -> {
                    val result = JSONObject(LampWidgetNative.lamp(path, device, action == "toggle"))
                    result.optString("error").takeIf(String::isNotEmpty)?.let { throw IllegalStateException(it) }
                    WidgetUi.setState(context, id, result.getString("state"))
                }
                "power", "quick" -> {
                    val config = JSONObject(LampWidgetNative.tvConfig(path, device))
                    config.optString("error").takeIf(String::isNotEmpty)?.let { throw IllegalStateException(it) }
                    val ip = config.getString("ip")
                    val wasOn = tvIsOn(ip)
                    if (kind == "quick" && (action == "speaker" || action == "headphones")) {
                        val audio = JSONObject(LampWidgetNative.tvAudio(path, device, action))
                        audio.optString("error").takeIf(String::isNotEmpty)?.let { throw IllegalStateException(it) }
                        WidgetUi.setAudio(context, id, audio.getInt("volume"), audio.getString("output"))
                        WidgetUi.setState(context, id, "on")
                    } else if (action == "toggle") {
                        if (wasOn) {
                            tvPowerOff(context, config)
                            WidgetUi.setState(context, id, "off")
                            if (kind == "quick") WidgetUi.setAudio(context, id, -1, "")
                        } else {
                            wakeTv(config)
                            WidgetUi.setState(context, id, "starting")
                        }
                        WidgetUi.enqueue(context, id, "refresh", 15)
                    } else {
                        WidgetUi.setState(context, id, if (wasOn) "on" else "off")
                        if (kind == "quick" && wasOn) {
                            val audio = JSONObject(LampWidgetNative.tvAudio(path, device, "refresh"))
                            audio.optString("error").takeIf(String::isNotEmpty)?.let { throw IllegalStateException(it) }
                            WidgetUi.setAudio(context, id, audio.getInt("volume"), audio.getString("output"))
                        }
                    }
                }
                "volume", "audio", "output" -> {
                    val result = JSONObject(LampWidgetNative.tvAudio(path, device, action))
                    result.optString("error").takeIf(String::isNotEmpty)?.let { throw IllegalStateException(it) }
                    WidgetUi.setAudio(context, id, result.getInt("volume"), result.getString("output"))
                }
            }
            Result.success()
        } catch (error: Throwable) {
            Log.w("FrogWidget", "Widget $kind action $action failed", error)
            if (kind == "power" || kind == "quick") WidgetUi.setState(context, id, "unavailable")
            if (kind == "volume" || kind == "audio" || kind == "output") WidgetUi.setAudio(context, id, -1, "")
            Result.failure()
        }
    }

    private fun tvIsOn(ip: String): Boolean = try {
        Socket().use { socket -> socket.connect(InetSocketAddress(ip, 3000), 1200); true }
    } catch (_: Exception) { false }

    private fun wakeTv(config: JSONObject) {
        val mac = config.getString("mac").replace("-", "").replace(":", "")
        require(mac.length == 12) { "Invalid TV MAC address" }
        val address = ByteArray(6) { mac.substring(it * 2, it * 2 + 2).toInt(16).toByte() }
        val packet = ByteArray(102) { index -> if (index < 6) 0xff.toByte() else address[(index - 6) % 6] }
        DatagramSocket().use { socket ->
            socket.broadcast = true
            socket.send(DatagramPacket(packet, packet.size, InetAddress.getByName(config.getString("broadcast")), 9))
        }
    }

    private fun tvPowerOff(context: Context, config: JSONObject) {
        val request = JSONObject().put("id", 1).put("action", "button").put("button", "POWER")
            .put("tv_ip", config.getString("ip")).put("client_key", config.getString("clientKey"))
        val response = JSONObject(TvPythonBridge.request(context, request.toString()))
        if (!response.optBoolean("ok")) throw IllegalStateException(response.optString("error", "TV power failed"))
    }
}
