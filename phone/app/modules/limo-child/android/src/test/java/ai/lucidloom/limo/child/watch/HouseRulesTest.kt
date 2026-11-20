package ai.lucidloom.limo.child.watch

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The folder's own promises, checked in its files: the watch reads nothing
 * on the screen, a window's class name only for Android's Settings and the
 * installer and only to match it against the guarded list, no package name
 * but the framework's and the guarded list's, and nothing a person reads has
 * dash punctuation.
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

  /** The code of a line: what is before a line comment, and nothing of a doc comment. */
  private fun code(line: String): String? {
    val code = line.substringBefore("//").trim()
    return if (code.startsWith("*") || code.startsWith("/*")) null else code
  }

  /** The module's other source sets: src/release/java, src/debug/watch, src/debug/java. */
  private fun moduleFile(vararg parts: String): File {
    val src = watchDir.path.substringBefore("${File.separator}main${File.separator}")
    return File(src, parts.joinToString(File.separator))
  }

  private val readsAClassName = Regex("""\.className\b|getClassName""")

  @Test
  fun theWatchReadsNothingOnTheScreenAndAClassNameOnlyForSettingsAndTheInstaller() {
    // What would read a window's content or its text.
    val forbidden = listOf(
      Regex("""AccessibilityNodeInfo"""),
      Regex("""rootInActiveWindow|getRootInActiveWindow"""),
      Regex("""\.windows\b|getWindows"""),
      Regex("""\.source\b|getSource"""),
      Regex("""contentDescription|getContentDescription"""),
      Regex("""\.text\b(?!\.)|getText\("""),
      Regex("""setServiceInfo|serviceInfo\.flags"""),
    )
    val release = moduleFile("release", "java", "ai", "lucidloom", "limo", "child", "watch", "WindowPage.kt")
    val debug = moduleFile("debug", "watch", "ai", "lucidloom", "limo", "child", "watch", "WindowPage.kt")
    val files = sources(watchDir) + release + debug
    assertTrue(files.size >= 12)
    val classNameReads = ArrayList<String>()
    for (file in files) {
      file.readLines().forEachIndexed { i, line ->
        val code = code(line) ?: return@forEachIndexed
        for (rule in forbidden) assertFalse("${file.name}:${i + 1} reads the screen: $line", rule.containsMatchIn(code))
        if (readsAClassName.containsMatchIn(code)) classNameReads += "${file.name}: $code"
      }
    }
    // The class name of a window is read in one place: the service, only
    // after asking that the package is Settings or the installer, and only
    // to hand it to WindowPage, which matches it against the fixed list.
    assertEquals(1, classNameReads.size)
    val read = classNameReads.single()
    assertTrue(read, read.startsWith("WatchService.kt: "))
    assertTrue(read, read.contains("if (Guarded.readsClassName(packageName)) WindowPage.of(packageName, event.className?.toString()) else null"))
    // The release copy of WindowPage only matches: no log, no probe.
    val releaseCode = release.readLines().mapNotNull(::code).joinToString("\n")
    assertTrue(releaseCode.contains("= Guarded.page(packageName, className)"))
    for (word in listOf("Log", "log", "probe", "Probe", "println", "print(")) assertFalse(word, releaseCode.contains(word))
    // The debug copy hands it to the probe, then matches it as release does.
    val debugCode = debug.readLines().mapNotNull(::code).joinToString("\n")
    assertTrue(debugCode.contains("PageProbe.window(packageName, className)"))
    assertTrue(debugCode.contains("return Guarded.page(packageName, className)"))
    // The fixed list's own rule: Settings and the installer, and pages only of them.
    assertEquals(setOf("com.android.settings", "com.android.packageinstaller", "com.google.android.packageinstaller"), Guarded.CLASS_NAME_PACKAGES)
    assertTrue(Guarded.PAGES.keys.all { it.first in Guarded.CLASS_NAME_PACKAGES })
    // No log line of the watch holds a class name: none names one.
    for (file in sources(watchDir)) {
      for (line in file.readLines()) {
        if (!line.contains("log.line(") && !line.contains("AndroidWatchLog.line(")) continue
        assertFalse("${file.name}: $line", line.contains("className") || line.contains("cls"))
      }
    }
    // A caller's number is never read: it is a parameter Android passes, named once and unused.
    val signals = File(watchDir, "WatchSignals.kt").readText()
    assertTrue(Regex("""phoneNumber""").findAll(signals).count() == 1)
  }

  @Test
  fun packageNamesAreHeldOnlyInTheFrameworksListAndTheGuardedList() {
    // Which app plays which part is asked of Android on the phone; the only
    // names written in the watch are the framework's own overlays (Roles.kt)
    // and the guarded list (Guarded.kt), each entry there with its reason.
    val packageLike = Regex(""""[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)+"""")
    for (file in sources(watchDir)) {
      val names = file.readLines().mapNotNull(::code).flatMap { c -> packageLike.findAll(c).map { it.value }.toList() }
      when (file.name) {
        "Roles.kt" -> assertTrue(names.toString(), names.all { it.startsWith("\"com.android.") || it.startsWith("\"com.google.android.") })
        "Guarded.kt" -> assertTrue(names.isNotEmpty())
        else -> assertTrue("${file.name} names a package: $names", names.isEmpty())
      }
    }
    // A maker's name, anything but Android's and Google's own, is in the guarded list only.
    assertTrue(Guarded.PACKAGES.keys.any { !it.startsWith("com.android.") && !it.startsWith("com.google.android.") })
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
