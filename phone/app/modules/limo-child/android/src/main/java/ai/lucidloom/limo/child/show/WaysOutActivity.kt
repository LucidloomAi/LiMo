package ai.lucidloom.limo.child.show

import android.app.Activity
import android.os.Bundle

/**
 * The ways out of a lock: ask with a typed reason, a code, the PIN. A stub:
 * it closes as soon as it opens.
 *
 * Folder show/ will hold what the child sees from Kotlin: the block cover,
 * the device lock and the warnings (native views in the accessibility
 * service's overlay windows), and this activity, because a keyboard cannot
 * type into an overlay. All of their words come from the Rust engine.
 */
class WaysOutActivity : Activity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    super.onCreate(savedInstanceState)
    finish()
  }
}
