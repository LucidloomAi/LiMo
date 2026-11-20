package ai.lucidloom.limo.child.show

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The table of the look ([Look]) checked against the design's own files:
 * the theme's tokens (design/tokens.json) and the approved drawings' sizes
 * (design/screens/phone.css). A change to the design that the table does
 * not follow fails here.
 */
class LookTest {
  private val tokens: String = File(REPO, "design/tokens.json").readText()
  private val css: Map<String, Map<String, String>> = readCss(File(REPO, "design/screens/phone.css").readText())

  private fun token(key: String): String =
    Regex("\"$key\"\\s*:\\s*\"([^\"]+)\"").find(tokens)?.groupValues?.get(1) ?: error("tokens.json has no $key")

  private fun tokenNumber(group: String, key: String): Float {
    val block = Regex("\"$group\"\\s*:\\s*\\{([^}]*)\\}").find(tokens)?.groupValues?.get(1) ?: error("no $group")
    return Regex("\"$key\"\\s*:\\s*([0-9.]+)").find(block)!!.groupValues[1].toFloat()
  }

  /** "#f6f1e4" or "rgba(27,38,56,0.16)" as ARGB, alpha rounded as Android's colour resources are. */
  private fun argb(value: String): Int {
    if (value.startsWith("#")) return (0xFF000000L or value.substring(1).toLong(16)).toInt()
    val (r, g, b, a) = value.removePrefix("rgba(").removeSuffix(")").split(",").map { it.trim().toFloat() }
    return (Math.round(a * 255) shl 24) or (r.toInt() shl 16) or (g.toInt() shl 8) or b.toInt()
  }

  private fun prop(selector: String, name: String): String =
    css[selector]?.get(name) ?: error("phone.css has no $name in $selector")

  private fun px(selector: String, name: String): Float = prop(selector, name).removeSuffix("px").toFloat()

  private fun pxs(selector: String, name: String): List<Float> =
    prop(selector, name).split(" ").map { if (it == "0") 0f else it.removeSuffix("px").toFloat() }

  /** "600 28px/1.1 var(--display)": weight, size, line height in pixels. */
  private fun font(selector: String): Triple<Int, Float, Float> {
    val m = Regex("""(\d{3}) ([0-9.]+)px/([0-9.]+)(px)?""").find(prop(selector, "font")) ?: error("no font in $selector")
    val size = m.groupValues[2].toFloat()
    val line = m.groupValues[3].toFloat().let { if (m.groupValues[4] == "px") it else it * size }
    return Triple(m.groupValues[1].toInt(), size, line)
  }

  private fun near(what: String, expected: Float, actual: Float) =
    assertEquals(what, expected.toDouble(), actual.toDouble(), 0.01)

  @Test
  fun theColoursAreTheThemesTokens() {
    assertEquals(argb(token("paper")), Look.PAPER)
    assertEquals(argb(token("paper2")), Look.PAPER2)
    assertEquals(argb(token("card")), Look.CARD)
    assertEquals(argb(token("ink")), Look.INK)
    assertEquals(argb(token("inkSoft")), Look.INK_SOFT)
    assertEquals(argb(token("marigold")), Look.MARIGOLD)
    assertEquals(argb(token("marigoldDeep")), Look.MARIGOLD_DEEP)
    assertEquals(argb(token("teal")), Look.TEAL)
    assertEquals(argb(token("coral")), Look.CORAL)
    assertEquals(argb(token("border")), Look.BORDER)
    assertEquals(argb(token("shadowInk")), Look.SHADOW)
    // The drawings' own washes.
    assertTrue(prop(".ph-frame, .ph-crop", "background-image").contains("rgba(27,38,56,.13)"))
    assertEquals(argb("rgba(27,38,56,0.13)"), Look.DOT)
    assertTrue(prop(".ph-req, .ph-wait", "border").contains("rgba(27,38,56,.42)"))
    assertEquals(argb("rgba(27,38,56,0.42)"), Look.REQUEST_EDGE)
    assertEquals("rgba(207,67,38,.06)", prop(".ph-wait", "background"))
    assertEquals(argb("rgba(207,67,38,0.06)"), Look.WAIT_WASH)
    assertEquals(argb("rgba(70,80,106,0.8)"), Look.PIN_HINT)
  }

  @Test
  fun theShapesAreTheThemesTokens() {
    near("border", tokenNumber("border", "width"), Look.BORDER_WIDTH)
    near("sm", tokenNumber("radius", "sm"), Look.RADIUS_SM)
    near("md", tokenNumber("radius", "md"), Look.RADIUS_MD)
    near("lg", tokenNumber("radius", "lg"), Look.RADIUS_LG)
    assertTrue(tokens.contains("\"card\": \"${Look.CARD_SHADOW.toInt()}px ${Look.CARD_SHADOW.toInt()}px 0 rgba(27,38,56,0.85)\""))
    assertTrue(tokens.contains("\"button\": \"${Look.BUTTON_SHADOW.toInt()}px ${Look.BUTTON_SHADOW.toInt()}px 0 rgba(27,38,56,0.85)\""))
    near("dot grid", 18f, Look.DOT_GRID)
    assertEquals("18px 18px", prop(".ph-frame, .ph-crop", "background-size"))
  }

  @Test
  fun theLockIsMeasuredFromTheDrawing() {
    assertEquals(listOf(Look.PAGE_TOP, Look.SIDE, Look.PAGE_BOTTOM), pxs(".ph-lock", "padding"))
    assertEquals(listOf(Look.CARD_PAD_V, Look.CARD_PAD_H), pxs(".ph-card", "padding"))
    near("card corner", Look.RADIUS_LG, px(".ph-card", "border-radius"))
    assertEquals("4px 4px 0 var(--shadow)", prop(".ph-card", "box-shadow"))
    assertEquals(listOf(Look.STAMP_MARGIN_TOP, Look.STAMP_MARGIN_END, 0f, 0f), pxs(".ph-stamp", "margin"))
    assertEquals(listOf(Look.STAMP_PAD_TOP, Look.STAMP_PAD_END, Look.STAMP_PAD_BOTTOM, Look.STAMP_PAD_START), pxs(".ph-stamp", "padding"))
    assertEquals("rotate(${Look.STAMP_TILT.toInt()}deg)", prop(".ph-stamp", "transform"))
    near("eyebrow", Look.EYEBROW_TOP, px(".ph-eyebrow", "margin-top"))
    near("until", Look.UNTIL_TOP, px(".ph-until", "margin-top"))
    near("until pad", Look.UNTIL_PAD, px(".ph-until", "padding-top"))
    assertEquals(listOf(Look.SECTION_TOP, Look.SECTION_SIDE, Look.SECTION_BOTTOM), pxs(".ph-sec", "margin"))
    near("row min", Look.ROW_MIN, px(".ph-row", "min-height"))
    assertEquals(listOf(Look.ROW_PAD_V, Look.ROW_PAD_END, Look.ROW_PAD_V, Look.ROW_PAD_START), pxs(".ph-row", "padding"))
    near("row gap", Look.ROW_INNER_GAP, px(".ph-row", "gap"))
    near("rows gap", Look.ROW_GAP, px(".ph-rows", "gap"))
    near("tile", Look.TILE, px(".ph-tile", "width"))
    near("tile corner", Look.TILE_RADIUS, px(".ph-tile", "border-radius"))
    assertEquals(".5", prop(".ph-row.off", "opacity"))
    assertEquals(listOf(Look.NOTE_PAD_TOP, Look.NOTE_PAD_END, Look.NOTE_PAD_BOTTOM, Look.NOTE_PAD_START), pxs(".ph-req, .ph-wait", "padding"))
    near("app icon", Look.APP_ICON, px(".ph-letter", "width"))
    near("tail", Look.TAIL_TOP, px(".ph-tail", "padding-top"))
    near("maker logo", Look.MAKER_LOGO, px(".ph-maker img", "width"))
    near("maker gap", Look.MAKER_GAP, px(".ph-maker", "gap"))
    near("button", Look.BUTTON_MIN, px(".ph-btn", "min-height"))
    assertEquals("3px 3px 0 var(--shadow)", prop(".ph-btn", "box-shadow"))
    // The strip pinned to the bottom: 64 for one button, 122 for two.
    assertEquals(listOf(Look.PIN_TOP, Look.SIDE, 0f), pxs(".ph-pin", "padding"))
    near("one pinned", 64f, px(".ph-frame", "--pin"))
    near("two pinned", 122f, px(".ph-frame.pin2", "--pin"))
    near("one pinned, measured", 64f, Look.PIN_TOP + Look.BUTTON_MIN + Look.PIN_BOTTOM)
    near("two pinned, measured", 122f, Look.PIN_TOP + 2 * Look.BUTTON_MIN + Look.PIN_GAP + Look.PIN_BOTTOM)
    near("block card", Look.MID_TOP, pxs(".ph-mid", "padding")[0])
  }

  @Test
  fun theWaysOutAndTheWarningAreMeasuredFromTheDrawing() {
    near("strip gap", Look.STRIP_GAP, px(".ph-strip", "gap"))
    near("strip pad", Look.STRIP_PAD_BOTTOM, px(".ph-strip", "padding-bottom"))
    near("back", Look.BUTTON_MIN, px(".ph-back", "height"))
    assertEquals(listOf(0f, Look.BACK_PAD_END, 0f, Look.BACK_PAD_START), pxs(".ph-back", "padding"))
    near("chip", Look.CHIP_HEIGHT, px(".ph-chip", "height"))
    near("chip corner", Look.CHIP_RADIUS, px(".ph-chip", "border-radius"))
    near("chips gap", Look.CHIP_GAP, px(".ph-chips", "gap"))
    near("field", Look.FIELD_HEIGHT, px(".ph-text", "height"))
    assertEquals(listOf(Look.FIELD_PAD_V, Look.FIELD_PAD_H), pxs(".ph-text", "padding"))
    assertEquals("inset 0 -${Look.FIELD_BAND.toInt()}px 0 var(--marigold)", prop(".ph-text", "box-shadow"))
    near("cell", Look.CELL_HEIGHT, px(".ph-cell", "height"))
    near("cells gap", Look.CELL_GAP, px(".ph-cells", "gap"))
    near("middle gap", Look.CELL_MIDDLE_GAP, px(".ph-cell.gap", "margin-left"))
    near("pin box", Look.PIN_BOX_HEIGHT, px(".ph-pinbox", "height"))
    near("pin dot", Look.PIN_DOT, px(".ph-pinbox .dot", "width"))
    near("message room", Look.MESSAGE_ROOM, px(".ph-err", "min-height"))
    near("keys gap", Look.KEY_GAP, px(".ph-pad", "gap"))
    assertTrue(prop(".ph-pad", "grid-auto-rows").contains("${Look.KEY_HEIGHT.toInt()}px"))
    near("warning side", Look.WARN_SIDE, px(".ph-nt", "left"))
    near("warning top", Look.WARN_TOP, px(".ph-nt", "top"))
    assertEquals(listOf(Look.WARN_PAD_TOP, Look.WARN_PAD_H, Look.WARN_PAD_BOTTOM), pxs(".ph-nt", "padding"))
    near("warning tile", Look.WARN_TILE, px(".ph-tile.lg", "width"))
  }

  @Test
  fun theTypeIsMeasuredFromTheDrawing() {
    val checks = listOf(
      ".ph-h1" to Look.TITLE,
      ".ph-until b" to Look.UNTIL,
      ".ph-stamp" to Look.STAMP,
      ".ph-btn" to Look.BUTTON,
      ".ph-foot" to Look.FOOT,
      ".ph-maker b" to Look.MAKER_NAME,
      ".ph-h2" to Look.HEADING,
      ".ph-chip" to Look.CHIP,
      ".ph-cell" to Look.CELL,
      ".ph-key" to Look.KEY,
      ".ph-st" to Look.SMALL_STAMP,
      ".ph-mark" to Look.MARK,
      ".ph-nt h3" to Look.WARN_HEADING,
      ".ph-pinbox .ph" to Look.PIN_PLACEHOLDER,
    )
    for ((selector, type) in checks) {
      val (weight, size, line) = font(selector)
      assertEquals(selector, weight, type.weight)
      near("$selector size", size, type.size)
      near("$selector line", line, type.line)
    }
    // Sizes and line heights set apart from a font shorthand.
    near("detail", 16f * 1.35f, Look.DETAIL.line)
    near("row title", 16f * 1.25f, Look.ROW_TITLE.line)
    near("row line", 14f * 1.25f, Look.ROW_SUB.line)
    near("message", px(".ph-err", "line-height"), Look.MESSAGE.line)
    near("note", 16f * 1.4f, Look.NOTE.line)
    near("small", 14f * 1.4f, Look.SMALL.line)
    // Letter spacing, in ems.
    assertEquals(".12em", prop(".ph-label, .ph-now span", "letter-spacing"))
    near("label", 0.12f, Look.LABEL.spacing)
    assertEquals(".18em", prop(".ph-stamp", "letter-spacing"))
    near("stamp", 0.18f, Look.STAMP.spacing)
    assertEquals("-.012em", prop(".ph-h1", "letter-spacing"))
    near("title", -0.012f, Look.TITLE.spacing)
    // Nothing smaller than 12, as the drawings promise.
    val all = Look::class.java.declaredFields.filter { it.type == Look.Type::class.java }.map {
      it.isAccessible = true
      it.get(Look) as Look.Type
    }
    assertTrue(all.size >= 30)
    assertTrue(all.all { it.size >= 12f })
  }

  companion object {
    /** A small reader of a plain style sheet: each rule's selector and its declarations. Comments and @ rules left out. */
    fun readCss(text: String): Map<String, Map<String, String>> {
      val plain = text.replace(Regex("/\\*.*?\\*/", RegexOption.DOT_MATCHES_ALL), "")
      val out = HashMap<String, MutableMap<String, String>>()
      for (m in Regex("([^{}@]+)\\{([^}]*)\\}").findAll(plain)) {
        val selector = m.groupValues[1].trim().replace(Regex("\\s+"), " ")
        val map = out.getOrPut(selector) { HashMap() }
        for (decl in m.groupValues[2].split(";")) {
          val i = decl.indexOf(':')
          if (i > 0) map[decl.substring(0, i).trim()] = decl.substring(i + 1).trim()
        }
      }
      return out
    }
  }
}
