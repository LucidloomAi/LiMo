package ai.lucidloom.limo.child.seam

/** What may have changed, as the `change` event says it (`ChildChange` in `src/childSeam.ts`). */
internal enum class SeamChange(val word: String) {
  PAIRING("pairing"),
  AGREEMENT("agreement"),
  STEPS("steps"),
  INTRO("intro"),
  TODAY("today"),
  RULES("rules"),
}

/**
 * When the `change` event is sent, and for what: the seam's throttle. Pure,
 * with the clock passed in, so the JVM tests drive it.
 *
 * - **Gathered.** What may have changed is noted, and looked at once, a
 *   little later ([gatherMs]), so a burst of signals (a tick, a permission,
 *   the apps) is one look.
 * - **At most once a second.** A look comes at least [gapMs] after the one
 *   before. The engine's words change once a minute while time is counted,
 *   and once a second only in the last minute, when Today shows the
 *   seconds.
 * - **Only what really changed.** At each look the seam works out the
 *   answer the screens would get now for each kind noted, and sends the
 *   kind only when that answer differs from the one it last sent for it, or
 *   when it remembers none ([prime] takes the answers the screens just
 *   asked for as sent).
 *
 * The seam notes changes only while the screens listen and the app is in
 * front (`SeamEvents`); [pause] drops what was noted when the app leaves
 * the front, and [reset] also forgets what was sent when the screens stop
 * listening.
 */
internal class SeamChanges(private val gatherMs: Long = GATHER_MS, private val gapMs: Long = GAP_MS) {
  private val noted = sortedSetOf<SeamChange>()
  private val sent = HashMap<SeamChange, Any?>()
  private var lookAt: Long? = null
  private var lastLook: Long? = null

  /**
   * Notes that [kinds] may have changed at [nowMs]. Returns when the look
   * should run (the same clock), or null when one is already planned.
   */
  fun note(kinds: Collection<SeamChange>, nowMs: Long): Long? {
    if (kinds.isEmpty()) return null
    noted += kinds
    if (lookAt != null) return null
    val earliest = lastLook?.let { it + gapMs } ?: Long.MIN_VALUE
    return maxOf(nowMs + gatherMs, earliest).also { lookAt = it }
  }

  /**
   * The look: [answers] gives the answer now for each kind noted; the kinds
   * whose answer differs from the one last sent are returned, in the
   * order of [SeamChange], and remembered as sent.
   */
  fun look(nowMs: Long, answers: (Collection<SeamChange>) -> Map<SeamChange, Any?>): List<SeamChange> {
    lookAt = null
    lastLook = nowMs
    if (noted.isEmpty()) return emptyList()
    val kinds = noted.toList()
    noted.clear()
    val now = answers(kinds)
    val out = ArrayList<SeamChange>()
    for (kind in kinds) {
      val answer = now[kind]
      if (sent.containsKey(kind) && sent[kind] == answer) continue
      sent[kind] = answer
      out += kind
    }
    return out
  }

  /**
   * The screens began to listen, having just asked for what they show:
   * the answers now are taken as sent, so their first look sends only what
   * changed after it, not every kind at once.
   */
  fun prime(answers: Map<SeamChange, Any?>) {
    sent.putAll(answers)
  }

  /**
   * The app left the front: nothing noted and nothing planned, but the
   * answers sent are remembered, so coming back sends only what changed.
   */
  fun pause() {
    noted.clear()
    lookAt = null
  }

  /** Nobody listens: nothing noted, nothing planned, and no answer remembered as sent. */
  fun reset() {
    pause()
    sent.clear()
  }

  companion object {
    /** How long a burst of signals is gathered into one look. */
    const val GATHER_MS = 150L

    /** The least time between two looks. */
    const val GAP_MS = 1_000L
  }
}
