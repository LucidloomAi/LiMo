package ai.lucidloom.limo.spike07

import java.io.ByteArrayOutputStream
import java.net.ConnectException
import java.net.InetAddress
import java.net.InetSocketAddress
import java.net.Socket
import java.net.SocketTimeoutException

/**
 * The loopback client: connects to 127.0.0.1 at a port on the phone (which
 * the PC's reverse carries over the cable to the PC's own loopback), sends
 * 256 KiB, reads them back, checks that exactly the same bytes came, and
 * only then closes.
 *
 * It never stops sending to say it is done (no `shutdownOutput`): adb's
 * streams have no half close, so a side that stops sending ends the whole
 * stream, and the bytes still on their way back are lost. The first run on
 * the phone showed exactly that (`cut_short`). Reading runs while writing,
 * so neither side waits on the other.
 */
object Loopback {
    /** How many bytes go each way. */
    const val BYTES = 256 * 1024

    /** How one run went. */
    data class Result(val ok: Boolean, val bytes: Long, val ms: Long, val why: String)

    /** The bytes sent: a pattern, nothing secret. */
    fun pattern(): ByteArray = ByteArray(BYTES) { (it % 253).toByte() }

    /** One run to [port]. */
    fun run(port: Int, connectTimeoutMs: Int = 5_000, readTimeoutMs: Int = 20_000): Result {
        val started = System.nanoTime()
        fun ms() = (System.nanoTime() - started) / 1_000_000
        if (port !in 1..65535) return Result(false, 0, 0, "bad_port")
        val sent = pattern()
        val back = ByteArrayOutputStream()
        try {
            Socket().use { socket ->
                socket.connect(InetSocketAddress(InetAddress.getByName("127.0.0.1"), port), connectTimeoutMs)
                socket.soTimeout = readTimeoutMs
                var writeError: Exception? = null
                val writer = Thread {
                    try {
                        socket.getOutputStream().write(sent)
                        socket.getOutputStream().flush()
                    } catch (e: Exception) {
                        writeError = e
                    }
                }
                writer.start()
                val buffer = ByteArray(64 * 1024)
                val input = socket.getInputStream()
                // Exactly as many bytes as were sent, then close: never wait
                // for the other side to close first.
                while (back.size() < BYTES) {
                    val n = input.read(buffer, 0, minOf(buffer.size, BYTES - back.size()))
                    if (n < 0) break
                    back.write(buffer, 0, n)
                }
                writer.join(readTimeoutMs.toLong())
                if (writeError != null) return Result(false, back.size().toLong(), ms(), "write_failed")
            }
        } catch (e: ConnectException) {
            return Result(false, 0, ms(), "refused")
        } catch (e: SocketTimeoutException) {
            return Result(false, back.size().toLong(), ms(), "timeout")
        } catch (e: Exception) {
            return Result(false, back.size().toLong(), ms(), "error")
        }
        val got = back.toByteArray()
        return when {
            got.size < BYTES -> Result(false, got.size.toLong(), ms(), "cut_short")
            !got.contentEquals(sent) -> Result(false, got.size.toLong(), ms(), "mismatch")
            else -> Result(true, got.size.toLong(), ms(), "none")
        }
    }
}
