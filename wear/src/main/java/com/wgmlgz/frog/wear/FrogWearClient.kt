package com.wgmlgz.frog.wear

import android.content.Context
import android.content.Intent
import android.util.Log
import androidx.wear.tiles.TileService
import com.google.android.gms.wearable.MessageEvent
import com.google.android.gms.wearable.Wearable
import com.google.android.gms.wearable.WearableListenerService
import org.json.JSONObject
import java.util.UUID

internal object FrogWearClient {
    const val CHANGED = "com.wgmlgz.frog.wear.CHANGED"
    private const val PREFS = "frog_wear"
    private val actions = setOf("status", "lights", "lamp", "power", "audio", "speaker", "headphones", "up", "down")

    fun prefs(context: Context) = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
    fun selectedTv(context: Context) = prefs(context).getString("selected_tv", "") ?: ""
    fun selectTv(context: Context, id: String) {
        prefs(context).edit().putString("selected_tv", id).apply()
        changed(context)
        send(context, "status")
    }

    fun send(context: Context, action: String, device: String = selectedTv(context), fromTile: Boolean = false) {
        if (action !in actions) return
        val preferences = prefs(context)
        val pendingSince = preferences.getLong("pending_since", 0)
        val pending = preferences.getString("pending", "") ?: ""
        if (pending.isNotEmpty() && System.currentTimeMillis() - pendingSince < 30_000 &&
            (action == "status" || pending != "status")) return
        val command = if (action == "audio") {
            if (preferences.getString("output", "") == "tv_speaker") "headphones" else "speaker"
        } else action
        val id = UUID.randomUUID().toString()
        val editor = preferences.edit().putString("pending", action).putString("request_id", id)
            .putLong("pending_since", System.currentTimeMillis()).remove("error")
        when (action) {
            "lights" -> {
                val old = preferences.getString("lights", "unknown") ?: "unknown"
                editor.putString("previous_lights", old)
                    .putString("lights", if (old == "on") "off" else "on")
            }
            "power" -> {
                val old = preferences.getString("power", "unknown") ?: "unknown"
                editor.putString("previous_power", old)
                    .putString("power", if (old == "on") "off" else "starting")
            }
            "audio", "speaker", "headphones" -> {
                editor.putString("previous_output", preferences.getString("output", "") ?: "")
                    .putString("output", if (command == "speaker") "tv_speaker" else "bt_soundbar")
            }
        }
        editor.apply()
        changed(context, updateTile = !fromTile)
        val request = JSONObject().put("id", id).put("action", command).put("device", device).toString()
        Wearable.getNodeClient(context).connectedNodes.addOnSuccessListener { nodes ->
            if (nodes.isEmpty()) {
                fail(context, id, "Phone not connected")
            } else {
                Wearable.getMessageClient(context).sendMessage(nodes.first().id, "/frog/request",
                    request.toByteArray(Charsets.UTF_8)).addOnFailureListener { error ->
                    fail(context, id, error.message ?: "Could not reach phone")
                }
            }
        }.addOnFailureListener { error -> fail(context, id, error.message ?: "Phone not connected") }
    }

    fun receive(context: Context, message: JSONObject) {
        val preferences = prefs(context)
        val id = message.optString("id")
        if (id != preferences.getString("request_id", "")) return
        val editor = preferences.edit().putString("pending", "").remove("pending_since")
        if (!message.optBoolean("ok")) {
            rollback(preferences, editor)
            editor.putString("error", message.optString("error", "Action failed"))
        } else {
            editor.remove("error")
            for (key in listOf("lights", "power", "output", "lamp")) {
                if (message.has(key)) editor.putString(key, message.optString(key))
            }
            if (message.has("volume")) editor.putInt("volume", message.optInt("volume", -1))
            if (message.has("devices")) {
                editor.putString("devices", message.getJSONArray("devices").toString())
                run {
                    val devices = message.getJSONArray("devices")
                    val current = selectedTv(context)
                    val stillPresent = (0 until devices.length()).mapNotNull { devices.optJSONObject(it) }
                        .any { it.optString("type") == "tv" && it.optString("id") == current }
                    val first = (0 until devices.length()).mapNotNull { devices.optJSONObject(it) }
                        .firstOrNull { it.optString("type") == "tv" }
                    if (!stillPresent && first != null) editor.putString("selected_tv", first.optString("id"))
                }
            }
        }
        editor.remove("previous_lights").remove("previous_power").remove("previous_output")
        editor.apply()
        changed(context)
    }

    fun expire(context: Context) {
        val preferences = prefs(context)
        if ((preferences.getString("pending", "") ?: "").isEmpty() ||
            System.currentTimeMillis() - preferences.getLong("pending_since", 0) < 30_000) return
        val editor = preferences.edit().putString("pending", "").remove("pending_since")
            .putString("error", "Phone did not respond")
        rollback(preferences, editor)
        editor.remove("previous_lights").remove("previous_power").remove("previous_output").apply()
        changed(context)
    }

    private fun rollback(preferences: android.content.SharedPreferences,
                         editor: android.content.SharedPreferences.Editor) {
        if (preferences.contains("previous_lights"))
            editor.putString("lights", preferences.getString("previous_lights", "unknown"))
        if (preferences.contains("previous_power"))
            editor.putString("power", preferences.getString("previous_power", "unknown"))
        if (preferences.contains("previous_output"))
            editor.putString("output", preferences.getString("previous_output", ""))
    }

    private fun fail(context: Context, id: String, error: String) {
        if (prefs(context).getString("request_id", "") != id) return
        val preferences = prefs(context)
        val editor = preferences.edit().putString("pending", "").putString("error", error)
        rollback(preferences, editor)
        editor.remove("previous_lights").remove("previous_power").remove("previous_output").apply()
        changed(context)
    }

    private fun changed(context: Context, updateTile: Boolean = true) {
        context.sendBroadcast(Intent(CHANGED).setPackage(context.packageName))
        if (updateTile) TileService.getUpdater(context).requestUpdate(FrogTileService::class.java)
    }
}

class FrogWearResponseListener : WearableListenerService() {
    override fun onMessageReceived(event: MessageEvent) {
        if (event.path != "/frog/response") return
        try { FrogWearClient.receive(this, JSONObject(String(event.data, Charsets.UTF_8))) }
        catch (error: Exception) { Log.w("FrogWear", "Invalid phone response", error) }
    }
}
