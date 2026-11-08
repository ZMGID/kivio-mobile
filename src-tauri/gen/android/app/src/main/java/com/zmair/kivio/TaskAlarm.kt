package com.zmair.kivio

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent

/** Inexact while-idle alarm. Brings the app forward so the scheduler can catch up. */
class TaskAlarm : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent?) {
        val launch = Intent(context, MainActivity::class.java).apply {
            addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)
            action = intent?.action
        }
        context.startActivity(launch)
    }
}
