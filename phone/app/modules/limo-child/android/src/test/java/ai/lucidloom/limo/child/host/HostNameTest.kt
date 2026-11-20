package ai.lucidloom.limo.child.host

import java.io.IOException
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

/**
 * The child's name the host keeps beside the engine's state: given by the
 * pairing, cleaned, kept in its own file, removed when the pairing is
 * forgotten, and never in a log line.
 */
class HostNameTest {
  @get:Rule val folder = TemporaryFolder()

  @Test
  fun aPairingKeepsTheNameCleanedBesideTheStateAndForgettingRemovesIt() {
    HostRig(folder.newFolder()).use { rig ->
      rig.core.start()
      assertNull(rig.core.childName)
      rig.pair("  Mi\u0007ra‎ ")
      assertEquals("Mira", rig.core.childName)
      assertEquals("""{"child_name":"Mira"}""", rig.names.text)
      assertEquals(HostChange.PAIRING, rig.events.first())
      // Read from its file, as a new process reads it.
      rig.kill()
      assertEquals("Mira", rig.core.childName)

      rig.core.forget()
      assertNull(rig.core.childName)
      assertNull(rig.names.text)
      assertEquals(HostChange.PAIRING, rig.events.last())
      assertTrue(rig.log.none { it.contains("Mira") })
    }
  }

  @Test
  fun aPairingWithNoNameKeepsNoneAndReplacesAnOlderOne() {
    HostRig(folder.newFolder()).use { rig ->
      rig.core.start()
      rig.pair("Asha")
      rig.pair(null)
      assertNull(rig.core.childName)
      assertNull(rig.names.text)
      assertEquals("host paired no_child_name", rig.log.last { it.startsWith("host paired") })
      rig.pair(" ‮ ")
      assertNull(rig.core.childName)
    }
  }

  @Test
  fun aNameFileLostOrDamagedIsNoNameAndANameNotKeptLeavesThePhonePaired() {
    HostRig(folder.newFolder()).use { rig ->
      rig.core.start()
      rig.pair("Asha")
      rig.names.text = """{"child_name":"""
      assertNull(rig.core.childName)
      rig.names.text = null
      assertNull(rig.core.childName)
      assertTrue(rig.core.paired)

      // A name that cannot be written: never the older pairing's name in its place.
      rig.names.text = ChildName.text("Asha")
      rig.names.refuseWrite = IOException("full")
      rig.pair("Mira")
      assertTrue(rig.core.paired)
      assertNull(rig.core.childName)
      assertTrue(rig.log.contains("host failed what=name_write error=IOException"))
      assertTrue(rig.log.none { it.contains("Mira") || it.contains("Asha") })
    }
  }

  @Test
  fun notPairedThereIsNoNameWhateverIsLeftInTheFile() {
    HostRig(folder.newFolder()).use { rig ->
      rig.core.start()
      rig.names.text = ChildName.text("Asha")
      assertFalse(rig.core.paired)
      assertNull(rig.core.childName)
    }
  }

  @Test
  fun aNameIsCleanedAsTheToolCleansIt() {
    assertEquals("Asha", ChildName.clean("  Asha\n"))
    assertEquals("Asha", ChildName.clean("⁦Asha⁩"))
    assertNull(ChildName.clean(null))
    assertNull(ChildName.clean(" \u0000‏ "))
    // Cut at 40 characters, never inside one.
    val faces = "😀".repeat(41)
    val cut = ChildName.clean(faces)!!
    assertEquals(40, cut.codePointCount(0, cut.length))
    assertEquals("😀".repeat(40), cut)
    assertEquals("Mira", ChildName.clean("Mira" + " ".repeat(40) + "Asha"))
  }

  @Test
  fun theNameFileReadsBackWhatItWroteAndNothingElse() {
    for (name in listOf("Asha", "Mira \"M\" \\ Asha", "Asha é́ 😀")) {
      assertEquals(name, ChildName.parse(ChildName.text(name)))
    }
    assertEquals("Asha", ChildName.parse(""" { "child_name" : " Asha " } """))
    for (bad in listOf("", "Asha", "{}", """{"child_name":7}""", """{"child_name":"Asha","x":1}""", """{"child_name":"Asha"} more""", """["Asha"]""")) {
      assertNull(bad, ChildName.parse(bad))
    }
  }
}
