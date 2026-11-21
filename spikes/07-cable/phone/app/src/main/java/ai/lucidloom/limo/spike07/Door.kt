package ai.lucidloom.limo.spike07

import java.security.MessageDigest

/**
 * The guarded door's rules, apart from Android so that the JVM tests run
 * them: what a write may hold, its check, and the status row the PC reads.
 */
object Door {
    /** The most a write may hold: about an offer's size, with room to spare. */
    const val MAX_BYTES = 1024

    /** The status row's columns, in the order the PC reads them. */
    val COLUMNS = arrayOf(
        "takes", "secret", "bytes", "check", "caller_uid", "why",
        "loopbacks", "loopback", "loopback_bytes", "loopback_ms", "loopback_why",
    )

    /** What became of one write. */
    sealed class Taken {
        /** Taken: how many bytes, and their check. */
        data class Ok(val bytes: Int, val check: String) : Taken()

        /** Refused, in one word. */
        data class Refused(val why: String, val bytes: Int) : Taken()
    }

    /**
     * Judges the first [length] bytes of [bytes]; [overflow] when the
     * writer sent more than [MAX_BYTES]. The text must be printable ASCII,
     * as an offer's is; line ends and spaces at its end are let through.
     */
    fun judge(bytes: ByteArray, length: Int, overflow: Boolean): Taken {
        if (overflow) return Taken.Refused("too_long", length)
        if (length == 0) return Taken.Refused("empty", 0)
        var end = length
        while (end > 0 && (bytes[end - 1] == '\n'.code.toByte() || bytes[end - 1] == '\r'.code.toByte() || bytes[end - 1] == ' '.code.toByte())) {
            end -= 1
        }
        if (end == 0) return Taken.Refused("empty", length)
        for (i in 0 until end) {
            val b = bytes[i].toInt() and 0xff
            if (b < 0x21 || b > 0x7e) return Taken.Refused("not_text", length)
        }
        return Taken.Ok(length, check(bytes, length))
    }

    /**
     * The first 8 hex digits, lowercase, of the SHA-256 of the first
     * [length] bytes: the same check the PC prints for what it sent.
     */
    fun check(bytes: ByteArray, length: Int): String {
        val digest = MessageDigest.getInstance("SHA-256")
        digest.update(bytes, 0, length)
        return digest.digest().take(4).joinToString("") { "%02x".format(it.toInt() and 0xff) }
    }

    /** The status row, in [COLUMNS] order. */
    fun row(s: SpikeState.Snapshot): Array<Any> = arrayOf(
        s.takes, s.secret, s.bytes, s.check, s.callerUid, s.why,
        s.loopbacks, s.loopback, s.loopbackBytes, s.loopbackMs, s.loopbackWhy,
    )
}
