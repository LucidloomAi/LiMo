package ai.lucidloom.limo.spike04

import java.net.Inet4Address
import java.net.InetAddress
import kotlin.random.Random

/**
 * The pure parts: the protocol, the log line formats, and small checks.
 * No Android classes here, so all of it is unit tested on the JVM.
 *
 * The protocol, one line each way:
 *   phone: "HELLO <nonce>\n"
 *   PC:    "LIMO <id> <version> <unix time> <nonce>\n", then it closes.
 * The PC answers "ERR\n" to anything else. A line is at most 256 bytes.
 */
object Protocol {
    /** The DNS-SD service type, as NsdManager wants it (no ".local."). */
    const val SERVICE_TYPE = "_limo-spike._tcp"
    const val DEFAULT_PORT = 47810
    const val MAX_LINE = 256
    const val MAX_NONCE = 64

    /** Connecting may take this long. */
    const val CONNECT_LIMIT_MS = 3000
    /** The whole exchange may take this long, so a repeat (every 5 s) never overlaps the next. */
    const val EXCHANGE_LIMIT_MS = 4000L

    /** How long a search runs before it is stopped. */
    const val SEARCH_MS = 30_000L
    /** The stability repeat: one HELLO every 5 s, 12 times, so 60 s. */
    const val REPEAT_EVERY_MS = 5_000L
    const val REPEAT_COUNT = 12

    fun isValidNonce(s: String): Boolean =
        s.isNotEmpty() && s.length <= MAX_NONCE &&
            s.all { (it in 'a'..'z') || (it in 'A'..'Z') || (it in '0'..'9') || it == '-' || it == '_' }

    /** 8 lowercase hex characters. */
    fun newNonce(random: Random = Random.Default): String =
        (random.nextInt().toLong() and 0xffffffffL).toString(16).padStart(8, '0')

    fun helloLine(nonce: String): String = "HELLO $nonce\n"

    data class Reply(val id: String, val version: String, val unixTime: Long, val nonce: String)

    /** The reply if it is a proper LIMO line for this nonce, otherwise why not, in plain words. */
    fun checkReply(line: String, sentNonce: String): Result<Reply> {
        val text = line.removeSuffix("\r")
        if (text == "ERR") return fail("the hub answered ERR")
        val p = text.split(' ')
        if (p.size != 5 || p[0] != "LIMO") return fail("the reply is not a LIMO line")
        if (p[1].isEmpty() || p[2].isEmpty()) return fail("the reply has an empty id or version")
        val time = p[3].toLongOrNull() ?: return fail("the reply's time is not a number")
        if (time < 0) return fail("the reply's time is not a number")
        if (p[4] != sentNonce) return fail("the reply's nonce does not match the one sent")
        return Result.success(Reply(p[1], p[2], time, p[4]))
    }

    private fun fail(why: String): Result<Reply> = Result.failure(IllegalArgumentException(why))

    /** Milliseconds with one decimal, from nanoseconds, with no locale involved. */
    fun msOneDecimal(nanos: Long): String {
        val tenths = (nanos.coerceAtLeast(0) + 50_000) / 100_000
        return "${tenths / 10}.${tenths % 10}"
    }

    /** The port from a text field or an extra: a whole number from 1 to 65535. */
    fun parsePort(text: String?): Int? {
        val n = text?.trim()?.toIntOrNull() ?: return null
        return if (n in 1..65535) n else null
    }

    /** A host from a text field or an extra: not empty, no spaces, at most 253 characters. */
    fun checkHost(text: String?): String? {
        val t = text?.trim() ?: return null
        if (t.isEmpty() || t.length > 253) return null
        if (t.any { it.isWhitespace() || it.isISOControl() }) return null
        return t
    }

    /** The address to greet: the first IPv4 one, else the first of any kind. */
    fun pickAddress(addresses: List<InetAddress>): InetAddress? =
        addresses.firstOrNull { it is Inet4Address } ?: addresses.firstOrNull()

    /** The TXT record "id", or "?" if it is missing. Only safe characters are kept. */
    fun txtId(attributes: Map<String, ByteArray?>?): String {
        val raw = attributes?.get("id") ?: return "?"
        val s = String(raw, Charsets.UTF_8)
        return if (isValidNonce(s)) s else "?"
    }

    /** NsdManager failure codes in plain words. */
    fun nsdError(code: Int): String = when (code) {
        0 -> "internal error in Android's discovery service (code 0)"
        3 -> "already active: Android is still busy with an earlier request (code 3)"
        4 -> "too many requests at once (code 4)"
        5 -> "the operation is not running (code 5)"
        6 -> "bad parameters (code 6)"
        else -> "failure code $code"
    }
}

/** The fixed, greppable log lines. Values that may hold spaces are quoted. */
object Lines {
    fun quote(s: String): String {
        val b = StringBuilder(s.length + 2)
        b.append('"')
        for (c in s) {
            when {
                c == '"' -> b.append("\\\"")
                c == '\\' -> b.append("\\\\")
                c.isISOControl() -> b.append("\\x").append(c.code.toString(16).padStart(2, '0'))
                else -> b.append(c)
            }
        }
        b.append('"')
        return b.toString()
    }

    fun searchStarted() = "search started"
    fun searchRunning(afterMs: Long) = "search running after_ms=$afterMs"
    fun searchFailed(error: String) = "search failed error=${quote(error)}"
    fun searchStopped(found: Int, afterMs: Long, reason: String) =
        "search stopped found=$found after_ms=$afterMs reason=$reason"
    fun searchSkipped(reason: String) = "search skipped reason=$reason"

    fun found(name: String, afterMs: Long) = "found name=${quote(name)} after_ms=$afterMs"
    fun foundAgain(name: String, afterMs: Long) = "found again name=${quote(name)} after_ms=$afterMs"
    fun lost(name: String, afterMs: Long) = "lost name=${quote(name)} after_ms=$afterMs"

    fun resolved(name: String, host: String, port: Int, id: String, afterMs: Long) =
        "resolved name=${quote(name)} host=$host port=$port id=$id after_ms=$afterMs"
    fun resolveFailed(name: String, error: String) =
        "resolve failed name=${quote(name)} error=${quote(error)}"

    // The fixed keys come first, in the agreed order; the extra ones (connect
    // time, which run, which network) follow at the end.
    fun helloOk(host: String, port: Int, run: String, net: String, rttMs: String, connectMs: String, reply: String) =
        "hello ok host=$host port=$port rtt_ms=$rttMs reply=${quote(reply)} connect_ms=$connectMs run=$run net=$net"
    fun helloFailed(host: String, port: Int, run: String, net: String, error: String) =
        "hello failed host=$host port=$port error=${quote(error)} run=$run net=$net"

    fun repeatStarted(host: String, port: Int) =
        "repeat started host=$host port=$port every_ms=${Protocol.REPEAT_EVERY_MS} count=${Protocol.REPEAT_COUNT}"
    fun repeatDone(ok: Int, failed: Int, slowestMs: String) =
        "repeat done ok=$ok failed=$failed slowest_ms=$slowestMs"
    fun repeatStopped(ok: Int, failed: Int, slowestMs: String, reason: String) =
        "repeat stopped ok=$ok failed=$failed slowest_ms=$slowestMs reason=$reason"

    fun wifi(connected: Boolean, addresses: List<String>) =
        if (connected) "wifi connected=yes address=${addresses.joinToString(",").ifEmpty { "none" }}"
        else "wifi connected=no"
}

/** Counts the stability repeat: how many succeeded, how many failed, the slowest success. */
class RepeatTally(val total: Int = Protocol.REPEAT_COUNT) {
    var ok = 0
        private set
    var failed = 0
        private set
    private var slowestNanos = -1L

    val count: Int get() = ok + failed
    val done: Boolean get() = count >= total

    fun record(success: Boolean, rttNanos: Long) {
        if (done) return
        if (success) {
            ok++
            if (rttNanos > slowestNanos) slowestNanos = rttNanos
        } else {
            failed++
        }
    }

    fun slowestMs(): String = if (slowestNanos < 0) "none" else Protocol.msOneDecimal(slowestNanos)

    fun doneLine(): String = Lines.repeatDone(ok, failed, slowestMs())
    fun stoppedLine(reason: String): String = Lines.repeatStopped(ok, failed, slowestMs(), reason)
}
