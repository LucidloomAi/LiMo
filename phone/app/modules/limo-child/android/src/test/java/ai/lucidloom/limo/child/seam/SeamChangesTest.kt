package ai.lucidloom.limo.child.seam

import ai.lucidloom.limo.child.host.HostChange
import ai.lucidloom.limo.child.watch.WatchChange
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The `change` event's throttle: a burst gathered into one look, at most one
 * look a second, and a kind sent only when its answer really changed.
 */
class SeamChangesTest {
  /** The answers now, by kind, as the test sets them. */
  private val now = HashMap<SeamChange, Any?>()
  private var looks = 0
  private val answers: (Collection<SeamChange>) -> Map<SeamChange, Any?> = { kinds ->
    looks += 1
    kinds.associateWith { now[it] }
  }

  @Test
  fun aBurstIsOneLookAndOnlyWhatChangedIsSent() {
    val gate = SeamChanges()
    now[SeamChange.TODAY] = "48 min"
    now[SeamChange.RULES] = "2 h"
    now[SeamChange.STEPS] = "all on"
    gate.prime(now.toMap())
    // The tick says the view changed, the watch says a permission did: one look, 150 ms on.
    assertEquals(1_150L, gate.note(listOf(SeamChange.TODAY, SeamChange.RULES), 1_000))
    assertNull(gate.note(listOf(SeamChange.STEPS, SeamChange.TODAY), 1_050))
    now[SeamChange.TODAY] = "47 min"
    assertEquals(listOf(SeamChange.TODAY), gate.look(1_150, answers))
    assertEquals(1, looks)
    // Nothing changed: nothing sent.
    gate.note(listOf(SeamChange.TODAY), 5_000)
    assertEquals(emptyList<SeamChange>(), gate.look(5_150, answers))
  }

  @Test
  fun atMostOneLookASecond() {
    val gate = SeamChanges()
    assertEquals(150L, gate.note(listOf(SeamChange.TODAY), 0))
    now[SeamChange.TODAY] = "0:42"
    assertEquals(listOf(SeamChange.TODAY), gate.look(150, answers))
    // In the last minute the words change every second: the next look waits for its second.
    assertEquals(1_150L, gate.note(listOf(SeamChange.TODAY), 400))
    now[SeamChange.TODAY] = "0:41"
    assertEquals(listOf(SeamChange.TODAY), gate.look(1_150, answers))
    // Long after, only the gathering.
    assertEquals(10_150L, gate.note(listOf(SeamChange.TODAY), 10_000))
  }

  @Test
  fun whatWasNeverSentIsSentAndAFailureOnce() {
    val gate = SeamChanges()
    gate.note(listOf(SeamChange.PAIRING), 0)
    now[SeamChange.PAIRING] = ChildSeam.FAILED
    assertEquals(listOf(SeamChange.PAIRING), gate.look(150, answers))
    gate.note(listOf(SeamChange.PAIRING), 2_000)
    assertEquals(emptyList<SeamChange>(), gate.look(2_150, answers))
    now[SeamChange.PAIRING] = mapOf("paired" to true, "childName" to "Mira")
    gate.note(listOf(SeamChange.PAIRING), 4_000)
    assertEquals(listOf(SeamChange.PAIRING), gate.look(4_150, answers))
  }

  @Test
  fun kindsAreSentInOneOrderAndResetForgetsWhatWasSent() {
    val gate = SeamChanges()
    for (kind in SeamChange.entries) now[kind] = kind.word
    gate.note(listOf(SeamChange.RULES, SeamChange.PAIRING, SeamChange.STEPS), 0)
    assertEquals(listOf(SeamChange.PAIRING, SeamChange.STEPS, SeamChange.RULES), gate.look(150, answers))
    gate.reset()
    gate.note(listOf(SeamChange.RULES), 300)
    assertEquals(listOf(SeamChange.RULES), gate.look(1_150, answers))
    // A look with nothing noted (one planned before a reset) asks nothing.
    val before = looks
    assertTrue(gate.look(3_000, answers).isEmpty())
    assertEquals(before, looks)
  }

  @Test
  fun whatTheHostAndTheWatchMayHaveChanged() {
    assertEquals(
      listOf(SeamChange.PAIRING, SeamChange.AGREEMENT, SeamChange.INTRO, SeamChange.TODAY, SeamChange.RULES),
      SeamEvents.onHost(HostChange.PAIRING),
    )
    assertEquals(listOf(SeamChange.TODAY, SeamChange.RULES), SeamEvents.onHost(HostChange.VIEW))
    assertEquals(listOf(SeamChange.STEPS, SeamChange.TODAY), SeamEvents.onWatch(WatchChange.PERMISSIONS))
    for (w in listOf(WatchChange.APPS, WatchChange.CLOCK, WatchChange.TIME_ZONE)) {
      assertEquals(listOf(SeamChange.TODAY, SeamChange.RULES), SeamEvents.onWatch(w))
    }
    for (w in listOf(WatchChange.FRONT, WatchChange.SCREEN, WatchChange.CALL)) assertTrue(SeamEvents.onWatch(w).isEmpty())
    assertEquals(listOf("pairing", "agreement", "steps", "intro", "today", "rules"), SeamChange.entries.map { it.word })
  }
}
