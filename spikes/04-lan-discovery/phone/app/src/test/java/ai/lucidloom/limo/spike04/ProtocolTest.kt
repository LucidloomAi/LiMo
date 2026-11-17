package ai.lucidloom.limo.spike04

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.net.InetAddress
import kotlin.random.Random

class ProtocolTest {

    // the reply line

    @Test
    fun acceptsAGoodReply() {
        val r = Protocol.checkReply("LIMO ab12cd34 0.1.0 1795000000 1a2b3c4d", "1a2b3c4d")
        assertTrue(r.isSuccess)
        assertEquals(Protocol.Reply("ab12cd34", "0.1.0", 1_795_000_000L, "1a2b3c4d"), r.getOrNull())
    }

    @Test
    fun acceptsACarriageReturnAtTheEnd() {
        assertTrue(Protocol.checkReply("LIMO ab12cd34 0.1.0 1795000000 n1\r", "n1").isSuccess)
    }

    @Test
    fun rejectsErr() {
        val r = Protocol.checkReply("ERR", "n1")
        assertEquals("the hub answered ERR", r.exceptionOrNull()?.message)
    }

    @Test
    fun rejectsAWrongNonce() {
        val r = Protocol.checkReply("LIMO ab12cd34 0.1.0 1795000000 other", "n1")
        assertTrue(r.exceptionOrNull()?.message!!.contains("nonce"))
    }

    @Test
    fun rejectsMalformedReplies() {
        for (bad in listOf(
            "",
            "LIMO",
            "LIMO ab12cd34 0.1.0 1795000000",
            "LIMO ab12cd34 0.1.0 1795000000 n1 extra",
            "LIMO ab12cd34 0.1.0 soon n1",
            "LIMO ab12cd34 0.1.0 -5 n1",
            "LIMO  0.1.0 1 n1",
            "limo ab12cd34 0.1.0 1 n1",
            "HELLO n1",
            "LIMO ab12cd34 0.1.0 1795000000 n1 ",
        )) {
            assertTrue("accepted \"$bad\"", Protocol.checkReply(bad, "n1").isFailure)
        }
    }

    // the hello line and nonce

    @Test
    fun helloLineHasOneLineEnd() {
        assertEquals("HELLO 1a2b3c4d\n", Protocol.helloLine("1a2b3c4d"))
        assertTrue(Protocol.helloLine("n".repeat(Protocol.MAX_NONCE)).length <= Protocol.MAX_LINE)
    }

    @Test
    fun noncesAreEightHexCharacters() {
        val random = Random(42)
        repeat(100) {
            val n = Protocol.newNonce(random)
            assertEquals(8, n.length)
            assertTrue(n, n.all { it in '0'..'9' || it in 'a'..'f' })
            assertTrue(Protocol.isValidNonce(n))
        }
        assertEquals("00000000", Protocol.newNonce(object : Random() {
            override fun nextBits(bitCount: Int) = 0
        }))
        assertEquals("ffffffff", Protocol.newNonce(object : Random() {
            override fun nextBits(bitCount: Int) = -1
        }))
    }

    @Test
    fun nonceRules() {
        assertTrue(Protocol.isValidNonce("a"))
        assertTrue(Protocol.isValidNonce("A-z_9"))
        assertTrue(Protocol.isValidNonce("n".repeat(64)))
        assertFalse(Protocol.isValidNonce(""))
        assertFalse(Protocol.isValidNonce("n".repeat(65)))
        assertFalse(Protocol.isValidNonce("a b"))
        assertFalse(Protocol.isValidNonce("a.b"))
        assertFalse(Protocol.isValidNonce("café"))
    }

    // numbers and fields

    @Test
    fun millisecondsWithOneDecimal() {
        assertEquals("0.0", Protocol.msOneDecimal(0))
        assertEquals("0.4", Protocol.msOneDecimal(440_000))
        assertEquals("0.5", Protocol.msOneDecimal(450_000))
        assertEquals("7.3", Protocol.msOneDecimal(7_260_000))
        assertEquals("19.0", Protocol.msOneDecimal(19_000_000))
        assertEquals("0.0", Protocol.msOneDecimal(-5))
    }

    @Test
    fun ports() {
        assertEquals(47810, Protocol.parsePort("47810"))
        assertEquals(1, Protocol.parsePort(" 1 "))
        assertEquals(65535, Protocol.parsePort("65535"))
        assertNull(Protocol.parsePort("0"))
        assertNull(Protocol.parsePort("65536"))
        assertNull(Protocol.parsePort("-1"))
        assertNull(Protocol.parsePort("port"))
        assertNull(Protocol.parsePort(""))
        assertNull(Protocol.parsePort(null))
    }

    @Test
    fun hosts() {
        assertEquals("192.168.68.200", Protocol.checkHost(" 192.168.68.200 "))
        assertEquals("den-pc.local", Protocol.checkHost("den-pc.local"))
        assertNull(Protocol.checkHost(""))
        assertNull(Protocol.checkHost("   "))
        assertNull(Protocol.checkHost("192.168 .68.200"))
        assertNull(Protocol.checkHost("a".repeat(254)))
        assertNull(Protocol.checkHost(null))
    }

    @Test
    fun picksIpv4First() {
        val v6 = InetAddress.getByAddress(ByteArray(16).also { it[15] = 1 })
        val v4 = InetAddress.getByAddress(byteArrayOf(192.toByte(), 168.toByte(), 68, 200.toByte()))
        assertEquals(v4, Protocol.pickAddress(listOf(v6, v4)))
        assertEquals(v6, Protocol.pickAddress(listOf(v6)))
        assertNull(Protocol.pickAddress(emptyList()))
    }

    @Test
    fun txtId() {
        assertEquals("ab12cd34", Protocol.txtId(mapOf("id" to "ab12cd34".toByteArray(), "v" to "0.1.0".toByteArray())))
        assertEquals("?", Protocol.txtId(mapOf("v" to "0.1.0".toByteArray())))
        assertEquals("?", Protocol.txtId(mapOf("id" to null)))
        assertEquals("?", Protocol.txtId(mapOf("id" to "bad id\n".toByteArray())))
        assertEquals("?", Protocol.txtId(null))
    }

    @Test
    fun nsdErrorsInPlainWords() {
        assertTrue(Protocol.nsdError(0).startsWith("internal error"))
        assertTrue(Protocol.nsdError(3).startsWith("already active"))
        assertTrue(Protocol.nsdError(4).startsWith("too many"))
        assertEquals("failure code 99", Protocol.nsdError(99))
    }

    // log lines

    @Test
    fun logLinesHaveTheFixedShapes() {
        assertEquals("search started", Lines.searchStarted())
        assertEquals("found name=\"LiMo spike on DEN\" after_ms=412", Lines.found("LiMo spike on DEN", 412))
        assertEquals(
            "resolved name=\"LiMo spike on DEN\" host=192.168.68.200 port=47810 id=ab12cd34 after_ms=655",
            Lines.resolved("LiMo spike on DEN", "192.168.68.200", 47810, "ab12cd34", 655),
        )
        assertEquals(
            "hello ok host=192.168.68.200 port=47810 rtt_ms=7.3 reply=\"LIMO ab12cd34 0.1.0 1 n1\" connect_ms=3.1 run=found net=wifi",
            Lines.helloOk("192.168.68.200", 47810, "found", "wifi", "7.3", "3.1", "LIMO ab12cd34 0.1.0 1 n1"),
        )
        assertEquals(
            "hello failed host=192.168.68.200 port=47810 error=\"connection refused\" run=direct net=wifi",
            Lines.helloFailed("192.168.68.200", 47810, "direct", "wifi", "connection refused"),
        )
        assertEquals("search failed error=\"internal error\"", Lines.searchFailed("internal error"))
        assertEquals(
            "resolve failed name=\"LiMo spike on DEN\" error=\"timed out\"",
            Lines.resolveFailed("LiMo spike on DEN", "timed out"),
        )
        assertEquals("wifi connected=yes address=192.168.68.80/22", Lines.wifi(true, listOf("192.168.68.80/22")))
        assertEquals("wifi connected=no", Lines.wifi(false, emptyList()))
    }

    @Test
    fun quotingKeepsLinesOnOneLine() {
        assertEquals("\"a\\\"b\"", Lines.quote("a\"b"))
        assertEquals("\"a\\\\b\"", Lines.quote("a\\b"))
        assertEquals("\"a\\x0ab\"", Lines.quote("a\nb"))
        assertEquals("\"café\"", Lines.quote("café"))
    }

    // the repeat

    @Test
    fun repeatTallyCountsTwelve() {
        val t = RepeatTally()
        assertEquals("none", t.slowestMs())
        repeat(11) { t.record(true, 7_000_000L + it * 1_000_000L) }
        assertFalse(t.done)
        t.record(false, 4_000_000_000L)
        assertTrue(t.done)
        assertEquals(11, t.ok)
        assertEquals(1, t.failed)
        // The slowest counts successes only.
        assertEquals("repeat done ok=11 failed=1 slowest_ms=17.0", t.doneLine())
        // Nothing is counted after the twelfth.
        t.record(true, 1L)
        assertEquals(12, t.count)
    }

    @Test
    fun repeatTallyStoppedEarly() {
        val t = RepeatTally()
        t.record(true, 19_000_000L)
        t.record(false, 0L)
        assertEquals("repeat stopped ok=1 failed=1 slowest_ms=19.0 reason=app_hidden", t.stoppedLine("app_hidden"))
        val none = RepeatTally()
        none.record(false, 0L)
        assertEquals("repeat done ok=0 failed=1 slowest_ms=none", none.doneLine())
    }
}
