package ai.lucidloom.limo.spike07

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

class DoorTest {
    @Before
    fun clean() = SpikeState.reset()

    private fun judge(text: String): Door.Taken {
        val bytes = text.toByteArray(Charsets.UTF_8)
        return Door.judge(bytes, bytes.size, false)
    }

    @Test
    fun anOfferShapedTextIsTakenWithItsLineFeed() {
        val sample = "spike07:" + "A".repeat(146) + "\n"
        val taken = judge(sample) as Door.Taken.Ok
        assertEquals(155, taken.bytes)
        assertEquals(Door.check(sample.toByteArray(), sample.length), taken.check)
    }

    @Test
    fun theCheckIsTheStartOfSha256AsThePcComputesIt() {
        // SHA-256 of "abc" begins ba7816bf, as the PC's own test says.
        assertEquals("ba7816bf", Door.check("abc".toByteArray(), 3))
        assertEquals("ba7816bf", Door.check("abcdef".toByteArray(), 3))
    }

    @Test
    fun whatIsNotAnOfferIsRefusedInAWord() {
        assertEquals(Door.Taken.Refused("empty", 0), judge(""))
        assertEquals(Door.Taken.Refused("empty", 3), judge("\r\n "))
        assertEquals(Door.Taken.Refused("not_text", 9), judge("two words"))
        assertEquals(Door.Taken.Refused("not_text", 5), judge("café"))
        assertEquals(Door.Taken.Refused("too_long", 1025), Door.judge(ByteArray(1025) { 'A'.code.toByte() }, 1025, true))
        // A carriage return before the line feed changes the check, not the verdict.
        val crlf = judge("spike07:AAAA\r\n") as Door.Taken.Ok
        val lf = judge("spike07:AAAA\n") as Door.Taken.Ok
        assertEquals(14, crlf.bytes)
        assertNotEquals(lf.check, crlf.check)
    }

    @Test
    fun theStatusRowIsInThePcsOrderAndNeverHoldsTheSecret() {
        assertArrayEquals(
            arrayOf("takes", "secret", "bytes", "check", "caller_uid", "why", "loopbacks", "loopback", "loopback_bytes", "loopback_ms", "loopback_why"),
            Door.COLUMNS,
        )
        SpikeState.recordWrite(judge("spike07:SECRETPART\n"), 2000)
        val row = Door.row(SpikeState.snapshot())
        assertEquals(Door.COLUMNS.size, row.size)
        assertEquals(1L, row[0])
        assertEquals("taken", row[1])
        assertEquals(19L, row[2])
        assertEquals(2000L, row[4])
        assertTrue(row.none { it.toString().contains("SECRETPART") })
        SpikeState.recordWrite(judge(""), 2000)
        assertEquals("refused", SpikeState.snapshot().secret)
        assertEquals("empty", SpikeState.snapshot().why)
        assertEquals(2L, SpikeState.snapshot().takes)
    }
}
