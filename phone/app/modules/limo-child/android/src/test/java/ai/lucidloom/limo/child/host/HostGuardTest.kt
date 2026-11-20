package ai.lucidloom.limo.child.host

import ai.lucidloom.limo.child.ffi.CoverKind
import ai.lucidloom.limo.child.ffi.GuardedPage
import ai.lucidloom.limo.child.ffi.PIN
import ai.lucidloom.limo.child.ffi.TryKind
import ai.lucidloom.limo.child.ffi.app
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

/**
 * The guard through the host, on the real engine: the watch says a guarded
 * page is in front and the guard goes to the shower at once; the PIN typed
 * on it, wrong and right, its secret wiped, kept before the answer, the
 * guard gone by the time the answer comes; logged by kind only.
 */
class HostGuardTest {
  @get:Rule val folder = TemporaryFolder()

  @Test
  fun aGuardedPageIsCoveredAtOnceAndTheGuardsPinOpensItThroughTheHost() {
    HostRig(folder.newFolder()).use { rig ->
      rig.pair()
      rig.front = app("com.android.settings", "Settings")
      rig.changed()
      assertEquals(CoverKind.NOTHING, rig.shower.last.kind)

      // The watch says the device admin's page is in front: the host ticks
      // at once, and the guard reaches the shower.
      rig.guarded = GuardedPage.DEVICE_ADMIN
      rig.changed()
      val cover = rig.shower.last
      assertEquals(CoverKind.GUARD, cover.kind)
      assertEquals(GuardedPage.DEVICE_ADMIN, cover.guard?.page)
      assertEquals("A guardian's PIN is needed here.", cover.guard?.title)
      assertTrue(rig.log.any { it.startsWith("note {\"note\":\"guarded_page_opened\"") && it.contains("\"page\":\"device_admin\"") })

      // A wrong PIN: wiped, kept before the answer, still guarded.
      val wrong = "000000".toByteArray()
      val writes = rig.store.writes
      val first = rig.core.tryGuardPin(wrong)
      assertTrue(wrong.all { it.toInt() == 0 })
      assertEquals(TryKind.WRONG, (first as WayOut.Done).value.kind)
      assertEquals("That PIN is not right. 4 more tries before a wait.", first.value.message)
      assertTrue(rig.store.writes > writes)
      assertEquals(CoverKind.GUARD, rig.shower.last.kind)

      // The right one: the guard is gone when the answer comes, and stays
      // gone while the page is in front, for five minutes.
      val right = PIN.toByteArray()
      val opened = rig.core.tryGuardPin(right)
      assertTrue(right.all { it.toInt() == 0 })
      assertEquals(TryKind.RELEASE_CONFIRMED, (opened as WayOut.Done).value.kind)
      assertTrue(opened.value.message.startsWith("These settings are open until "))
      assertEquals(CoverKind.NOTHING, rig.shower.last.kind)
      rig.pass(4 * 60)
      assertEquals(CoverKind.NOTHING, rig.shower.last.kind)
      rig.pass(60)
      assertEquals(CoverKind.GUARD, rig.shower.last.kind)

      // Logged by kind, never the PIN.
      assertTrue(rig.log.contains("way_out guard_pin answer=wrong"))
      assertTrue(rig.log.contains("way_out guard_pin answer=release_confirmed"))
      assertFalse(rig.log.any { it.contains(PIN) || it.contains("000000") })
    }
  }

  @Test
  fun theStandInShowerLogsTheGuardByItsPageOnly() {
    val lines = ArrayList<String>()
    HostRig(folder.newFolder()).use { rig ->
      rig.pair()
      rig.core.shower = LoggingShower { lines += it }
      rig.front = app("com.android.settings", "Settings")
      rig.guarded = GuardedPage.ACCESSIBILITY
      rig.changed()
    }
    assertTrue(lines.toString(), lines.contains("cover now guard page=accessibility"))
  }
}
