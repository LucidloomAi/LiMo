package ai.lucidloom.limo.spike05

import android.app.Application

/**
 * Runs whenever Android starts this app's process, whatever part of the app
 * it was started for. The dead man's timer is checked right away.
 */
class SpikeApp : Application() {

    override fun onCreate() {
        super.onCreate()
        val app = this
        Strict.worker.execute { Strict.check(app, "process_start") }
    }
}
