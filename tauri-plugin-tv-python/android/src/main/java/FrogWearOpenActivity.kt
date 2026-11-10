package com.wgmlgz.frog.tvpython

import android.app.Activity
import android.content.Intent
import android.os.Bundle

/** A browsable entry point for the watch's explicit Open on phone action. */
class FrogWearOpenActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        packageManager.getLaunchIntentForPackage(packageName)?.apply {
            addFlags(Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_NEW_TASK)
        }?.let(::startActivity)
        finish()
    }
}
