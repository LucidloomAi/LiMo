package ai.lucidloom.limo.spike04

import android.net.Network
import java.io.ByteArrayOutputStream
import java.io.IOException
import java.net.ConnectException
import java.net.InetAddress
import java.net.InetSocketAddress
import java.net.NoRouteToHostException
import java.net.Socket
import java.net.SocketTimeoutException
import java.net.UnknownHostException

/**
 * One HELLO over a plain TCP socket. Blocking: call it off the main thread.
 *
 * When the Wi-Fi network is known, the socket is bound to it, so the HELLO
 * goes over Wi-Fi even if Android has made mobile data the default network.
 */
object HelloClient {

    class Outcome(
        val ok: Boolean,
        /** From the start of connecting to the reply's line end. */
        val rttNanos: Long,
        val connectNanos: Long,
        /** The reply line as received, without its line end. */
        val reply: String,
        /** Empty when ok, otherwise what went wrong in plain words. */
        val error: String,
        /** "wifi" when the socket was bound to the Wi-Fi network, else "default". */
        val net: String,
    )

    fun send(address: InetAddress, port: Int, wifi: Network?): Outcome {
        val net = if (wifi != null) "wifi" else "default"
        val nonce = Protocol.newNonce()
        val start = System.nanoTime()
        var connectNanos = 0L
        var stage = "connect"
        try {
            val socket = wifi?.socketFactory?.createSocket() ?: Socket()
            socket.use { s ->
                s.tcpNoDelay = true
                s.connect(InetSocketAddress(address, port), Protocol.CONNECT_LIMIT_MS)
                connectNanos = System.nanoTime() - start
                stage = "send"
                s.getOutputStream().apply {
                    write(Protocol.helloLine(nonce).toByteArray(Charsets.US_ASCII))
                    flush()
                }
                stage = "reply"
                val reply = readLine(s, start) ?: return Outcome(
                    false, System.nanoTime() - start, connectNanos, "",
                    "no reply: the connection closed with nothing sent back", net,
                )
                val rtt = System.nanoTime() - start
                val checked = Protocol.checkReply(reply, nonce)
                return if (checked.isSuccess) {
                    Outcome(true, rtt, connectNanos, reply, "", net)
                } else {
                    val why = checked.exceptionOrNull()?.message ?: "bad reply"
                    Outcome(false, rtt, connectNanos, reply, "$why: ${Lines.quote(reply)}", net)
                }
            }
        } catch (e: Exception) {
            return Outcome(false, System.nanoTime() - start, connectNanos, "", words(e, stage), net)
        }
    }

    /**
     * Reads to the first line end, at most 256 bytes, within the exchange
     * limit counted from [start]. Null if the PC closed before sending anything.
     */
    private fun readLine(s: Socket, start: Long): String? {
        val input = s.getInputStream()
        val buf = ByteArrayOutputStream()
        val chunk = ByteArray(Protocol.MAX_LINE)
        while (true) {
            val leftMs = Protocol.EXCHANGE_LIMIT_MS - (System.nanoTime() - start) / 1_000_000
            if (leftMs <= 0) throw SocketTimeoutException("no line end in time")
            s.soTimeout = leftMs.toInt()
            val room = Protocol.MAX_LINE - buf.size()
            if (room <= 0) throw IOException("the reply was longer than ${Protocol.MAX_LINE} bytes")
            val n = input.read(chunk, 0, room)
            if (n < 0) {
                if (buf.size() == 0) return null
                throw IOException("the connection closed before a line end: ${Lines.quote(buf.toString(Charsets.UTF_8.name()))}")
            }
            for (i in 0 until n) {
                if (chunk[i] == '\n'.code.toByte()) {
                    buf.write(chunk, 0, i)
                    return buf.toString(Charsets.UTF_8.name())
                }
            }
            buf.write(chunk, 0, n)
        }
    }

    /** An exception in plain words, with the exception's own text after it. */
    fun words(e: Exception, stage: String): String {
        val msg = e.message ?: ""
        val what = when {
            e is SocketTimeoutException && stage == "connect" ->
                "timed out while connecting (${Protocol.CONNECT_LIMIT_MS / 1000} s): nothing answered, so a firewall or the router may be dropping it, or no device has that address"
            e is SocketTimeoutException ->
                "connected, but no reply within ${Protocol.EXCHANGE_LIMIT_MS / 1000} s"
            e is NoRouteToHostException || msg.contains("EHOSTUNREACH") ->
                "no route to that address: is the phone on the same network as the PC?"
            msg.contains("ENETUNREACH") ->
                "network unreachable: is Wi-Fi connected?"
            msg.contains("ETIMEDOUT") ->
                "timed out while connecting: nothing answered, so a firewall or the router may be dropping it"
            e is ConnectException || msg.contains("ECONNREFUSED") ->
                "connection refused: the PC answered, but nothing listens on that port, or its firewall rejected it"
            e is UnknownHostException -> "unknown host: the name could not be looked up"
            stage == "reply" && e is IOException -> "reading the reply failed"
            stage == "send" -> "could not send the HELLO"
            else -> "could not connect"
        }
        return "$what [${e.javaClass.simpleName}: $msg]"
    }
}
