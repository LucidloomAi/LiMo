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
 * 256 KiB, stops sending, and checks that exactly the same bytes come back.
 * Reading runs while writing, so neither side can wait on the other.
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
        try {
            Socket().use { socket ->
                socket.connect(InetSocketAddress(InetAddress.getByName("127.0.0.1"), port), connectTimeoutMs)
                socket.soTimeout = readTimeoutMs
                var writeError: Exception? = null
                val writer = Thread {
                    try {
                        socket.getOutputStream().write(sent)
                        socket.getOutputStream().flush()
                        socket.shutdownOutput()
                    } catch (e: Exception) {
                        writeError = e
                    }
                }
                writer.start()
                val back = ByteArrayOutputStream()
                val buffer = ByteArray(64 * 1024)
                val input = socket.getInputStream()
                while (true) {
                    val n = input.read(buffer)
                    if (n < 0) break
                    back.write(buffer, 0, n)
                    if (back.size() > BYTES) break
                }
                writer.join(readTimeoutMs.toLong())
                if (writeError != null) return Result(false, back.size().toLong(), ms(), "write_failed")
                val got = back.toByteArray()
                return when {
                    got.size < BYTES -> Result(false, got.size.toLong(), ms(), "cut_short")
                    got.size > BYTES -> Result(false, got.size.toLong(), ms(), "too_much")
                    !got.contentEquals(sent) -> Result(false, got.size.toLong(), ms(), "mismatch")
                    else -> Result(true, got.size.toLong(), ms(), "none")
                }
            }
        } catch (e: ConnectException) {
            return Result(false, 0, ms(), "refused")
        } catch (e: SocketTimeoutException) {
            return Result(false, 0, ms(), "timeout")
        } catch (e: Exception) {
            return Result(false, 0, ms(), "error")
        }
    }
}
