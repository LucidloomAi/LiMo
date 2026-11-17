package ai.lucidloom.limo.spike05

import android.app.admin.DevicePolicyManager
import android.content.Context
import android.content.Intent
import android.provider.Settings
import android.provider.Telephony
import android.telecom.TelecomManager
import android.view.inputmethod.InputMethodManager

/**
 * Packages looked up on this phone at run time, the same idea as spike 02's
 * never cover list.
 *
 * [neverSuspend]: this app, every launcher (home screen app), every dialer
 * including the system and default ones, every app that opens Settings, the
 * default SMS app, every keyboard, and every active device admin. Added to
 * [SuspendRules.NEVER_SUSPEND_FIXED].
 *
 * [launchable]: every package with an icon in the app drawer.
 */
class SystemPackages(val neverSuspend: Set<String>, val launchable: Set<String>) {

    companion object {
        @Suppress("DEPRECATION")
        fun load(ctx: Context): SystemPackages {
            val pm = ctx.packageManager
            val never = HashSet<String>()
            never.add(ctx.packageName)

            fun handlers(intent: Intent): List<String> = try {
                pm.queryIntentActivities(intent, 0).mapNotNull { it.activityInfo?.packageName }
            } catch (_: Exception) {
                emptyList()
            }

            never += handlers(Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_HOME))
            never += handlers(Intent(Intent.ACTION_DIAL))
            never += handlers(Intent(Settings.ACTION_SETTINGS))

            try {
                val telecom = ctx.getSystemService(TelecomManager::class.java)
                telecom?.defaultDialerPackage?.let { never.add(it) }
                telecom?.systemDialerPackage?.let { never.add(it) }
            } catch (_: Exception) {
            }

            try {
                Telephony.Sms.getDefaultSmsPackage(ctx)?.let { never.add(it) }
            } catch (_: Exception) {
            }

            try {
                val imm = ctx.getSystemService(InputMethodManager::class.java)
                imm?.inputMethodList?.forEach { never.add(it.packageName) }
            } catch (_: Exception) {
            }
            try {
                val id = Settings.Secure.getString(ctx.contentResolver, Settings.Secure.DEFAULT_INPUT_METHOD)
                id?.substringBefore('/')?.takeIf { it.isNotBlank() }?.let { never.add(it) }
            } catch (_: Exception) {
            }

            try {
                val dpm = ctx.getSystemService(DevicePolicyManager::class.java)
                dpm?.activeAdmins?.forEach { never.add(it.packageName) }
            } catch (_: Exception) {
            }

            val launchable = handlers(Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_LAUNCHER)).toSet()
            return SystemPackages(never, launchable)
        }
    }
}
