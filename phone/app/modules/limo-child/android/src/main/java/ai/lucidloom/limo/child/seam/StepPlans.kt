package ai.lucidloom.limo.child.seam

/** One of Android's own settings pages, opened for a step. */
internal enum class Page {
  /** LiMo's notification settings (`ACTION_APP_NOTIFICATION_SETTINGS`, Android 8 and later). */
  APP_NOTIFICATIONS,

  /** LiMo's own page in Android's list of apps (`ACTION_APPLICATION_DETAILS_SETTINGS`), with its permissions. */
  APP_DETAILS,

  /** Usage access for LiMo itself (`ACTION_USAGE_ACCESS_SETTINGS` with LiMo's package), Android 10 and later. */
  USAGE_FOR_LIMO,

  /** The list of apps with usage access (`ACTION_USAGE_ACCESS_SETTINGS`). */
  USAGE_LIST,

  /** Android's accessibility settings (`ACTION_ACCESSIBILITY_SETTINGS`), asked to show LiMo's row. */
  ACCESSIBILITY_LIST,

  /** The list of apps left out of battery optimisation (`ACTION_IGNORE_BATTERY_OPTIMIZATION_SETTINGS`). */
  BATTERY_LIST,
}

/** A screen Android shows over the app as a question, answered with a result. */
internal enum class Question {
  /** Whether LiMo may be left out of battery optimisation (`ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS`). */
  BATTERY,

  /** Whether LiMo may be a device admin app (`ACTION_ADD_DEVICE_ADMIN`). */
  DEVICE_ADMIN,
}

/** One way to open a step, tried in order until one opens. */
internal sealed class StepWay {
  /** Android's runtime permission question for [permission]; answered over the app. */
  data class Ask(val permission: String) : StepWay()

  /** A screen of Android's that answers with a result. */
  data class Answer(val question: Question) : StepWay()

  /** A settings page; the step is opened once it is. */
  data class Open(val page: Page) : StepWay()
}

/**
 * What Android has to show for each step, on each version: the ways to
 * try, in order. Pure, so the JVM tests check every version.
 */
internal object StepPlans {
  const val POST_NOTIFICATIONS = "android.permission.POST_NOTIFICATIONS"
  const val READ_PHONE_STATE = "android.permission.READ_PHONE_STATE"

  /** Android 13, from which notifications are a runtime permission. */
  const val TIRAMISU = 33

  /** Android 10, whose Settings opens usage access for one package. */
  const val Q = 29

  /**
   * The ways for [step] on Android [sdk]. [granted] and [blocked] are the
   * step's runtime permission as Android and Expo's permission service know
   * it now: blocked means Android will not ask again (the person refused it
   * for good, or Android refused it without asking), so its own page is
   * opened instead of the question.
   */
  fun ways(step: Step, sdk: Int, granted: Boolean, blocked: Boolean): List<StepWay> = when (step) {
    Step.NOTIFICATIONS ->
      // Android 13 and later ask; before, notifications are on unless a
      // person switched them off, in LiMo's notification settings.
      if (sdk >= TIRAMISU && !granted && !blocked) {
        listOf(StepWay.Ask(POST_NOTIFICATIONS), StepWay.Open(Page.APP_NOTIFICATIONS), StepWay.Open(Page.APP_DETAILS))
      } else {
        listOf(StepWay.Open(Page.APP_NOTIFICATIONS), StepWay.Open(Page.APP_DETAILS))
      }
    Step.PHONE ->
      if (blocked && !granted) {
        listOf(StepWay.Open(Page.APP_DETAILS))
      } else {
        listOf(StepWay.Ask(READ_PHONE_STATE), StepWay.Open(Page.APP_DETAILS))
      }
    Step.USAGE ->
      if (sdk >= Q) listOf(StepWay.Open(Page.USAGE_FOR_LIMO), StepWay.Open(Page.USAGE_LIST)) else listOf(StepWay.Open(Page.USAGE_LIST))
    // No app may open another app's accessibility page directly (Android
    // keeps that page for itself), so the list, with LiMo's row asked for.
    Step.ACCESSIBILITY -> listOf(StepWay.Open(Page.ACCESSIBILITY_LIST))
    Step.BATTERY -> listOf(StepWay.Answer(Question.BATTERY), StepWay.Open(Page.BATTERY_LIST))
    Step.PROTECTION -> listOf(StepWay.Answer(Question.DEVICE_ADMIN))
  }

  /**
   * After Android answered a permission question: whether to open the
   * step's own page as well. Only when the answer is no, LiMo had asked
   * before ([askedBefore]; Android always shows the first question), Android
   * would not have offered the question again ([rationaleBefore] false), and
   * will not ask again now ([blockedAfter]): then Android said no without
   * showing anything, and the page is the only way left. A no the person
   * gave in the question itself is left as it is.
   */
  fun pageAfterAnswer(granted: Boolean, askedBefore: Boolean, rationaleBefore: Boolean, blockedAfter: Boolean): Boolean =
    !granted && askedBefore && !rationaleBefore && blockedAfter

  /** The request code a screen that answers is started with, under 0x10000 as AndroidX asks. */
  fun requestCode(question: Question): Int = 0x4C30 + question.ordinal

  /** The page to open after such an answer: the first page among [ways]. */
  fun pageOf(ways: List<StepWay>): Page? = ways.filterIsInstance<StepWay.Open>().firstOrNull()?.page
}
