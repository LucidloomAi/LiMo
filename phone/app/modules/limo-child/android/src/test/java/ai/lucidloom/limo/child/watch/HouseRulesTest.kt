package ai.lucidloom.limo.child.watch

import java.io.File
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The folder's own promises, checked in its files: the watch reads nothing
 * on the screen, and nothing a person reads has dash punctuation.
 */
class HouseRulesTest {
  private val watchDir: File by lazy {
    var dir: File? = File(System.getProperty("user.dir") ?: ".").absoluteFile
    while (dir != null) {
      val found = File(dir, "src/main/java/ai/lucidloom/limo/child/watch")
      if (found.isDirectory) return@lazy found
      dir = dir.parentFile
    }
    error("the watch folder was not found from the tests' working folder")
  }

  private val testDir: File by lazy { File(watchDir.path.replace("src${File.separator}main", "src${File.separator}test")) }

  private fun sources(dir: File) = dir.listFiles { f -> f.name.endsWith(".kt") }.orEmpty().sortedBy { it.name }

  @Test
  fun theWatchReadsNothingOnTheScreen() {
    // What would read a window's content, its text or a class name.
    val forbidden = listOf(
      Regex("""AccessibilityNodeInfo"""),
      Regex("""rootInActiveWindow|getRootInActiveWindow"""),
      Regex("""\.windows\b|getWindows"""),
      Regex("""\.source\b|getSource"""),
      Regex("""contentDescription|getContentDescription"""),
      Regex("""\.text\b(?!\.)|getText\("""),
      Regex("""\.className\b|getClassName"""),
      Regex("""setServiceInfo|serviceInfo\.flags"""),
    )
    val files = sources(watchDir)
    assertTrue(files.size >= 10)
    for (file in files) {
      file.readLines().forEachIndexed { i, line ->
        val code = line.substringBefore("//").trim()
        if (code.startsWith("*") || code.startsWith("/*")) return@forEachIndexed
        for (rule in forbidden) assertFalse("${file.name}:${i + 1} reads the screen: $line", rule.containsMatchIn(code))
      }
    }
    // A caller's number is never read: it is a parameter Android passes, named once and unused.
    val signals = File(watchDir, "WatchSignals.kt").readText()
    assertTrue(Regex("""phoneNumber""").findAll(signals).count() == 1)
  }

  @Test
  fun noDashPunctuationInAnythingAPersonReads() {
    val em = 0x2014.toChar()
    val en = 0x2013.toChar()
    val files = sources(watchDir) + sources(testDir) + File(watchDir, "README.md")
    for (file in files) {
      val markdown = file.name.endsWith(".md")
      var fenced = false
      file.readLines().forEachIndexed { i, line ->
        val at = "${file.name}:${i + 1}"
        assertFalse("$at has a long dash", line.contains(em) || line.contains(en))
        val trimmed = line.trim()
        if (trimmed.startsWith("```")) {
          fenced = !fenced
          return@forEachIndexed
        }
        if (fenced) return@forEachIndexed
        val prose = when {
          markdown -> if (line.startsWith("    ") || trimmed.startsWith("|")) null else trimmed
          trimmed.startsWith("//") -> trimmed.removePrefix("//")
          trimmed.startsWith("/**") -> trimmed.removePrefix("/**")
          trimmed.startsWith("*") -> trimmed.removePrefix("*")
          else -> line.substringAfter("// ", "").ifEmpty { null }
        } ?: return@forEachIndexed
        val body = prose.trim().removePrefix("- ")
        // Table rows, in a README or a comment, have their own rules.
        if (body.startsWith("|")) return@forEachIndexed
        assertFalse("$at has a hyphen used as a dash", body.contains(" - "))
        assertFalse("$at has a double hyphen as a dash", body.contains(" --") || body.contains("-- "))
      }
    }
  }
}
