package com.wgmlgz.frog.tvpython

import android.app.PendingIntent
import android.appwidget.AppWidgetManager
import android.appwidget.AppWidgetProvider
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.graphics.Color
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

object LampWidgetNative {
    init { System.loadLibrary("frog_lib") }
    external fun run(configPath: String, toggle: Boolean): String
    external fun devices(configPath: String): String
    external fun tvConfig(configPath: String, deviceId: String): String
    external fun tvAudio(configPath: String, deviceId: String, action: String): String
    external fun lamp(configPath: String, deviceId: String, toggle: Boolean): String
}

class AllLightsWidgetProvider : AppWidgetProvider() {
    override fun onUpdate(context: Context, manager: AppWidgetManager, widgetIds: IntArray) {
        renderAll(context)
        enqueue(context, false)
    }

    companion object {
        private const val PREFS = "frog_lights_widget"
        private const val WORK_NAME = "frog_lights_widget_work"
        private const val STATE = "state"

        fun enqueue(context: Context, toggle: Boolean) {
            val widgets = AppWidgetManager.getInstance(context).getAppWidgetIds(
                ComponentName(context, AllLightsWidgetProvider::class.java)
            )
            if (widgets.isEmpty()) return
            val request = OneTimeWorkRequestBuilder<LampWidgetWorker>()
                .setInputData(Data.Builder().putBoolean("toggle", toggle).build())
                .build()
            WorkManager.getInstance(context).enqueueUniqueWork(
                WORK_NAME, ExistingWorkPolicy.APPEND_OR_REPLACE, request
            )
        }

        fun setState(context: Context, state: String) {
            val preferences = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            if (preferences.getString(STATE, "off") == state) return
            preferences.edit().putString(STATE, state).apply()
            renderAll(context)
        }

        fun renderAll(context: Context) {
            val manager = AppWidgetManager.getInstance(context)
            val component = ComponentName(context, AllLightsWidgetProvider::class.java)
            val ids = manager.getAppWidgetIds(component)
            if (ids.isEmpty()) return
            Log.d("FrogWidget", "Render all lights widget")
            val state = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
                .getString(STATE, "off") ?: "off"
            for (id in ids) {
                val views = RemoteViews(context.packageName, R.layout.all_lights_widget)
                val background = when (state) {
                    "on" -> R.drawable.widget_on
                    "unavailable" -> R.drawable.widget_unavailable
                    else -> R.drawable.widget_off
                }
                views.setInt(R.id.all_lights_widget, "setBackgroundResource", background)
                views.setTextViewText(R.id.widget_label, when (state) {
                    "busy" -> "Wait"
                    "empty" -> "Add light"
                    "unavailable" -> "Offline"
                    else -> "Lights"
                })
                views.setTextColor(R.id.widget_label, Color.WHITE)
                if (state == "on") {
                    views.setTextColor(R.id.widget_label, Color.rgb(30, 41, 59))
                    views.setImageViewResource(R.id.widget_power_icon, R.drawable.widget_power_dark)
                }
                val intent = Intent(context, LampWidgetTapReceiver::class.java).apply {
                    action = LampWidgetTapReceiver.ACTION_TOGGLE
                }
                val pending = PendingIntent.getBroadcast(
                    context, 0, intent, PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
                )
                views.setOnClickPendingIntent(R.id.all_lights_widget, pending)
                manager.updateAppWidget(id, views)
            }
        }
    }
}

class LampWidgetTapReceiver : android.content.BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action != ACTION_TOGGLE) return
        AllLightsWidgetProvider.enqueue(context, true)
    }

    companion object {
        const val ACTION_TOGGLE = "com.wgmlgz.frog.TOGGLE_ALL_LIGHTS"
    }
}

class LampWidgetWorker(context: Context, params: WorkerParameters) : Worker(context, params) {
    override fun doWork(): Result {
        val config = File(applicationContext.dataDir, "frog.toml")
        return try {
            val response = JSONObject(LampWidgetNative.run(config.absolutePath, inputData.getBoolean("toggle", false)))
            val state = response.optString("state", "unavailable")
            AllLightsWidgetProvider.setState(applicationContext, state)
            val error = response.optString("error")
            if (error.isNotEmpty()) {
                Log.w("FrogWidget", error)
                Result.failure()
            } else {
                Result.success()
            }
        } catch (error: Exception) {
            Log.w("FrogWidget", "Could not update lights", error)
            AllLightsWidgetProvider.setState(applicationContext, "unavailable")
            Result.failure()
        }
    }
}
