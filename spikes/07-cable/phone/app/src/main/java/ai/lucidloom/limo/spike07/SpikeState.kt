package ai.lucidloom.limo.spike07

/**
 * What the test app has seen since its process started, in memory only:
 * the door's last write (never what it held: its length and check) and
 * the loopback's last run. Nothing is written to the phone's storage.
 */
object SpikeState {
    /** One moment's copy. */
    data class Snapshot(
        val takes: Long = 0,
        val secret: String = "none",
        val bytes: Long = 0,
        val check: String = "none",
        val callerUid: Long = -1,
        val why: String = "none",
        val loopbacks: Long = 0,
        val loopback: String = "none",
        val loopbackBytes: Long = 0,
        val loopbackMs: Long = 0,
        val loopbackWhy: String = "none",
    )

    private var now = Snapshot()
    private val listeners = mutableListOf<() -> Unit>()

    /** The state now. */
    @Synchronized
    fun snapshot(): Snapshot = now

    /** A write to the door, taken or refused, by the program with [uid]. */
    fun recordWrite(taken: Door.Taken, uid: Int) {
        synchronized(this) {
            now = when (taken) {
                is Door.Taken.Ok -> now.copy(
                    takes = now.takes + 1, secret = "taken", bytes = taken.bytes.toLong(),
                    check = taken.check, callerUid = uid.toLong(), why = "none",
                )
                is Door.Taken.Refused -> now.copy(
                    takes = now.takes + 1, secret = "refused", bytes = taken.bytes.toLong(),
                    check = "none", callerUid = uid.toLong(), why = taken.why,
                )
            }
        }
        changed()
    }

    /** A loopback run finished. */
    fun recordLoopback(result: Loopback.Result) {
        synchronized(this) {
            now = now.copy(
                loopbacks = now.loopbacks + 1,
                loopback = if (result.ok) "ok" else "failed",
                loopbackBytes = result.bytes,
                loopbackMs = result.ms,
                loopbackWhy = result.why,
            )
        }
        changed()
    }

    /** Calls [listener] after every change, until removed. */
    @Synchronized
    fun listen(listener: () -> Unit) {
        listeners.add(listener)
    }

    /** Stops calling [listener]. */
    @Synchronized
    fun unlisten(listener: () -> Unit) {
        listeners.remove(listener)
    }

    private fun changed() {
        val each = synchronized(this) { listeners.toList() }
        each.forEach { it() }
    }

    /** Back to nothing seen: for the tests. */
    @Synchronized
    fun reset() {
        now = Snapshot()
        listeners.clear()
    }
}
