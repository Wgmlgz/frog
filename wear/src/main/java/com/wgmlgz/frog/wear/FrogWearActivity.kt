package com.wgmlgz.frog.wear

import android.app.Activity
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.graphics.Color
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.os.Bundle
import android.view.Gravity
import android.view.View
import android.widget.Button
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import org.json.JSONArray

class FrogWearActivity : Activity() {
    private val updates = object : BroadcastReceiver() {
        override fun onReceive(context: Context, intent: Intent) = render()
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        window.decorView.setBackgroundColor(Color.BLACK)
        render()
    }

    override fun onResume() {
        super.onResume()
        registerReceiver(updates, IntentFilter(FrogWearClient.CHANGED), RECEIVER_NOT_EXPORTED)
        FrogWearClient.send(this, "status")
    }

    override fun onPause() {
        unregisterReceiver(updates)
        super.onPause()
    }

    private fun render() {
        FrogWearClient.expire(this)
        val preferences = FrogWearClient.prefs(this)
        val pending = preferences.getString("pending", "") ?: ""
        val pendingActive = pending.isNotEmpty() &&
            System.currentTimeMillis() - preferences.getLong("pending_since", 0) < 30_000
        val error = preferences.getString("error", "") ?: ""
        val root = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            gravity = Gravity.CENTER_HORIZONTAL
            setPadding(dp(24), dp(22), dp(24), dp(34))
        }
        val scroll = ScrollView(this).apply {
            isFillViewport = true
            addView(root)
        }
        setContentView(scroll)
        root.addView(label("Frog", 23f, true))
        root.addView(label(when {
            pendingActive -> "Working…"
            error.isNotEmpty() -> error
            else -> "Home controls"
        }, 13f, false, if (error.isNotEmpty()) Color.rgb(255, 170, 170) else Color.LTGRAY))

        val lights = preferences.getString("lights", "unknown") ?: "unknown"
        root.addView(button("All lights  ·  ${lights.replaceFirstChar { it.uppercase() }}", !pendingActive) {
            FrogWearClient.send(this, "lights")
        })

        val devices = try { JSONArray(preferences.getString("devices", "[]")) } catch (_: Exception) { JSONArray() }
        val tvs = (0 until devices.length()).mapNotNull { devices.optJSONObject(it) }
            .filter { it.optString("type") == "tv" }
        val lamps = (0 until devices.length()).mapNotNull { devices.optJSONObject(it) }
            .filter { it.optString("type") == "lamp" }
        val selected = FrogWearClient.selectedTv(this)
        if (tvs.isNotEmpty()) {
            root.addView(label("TV", 17f, true))
            if (tvs.size > 1) root.addView(button(
                tvs.firstOrNull { it.optString("id") == selected }?.optString("name") ?: "Choose TV", true) {
                val current = tvs.indexOfFirst { it.optString("id") == selected }
                FrogWearClient.selectTv(this, tvs[(current + 1) % tvs.size].optString("id"))
            })
            val power = preferences.getString("power", "unknown") ?: "unknown"
            root.addView(button("Power  ·  ${power.replaceFirstChar { it.uppercase() }}", !pendingActive) {
                FrogWearClient.send(this, "power")
            })
            val output = preferences.getString("output", "")
            root.addView(label("Audio  ·  " + when (output) {
                "tv_speaker" -> "TV speakers"; "bt_soundbar" -> "Headphones"; else -> "Unknown"
            }, 14f))
            val row = LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL }
            row.addView(button("Speaker", !pendingActive) { FrogWearClient.send(this, "speaker") },
                LinearLayout.LayoutParams(0, dp(48), 1f))
            row.addView(button("Headphones", !pendingActive) { FrogWearClient.send(this, "headphones") },
                LinearLayout.LayoutParams(0, dp(48), 1f))
            root.addView(row)
            val volume = preferences.getInt("volume", -1)
            root.addView(label("Volume  ·  ${if (volume < 0) "—" else "$volume%"}", 14f))
            val volumeRow = LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL }
            volumeRow.addView(button("−", !pendingActive) { FrogWearClient.send(this, "down") },
                LinearLayout.LayoutParams(0, dp(48), 1f))
            volumeRow.addView(button("+", !pendingActive) { FrogWearClient.send(this, "up") },
                LinearLayout.LayoutParams(0, dp(48), 1f))
            root.addView(volumeRow)
        }
        if (lamps.isNotEmpty()) {
            root.addView(label("Lamps", 17f, true))
            lamps.forEach { lamp ->
                root.addView(button(lamp.optString("name", "Lamp"), !pendingActive) {
                    FrogWearClient.send(this, "lamp", lamp.optString("id"))
                })
            }
        }
        root.addView(button("Refresh", !pendingActive) { FrogWearClient.send(this, "status") })
    }

    private fun label(value: String, size: Float, bold: Boolean = false, color: Int = Color.WHITE): View =
        TextView(this).apply {
            text = value
            textSize = size
            setTextColor(color)
            gravity = Gravity.CENTER
            if (bold) typeface = Typeface.DEFAULT_BOLD
            setPadding(0, dp(7), 0, dp(7))
        }

    private fun button(value: String, enabled: Boolean, onTap: () -> Unit): Button = Button(this).apply {
        text = value
        textSize = 13f
        isAllCaps = false
        isEnabled = enabled
        setTextColor(if (enabled) Color.BLACK else Color.LTGRAY)
        background = GradientDrawable().apply {
            setColor(if (enabled) Color.WHITE else Color.DKGRAY)
            cornerRadius = dp(24).toFloat()
        }
        setPadding(dp(10), 0, dp(10), 0)
        minHeight = dp(48)
        layoutParams = LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, dp(48)).apply {
            setMargins(0, dp(4), 0, dp(4))
        }
        setOnClickListener { onTap() }
    }

    private fun dp(value: Int) = (value * resources.displayMetrics.density).toInt()
}
