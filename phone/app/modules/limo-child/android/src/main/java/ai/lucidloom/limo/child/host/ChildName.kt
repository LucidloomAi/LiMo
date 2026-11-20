package ai.lucidloom.limo.child.host

/**
 * The child's name, as the host keeps it beside the engine's state, in the
 * app's private `files/limo/child.json`: `{"child_name": "Asha"}`. The
 * pairing gives it (the welcome grant holds no name); the child's screens
 * and the lock show it. It is never logged, reported, or put in the
 * engine's state. Pure, so the JVM tests check it.
 */
internal object ChildName {
  /** The longest name kept, in characters (code points). */
  const val MAX_CHARS = 40

  /**
   * [raw] as a name to keep: control characters and direction marks
   * dropped, trimmed, cut at [MAX_CHARS] characters; null when nothing is
   * left, which is no name.
   */
  fun clean(raw: String?): String? {
    if (raw == null) return null
    val kept = StringBuilder()
    var i = 0
    while (i < raw.length) {
      val cp = raw.codePointAt(i)
      i += Character.charCount(cp)
      if (Character.getType(cp) == Character.CONTROL.toInt() || cp in DIRECTION_MARKS) continue
      kept.appendCodePoint(cp)
    }
    val trimmed = kept.toString().trim()
    val cut = if (trimmed.codePointCount(0, trimmed.length) > MAX_CHARS) {
      trimmed.substring(0, trimmed.offsetByCodePoints(0, MAX_CHARS)).trim()
    } else {
      trimmed
    }
    return cut.ifEmpty { null }
  }

  /** The file's text for [name]. */
  fun text(name: String): String = buildString {
    append("{\"child_name\":\"")
    for (c in name) {
      when {
        c == '"' -> append("\\\"")
        c == '\\' -> append("\\\\")
        c < ' ' || c.code > 0x7e -> append("\\u%04x".format(c.code))
        else -> append(c)
      }
    }
    append("\"}")
  }

  /**
   * The name in a file's [text], cleaned, or null when the text is not
   * exactly one object with the string `child_name` (a file lost, cut short
   * or damaged is no name).
   */
  fun parse(text: String): String? {
    var at = 0
    fun space() {
      while (at < text.length && text[at] in " \t\r\n") at += 1
    }
    fun take(c: Char): Boolean {
      space()
      if (at < text.length && text[at] == c) {
        at += 1
        return true
      }
      return false
    }
    fun string(): String? {
      if (!take('"')) return null
      val out = StringBuilder()
      while (at < text.length) {
        val c = text[at++]
        when {
          c == '"' -> return out.toString()
          c == '\\' -> {
            if (at >= text.length) return null
            when (val e = text[at++]) {
              '"', '\\', '/' -> out.append(e)
              'b' -> out.append('\b')
              'f' -> out.append('\u000C')
              'n' -> out.append('\n')
              'r' -> out.append('\r')
              't' -> out.append('\t')
              'u' -> {
                if (at + 4 > text.length) return null
                out.append((text.substring(at, at + 4).toIntOrNull(16) ?: return null).toChar())
                at += 4
              }
              else -> return null
            }
          }
          c < ' ' -> return null
          else -> out.append(c)
        }
      }
      return null
    }
    if (!take('{')) return null
    if (string() != "child_name" || !take(':')) return null
    val name = string() ?: return null
    if (!take('}')) return null
    space()
    return if (at == text.length) clean(name) else null
  }

  /** Marks that change the direction text runs in: never part of a name. */
  private val DIRECTION_MARKS = setOf(0x061C, 0x200E, 0x200F, 0x202A, 0x202B, 0x202C, 0x202D, 0x202E, 0x2066, 0x2067, 0x2068, 0x2069)
}

/** Where the host keeps the child's name: `files/limo/child.json`. */
internal interface NameStore {
  /** The name kept, or null when none is (or it cannot be read). */
  fun read(): String?

  /** Keeps [name] in place of any kept before. Throws when it cannot. */
  fun write(name: String)

  /** No name is kept any more. */
  fun delete()
}
