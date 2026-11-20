package ai.lucidloom.limo.child.dev

import ai.lucidloom.limo.child.ffi.CoverKind
import ai.lucidloom.limo.child.host.HostChange
import ai.lucidloom.limo.child.host.HostRig
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

/**
 * The test family's pairing file, pushed over the cable: taken once,
 * deleted, and the phone paired from it; a file that is not right refused
 * and deleted, its contents never in the log line.
 */
class DevPairingTest {
  @get:Rule val folder = TemporaryFolder()

  private fun hex(bytes: ByteArray) = bytes.joinToString("") { "%02x".format(it) }

  @Test
  fun thePairingFileIsTakenOnceDeletedAndPairsThePhone() {
    HostRig(folder.newFolder()).use { rig ->
      rig.core.start()
      val welcome = rig.family.welcome(rig.child, "Phone", rig.wall)
      val file = File(folder.newFolder("limo-dev"), "pairing.json")
      // As the development tool writes it: the welcome grant as an object, the secret in hex.
      file.writeText(
        """{ "welcome": ${String(welcome.welcome, Charsets.UTF_8)},
           |  "code_secret_hex": "${hex(welcome.codeSecret)}",
           |  "made_at": ${rig.wall - 3} }""".trimMargin(),
      )

      val taken = DevPairing.take(file, rig.wall)
      assertFalse("the file is deleted as soon as it is read", file.exists())
      assertTrue(taken is DevPairing.Taken.Pairing)
      val pairing = taken as DevPairing.Taken.Pairing
      assertTrue(pairing.welcome.contentEquals(welcome.welcome))
      assertTrue(pairing.codeSecret.contentEquals(welcome.codeSecret))

      // The host makes the engine from it, keeps the state, and wipes both.
      assertTrue(rig.core.pair(pairing.welcome, pairing.codeSecret))
      assertTrue(pairing.welcome.all { it.toInt() == 0 } && pairing.codeSecret.all { it.toInt() == 0 })
      assertEquals(1 + rig.engines.current.orders.count { it.saveState }, rig.store.writes)
      assertTrue(rig.events.contains(HostChange.PAIRING))
      assertEquals(CoverKind.NOTHING, rig.shower.last.kind)
      rig.pass(10)
      assertTrue(rig.keeper.last.counting)

      // Taken once: a second broadcast finds no file, and nothing changes.
      assertEquals(DevPairing.Taken.NoFile, DevPairing.take(file, rig.wall))
      assertEquals(1, rig.engines.made.size)
    }
  }

  @Test
  fun aPairingFileThatIsNotRightIsRefusedAndDeletedWithoutQuotingIt() {
    val now = 1_795_000_000L
    val dir = folder.newFolder("limo-dev")
    val secret = "ab".repeat(32)
    val welcome = """{"child_id":"Mira","device_id":"d1"}"""
    val cases = listOf(
      "not_json" to "Mira's pairing",
      "not_json" to """{"welcome": {"a": 1}, "code_secret_hex": "$secret", "made_at": $now} and more""",
      "no_welcome" to """{"code_secret_hex": "$secret", "made_at": $now}""",
      "no_welcome" to """{"welcome": "Mira", "code_secret_hex": "$secret", "made_at": $now}""",
      "no_code_secret" to """{"welcome": $welcome, "made_at": $now}""",
      "no_made_at" to """{"welcome": $welcome, "code_secret_hex": "$secret"}""",
      "no_made_at" to """{"welcome": $welcome, "code_secret_hex": "$secret", "made_at": "yesterday"}""",
      "old_file" to """{"welcome": $welcome, "code_secret_hex": "$secret", "made_at": ${now - 2 * 60 * 60}}""",
      "code_secret_not_hex" to """{"welcome": $welcome, "code_secret_hex": "Mira is not hex", "made_at": $now}""",
      "code_secret_not_32_bytes" to """{"welcome": $welcome, "code_secret_hex": "abcd", "made_at": $now}""",
    )
    for ((why, text) in cases) {
      val file = File(dir, "pairing.json")
      file.writeText(text)
      val taken = DevPairing.take(file, now)
      assertEquals(text, DevPairing.Taken.Refused(why), taken)
      assertFalse(file.exists())
      // The word for the log names what was wrong, never what the file held.
      assertFalse((taken as DevPairing.Taken.Refused).why.contains("Mira"))
    }
    // Strings with escapes and a nested object in the welcome are read as they are.
    val nested = """{"welcome": {"a": [1, 2.5e3, true, null, {"b": "q\"é"}]}, "code_secret_hex": "$secret", "made_at": $now}"""
    val taken = DevPairing.read(nested, now) as DevPairing.Taken.Pairing
    assertEquals("""{"a": [1, 2.5e3, true, null, {"b": "q\"é"}]}""", String(taken.welcome, Charsets.UTF_8))
    assertEquals(32, taken.codeSecret.size)
    assertTrue(taken.codeSecret.all { it == 0xab.toByte() })
  }
}
