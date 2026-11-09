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

class WidgetRefreshReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action != Intent.ACTION_MY_PACKAGE_REPLACED) return
        try {
            AllLightsWidgetProvider.refreshAfterInstall(context)
            WidgetUi.renderAllConfigured(context)
            WidgetUi.refreshAll(context)
            WidgetUi.scheduleRepaint(context, -1, 10)
        } catch (error: Throwable) {
            Log.e("FrogWidget", "Could not restore widgets after app update", error)
        }
    }
}

open class ConfiguredWidgetProvider : AppWidgetProvider() {
    override fun onUpdate(context: Context, manager: AppWidgetManager, widgetIds: IntArray) {
        Log.d("FrogWidget", "onUpdate ${javaClass.simpleName}: ${widgetIds.joinToString()}")
        widgetIds.forEach {
            WidgetUi.render(context, it, partial = true)
            if (WidgetUi.shouldRefreshFromUpdate(context, it)) WidgetUi.enqueue(context, it, "refresh")
        }
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
                WidgetUi.scheduleRepaint(this, id, 2)
                setResult(RESULT_OK, Intent().putExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, id))
                finish()
            }.setOnCancelListener { finish() }.show()
    }
}

internal object WidgetUi {
    private const val PREFS = "frog_extra_widgets"
    private const val RENDER_VERSION = 3
    private fun renderStamp(context: Context) =
        context.packageManager.getPackageInfo(context.packageName, 0).lastUpdateTime + RENDER_VERSION
    private fun prefs(context: Context) = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
    private const val PENDING_TIMEOUT_MS = 45_000L
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
            .remove("state_$id").remove("volume_$id").remove("output_$id")
            .remove("pending_count_$id").remove("pending_action_$id").remove("pending_since_$id")
            .remove("rendered_$id").remove("last_update_refresh_$id").apply()
    }
    fun rendered(context: Context, id: Int) =
        (prefs(context).all["rendered_$id"] as? Number)?.toLong() == renderStamp(context)
    fun shouldRefreshFromUpdate(context: Context, id: Int): Boolean {
        val preferences = prefs(context)
        val now = System.currentTimeMillis()
        if (now - preferences.getLong("last_update_refresh_$id", 0) <= 30_000) return false
        preferences.edit().putLong("last_update_refresh_$id", now).apply()
        return true
    }
    fun state(context: Context, id: Int) = prefs(context).getString("state_$id", "unknown") ?: "unknown"
    fun isPending(context: Context, id: Int): Boolean {
        val preferences = prefs(context)
        return preferences.getInt("pending_count_$id", 0) > 0 &&
            System.currentTimeMillis() - preferences.getLong("pending_since_$id", 0) < PENDING_TIMEOUT_MS
    }
    fun beginAction(context: Context, id: Int, action: String): Boolean {
        val kind = type(context, id) ?: return false
        if (device(context, id).isEmpty()) return false
        val preferences = prefs(context)
        val alreadyPending = isPending(context, id)
        val repeatedVolume = (action == "up" || action == "down") && alreadyPending &&
            preferences.getString("pending_action_$id", "") == "volume"
        if (alreadyPending && !repeatedVolume) return false
        val editor = preferences.edit()
            .putInt("pending_count_$id", if (repeatedVolume) preferences.getInt("pending_count_$id", 0) + 1 else 1)
            .putString("pending_action_$id", if (action == "up" || action == "down") "volume" else action)
            .putLong("pending_since_$id", System.currentTimeMillis())
        when (action) {
            "toggle" -> if (kind == "lamp" || kind == "power" || kind == "quick") {
                editor.putString("state_$id", if (state(context, id) == "on") "off" else "on")
            }
            "speaker", "headphones" -> editor.putString("output_$id",
                if (action == "speaker") "tv_speaker" else "bt_soundbar")
            "up", "down" -> {
                val volume = preferences.getInt("volume_$id", -1)
                if (volume >= 0) editor.putInt("volume_$id", (volume + if (action == "up") 1 else -1).coerceIn(0, 100))
            }
        }
        editor.apply()
        render(context, id, partial = true)
        return true
    }
    fun finishAction(context: Context, id: Int, state: String? = null,
                     volume: Int? = null, output: String? = null) {
        val preferences = prefs(context)
        val remaining = (preferences.getInt("pending_count_$id", 0) - 1).coerceAtLeast(0)
        val editor = preferences.edit().putInt("pending_count_$id", remaining)
        if (remaining == 0) {
            editor.remove("pending_action_$id").remove("pending_since_$id")
            if (state != null) editor.putString("state_$id", state)
            if (volume != null) editor.putInt("volume_$id", volume)
            if (output != null) editor.putString("output_$id", output)
        }
        editor.apply()
        if (remaining == 0) render(context, id)
    }
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
            if (delaySeconds > 0) "frog_widget_${id}_scheduled"
            else if (action == "refresh") "frog_widget_${id}_refresh" else "frog_widget_${id}_action",
            if (action == "refresh") ExistingWorkPolicy.REPLACE else ExistingWorkPolicy.APPEND_OR_REPLACE,
            request
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
    fun scheduleRepaint(context: Context, id: Int, delaySeconds: Long) {
        val request = OneTimeWorkRequestBuilder<WidgetRepaintWorker>()
            .setInputData(Data.Builder().putInt("id", id).build())
            .setInitialDelay(delaySeconds, TimeUnit.SECONDS).build()
        WorkManager.getInstance(context).enqueueUniqueWork(
            "frog_widget_repaint_$id", ExistingWorkPolicy.REPLACE, request)
    }
    fun renderAllConfigured(context: Context) {
        val manager = AppWidgetManager.getInstance(context)
        listOf(LampToggleWidgetProvider::class.java, TvPowerWidgetProvider::class.java,
            TvVolumeWidgetProvider::class.java, TvAudioWidgetProvider::class.java,
            TvOutputWidgetProvider::class.java, TvQuickWidgetProvider::class.java).forEach { provider ->
            manager.getAppWidgetIds(ComponentName(context, provider)).forEach { render(context, it) }
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
    fun render(context: Context, id: Int, partial: Boolean = false) {
        val kind = type(context, id) ?: return
        val state = state(context, id)
        Log.d("FrogWidget", "Render widget $id ($kind): ${name(context, id)}, $state")
        val views = when (kind) {
            "lamp" -> RemoteViews(context.packageName, R.layout.all_lights_widget).apply {
                setImageViewResource(R.id.widget_dynamic_background,
                    if (state == "on") R.drawable.widget_on else R.drawable.widget_off)
                setTextViewText(R.id.widget_label,
                    if (isPending(context, id)) "Wait" else name(context, id).ifEmpty { "Lamp" }.take(12))
                setTextColor(R.id.widget_label, if (state == "on") Color.rgb(30, 41, 59) else Color.WHITE)
                setImageViewResource(R.id.widget_power_icon, if (state == "on") R.drawable.widget_power_dark else R.drawable.widget_power)
                setOnClickPendingIntent(R.id.all_lights_widget, click(context, id, "toggle"))
            }
            "power" -> RemoteViews(context.packageName, R.layout.tv_power_widget).apply {
                setImageViewResource(R.id.widget_dynamic_background,
                    if (state == "on") R.drawable.widget_on else R.drawable.widget_off)
                val foreground = if (state == "on") Color.rgb(30, 41, 59) else Color.WHITE
                setTextColor(R.id.tv_power_title, foreground)
                setTextColor(R.id.tv_power_status, foreground)
                setTextViewText(R.id.tv_power_title, name(context, id).ifEmpty { "TV" }.take(18))
                setTextViewText(R.id.tv_power_status, when (state) {
                    "on" -> if (isPending(context, id)) "Turning on…" else "On"
                    "off" -> if (isPending(context, id)) "Turning off…" else "Off"
                    "starting" -> "Starting…"; "unavailable" -> "Offline"; else -> "Checking…"
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
                setImageViewResource(R.id.tv_output_speaker,
                    if (output == "tv_speaker") R.drawable.widget_speaker_selected_light else R.drawable.widget_speaker)
                setImageViewResource(R.id.tv_output_headphones,
                    if (output == "bt_soundbar") R.drawable.widget_headphones_selected_light else R.drawable.widget_headphones)
                setOnClickPendingIntent(R.id.tv_output_speaker, click(context, id, "speaker"))
                setOnClickPendingIntent(R.id.tv_output_headphones, click(context, id, "headphones"))
            }
            "quick" -> RemoteViews(context.packageName, R.layout.tv_quick_widget).apply {
                setImageViewResource(R.id.widget_dynamic_background,
                    if (state == "on") R.drawable.widget_on else R.drawable.widget_off)
                val on = state == "on"
                val foreground = if (on) Color.rgb(30, 41, 59) else Color.WHITE
                val output = prefs(context).getString("output_$id", "")
                setTextViewText(R.id.tv_quick_title, name(context, id).ifEmpty { "TV" }.take(18))
                setTextViewText(R.id.tv_quick_status, when {
                    isPending(context, id) && prefs(context).getString("pending_action_$id", "") == "toggle" ->
                        if (on) "Turning on…" else "Turning off…"
                    isPending(context, id) -> "Switching audio…"
                    state == "on" -> if (output.isNullOrEmpty()) "Audio unavailable" else "On"
                    state == "off" -> "Off"
                    state == "starting" -> "Starting…"
                    state == "unavailable" -> "Offline"
                    else -> "Checking…"
                })
                setTextColor(R.id.tv_quick_title, foreground)
                setTextColor(R.id.tv_quick_status, foreground)
                setImageViewResource(R.id.tv_quick_power_icon, if (on) R.drawable.widget_power_dark else R.drawable.widget_power)
                setImageViewResource(R.id.tv_quick_speaker,
                    if (output == "tv_speaker") {
                        if (on) R.drawable.widget_speaker_selected_dark else R.drawable.widget_speaker_selected_light
                    } else if (on) R.drawable.widget_speaker_dark else R.drawable.widget_speaker)
                setImageViewResource(R.id.tv_quick_headphones,
                    if (output == "bt_soundbar") {
                        if (on) R.drawable.widget_headphones_selected_dark else R.drawable.widget_headphones_selected_light
                    } else if (on) R.drawable.widget_headphones_dark else R.drawable.widget_headphones)
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
                setImageViewResource(R.id.tv_audio_speaker,
                    if (output == "tv_speaker") R.drawable.widget_speaker_selected_light else R.drawable.widget_speaker)
                setImageViewResource(R.id.tv_audio_headphones,
                    if (output == "bt_soundbar") R.drawable.widget_headphones_selected_light else R.drawable.widget_headphones)
                setOnClickPendingIntent(R.id.tv_audio_minus, click(context, id, "down"))
                setOnClickPendingIntent(R.id.tv_audio_plus, click(context, id, "up"))
                setOnClickPendingIntent(R.id.tv_audio_speaker, click(context, id, "speaker"))
                setOnClickPendingIntent(R.id.tv_audio_headphones, click(context, id, "headphones"))
                openApp(context, id)?.let { setOnClickPendingIntent(R.id.tv_audio_open, it) }
            }
        }
        val manager = AppWidgetManager.getInstance(context)
        if (partial) manager.partiallyUpdateAppWidget(id, views)
        else manager.updateAppWidget(id, views)
        prefs(context).edit().putLong("rendered_$id", renderStamp(context)).apply()
    }
}

class WidgetRepaintWorker(context: Context, params: WorkerParameters) : Worker(context, params) {
    override fun doWork(): Result = try {
        val id = inputData.getInt("id", -1)
        if (id < 0) {
            AllLightsWidgetProvider.refreshAfterInstall(applicationContext)
            WidgetUi.renderAllConfigured(applicationContext)
        } else {
            WidgetUi.render(applicationContext, id)
        }
        Result.success()
    } catch (error: Throwable) {
        Log.w("FrogWidget", "Could not repaint widget after launcher setup", error)
        Result.failure()
    }
}

class ExtraWidgetTapReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action != "com.wgmlgz.frog.WIDGET_ACTION") return
        val id = intent.getIntExtra("id", -1)
        val action = intent.getStringExtra("widgetAction") ?: return
        if (WidgetUi.beginAction(context, id, action)) {
            WidgetUi.enqueue(context, id, action)
            WidgetUi.enqueue(context, id, "refresh", 50)
        }
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
        if (action == "refresh" && WidgetUi.isPending(context, id)) return Result.success()
        return try {
            when (kind) {
                "lamp" -> {
                    val result = JSONObject(LampWidgetNative.lamp(path, device, action == "toggle"))
                    result.optString("error").takeIf(String::isNotEmpty)?.let { throw IllegalStateException(it) }
                    if (action == "toggle") WidgetUi.finishAction(context, id, state = result.getString("state"))
                    else if (!WidgetUi.isPending(context, id)) WidgetUi.setState(context, id, result.getString("state"))
                }
                "power", "quick" -> {
                    val config = JSONObject(LampWidgetNative.tvConfig(path, device))
                    config.optString("error").takeIf(String::isNotEmpty)?.let { throw IllegalStateException(it) }
                    val ip = config.getString("ip")
                    val wasOn = tvIsOn(ip)
                    if (kind == "quick" && (action == "speaker" || action == "headphones")) {
                        val audio = readQuickAudio(path, device, action)
                        WidgetUi.finishAction(context, id, state = "on", volume = audio.first, output = audio.second)
                    } else if (action == "toggle") {
                        if (wasOn) {
                            tvPowerOff(context, config)
                            WidgetUi.finishAction(context, id, state = "off",
                                volume = if (kind == "quick") -1 else null,
                                output = if (kind == "quick") "" else null)
                        } else {
                            wakeTv(config)
                            WidgetUi.finishAction(context, id, state = "starting")
                        }
                        WidgetUi.enqueue(context, id, "refresh", 15)
                    } else {
                        if (!WidgetUi.isPending(context, id)) {
                            WidgetUi.setState(context, id, if (wasOn) "on" else "off")
                            if (kind == "quick" && wasOn) {
                                val audio = try { readQuickAudio(path, device, "refresh") }
                                catch (error: Exception) {
                                    Log.w("FrogWidget", "TV audio refresh failed", error)
                                    -1 to ""
                                }
                                if (!WidgetUi.isPending(context, id)) WidgetUi.setAudio(context, id, audio.first, audio.second)
                            }
                        }
                    }
                }
                "volume", "audio", "output" -> {
                    val result = JSONObject(LampWidgetNative.tvAudio(path, device, action))
                    result.optString("error").takeIf(String::isNotEmpty)?.let { throw IllegalStateException(it) }
                    if (action == "refresh") {
                        if (!WidgetUi.isPending(context, id)) WidgetUi.setAudio(context, id, result.getInt("volume"), result.getString("output"))
                    } else WidgetUi.finishAction(context, id, volume = result.getInt("volume"), output = result.getString("output"))
                }
            }
            Result.success()
        } catch (error: Throwable) {
            Log.w("FrogWidget", "Widget $kind action $action failed", error)
            if (action == "refresh") {
                if (!WidgetUi.isPending(context, id)) {
                    if (kind == "power" || kind == "quick") WidgetUi.setState(context, id, "unavailable")
                    if (kind == "volume" || kind == "audio" || kind == "output") WidgetUi.setAudio(context, id, -1, "")
                }
            } else if (kind == "quick" && (action == "speaker" || action == "headphones")) {
                WidgetUi.finishAction(context, id, state = "on", volume = -1, output = "")
            } else if (kind == "power" || kind == "quick") {
                WidgetUi.finishAction(context, id, state = "unavailable")
            } else if (kind == "lamp") {
                WidgetUi.finishAction(context, id, state = "unavailable")
            } else WidgetUi.finishAction(context, id, volume = -1, output = "")
            Result.failure()
        }
    }

    private fun readQuickAudio(path: String, device: String, action: String): Pair<Int, String> {
        val audio = JSONObject(LampWidgetNative.tvAudio(path, device, action))
        val error = audio.optString("error")
        if (error.isNotEmpty()) throw IllegalStateException(error)
        return audio.getInt("volume") to audio.getString("output")
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
