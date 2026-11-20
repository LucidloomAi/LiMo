package ai.lucidloom.limo.child.seam

import ai.lucidloom.limo.child.keep.AdminReceiver
import ai.lucidloom.limo.child.keep.Battery
import ai.lucidloom.limo.child.show.WaysOutActivity
import ai.lucidloom.limo.child.watch.WatchService
import android.app.Activity
import android.app.admin.DevicePolicyManager
import android.content.ActivityNotFoundException
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.provider.Settings
import expo.modules.interfaces.permissions.Permissions
import expo.modules.interfaces.permissions.PermissionsResponse
import expo.modules.interfaces.permissions.PermissionsStatus

/**
 * Opens what Android has to show for a step (`child.openStep`), and LiMo's
 * ways out (`child.askForMoreTime`). Everything here runs on the main
 * thread. The ways for each step, on each version, are [StepPlans]'.
 *
 * - **A question Android asks over the app** (notifications on Android 13
 *   and later, the Phone permission) goes through Expo's permission service
 *   and is opened once it is answered. Where Android will not ask again,
 *   the step's own page in Settings is opened instead.
 * - **A screen that answers** (battery, the device admin) is started for a
 *   result, and the step is opened once it answers.
 * - **A settings page** (usage access, accessibility, and the pages above
 *   where the question cannot be asked) is opened once it is open.
 *
 * Nothing that opens is told the child's name, nor anything else of the
 * family: only LiMo's own package and components.
 */
internal class StepOpener(
  private val app: Context,
  private val main: Handler,
  private val log: SeamLog,
) {
  /** The steps waiting for a screen's answer, by request code. */
  private val waiting = HashMap<Int, () -> Unit>()

  /**
   * Opens [step] from [activity], with Expo's [permissions]; [done] is told
   * once, on the main thread: true once Android's screen or question is
   * open or answered, false when nothing could be opened.
   */
  fun open(step: Step, activity: Activity?, permissions: Permissions?, done: (Boolean) -> Unit) {
    main.post {
      try {
        if (activity == null || activity.isFinishing) {
          done(false)
        } else {
          openOn(step, activity, permissions, done)
        }
      } catch (e: Exception) {
        log.line("seam failed what=open_step step=${step.id} error=${e.javaClass.simpleName}")
        done(false)
      }
    }
  }

  /** From the activity's results: a step waiting for [requestCode] is answered. Any other code is not LiMo's. */
  fun result(requestCode: Int) {
    main.post { waiting.remove(requestCode)?.invoke() }
  }

  /**
   * The app came back to the front: a screen's answer that did not come
   * (Android delivers it before this) is taken as given, so the screens
   * never wait for ever.
   */
  fun backInFront() {
    main.post {
      val all = waiting.values.toList()
      waiting.clear()
      for (answered in all) answered()
    }
  }

  /**
   * Opens LiMo's ways out at asking for more time, with no lock behind it
   * (`WaysOutActivity.openAsking`), and nothing else. True when it opened.
   */
  fun askForMoreTime(activity: Activity?): Boolean {
    val from: Context = activity ?: app
    return try {
      WaysOutActivity.openAsking(from)
      true
    } catch (e: ActivityNotFoundException) {
      log.line("seam failed what=ways_out error=${e.javaClass.simpleName}")
      false
    } catch (e: SecurityException) {
      log.line("seam failed what=ways_out error=${e.javaClass.simpleName}")
      false
    }
  }

  private fun openOn(step: Step, activity: Activity, permissions: Permissions?, done: (Boolean) -> Unit) {
    val permission = runtimePermission(step)
    val now = permission?.let { read(permissions, it) }
    val ways = StepPlans.ways(step, Build.VERSION.SDK_INT, granted = now?.granted == true, blocked = now?.blocked == true)
    for (way in ways) {
      when (way) {
        is StepWay.Ask -> {
          if (permissions == null) continue
          val rationaleBefore = activity.shouldShowRequestPermissionRationale(way.permission)
          val askedBefore = now?.asked == true
          try {
            permissions.askForPermissions({ result ->
              val r = result[way.permission]
              val granted = r?.status == PermissionsStatus.GRANTED
              val blockedAfter = r != null && r.status == PermissionsStatus.DENIED && !r.canAskAgain
              if (StepPlans.pageAfterAnswer(granted, askedBefore, rationaleBefore, blockedAfter)) {
                StepPlans.pageOf(ways)?.let { start(activity, it) }
              }
              log.line("seam step=${step.id} asked granted=$granted")
              done(true)
            }, way.permission)
            return
          } catch (e: IllegalStateException) {
            log.line("seam failed what=ask step=${step.id} error=${e.javaClass.simpleName}")
          }
        }
        is StepWay.Answer -> {
          val code = StepPlans.requestCode(way.question)
          try {
            activity.startActivityForResult(intent(way.question), code)
            waiting.remove(code)?.invoke()
            waiting[code] = {
              log.line("seam step=${step.id} answered")
              done(true)
            }
            return
          } catch (_: ActivityNotFoundException) {
            // The next way.
          } catch (_: SecurityException) {
            // The next way.
          }
        }
        is StepWay.Open -> if (start(activity, way.page)) {
          log.line("seam step=${step.id} opened page=${way.page.name.lowercase()}")
          done(true)
          return
        }
      }
    }
    log.line("seam step=${step.id} not_opened")
    done(false)
  }

  /** Starts [page] from [activity], so that Back comes back to LiMo. True when it opened. */
  private fun start(activity: Activity, page: Page): Boolean = try {
    activity.startActivity(intent(page))
    true
  } catch (_: ActivityNotFoundException) {
    false
  } catch (_: SecurityException) {
    false
  }

  private fun intent(page: Page): Intent {
    val pkg = app.packageName
    return when (page) {
      Page.APP_NOTIFICATIONS -> Intent(Settings.ACTION_APP_NOTIFICATION_SETTINGS).putExtra(Settings.EXTRA_APP_PACKAGE, pkg)
      Page.APP_DETAILS -> Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.fromParts("package", pkg, null))
      Page.USAGE_FOR_LIMO -> Intent(Settings.ACTION_USAGE_ACCESS_SETTINGS, Uri.fromParts("package", pkg, null))
      Page.USAGE_LIST -> Intent(Settings.ACTION_USAGE_ACCESS_SETTINGS)
      Page.ACCESSIBILITY_LIST -> {
        // A hint that Settings on many phones reads to scroll to the row
        // with this key and light it up; others ignore it.
        val row = ComponentName(app, WatchService::class.java).flattenToString()
        Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS)
          .putExtra(SETTINGS_ROW, row)
          .putExtra(SETTINGS_ROW_ARGS, Bundle().apply { putString(SETTINGS_ROW, row) })
      }
      Page.BATTERY_LIST -> Battery.list()
    }
  }

  private fun intent(question: Question): Intent = when (question) {
    Question.BATTERY -> Battery.question(app)
    // The page shows the admin's own description (limo_admin_description)
    // as its explanation.
    Question.DEVICE_ADMIN -> Intent(DevicePolicyManager.ACTION_ADD_DEVICE_ADMIN)
      .putExtra(DevicePolicyManager.EXTRA_DEVICE_ADMIN, ComponentName(app, AdminReceiver::class.java))
  }

  /** The step's runtime permission, where it has one on this Android. */
  private fun runtimePermission(step: Step): String? = when (step) {
    Step.NOTIFICATIONS -> if (Build.VERSION.SDK_INT >= StepPlans.TIRAMISU) StepPlans.POST_NOTIFICATIONS else null
    Step.PHONE -> StepPlans.READ_PHONE_STATE
    else -> null
  }

  private class Reading(val granted: Boolean, val asked: Boolean, val blocked: Boolean)

  /** A runtime permission now: granted by Android, and asked or blocked as Expo's permission service knows it. */
  private fun read(permissions: Permissions?, permission: String): Reading {
    val granted = app.checkSelfPermission(permission) == PackageManager.PERMISSION_GRANTED
    var response: PermissionsResponse? = null
    try {
      permissions?.getPermissions({ result -> response = result[permission] }, permission)
    } catch (e: Exception) {
      log.line("seam failed what=read_permission error=${e.javaClass.simpleName}")
    }
    val r = response
    return Reading(
      granted = granted,
      asked = r != null && r.status != PermissionsStatus.UNDETERMINED,
      blocked = !granted && r != null && r.status == PermissionsStatus.DENIED && !r.canAskAgain,
    )
  }

  companion object {
    private const val SETTINGS_ROW = ":settings:fragment_args_key"
    private const val SETTINGS_ROW_ARGS = ":settings:show_fragment_args"
  }
}
