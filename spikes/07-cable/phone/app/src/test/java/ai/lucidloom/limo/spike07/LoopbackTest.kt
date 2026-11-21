package ai.lucidloom.limo.spike07

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.net.InetAddress
import java.net.ServerSocket
import kotlin.concurrent.thread

class LoopbackTest {
    /** An echo server on this computer's loopback, as the PC's; [change] alters what goes back. */
    private fun echo(change: (ByteArray, Int) -> Unit = { _, _ -> }): Int {
        val server = ServerSocket(0, 1, InetAddress.getByName("127.0.0.1"))
        thread {
            server.use {
                it.accept().use { socket ->
                    val buffer = ByteArray(64 * 1024)
                    val input = socket.getInputStream()
                    val output = socket.getOutputStream()
                    while (true) {
                        val n = input.read(buffer)
                        if (n < 0) break
                        change(buffer, n)
                        output.write(buffer, 0, n)
                    }
                    socket.shutdownOutput()
                }
            }
        }
        return server.localPort
    }

    @Test
    fun everyByteComesBackThroughAnEchoServer() {
        val result = Loopback.run(echo())
        assertTrue(result.why, result.ok)
        assertEquals(Loopback.BYTES.toLong(), result.bytes)
    }

    @Test
    fun aChangedByteIsAMismatchAndNoServerIsRefused() {
        val changed = Loopback.run(echo { buffer, n -> if (n > 10) buffer[10] = (buffer[10] + 1).toByte() })
        assertEquals("mismatch", changed.why)
        val nobody = ServerSocket(0).use { it.localPort }
        assertEquals("refused", Loopback.run(nobody).why)
        assertEquals("bad_port", Loopback.run(0).why)
    }

    @Test
    fun aPcThatClosesFirstCutsTheRunShortAndItIsSaid() {
        val server = ServerSocket(0, 1, InetAddress.getByName("127.0.0.1"))
        thread {
            server.use {
                it.accept().use { socket ->
                    // Sends back the first bytes only, ends its side, and
                    // takes the rest without answering.
                    val buffer = ByteArray(1000)
                    val n = socket.getInputStream().read(buffer)
                    socket.getOutputStream().write(buffer, 0, n)
                    socket.shutdownOutput()
                    while (socket.getInputStream().read(buffer) >= 0) {
                        // drained
                    }
                }
            }
        }
        val result = Loopback.run(server.localPort)
        assertEquals("cut_short", result.why)
        assertTrue(result.bytes in 1 until Loopback.BYTES.toLong())
    }

    @Test
    fun theLastRunIsKeptForTheStatusRow() {
        SpikeState.reset()
        SpikeState.recordLoopback(Loopback.Result(true, 262144, 85, "none"))
        val s = SpikeState.snapshot()
        assertEquals(1L, s.loopbacks)
        assertEquals("ok", s.loopback)
        assertEquals(262144L, s.loopbackBytes)
    }
}
