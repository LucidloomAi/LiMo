package ai.lucidloom.limo.core

import ai.lucidloom.limo.core.ffi.BudgetInput
import ai.lucidloom.limo.core.ffi.CoreException
import android.util.Log
import expo.modules.kotlin.exception.CodedException
import expo.modules.kotlin.modules.Module
import expo.modules.kotlin.modules.ModuleDefinition
import ai.lucidloom.limo.core.ffi.coreVersion as rustCoreVersion
import ai.lucidloom.limo.core.ffi.decideLease as rustDecideLease

private const val TAG = "LiMoSpike03"
private const val U32_MAX = 4_294_967_295.0

/**
 * Exposes the Rust core to JavaScript. The calls go
 * JavaScript, this module (Kotlin), the UniFFI bindings (Kotlin over JNA), Rust.
 *
 * Errors reach JavaScript as an Error whose `code` is the Rust variant name,
 * for example "ZeroLease". Two more codes come from this module itself:
 * "InvalidInput" when a number cannot be a u32, and "CoreUnavailable" when the
 * Rust library could not be loaded or called at all.
 */
class LimoCoreModule : Module() {
  override fun definition() = ModuleDefinition {
    Name("LimoCore")

    Function("coreVersion") {
      callCore { rustCoreVersion() }
    }

    Function("decideLease") { dailyBudgetSecs: Double, usedSecs: Double, leaseSecs: Double ->
      val input = BudgetInput(
        dailyBudgetSecs = toU32("dailyBudgetSecs", dailyBudgetSecs),
        usedSecs = toU32("usedSecs", usedSecs),
        leaseSecs = toU32("leaseSecs", leaseSecs),
      )
      val started = System.nanoTime()
      val decision = callCore { rustDecideLease(input) }
      val kotlinToRustNanos = System.nanoTime() - started
      mapOf(
        "grantedSecs" to decision.grantedSecs.toDouble(),
        "remainingSecs" to decision.remainingSecs.toDouble(),
        "locked" to decision.locked,
        "kotlinToRustNanos" to kotlinToRustNanos.toDouble(),
      )
    }

    // One line to logcat under the tag LiMoSpike03. Does not touch Rust, so it
    // still works when the Rust library failed to load.
    Function("log") { line: String ->
      Log.i(TAG, line)
      Unit
    }
  }

  private fun toU32(name: String, value: Double): UInt {
    if (!value.isFinite() || value != Math.floor(value) || value < 0.0 || value > U32_MAX) {
      throw CodedException("InvalidInput", "$name must be a whole number from 0 to 4294967295, got $value", null)
    }
    return value.toLong().toUInt()
  }

  private inline fun <T> callCore(block: () -> T): T {
    try {
      return block()
    } catch (e: CoreException) {
      throw CodedException(coreErrorName(e), e.message, e)
    } catch (e: Throwable) {
      // For example UnsatisfiedLinkError when the .so is missing for this ABI,
      // or a UniFFI checksum mismatch when bindings and library do not match.
      Log.e(TAG, "core call failed", e)
      throw CodedException("CoreUnavailable", "The Rust core could not be called: $e", e)
    }
  }

  // The Rust variant name. Written out rather than read from the class name,
  // so it survives code shrinking; a new Rust variant makes this fail to
  // compile until it is added here.
  private fun coreErrorName(e: CoreException): String = when (e) {
    is CoreException.ZeroLease -> "ZeroLease"
    is CoreException.UsedMoreThanADay -> "UsedMoreThanADay"
  }
}
