package ai.lucidloom.limo.child.watch

import ai.lucidloom.limo.child.keep.AdminReceiver
import android.Manifest
import android.app.AppOpsManager
import android.app.KeyguardManager
import android.app.NotificationManager
import android.app.admin.DevicePolicyManager
import android.app.usage.UsageEvents
import android.app.usage.UsageStatsManager
import android.content.ComponentName
import android.content.ContentResolver
import android.content.Context
import android.content.Intent
import android.content.pm.ApplicationInfo
import android.content.pm.PackageManager
import android.media.AudioManager
import android.os.Build
import android.os.PowerManager
import android.os.Process
import android.os.SystemClock
import android.provider.Settings
import android.telecom.InCallService
import android.telecom.TelecomManager
import android.telephony.TelephonyManager
import android.text.format.DateFormat
import android.view.inputmethod.InputMethodManager
import java.util.TimeZone

/**
 * Each reading of [PhoneSource] from Android's own call, kept thin: the
 * decisions and the fallbacks are [WatchCore]'s. Any of these may throw;
 * none reads anything on the screen.
 *
 * Package visibility (Android 11 and later): LiMo sees only the apps its
 * manifest's `<queries>` name (launchable apps, home apps, diallers,
 * keyboards). An app it cannot see has no label here, and cannot be found
 * as an in call screen or an emergency dialler; [FrontWatch] learns those
 * from what comes to the front instead.
 */
internal class AndroidPhone(private val context: Context) : PhoneSource {
  override val ownPackage: String = context.packageName
  override val stopsReported: Boolean = Build.VERSION.SDK_INT >= 29

  private val pm: PackageManager get() = context.packageManager
  private val resolver: ContentResolver get() = context.contentResolver

  private inline fun <reified T : Any> service(): T =
    requireNotNull(context.getSystemService(T::class.java)) { "no ${T::class.java.simpleName}" }

  // clocks

  override fun wallMs(): Long = System.currentTimeMillis()

  override fun elapsedMs(): Long = SystemClock.elapsedRealtime()

  override fun bootCount(): Int = Settings.Global.getInt(resolver, Settings.Global.BOOT_COUNT)

  override fun utcOffsetSecs(wallMs: Long): Int = TimeZone.getDefault().getOffset(wallMs) / 1_000

  override fun hours24(): Boolean = DateFormat.is24HourFormat(context)

  override fun autoTime(): Boolean = Settings.Global.getInt(resolver, Settings.Global.AUTO_TIME) != 0

  // the screen and calls

  override fun interactive(): Boolean = service<PowerManager>().isInteractive

  override fun keyguardLocked(): Boolean = service<KeyguardManager>().isKeyguardLocked

  override fun audioMode(): Int = service<AudioManager>().mode

  override val callStateNeedsPhone: Boolean = Build.VERSION.SDK_INT >= 31

  // Marked deprecated from Android 12 in favour of one SIM's state
  // (getCallStateForSubscription); this one tells every call on the phone,
  // which is what a lock must step aside for.
  @Suppress("DEPRECATION")
  override fun telephonyCallState(): Int = service<TelephonyManager>().callState

  // permissions

  // Android has marked each of these two names deprecated in turn (the
  // newer SDK marks the newer one). Both read the same mode.
  @Suppress("DEPRECATION")
  override fun usageAccess(): Boolean {
    val ops = service<AppOpsManager>()
    val mode = if (Build.VERSION.SDK_INT >= 29) {
      ops.unsafeCheckOpNoThrow(AppOpsManager.OPSTR_GET_USAGE_STATS, Process.myUid(), ownPackage)
    } else {
      ops.checkOpNoThrow(AppOpsManager.OPSTR_GET_USAGE_STATS, Process.myUid(), ownPackage)
    }
    return mode == AppOpsManager.MODE_ALLOWED ||
      (mode == AppOpsManager.MODE_DEFAULT && context.checkSelfPermission(Manifest.permission.PACKAGE_USAGE_STATS) == PackageManager.PERMISSION_GRANTED)
  }

  override fun accessibilityEnabled(): Boolean {
    if (Settings.Secure.getInt(resolver, Settings.Secure.ACCESSIBILITY_ENABLED, 0) != 1) return false
    val enabled = Settings.Secure.getString(resolver, Settings.Secure.ENABLED_ACCESSIBILITY_SERVICES)
    return AccessibilitySetting.names(enabled, ownPackage, WatchService::class.java.name)
  }

  override fun notificationsEnabled(): Boolean = service<NotificationManager>().areNotificationsEnabled()

  override fun phoneGranted(): Boolean =
    context.checkSelfPermission(Manifest.permission.READ_PHONE_STATE) == PackageManager.PERMISSION_GRANTED

  override fun batteryExempt(): Boolean = service<PowerManager>().isIgnoringBatteryOptimizations(ownPackage)

  override fun deviceAdmin(): Boolean =
    service<DevicePolicyManager>().isAdminActive(ComponentName(context, AdminReceiver::class.java))

  // usage

  override fun usageEvents(fromWallMs: Long, toWallMs: Long): List<UsageEvent>? {
    // With usage access off Android answers with no events rather than a
    // refusal, so ask first.
    if (!usageAccess()) return null
    val events = service<UsageStatsManager>().queryEvents(fromWallMs, toWallMs) ?: return emptyList()
    val out = ArrayList<UsageEvent>()
    val event = UsageEvents.Event()
    while (events.hasNextEvent()) {
      if (!events.getNextEvent(event)) break
      // Only these three are read: the kind, the package and the time.
      val kind = usageKindOf(event.eventType) ?: continue
      val pkg = event.packageName ?: continue
      out.add(UsageEvent(pkg, kind, event.timeStamp))
    }
    return out
  }

  // the roles

  @Suppress("DEPRECATION")
  override fun homePackages(): Set<String> {
    val intent = Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_HOME)
    val all = pm.queryIntentActivities(intent, 0).mapNotNull { it.activityInfo }
    val chosen = pm.resolveActivity(intent, PackageManager.MATCH_DEFAULT_ONLY)?.activityInfo?.packageName
    if (chosen != null && all.any { it.packageName == chosen }) return setOf(chosen)
    // No home app chosen yet (Android asks each time): the preinstalled ones.
    return all.filter { isPreinstalled(it.applicationInfo) }.map { it.packageName }.toSet()
  }

  override fun diallerPackages(): Set<String> {
    val telecom = service<TelecomManager>()
    val out = LinkedHashSet<String>()
    telecom.defaultDialerPackage?.let(out::add)
    if (Build.VERSION.SDK_INT >= 29) telecom.systemDialerPackage?.let(out::add)
    return out
  }

  @Suppress("DEPRECATION")
  override fun inCallScreens(): Set<String> =
    pm.queryIntentServices(Intent(InCallService.SERVICE_INTERFACE), PackageManager.GET_META_DATA)
      .mapNotNull { it.serviceInfo }
      .filter { info ->
        info.metaData?.getBoolean(TelecomManager.METADATA_IN_CALL_SERVICE_UI, false) == true &&
          isPreinstalled(info.applicationInfo)
      }
      .map { it.packageName }
      .toSet()

  /**
   * The apps that open for an emergency call. Android's telecom service
   * names the one it is set up with only to system apps, so LiMo asks which
   * apps handle the emergency dial actions. Any app may say it does; only
   * preinstalled ones are believed. On Android 11 and later the answer holds
   * only the apps the manifest's `<queries>` let LiMo see.
   */
  @Suppress("DEPRECATION")
  override fun emergencyPackages(): Set<String> {
    val out = LinkedHashSet<String>()
    for (action in EMERGENCY_ACTIONS) {
      pm.queryIntentActivities(Intent(action), 0)
        .mapNotNull { it.activityInfo }
        .filter { isPreinstalled(it.applicationInfo) }
        .forEach { out.add(it.packageName) }
    }
    out.removeAll(Overlays.FRAMEWORK)
    return out
  }

  override fun keyboardPackages(): Set<String> {
    val out = HashSet<String>()
    service<InputMethodManager>().inputMethodList.forEach { out.add(it.packageName) }
    Settings.Secure.getString(resolver, Settings.Secure.DEFAULT_INPUT_METHOD)
      ?.substringBefore('/')
      ?.takeIf { it.isNotBlank() }
      ?.let(out::add)
    return out
  }

  @Suppress("DEPRECATION")
  override fun launchablePackages(): Set<String> =
    pm.queryIntentActivities(LAUNCHER, 0).mapNotNull { it.activityInfo?.packageName }.toSet()

  @Suppress("DEPRECATION")
  override fun launchableApps(): Map<String, String?> {
    val out = LinkedHashMap<String, String?>()
    for (resolved in pm.queryIntentActivities(LAUNCHER, 0)) {
      val info = resolved.activityInfo ?: continue
      if (out.containsKey(info.packageName)) continue
      out[info.packageName] = try {
        pm.getApplicationLabel(info.applicationInfo).toString()
      } catch (_: Exception) {
        null
      }
    }
    return out
  }

  @Suppress("DEPRECATION")
  override fun label(packageName: String): String? = try {
    pm.getApplicationLabel(pm.getApplicationInfo(packageName, 0)).toString()
  } catch (_: PackageManager.NameNotFoundException) {
    null
  }

  private fun isPreinstalled(info: ApplicationInfo?): Boolean =
    info != null && (info.flags and ApplicationInfo.FLAG_SYSTEM) != 0

  private companion object {
    val LAUNCHER: Intent = Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_LAUNCHER)

    /** `Intent.ACTION_DIAL_EMERGENCY` (not public), and the action Android's lock screen uses. */
    val EMERGENCY_ACTIONS = listOf(
      "android.intent.action.DIAL_EMERGENCY",
      "com.android.phone.EmergencyDialer.DIAL",
    )
  }
}
