package ai.lucidloom.limo.child.host

import ai.lucidloom.limo.child.ffi.CASINO
import ai.lucidloom.limo.child.ffi.CoverKind
import ai.lucidloom.limo.child.ffi.EngineException
import ai.lucidloom.limo.child.ffi.GAME
import ai.lucidloom.limo.child.ffi.app
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

/**
 * Setup through the host, on the real engine: from pairing until a guardian
 * finishes setup, nothing is counted or covered and LiMo's notification says
 * so; finished, it is kept before the answer, ticked at once, and stays
 * finished through a restart of the process; finished again, nothing changes.
 */
class HostSetupTest {
  @get:Rule val folder = TemporaryFolder()

  private val notFinished = "Setup is not finished on this phone."

  @Test
  fun untilSetupIsFinishedNothingIsCountedOrCoveredAndTheNotificationSaysSo() {
    HostRig(folder.newFolder()).use { rig ->
      rig.core.start()
      rig.pair(finished = false)
      assertEquals(1 + rig.engines.current.orders.count { it.saveState }, rig.store.writes)

      // A minute in a game, then the blocked casino: nothing counted, nothing covered, no warning.
      rig.pass(60)
      assertEquals(notFinished, rig.keeper.last.words)
      assertFalse(rig.keeper.last.counting)
      rig.front = app(CASINO, "Casino")
      rig.changed()
      assertEquals(CoverKind.NOTHING, rig.shower.last.kind)
      assertTrue(rig.keeper.warnings.isEmpty())
      assertEquals(0L, rig.engines.current.real.view()!!.countedSecs)
      assertTrue(rig.core.view().json!!.contains(""""setup":{"finished":false,"finished_at":null,"words":"$notFinished"}"""))

      // Finished: kept before the answer, told, and ticked at once, so the casino is covered there and then.
      val writes = rig.store.writes
      val told = rig.events.size
      assertEquals(WayOut.Done(true), rig.core.finishSetup())
      assertTrue(rig.store.writes > writes)
      assertEquals(CoverKind.BLOCK, rig.shower.last.kind)
      assertTrue(rig.events.drop(told).contains(HostChange.VIEW))
      assertTrue(rig.log.contains("host setup finished"))
      assertTrue(rig.engines.current.real.view()!!.setupFinished)

      // Counting from then on.
      rig.front = app(GAME, "Game")
      rig.changed()
      rig.pass(60)
      assertEquals("29 min left", rig.keeper.last.words)
      assertTrue(rig.keeper.last.counting)

      // Finished again: nothing changes, nothing is written.
      val before = rig.store.writes
      assertEquals(WayOut.Done(false), rig.core.finishSetup())
      assertEquals(before, rig.store.writes)
      assertTrue(rig.log.contains("host setup already"))

      // A new process on what was kept: still finished, still counting.
      rig.kill()
      rig.core.start()
      rig.pass(5)
      assertTrue(rig.keeper.last.counting)
      assertTrue(rig.engines.current.real.view()!!.setupFinished)
    }
  }

  @Test
  fun setupCannotBeFinishedWhenNotPairedOrWhenTheKeptStateCannotBeRead() {
    HostRig(folder.newFolder()).use { rig ->
      rig.core.start()
      assertEquals(WayOut.NotPaired, rig.core.finishSetup())

      rig.store.bytes = "not LiMo's".toByteArray()
      rig.kill()
      rig.core.start()
      val answer = rig.core.finishSetup()
      assertTrue(answer is WayOut.Refused && answer.refusal is EngineException.CannotCheck)
      assertEquals(0, rig.store.writes)
      assertTrue(rig.log.contains("host setup refused error=CannotCheck"))
    }
  }
}
