package ai.lucidloom.limo.child.host

import java.io.File
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The promises of host/, keep/ and dev/, checked in their files: no log
 * line carries a label, the child's name, a typed reason, a code, a PIN, a
 * secret or an error's own words; and nothing a person reads has dash
 * punctuation.
 */
class HostHouseRulesTest {
  private val moduleDir: File by lazy {
    var dir: File? = File(System.getProperty("user.dir") ?: ".").absoluteFile
    while (dir != null) {
      if (File(dir, "src/main/java/ai/lucidloom/limo/child/host").isDirectory) return@lazy dir
      dir = dir.parentFile
    }
    error("the module was not found from the tests' working folder")
  }

  private val folders: List<File> by lazy {
    listOf(
      "src/main/java/ai/lucidloom/limo/child/host",
      "src/main/java/ai/lucidloom/limo/child/keep",
      "src/debug/java/ai/lucidloom/limo/child/dev",
      "src/test/java/ai/lucidloom/limo/child/host",
      "src/test/java/ai/lucidloom/limo/child/keep",
      "src/test/java/ai/lucidloom/limo/child/dev",
    ).map { File(moduleDir, it) }
  }

  private fun files(dir: File, ext: String) = dir.listFiles { f -> f.name.endsWith(ext) }.orEmpty().sortedBy { it.name }

  @Test
  fun noLogLineCarriesALabelATypedReasonASecretOrAnErrorsWords() {
    val logCall = Regex("""(Log\.[a-z]\(|log\.line\(|AndroidHostLog\.line\()""")
    val forbidden = Regex("""\$\{?[A-Za-z_.?]*(label|reason|message|words|title|welcome|secret|pin\b|code\b|localizedMessage|childName)""", RegexOption.IGNORE_CASE)
    val sources = folders.take(3).flatMap { files(it, ".kt") }
    assertTrue(sources.size >= 10)
    for (file in sources) {
      file.readLines().forEachIndexed { i, line ->
        if (!logCall.containsMatchIn(line)) return@forEachIndexed
        assertFalse("${file.name}:${i + 1} logs what it must not: $line", forbidden.containsMatchIn(line))
      }
    }
  }

  @Test
  fun noDashPunctuationInAnythingAPersonReads() {
    val em = 0x2014.toChar()
    val en = 0x2013.toChar()
    val readmes = folders.take(3).map { File(it, "README.md") }
    for (readme in readmes) assertTrue("${readme.path} is missing", readme.isFile)
    val all = folders.flatMap { files(it, ".kt") } + readmes + files(File(moduleDir, "src/main/res/values"), ".xml") +
      File(moduleDir, "src/main/AndroidManifest.xml") + File(moduleDir, "src/debug/AndroidManifest.xml")
    for (file in all) {
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
          else -> line.substringAfter("// ", "").ifEmpty { null } ?: if (file.name.endsWith(".xml")) trimmed else null
        } ?: return@forEachIndexed
        val body = prose.trim().removePrefix("- ")
        if (body.startsWith("|")) return@forEachIndexed
        assertFalse("$at has a hyphen used as a dash", body.contains(" - "))
        assertFalse("$at has a double hyphen as a dash", (body.contains(" --") || body.contains("-- ")) && !body.contains("<!--") && !body.contains("-->"))
      }
    }
  }
}
