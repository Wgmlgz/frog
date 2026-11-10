package com.wgmlgz.frog.wear

import android.content.Intent
import android.net.Uri
import android.util.Log
import androidx.wear.protolayout.ActionBuilders
import androidx.wear.protolayout.ColorBuilders
import androidx.wear.protolayout.DimensionBuilders
import androidx.wear.protolayout.LayoutElementBuilders
import androidx.wear.protolayout.ModifiersBuilders
import androidx.wear.protolayout.ResourceBuilders
import androidx.wear.protolayout.TimelineBuilders
import androidx.wear.remote.interactions.RemoteActivityHelper
import androidx.wear.tiles.RequestBuilders
import androidx.wear.tiles.TileBuilders
import androidx.wear.tiles.TileService
import com.google.common.util.concurrent.Futures
import com.google.common.util.concurrent.ListenableFuture
import com.google.common.util.concurrent.MoreExecutors
import java.util.UUID

class FrogTileService : TileService() {
    override fun onTileRequest(requestParams: RequestBuilders.TileRequest): ListenableFuture<TileBuilders.Tile> {
        FrogWearClient.expire(this)
        val preferences = FrogWearClient.prefs(this)
        val clicked = requestParams.currentState.lastClickableId
        val action = clicked.substringBefore('|')
        if ('|' in clicked && action in setOf("lights", "power", "audio", "phone") &&
            clicked != preferences.getString("last_tile_click", "")) {
            preferences.edit().putString("last_tile_click", clicked).apply()
            if (action == "phone") openPhone() else FrogWearClient.send(this, action, fromTile = true)
        } else if (clicked.isEmpty() &&
            System.currentTimeMillis() - preferences.getLong("last_tile_status", 0) > 30_000) {
            preferences.edit().putLong("last_tile_status", System.currentTimeMillis()).apply()
            FrogWearClient.send(this, "status", fromTile = true)
        }

        val nonce = UUID.randomUUID().toString()
        val lightsOn = preferences.getString("lights", "") == "on"
        val tvOn = preferences.getString("power", "") == "on"
        val audioIcon = when (preferences.getString("output", "")) {
            "tv_speaker" -> "speaker"; "bt_soundbar" -> "headphones"; else -> "switch"
        }
        val root = LayoutElementBuilders.Column.Builder()
            .setWidth(DimensionBuilders.dp(224f))
            .setHorizontalAlignment(LayoutElementBuilders.HORIZONTAL_ALIGN_CENTER)
            .addContent(label("Frog", 17f))
            .addContent(vertical(13f))
            .addContent(LayoutElementBuilders.Row.Builder()
                .setVerticalAlignment(LayoutElementBuilders.VERTICAL_ALIGN_CENTER)
                .addContent(label("Lights", 17f))
                .addContent(horizontal(8f))
                .addContent(actionButton("lights|$nonce", if (lightsOn) "power_on" else "power_off", lightsOn))
                .build())
            .addContent(vertical(10f))
            .addContent(LayoutElementBuilders.Row.Builder()
                .setVerticalAlignment(LayoutElementBuilders.VERTICAL_ALIGN_CENTER)
                .addContent(label("TV", 17f))
                .addContent(horizontal(8f))
                .addContent(actionButton("power|$nonce", if (tvOn) "power_on" else "power_off", tvOn))
                .addContent(horizontal(18f))
                .addContent(label("Phone", 17f))
                .addContent(horizontal(8f))
                .addContent(actionButton("phone|$nonce", "phone", false))
                .build())
            .addContent(vertical(10f))
            .addContent(LayoutElementBuilders.Row.Builder()
                .setVerticalAlignment(LayoutElementBuilders.VERTICAL_ALIGN_CENTER)
                .addContent(label("TV audio", 17f))
                .addContent(horizontal(8f))
                .addContent(actionButton("audio|$nonce", audioIcon, true))
                .build())
            .build()
        val layout = LayoutElementBuilders.Layout.Builder().setRoot(root).build()
        val timeline = TimelineBuilders.Timeline.Builder()
            .addTimelineEntry(TimelineBuilders.TimelineEntry.Builder().setLayout(layout).build()).build()
        return Futures.immediateFuture(TileBuilders.Tile.Builder().setResourcesVersion("2")
            .setTileTimeline(timeline).build())
    }

    override fun onTileResourcesRequest(requestParams: RequestBuilders.ResourcesRequest):
        ListenableFuture<ResourceBuilders.Resources> = Futures.immediateFuture(
            ResourceBuilders.Resources.Builder().setVersion("2")
                .addIdToImageMapping("power_on", drawable(R.drawable.wear_power_on))
                .addIdToImageMapping("power_off", drawable(R.drawable.wear_power_off))
                .addIdToImageMapping("headphones", drawable(R.drawable.wear_headphones))
                .addIdToImageMapping("speaker", drawable(R.drawable.wear_speaker))
                .addIdToImageMapping("switch", drawable(R.drawable.wear_switch))
                .addIdToImageMapping("phone", drawable(R.drawable.wear_phone))
                .build())

    private fun drawable(id: Int) = ResourceBuilders.ImageResource.Builder()
        .setAndroidResourceByResId(ResourceBuilders.AndroidImageResourceByResId.Builder()
            .setResourceId(id).build()).build()

    private fun label(value: String, size: Float) = LayoutElementBuilders.Text.Builder()
        .setText(value)
        .setFontStyle(LayoutElementBuilders.FontStyle.Builder()
            .setSize(DimensionBuilders.sp(size))
            .setColor(ColorBuilders.argb(WHITE))
            .setWeight(LayoutElementBuilders.FONT_WEIGHT_BOLD).build())
        .build()

    private fun vertical(height: Float) = LayoutElementBuilders.Spacer.Builder()
        .setHeight(DimensionBuilders.dp(height)).build()
    private fun horizontal(width: Float) = LayoutElementBuilders.Spacer.Builder()
        .setWidth(DimensionBuilders.dp(width)).build()

    private fun actionButton(id: String, icon: String, filled: Boolean): LayoutElementBuilders.LayoutElement {
        val click = ModifiersBuilders.Clickable.Builder().setId(id)
            .setOnClick(ActionBuilders.LoadAction.Builder().build()).build()
        val image = LayoutElementBuilders.Image.Builder().setResourceId(icon)
            .setWidth(DimensionBuilders.dp(18f)).setHeight(DimensionBuilders.dp(18f)).build()
        val modifiers = ModifiersBuilders.Modifiers.Builder().setClickable(click)
        if (filled) modifiers.setBackground(ModifiersBuilders.Background.Builder()
            .setColor(ColorBuilders.argb(WHITE))
            .setCorner(ModifiersBuilders.Corner.Builder().setRadius(DimensionBuilders.dp(18f)).build())
            .build())
        return LayoutElementBuilders.Box.Builder()
            .setWidth(DimensionBuilders.dp(36f)).setHeight(DimensionBuilders.dp(36f))
            .setHorizontalAlignment(LayoutElementBuilders.HORIZONTAL_ALIGN_CENTER)
            .setVerticalAlignment(LayoutElementBuilders.VERTICAL_ALIGN_CENTER)
            .setModifiers(modifiers.build())
            .addContent(image).build()
    }

    private fun openPhone() {
        val intent = Intent(Intent.ACTION_VIEW, Uri.parse("frog://open"))
            .addCategory(Intent.CATEGORY_BROWSABLE)
        val future = RemoteActivityHelper(this).startRemoteActivity(intent, null)
        future.addListener({
            try { future.get() } catch (error: Exception) {
                Log.w("FrogWear", "Could not open Frog on phone", error)
            }
        }, MoreExecutors.directExecutor())
    }

    companion object { private const val WHITE = 0xffffffff.toInt() }
}
