package ai.lucidloom.limo.child.seam

import ai.lucidloom.limo.child.watch.LaunchableApp
import ai.lucidloom.limo.child.watch.LimoPermissions

/**
 * The six steps of setup, in the order setup asks for them, by the screens'
 * names (`STEP_IDS` in `src/childSeam.ts`) and the engine's
 * (`guardian_needed.off[].permission` in its view).
 */
internal enum class Step(val id: String, val enginePermission: String) {
  NOTIFICATIONS("notifications", "notifications"),
  PHONE("phone", "phone"),
  USAGE("usage", "usage_access"),
  ACCESSIBILITY("accessibility", "accessibility"),
  BATTERY("battery", "battery"),
  PROTECTION("protection", "protection");

  companion object {
    fun ofId(id: String?): Step? = entries.firstOrNull { it.id == id }

    fun ofEnginePermission(name: String?): Step? = entries.firstOrNull { it.enginePermission == name }
  }
}

/**
 * A request the seam cannot answer. [code] reaches JavaScript as the error's
 * `code`; the message names at most a field or a request, never a name, a
 * label or a sentence.
 */
internal class SeamProblem(val code: String, message: String) : Exception(message) {
  companion object {
    const val UNKNOWN_REQUEST = "UnknownRequest"
    const val NOT_PAIRED = "NotPaired"
    const val CANNOT_READ = "CannotRead"
    const val BAD_VIEW = "BadView"
    const val BAD_ARGS = "BadArgs"
    const val NOT_OPENED = "NotOpened"
    const val NO_ANSWER = "NoAnswer"
  }
}

/**
 * The seam's answers, turned from what the host, the watch and Android give
 * into exactly the shapes the readers in `src/childSeam.ts` accept. Pure:
 * no Android, so the JVM tests turn the engine's REAL view with it.
 *
 * Nothing here decides a rule or makes a sentence about time: every word in
 * Today and the rules is the engine's, passed through. All this adds is the
 * name of each app, from the watch, in place of its package.
 */
internal object SeamAnswers {
  /**
   * The child's name when the phone is paired and no name is kept (its file
   * lost or damaged): plain words that read in every sentence the screens
   * make with a name ("This phone is now your child's.", "I am your child's
   * guardian, and I agree.").
   */
  const val NO_NAME = "your child"

  /** `child.pairing` with no engine: `{ paired: false }`. */
  fun notPaired(): Map<String, Any?> = mapOf("paired" to false)

  /** `child.pairing` with an engine: `{ paired: true, childName }`, the name the host keeps, or [NO_NAME]. */
  fun paired(childName: String?): Map<String, Any?> =
    mapOf("paired" to true, "childName" to (childName?.trim()?.takeIf { it.isNotEmpty() } ?: NO_NAME))

  /**
   * `child.steps`: each of the six "on" only when Android says so in this
   * reading, otherwise "off".
   */
  fun steps(p: LimoPermissions): Map<String, String> {
    fun word(on: Boolean) = if (on) "on" else "off"
    return linkedMapOf(
      Step.NOTIFICATIONS.id to word(p.notifications),
      Step.PHONE.id to word(p.phone),
      Step.USAGE.id to word(p.usageAccess),
      // Switched on in Android's settings: what the guardian does on this step.
      Step.ACCESSIBILITY.id to word(p.accessibility),
      Step.BATTERY.id to word(p.batteryExempt),
      Step.PROTECTION.id to word(p.deviceAdmin),
    )
  }

  /**
   * Each app's own name, by its package, from the watch's list of the apps
   * a child can open. An app with no name is not in it, so it is never
   * shown as a package.
   */
  fun namesOf(apps: List<LaunchableApp>): Map<String, String> {
    val out = LinkedHashMap<String, String>()
    for (app in apps) {
      val name = app.label?.trim()
      if (!name.isNullOrEmpty()) out[app.packageName] = name
    }
    return out
  }

  /** The engine's whole view, read from its JSON. */
  fun readView(json: String): Map<String, Any?> {
    val value = try {
      SeamJson.read(json)
    } catch (e: SeamJson.Bad) {
      throw SeamProblem(SeamProblem.BAD_VIEW, "The engine's view is not JSON.")
    }
    @Suppress("UNCHECKED_CAST")
    return value as? Map<String, Any?> ?: throw bad("view")
  }

  /** This phone's id at the master, from the engine's view. */
  fun deviceId(view: Map<String, Any?>): String = str(view, "device_id", "view").also {
    if (it.isEmpty()) throw bad("view.device_id")
  }

  /**
   * `child.today`, from the engine's `today` (its README, "The child's own
   * screens"): its kinds and keys to the seam's, each app's package to its
   * name (an app with no name left out), and what is switched off as the two
   * lists the reader wants, in setup order.
   */
  fun today(view: Map<String, Any?>, names: Map<String, String>): Map<String, Any?> {
    val t = obj(view, "today", "view")
    val left = obj(t, "left", "today")
    // The engine's kinds to the seam's. A kind this app does not know is
    // refused, never guessed.
    val kind = when (str(left, "kind", "today.left")) {
      "left" -> "left"
      "no_limit" -> "no-limit"
      "open" -> "open"
      else -> throw bad("today.left.kind")
    }
    val apps = list(t, "apps", "today").mapIndexedNotNull { i, raw ->
      val app = asObj(raw, "today.apps[$i]")
      val name = names[str(app, "package", "today.apps[$i]")] ?: return@mapIndexedNotNull null
      linkedMapOf("name" to name, "minutes" to str(app, "minutes", "today.apps[$i]"))
    }
    return linkedMapOf(
      "left" to linkedMapOf("kind" to kind, "words" to str(left, "words", "today.left")),
      "used" to str(t, "used", "today"),
      "usedPart" to numberOrNull(t, "used_part", "today"),
      "where" to str(t, "where_used", "today"),
      "bedtime" to strOrNull(t, "bedtime", "today"),
      "apps" to apps,
      "guardianNeeded" to guardianNeeded(t["guardian_needed"]),
    )
  }

  /**
   * `child.rules`, from the engine's `rules_words`: the words as they are,
   * and each list of packages as the apps' names (an app with no name left
   * out), in the rules' own order.
   */
  fun rules(view: Map<String, Any?>, names: Map<String, String>): Map<String, Any?> {
    val r = obj(view, "rules_words", "view")
    fun named(key: String) = list(r, key, "rules_words").mapIndexedNotNull { i, p ->
      names[p as? String ?: throw bad("rules_words.$key[$i]")]
    }
    return linkedMapOf(
      "timeADay" to str(r, "time_a_day", "rules_words"),
      "screensOff" to strOrNull(r, "screens_off", "rules_words"),
      "alwaysOpen" to named("always_open"),
      "notAllowed" to named("not_allowed"),
    )
  }

  /**
   * The steps that are off and the engine's sentence for each: `off` in
   * setup order, `words[i]` said of `off[i]`. A permission this seam does not
   * know is left out with its sentence (a newer engine may know more); null
   * when nothing is left.
   */
  private fun guardianNeeded(raw: Any?): Map<String, Any?>? {
    if (raw == null) return null
    val what = "today.guardian_needed"
    val said = sortedMapOf<Step, String>()
    list(asObj(raw, what), "off", what).forEachIndexed { i, entry ->
      val off = asObj(entry, "$what.off[$i]")
      val words = str(off, "words", "$what.off[$i]")
      if (words.isBlank()) throw bad("$what.off[$i].words")
      val step = Step.ofEnginePermission(off["permission"] as? String) ?: return@forEachIndexed
      said.putIfAbsent(step, words)
    }
    if (said.isEmpty()) return null
    return linkedMapOf("off" to said.keys.map { it.id }, "words" to said.values.toList())
  }

  // Reading the view's JSON, naming the field when it is not as the engine's
  // README says.

  private fun bad(field: String) = SeamProblem(SeamProblem.BAD_VIEW, "The engine's view has no usable $field.")

  @Suppress("UNCHECKED_CAST")
  private fun asObj(x: Any?, what: String): Map<String, Any?> = x as? Map<String, Any?> ?: throw bad(what)

  private fun obj(o: Map<String, Any?>, key: String, what: String) = asObj(o[key], "$what.$key")

  private fun str(o: Map<String, Any?>, key: String, what: String): String = o[key] as? String ?: throw bad("$what.$key")

  private fun strOrNull(o: Map<String, Any?>, key: String, what: String): String? = when (val v = o[key]) {
    null -> null
    is String -> v
    else -> throw bad("$what.$key")
  }

  private fun numberOrNull(o: Map<String, Any?>, key: String, what: String): Double? = when (val v = o[key]) {
    null -> null
    is Number -> v.toDouble().takeIf { it.isFinite() } ?: throw bad("$what.$key")
    else -> throw bad("$what.$key")
  }

  private fun list(o: Map<String, Any?>, key: String, what: String): List<*> = o[key] as? List<*> ?: throw bad("$what.$key")
}
