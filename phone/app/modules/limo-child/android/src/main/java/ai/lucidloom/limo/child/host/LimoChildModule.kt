package ai.lucidloom.limo.child.host

import ai.lucidloom.limo.child.keep.Keeper
import ai.lucidloom.limo.child.module.BuildConfig
import android.os.Build
import expo.modules.kotlin.exception.CodedException
import expo.modules.kotlin.modules.Module
import expo.modules.kotlin.modules.ModuleDefinition

/**
 * The child form's side of the host seam (src/host.ts). The screens send a
 * request by name; this module answers it.
 *
 * Folder host/ holds the engine host ([Host]): the Rust engine called from
 * Kotlin once a second at most, its state file, and the orders it gives.
 * JavaScript is never on that path: these requests are for setup and the
 * child's own screens only.
 *
 * For now it answers one request, "about", so the start screen can show that
 * the module is linked. Any other name is refused with the code
 * "UnknownRequest".
 */
class LimoChildModule : Module() {
  override fun definition() = ModuleDefinition {
    Name("LimoChild")

    // The app opened: the engine's host and LiMo's keeper start with it
    // when the phone is paired (keep/Keeper). Nothing here waits for them.
    OnCreate {
      appContext.reactContext?.let { Keeper.start(it, "app") }
    }

    AsyncFunction("request") { name: String, _: Map<String, Any?> ->
      when (name) {
        "about" -> mapOf(
          "form" to "child",
          "module" to BuildConfig.LIMO_CHILD_MODULE_VERSION,
          "android" to Build.VERSION.RELEASE,
          "sdk" to Build.VERSION.SDK_INT,
        )
        else -> throw UnknownRequestException(name)
      }
    }
  }
}

internal class UnknownRequestException(name: String) :
  CodedException("UnknownRequest", "The child module has no request named $name.", null)
