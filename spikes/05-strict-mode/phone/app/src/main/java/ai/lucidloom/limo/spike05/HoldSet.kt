package ai.lucidloom.limo.spike05

/**
 * Broadcasts this app is still holding open (with goAsync), so its process
 * stays alive until the work they started is done. Pure, so it runs in plain
 * JVM unit tests: a hold is just "the call that finishes the broadcast".
 *
 * Why it exists: Android delivers this app's broadcasts one after another,
 * and it removes an admin only after it has delivered "admin disabled" to
 * this app. A release that waits for the admin to go while one of this app's
 * broadcasts is still open waits on itself: on the phone that showed as a 5
 * second wait that failed, twice, in the timer path. So just before that
 * wait the release finishes every held broadcast ([finishAllAndClose]), and
 * until the wait is over a new broadcast is not held at all ([hold] finishes
 * it at once and reports [Hold.held] false). The work each broadcast started
 * still runs; only the hold on Android's broadcast queue is let go.
 */
class HoldSet {

    class Hold internal constructor(private val finisher: () -> Unit, val held: Boolean) {
        private var done = false

        /** Finishes the broadcast once; later calls do nothing. */
        fun finish() {
            val first = synchronized(this) {
                if (done) false else {
                    done = true
                    true
                }
            }
            if (first) {
                try {
                    finisher()
                } catch (_: IllegalStateException) {
                    // Already finished by Android itself: nothing to do.
                }
            }
        }
    }

    private val lock = Any()
    private val held = ArrayList<Hold>()
    private var open = true

    /** Holds a broadcast, unless a release is waiting on Android: then it is finished at once. */
    fun hold(finisher: () -> Unit): Hold {
        synchronized(lock) {
            if (open) {
                val h = Hold(finisher, held = true)
                held.add(h)
                return h
            }
        }
        val h = Hold(finisher, held = false)
        h.finish()
        return h
    }

    /** The work a broadcast started is done: finish it (if not already). */
    fun done(h: Hold) {
        synchronized(lock) { held.remove(h) }
        h.finish()
    }

    /** Before waiting on Android: finish every held broadcast, and hold no new one until [reopen]. */
    fun finishAllAndClose(): Int {
        val all = synchronized(lock) {
            open = false
            val copy = held.toList()
            held.clear()
            copy
        }
        for (h in all) h.finish()
        return all.size
    }

    fun reopen() {
        synchronized(lock) { open = true }
    }

    /** How many are held now (for tests and the log). */
    fun count(): Int = synchronized(lock) { held.size }
}
