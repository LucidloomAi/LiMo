package ai.lucidloom.limo.child.host

import ai.lucidloom.limo.child.keep.Keeper
import ai.lucidloom.limo.child.module.BuildConfig
import ai.lucidloom.limo.child.seam.Seam
import ai.lucidloom.limo.child.seam.SeamProblem
import ai.lucidloom.limo.child.seam.Step
import android.os.Build
import expo.modules.kotlin.Promise
import expo.modules.kotlin.modules.Module
import expo.modules.kotlin.modules.ModuleDefinition

/**
 * The child form's side of the host seam (src/host.ts). The screens send a
 * request by name; this module answers it, through the seam (folder
 * seam/, its README says what each request does on Android), and sends the
 * `change` event when an answer may have changed.
 *
 * Folder host/ holds the engine host ([Host]): the Rust engine called from
 * Kotlin once a second at most, its state file, and the orders it gives.
 * JavaScript is never on that path: these requests are for setup and the
 * child's own screens only.
 *
 * Requests run on Expo's own queue, never the main thread. A name the module
 * does not know is refused with the code "UnknownRequest".
 */
class LimoChildModule : Module() {
  private val seam: Seam? get() = appContext.reactContext?.let { Seam.of(it) }

  override fun definition() = ModuleDefinition {
    Name("LimoChild")

    Events("change")

    // The app opened: the engine's host and LiMo's keeper start with it
    // when the phone is paired (keep/Keeper). Nothing here waits for them.
    OnCreate {
      appContext.reactContext?.let { Keeper.start(it, "app") }
    }

    // The screens listen for changes only while some screen is shown.
    OnStartObserving("change") {
      seam?.events?.listening { what -> sendEvent("change", mapOf("what" to what)) }
    }
    OnStopObserving("change") {
      seam?.events?.listening(null)
    }
    OnActivityEntersForeground {
      seam?.let {
        it.opener.backInFront()
        it.events.front(true)
      }
    }
    OnActivityEntersBackground {
      seam?.events?.front(false)
    }
    OnActivityResult { _, payload ->
      seam?.opener?.result(payload.requestCode)
    }

    AsyncFunction("request") { name: String, args: Map<String, Any?>, promise: Promise ->
      val seam = seam
      when {
        name == "about" -> promise.resolve(
          mapOf(
            "form" to "child",
            "module" to BuildConfig.LIMO_CHILD_MODULE_VERSION,
            "android" to Build.VERSION.RELEASE,
            "sdk" to Build.VERSION.SDK_INT,
          ),
        )
        seam == null -> promise.reject("Failed", "The app is not ready yet.", null)
        name == "child.openStep" -> {
          val step = Step.ofId(args["step"] as? String)
          if (step == null) {
            promise.reject(SeamProblem.BAD_ARGS, "child.openStep needs one of the six steps.", null)
          } else {
            seam.opener.open(step, appContext.currentActivity, appContext.permissions) { opened ->
              if (opened) promise.resolve(null) else promise.reject(SeamProblem.NOT_OPENED, "Android did not open the screen for this step.", null)
            }
          }
        }
        name == "child.askForMoreTime" -> when {
          !seam.paired -> promise.reject(SeamProblem.NOT_PAIRED, "The phone is not paired.", null)
          seam.opener.askForMoreTime(appContext.currentActivity) -> promise.resolve(null)
          else -> promise.reject(SeamProblem.NOT_OPENED, "Android did not open LiMo's ways out.", null)
        }
        else -> try {
          promise.resolve(seam.requests.answer(name))
        } catch (e: SeamProblem) {
          promise.reject(e.code, e.message, null)
        } catch (e: Exception) {
          AndroidHostLog.line("host failed what=request error=${e.javaClass.simpleName}")
          promise.reject("Failed", "The child module could not answer $name.", null)
        }
      }
    }
  }
}
