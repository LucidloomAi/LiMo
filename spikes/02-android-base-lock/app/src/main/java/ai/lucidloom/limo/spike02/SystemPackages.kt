package ai.lucidloom.limo.spike02

import android.content.Context
import android.content.Intent
import android.os.Build
import android.provider.Settings
import android.telecom.TelecomManager
import android.view.inputmethod.InputMethodManager

/**
 * Packages looked up on this phone at run time.
 *
 * [neverCover]: this app, every launcher (home screen app), every dialer
 * including the system and default ones, and every app that handles the
 * Settings intent. Added to [CoverRules.NEVER_COVER_FIXED].
 *
 * [keyboards]: every installed input method. Their windows are ignored.
 */
class SystemPackages(val neverCover: Set<String>, val keyboards: Set<String>) {

    companion object {
        @Suppress("DEPRECATION")
        fun load(ctx: Context): SystemPackages {
            val pm = ctx.packageManager
            val never = HashSet<String>()
            never.add(ctx.packageName)

            fun addHandlers(intent: Intent) {
                try {
                    for (ri in pm.queryIntentActivities(intent, 0)) {
                        ri.activityInfo?.packageName?.let { never.add(it) }
                    }
                } catch (_: Exception) {
                }
            }

            addHandlers(Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_HOME))
            addHandlers(Intent(Intent.ACTION_DIAL))
            addHandlers(Intent(Settings.ACTION_SETTINGS))

            try {
                val telecom = ctx.getSystemService(TelecomManager::class.java)
                telecom?.defaultDialerPackage?.let { never.add(it) }
                if (Build.VERSION.SDK_INT >= 29) {
                    telecom?.systemDialerPackage?.let { never.add(it) }
                }
            } catch (_: Exception) {
            }

            val keyboards = HashSet<String>()
            try {
                val imm = ctx.getSystemService(InputMethodManager::class.java)
                imm?.inputMethodList?.forEach { keyboards.add(it.packageName) }
            } catch (_: Exception) {
            }
            // The keyboard in use, even if the list above missed it.
            try {
                val id = Settings.Secure.getString(ctx.contentResolver, Settings.Secure.DEFAULT_INPUT_METHOD)
                id?.substringBefore('/')?.takeIf { it.isNotBlank() }?.let { keyboards.add(it) }
            } catch (_: Exception) {
            }

            return SystemPackages(never, keyboards)
        }
    }
}
