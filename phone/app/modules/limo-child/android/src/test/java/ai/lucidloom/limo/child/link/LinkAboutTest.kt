package ai.lucidloom.limo.child.link

import ai.lucidloom.limo.child.ffi.PhoneAbout
import ai.lucidloom.limo.child.ffi.testkit.TestAbout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

/**
 * What the phone is (its maker and model, Android's version, LiMo's
 * version), told to the family's PC once a session and again when it
 * changed, cleaned by the link, and never written in the log.
 */
class LinkAboutTest {
  @get:Rule val folder = TemporaryFolder()

  private val galaxy = TestAbout("samsung SM-G988B", 33, "0.4.0")
  private val updated = TestAbout("samsung SM-G988B", 33, "0.5.0")

  @Test
  fun thePhoneTellsThePcWhatItIsOnceASessionAndAgainWhenItChanged() {
    LinkRig(folder.newFolder()).use { r ->
      val device = r.pairOverTheCable()
      r.passUntil("link open via=cable")
      r.pass(3)
      assertEquals(listOf(galaxy), r.family.aboutsOf(device))
      // Asked at every round, said once.
      val asked = r.aboutAsked
      r.pass(30)
      assertTrue(r.aboutAsked > asked + 10)
      assertEquals(listOf(galaxy), r.family.aboutsOf(device))
      // An update of the app changes its version: said again, in the same session.
      r.about = PhoneAbout("samsung", "SM-G988B", 33, "0.5.0")
      r.pass(2)
      assertEquals(listOf(galaxy, updated), r.family.aboutsOf(device))
      // A new session says it again, once.
      r.wires.pullCable()
      r.pass(2)
      val back = r.mark()
      r.wires.cableIn = true
      r.core.poke()
      r.passUntil("link open via=cable", from = back)
      r.pass(30)
      assertEquals(listOf(galaxy, updated, updated), r.family.aboutsOf(device))
      assertTrue(r.core.inReach)
      // Never in the log: only that it went.
      assertEquals(3, r.log.count { it == "link about sent" })
      for (line in r.log) {
        assertFalse(line, line.contains("SM-G988B") || line.contains("samsung") || line.contains("0.4.0") || line.contains("0.5.0"))
      }
    }
  }

  @Test
  fun whatAndroidGivesIsCleanedAndAVersionBelowZeroIsNeverSentNorEndsTheSession() {
    LinkRig(folder.newFolder()).use { r ->
      r.about = PhoneAbout(" Google\n", "Pixel 7‮", -1, "4".repeat(60))
      val device = r.pairOverTheCable()
      r.passUntil("link open via=cable")
      r.pass(5)
      // Refused by the bridge, once, and the session goes on.
      assertTrue(r.family.aboutsOf(device).isEmpty())
      assertEquals(1, r.log.count { it == "link failed what=about error=OutOfRange" })
      assertTrue(r.core.inReach)
      assertFalse(r.log.contains("link closed"))
      // What Android gives, odd characters and all, arrives cleaned and cut.
      r.about = PhoneAbout(" Google\n", "Pixel 7‮", 34, "4".repeat(60))
      r.pass(2)
      assertEquals(listOf(TestAbout("Google Pixel 7", 34, "4".repeat(40))), r.family.aboutsOf(device))
    }
  }
}
