package ai.lucidloom.limo.child.show

/**
 * The look of the covers and the ways out, in one table: every colour and
 * every size the views use, measured from the approved drawings
 * (design/screens/phone.css and phone.js) and the theme's tokens
 * (design/tokens.json). Sizes are density independent pixels, so 390 by 844
 * on the drawing is 390 by 844 dp on a phone; type sizes are scaled pixels,
 * so they follow Android's font size. No Android in here: `LookTest` checks
 * the table against the design's own files.
 */
internal object Look {
  // Colours, ARGB. The first nine are the theme's tokens.

  const val PAPER = 0xFFF6F1E4.toInt()
  const val PAPER2 = 0xFFEFE7D2.toInt()
  const val CARD = 0xFFFFFDF6.toInt()
  const val INK = 0xFF1B2638.toInt()
  const val INK_SOFT = 0xFF46506A.toInt()
  const val MARIGOLD = 0xFFE89C1C.toInt()
  const val MARIGOLD_DEEP = 0xFFB97A08.toInt()
  const val TEAL = 0xFF0E7C66.toInt()
  const val CORAL = 0xFFCF4326.toInt()

  /** rgba(27,38,56,.16): a dashed divider. */
  const val BORDER = 0x291B2638

  /** rgba(27,38,56,.85): the hard shadow of a card or a button. */
  const val SHADOW = 0xD91B2638.toInt()

  /** rgba(27,38,56,.13): the dots of the page. */
  const val DOT = 0x211B2638

  /** rgba(27,38,56,.42): the dashed edge of a request that waits. */
  const val REQUEST_EDGE = 0x6B1B2638

  /** rgba(207,67,38,.06): the wash of the wait after wrong tries. */
  const val WAIT_WASH = 0x0FCF4326

  /** rgba(70,80,106,.8): the words in an empty PIN box. */
  const val PIN_HINT = 0xCC46506A.toInt()

  // Shape

  const val BORDER_WIDTH = 1.5f
  const val STAMP_BORDER = 2f
  const val RADIUS_SM = 7f
  const val RADIUS_MD = 10f
  const val RADIUS_LG = 14f
  const val CARD_SHADOW = 4f
  const val BUTTON_SHADOW = 3f

  /** A dash of a dashed line, and the gap after it, as a browser draws a dashed border of 1.5. */
  const val DASH = 4.5f
  const val DASH_GAP = 3f

  /** The page's dots: one in the middle of every square of 18, of radius 1, soft to 1.2. */
  const val DOT_GRID = 18f
  const val DOT_RADIUS = 1.1f

  // The page

  const val SIDE = 16f
  const val PAGE_TOP = 4f
  const val PAGE_BOTTOM = 6f

  /** Wider than this (a phone on its side, a tablet, an unfolded phone), the page keeps this width in the middle. */
  const val PAGE_MOST = 600f

  /** The strip that holds what is pinned to the bottom: room above and below its buttons, and between two. */
  const val PIN_TOP = 8f
  const val PIN_BOTTOM = 8f
  const val PIN_GAP = 10f

  /** LiMo and the clock at the top of a lock. */
  const val TOP_PAD = 2f
  const val MARK_ICON = 24f
  const val MARK_GAP = 7f
  const val NOW_GAP = 5f
  const val TOP_TO_CARD = 10f

  // The reason card

  const val CARD_PAD_V = 14f
  const val CARD_PAD_H = 16f
  const val WHY_ICON = 64f
  const val STAMP_MARGIN_TOP = 10f
  const val STAMP_MARGIN_END = 4f
  const val STAMP_PAD_TOP = 7f
  const val STAMP_PAD_END = 8f
  const val STAMP_PAD_BOTTOM = 6f
  const val STAMP_PAD_START = 11f
  const val STAMP_TILT = -4f
  const val EYEBROW_TOP = 10f
  const val TITLE_TOP = 6f
  const val DETAIL_TOP = 6f
  const val UNTIL_TOP = 12f
  const val UNTIL_PAD = 10f
  const val UNTIL_WORDS_TOP = 6f

  // What you can do

  const val SECTION_TOP = 16f
  const val SECTION_SIDE = 2f
  const val SECTION_BOTTOM = 8f
  const val ROW_GAP = 8f
  const val ROW_MIN = 52f
  const val ROW_PAD_V = 5f
  const val ROW_PAD_START = 5f
  const val ROW_PAD_END = 14f
  const val ROW_INNER_GAP = 12f
  const val TILE = 40f
  const val TILE_RADIUS = 8f
  const val TILE_ICON = 32f
  const val ROW_SUB_TOP = 1f
  const val CHEVRON = 16f
  const val OFF_ALPHA = 0.5f

  /** A request that waits, the wait after wrong tries, and the words when no way out is offered. */
  const val NOTE_GAP = 10f
  const val NOTE_PAD_TOP = 10f
  const val NOTE_PAD_END = 12f
  const val NOTE_PAD_BOTTOM = 10f
  const val NOTE_PAD_START = 10f
  const val NOTE_ICON = 24f
  const val NOTE_BOTTOM = 8f
  const val SMALL_STAMP_PAD_TOP = 5f
  const val SMALL_STAMP_PAD_H = 8f
  const val SMALL_STAMP_PAD_BOTTOM = 4f
  const val SMALL_STAMP_RADIUS = 6f

  /** The apps that stay open: four to a row. */
  const val APPS_PER_ROW = 4
  const val APP_ICON = 48f
  const val APP_GAP = 4f
  const val APP_ROW_GAP = 12f

  // The end of the page

  const val TAIL_TOP = 18f
  const val TAIL_AFTER_EMERGENCY = 10f
  const val FOOT_SIDE = 4f
  const val MAKER_TOP = 8f
  const val MAKER_GAP = 7f
  const val MAKER_LOGO = 22f
  const val MAKER_LOGO_START = 2f
  const val END_TOP = 16f

  // Buttons

  const val BUTTON_MIN = 48f
  const val BUTTON_PAD_H = 16f
  const val TAP_MIN = 48f

  // The block screen

  const val MID_TOP = 20f
  const val MID_BOTTOM = 6f
  const val BLOCK_BUTTON_TOP = 20f

  // A way out

  const val STRIP_GAP = 10f
  const val STRIP_ICON = 24f
  const val STRIP_ICON_TOP = 1f
  const val STRIP_PAD_BOTTOM = 10f
  const val STRIP_UNTIL_TOP = 1f
  const val BACK_TOP = 10f
  const val BACK_PAD_START = 10f
  const val BACK_PAD_END = 16f
  const val BACK_GAP = 4f
  const val HEADING_TOP = 8f
  const val NOTE_TOP = 6f
  const val SMALL_TOP = 10f
  const val LABEL_GAP_TOP = 12f
  const val LABEL_HINT_GAP = 6f
  const val CHIPS_TOP = 6f
  const val CHIP_GAP = 8f
  const val CHIP_HEIGHT = 48f
  const val CHIP_PAD_H = 4f
  const val CHIP_RADIUS = 8f
  const val FIELD_HEIGHT = 88f
  const val FIELD_TOP = 6f
  const val FIELD_PAD_V = 9f
  const val FIELD_PAD_H = 12f
  const val FIELD_BAND = 4f
  const val COUNT_TOP = 4f
  const val CELLS_TOP = 14f
  const val CELL_GAP = 6f
  const val CELL_MIDDLE_GAP = 10f
  const val CELL_HEIGHT = 52f
  const val CELL_RADIUS = 8f
  const val PIN_BOX_HEIGHT = 52f
  const val PIN_BOX_TOP = 14f
  const val PIN_BOX_PAD_H = 16f
  const val PIN_DOT = 12f
  const val PIN_DOT_GAP = 10f
  const val MESSAGE_TOP = 6f
  const val MESSAGE_ROOM = 36f
  const val KEY_HEIGHT = 48f
  const val KEY_GAP = 8f
  const val KEYS_TOP = 4f
  const val DELETE_WIDTH = 24f
  const val DELETE_HEIGHT = 16f

  /** How long a right code or PIN shows its answer before the screen closes, in milliseconds. */
  const val OPEN_SHOWN_MS = 1_500L

  // The warning

  const val WARN_SIDE = 12f
  const val WARN_TOP = 8f
  const val WARN_PAD_TOP = 14f
  const val WARN_PAD_H = 14f
  const val WARN_PAD_BOTTOM = 12f
  const val WARN_GAP = 12f
  const val WARN_TILE = 48f
  const val WARN_TILE_RADIUS = 10f
  const val WARN_HEADING_TOP = 2f
  const val WARN_HINT_TOP = 6f
  const val WARN_FOOT_TOP = 12f
  const val WARN_FOOT_PAD = 10f
  const val WARN_OK_PAD_H = 22f

  /** A warning goes by itself after this long, in milliseconds: long enough to read, short enough not to stand in the way. */
  const val WARNING_SHOWN_MS = 10_000L

  /** A swipe up or to either side pushes the warning away once it has gone this far. */
  const val SWIPE_AWAY = 48f

  /** Or when the finger lifts after a flick this quick on average: dp a millisecond, so 1,000 dp a second. */
  const val SWIPE_SPEED = 1f

  // Type

  /** The three families of the theme. */
  enum class Family { DISPLAY, BODY, MONO }

  /**
   * One kind of text: its family, weight, size and line height (scaled
   * pixels), letter spacing (in ems), colour, and whether it is set in
   * capitals.
   */
  data class Type(
    val family: Family,
    val weight: Int,
    val size: Float,
    val line: Float,
    val spacing: Float = 0f,
    val color: Int = INK,
    val caps: Boolean = false,
  )

  val MARK = Type(Family.DISPLAY, 600, 22f, 22f, -0.01f)
  val NOW = Type(Family.DISPLAY, 600, 22f, 22f)
  val LABEL = Type(Family.MONO, 500, 12f, 12f, 0.12f, INK_SOFT, caps = true)
  val LABEL_HINT = Type(Family.BODY, 400, 14f, 14f, 0f, INK_SOFT)
  val STAMP = Type(Family.MONO, 600, 13f, 13f, 0.18f, CORAL, caps = true)
  val TITLE = Type(Family.DISPLAY, 600, 28f, 30.8f, -0.012f)
  val DETAIL = Type(Family.BODY, 400, 16f, 21.6f, 0f, INK_SOFT)
  val UNTIL = Type(Family.DISPLAY, 600, 24f, 26.4f, 0f, MARIGOLD_DEEP)
  val ROW_TITLE = Type(Family.BODY, 600, 16f, 20f)
  val ROW_SUB = Type(Family.BODY, 400, 14f, 17.5f, 0f, INK_SOFT)
  val NOTE_TEXT = Type(Family.BODY, 400, 14f, 18.9f)
  val SMALL_STAMP = Type(Family.MONO, 600, 12f, 12f, 0.1f, INK, caps = true)
  val APP_NAME = Type(Family.BODY, 400, 14f, 16.8f)
  val BUTTON = Type(Family.BODY, 600, 16f, 16f)
  val FOOT = Type(Family.MONO, 500, 12f, 16.8f, 0f, INK_SOFT)
  val MAKER_WORDS = Type(Family.MONO, 500, 12f, 12f, 0.12f, INK_SOFT, caps = true)
  val MAKER_NAME = Type(Family.BODY, 600, 14f, 14f, -0.005f)
  val STRIP_TITLE = Type(Family.BODY, 600, 16f, 20f)
  val STRIP_UNTIL = Type(Family.BODY, 600, 14f, 17.5f, 0f, MARIGOLD_DEEP)
  val HEADING = Type(Family.DISPLAY, 600, 28f, 30.8f, -0.012f)
  val NOTE = Type(Family.BODY, 400, 16f, 22.4f, 0f, INK_SOFT)
  val SMALL = Type(Family.BODY, 400, 14f, 19.6f, 0f, INK_SOFT)
  val CHIP = Type(Family.MONO, 500, 14f, 14f)
  val FIELD = Type(Family.BODY, 400, 16f, 22.4f)
  val COUNT = Type(Family.MONO, 500, 12f, 12f, 0f, INK_SOFT)
  val CELL = Type(Family.MONO, 600, 22f, 22f)
  val PIN_PLACEHOLDER = Type(Family.MONO, 500, 14f, 14f, 0f, PIN_HINT)
  val MESSAGE = Type(Family.BODY, 600, 14f, 18f, 0f, CORAL)
  val KEY = Type(Family.MONO, 600, 22f, 22f)
  val WARN_HEADING = Type(Family.DISPLAY, 600, 20f, 24f, -0.005f)
  val WARN_HINT = Type(Family.BODY, 400, 14f, 19.6f, 0f, INK_SOFT)
}
