package io.github.lobojoel2003.hushcap

import android.app.Application
import android.app.NotificationChannel
import android.app.NotificationManager
import io.github.lobojoel2003.hushcap.model.ModelManager
import io.github.lobojoel2003.hushcap.settings.Settings

class HushcapApp : Application() {
    lateinit var settings: Settings
        private set
    lateinit var models: ModelManager
        private set

    override fun onCreate() {
        super.onCreate()
        settings = Settings(this)
        models = ModelManager(this)
        val channel = NotificationChannel(
            CHANNEL_CAPTIONS,
            getString(R.string.notification_channel_name),
            NotificationManager.IMPORTANCE_LOW,
        ).apply {
            description = getString(R.string.notification_channel_description)
            setShowBadge(false)
        }
        getSystemService(NotificationManager::class.java).createNotificationChannel(channel)
    }

    companion object {
        const val CHANNEL_CAPTIONS = "captions"
    }
}
