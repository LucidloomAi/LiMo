package ai.lucidloom.limo.child.seam

import java.io.File
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The seam's promises, checked in its files: no log line carries a name, a
 * label, a sentence or the view; nothing here makes a sentence about time;
 * and nothing a person reads has dash punctuation.
 */
class SeamHouseRulesTest {
  private val moduleDir: File by lazy {
    var dir: File? = File(System.getProperty("user.dir") ?: ".").absoluteFile
    while (dir != null) {
      if (File(dir, "src/main/java/ai/lucidloom/limo/child/seam").isDirectory) return@lazy dir
      dir = dir.parentFile
    }
    error("the module was not found from the tests' working folder")
  }

  private val main by lazy { File(moduleDir, "src/main/java/ai/lucidloom/limo/child/seam") }
  private val tests by lazy { File(moduleDir, "src/test/java/ai/lucidloom/limo/child/seam") }

  private fun sources(dir: File) = dir.listFiles { f -> f.name.endsWith(".kt") }.orEmpty().sortedBy { it.name }

  @Test
  fun noLogLineCarriesANameALabelASentenceOrTheView() {
    val logCall = Regex("""(Log\.[a-z]\(|log\.line\(|AndroidSeamLog\.line\(|AndroidHostLog\.line\()""")
    val forbidden = Regex("""\$\{?[A-Za-z_.?]*(label|reason|message|words|title|childName|names|answer|view|json|text)""", RegexOption.IGNORE_CASE)
    val files = sources(main) + File(moduleDir, "src/main/java/ai/lucidloom/limo/child/host/LimoChildModule.kt")
    assertTrue(files.size >= 7)
    for (file in files) {
      file.readLines().forEachIndexed { i, line ->
        if (!logCall.containsMatchIn(line)) return@forEachIndexed
        assertFalse("${file.name}:${i + 1} logs what it must not: $line", forbidden.containsMatchIn(line))
      }
    }
  }

  @Test
  fun nothingHereMakesASentenceAboutTime() {
    // The engine's words are passed through: no span, clock time or phrase of Today's is written here.
    val timeWords = Regex(
      """"[^"]*(\d\s*(min|h|hours?|minutes?)\b|\b\d{1,2}:\d{2}\b|\b(am|pm)\b|left today|used of|used today|no limit|open until|screens are off|switched off)[^"]*"""",
      RegexOption.IGNORE_CASE,
    )
    for (file in sources(main)) {
      file.readLines().forEachIndexed { i, line ->
        val code = line.substringBefore("//").trim()
        if (code.startsWith("*") || code.startsWith("/*")) return@forEachIndexed
        assertFalse("${file.name}:${i + 1} makes a sentence about time: $line", timeWords.containsMatchIn(code))
      }
    }
  }

  @Test
  fun noDashPunctuationInAnythingAPersonReads() {
    val em = 0x2014.toChar()
    val en = 0x2013.toChar()
    val readme = File(main, "README.md")
    assertTrue("${readme.path} is missing", readme.isFile)
    for (file in sources(main) + sources(tests) + readme) {
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
        if (body.startsWith("|")) return@forEachIndexed
        assertFalse("$at has a hyphen used as a dash", body.contains(" - "))
        assertFalse("$at has a double hyphen as a dash", body.contains(" --") || body.contains("-- "))
      }
    }
  }
}
