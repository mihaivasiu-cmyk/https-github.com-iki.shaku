package org.iki.shakuhachi

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.Path
import android.graphics.RectF
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.text.TextUtils
import android.view.MotionEvent
import android.view.View
import android.view.ViewConfiguration
import android.view.ViewGroup
import android.widget.LinearLayout
import android.widget.PopupMenu
import android.widget.TextView
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.cos
import kotlin.math.hypot
import kotlin.math.max
import kotlin.math.min
import kotlin.math.roundToInt
import kotlin.math.sin

// Hand-drawn controls, ported from the desktop `widgets.rs` (sizes in dp ≈ egui points).

/** Anything that repaints itself for the day / night palette. */
interface Themed {
    fun theme(p: Palette)
}

/** Walk a view tree and apply the palette to every [Themed] view. */
fun View.applyTheme(p: Palette) {
    if (this is Themed) theme(p)
    if (this is ViewGroup) for (i in 0 until childCount) getChildAt(i).applyTheme(p)
}

enum class VAlign { TOP, CENTER, BOTTOM }

/**
 * Base for the custom controls: palette, dp, paints, text helpers, and the
 * touch policy that keeps the page scrollable on a phone:
 *
 *  * instrument surfaces (pads, muraiki, the stage faders, loop mixer) take the finger at once;
 *  * knobs only when it lands on the dial — the same hit box as the desktop — so a
 *    drag that starts on a knob's label scrolls the page;
 *  * horizontal drags (loop trims) wait until the finger has picked a direction;
 *  * taps (buttons, segments, steps, rockers) never claim: a drag that starts on
 *    them scrolls the page and cancels the tap.
 */
abstract class IkiView(c: Context) : View(c), Themed {
    protected var pal: Palette = Palette.DAY
    protected val density = c.resources.displayMetrics.density
    protected val slop = ViewConfiguration.get(c).scaledTouchSlop.toFloat()
    protected val fill = Paint(Paint.ANTI_ALIAS_FLAG)
    protected val stroke = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        style = Paint.Style.STROKE
        strokeCap = Paint.Cap.ROUND
        strokeJoin = Paint.Join.ROUND
    }
    protected val text = Paint(Paint.ANTI_ALIAS_FLAG).apply { typeface = Typeface.SANS_SERIF }
    private val fm = Paint.FontMetrics()
    protected val r = RectF()

    override fun theme(p: Palette) {
        pal = p
        invalidate()
    }

    fun dp(v: Float) = v * density
    fun dp(v: Int) = v * density

    /** Keep the enclosing ScrollView from stealing this gesture. */
    protected fun claim() {
        parent?.requestDisallowInterceptTouchEvent(true)
    }

    protected fun Canvas.label(
        s: String, x: Float, y: Float, sizeDp: Float, color: Int,
        align: Paint.Align = Paint.Align.CENTER, v: VAlign = VAlign.CENTER, bold: Boolean = false,
    ) {
        text.textSize = dp(sizeDp)
        text.color = color
        text.textAlign = align
        text.isFakeBoldText = bold
        text.getFontMetrics(fm)
        val base = when (v) {
            VAlign.TOP -> y - fm.ascent
            VAlign.CENTER -> y - (fm.ascent + fm.descent) / 2
            VAlign.BOTTOM -> y - fm.descent
        }
        drawText(s, x, base, text)
    }

    /** [s] shortened with "…" to fit [widthPx] at [sizeDp]. */
    protected fun fit(s: String, sizeDp: Float, widthPx: Float): String {
        text.textSize = dp(sizeDp)
        return TextUtils.ellipsize(s, android.text.TextPaint(text), max(0f, widthPx), TextUtils.TruncateAt.END).toString()
    }

    protected fun roundRect(c: Canvas, l: Float, t: Float, rr: Float, b: Float, radiusDp: Float, paint: Paint) {
        r.set(l, t, rr, b)
        c.drawRoundRect(r, dp(radiusDp), dp(radiusDp), paint)
    }

    /** Desktop `paint_fader`: slot, filled part, cap with a centre line. */
    protected fun paintFader(c: Canvas, x: Float, top: Float, bot: Float, value: Double, ink: Int, live: Boolean) {
        fill.color = pal.surface2
        roundRect(c, x - dp(3), top, x + dp(3), bot, 3f, fill)
        stroke.color = pal.border
        stroke.strokeWidth = dp(1)
        roundRect(c, x - dp(3), top, x + dp(3), bot, 3f, stroke)
        val y = bot - (bot - top) * value.toFloat()
        fill.color = Palette.blend(ink, pal.surface, 0.35f)
        roundRect(c, x - dp(3), y, x + dp(3), bot, 3f, fill)
        fill.color = if (live) ink else pal.metal
        roundRect(c, x - dp(11), y - dp(6), x + dp(11), y + dp(6), 4f, fill)
        stroke.color = Palette.blend(pal.subtle, pal.border, 0.3f)
        roundRect(c, x - dp(11), y - dp(6), x + dp(11), y + dp(6), 4f, stroke)
        stroke.color = pal.fg
        c.drawLine(x - dp(7), y, x + dp(7), y, stroke)
    }

    protected fun faderValue(y: Float, top: Float, bot: Float): Double =
        ((bot - y) / max(1f, bot - top)).toDouble().coerceIn(0.0, 1.0)
}

/** Tap semantics shared by buttons: held look, fire on release inside, cancelled by a scroll. */
abstract class Tappable(c: Context) : IkiView(c) {
    protected var held = false
    var enabledTap = true
    var onClick: (() -> Unit)? = null

    /** Act the moment the finger lands instead of on release (looper transport). */
    var fireOnDown = false
    private var firedOnDown = false

    @SuppressLint("ClickableViewAccessibility")
    override fun onTouchEvent(e: MotionEvent): Boolean {
        when (e.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                if (!hit(e.x, e.y)) return false
                held = true
                invalidate()
                firedOnDown = false
                if (fireOnDown && enabledTap) {
                    firedOnDown = true
                    tapped(e.x, e.y)
                }
            }
            MotionEvent.ACTION_UP -> {
                val fire = !firedOnDown && held && enabledTap && e.x in 0f..width.toFloat() && e.y in 0f..height.toFloat()
                firedOnDown = false
                held = false
                invalidate()
                if (fire) {
                    tapped(e.x, e.y)
                    performClick()
                }
            }
            MotionEvent.ACTION_CANCEL -> {
                held = false
                invalidate()
            }
        }
        return true
    }

    override fun performClick(): Boolean {
        super.performClick()
        return true
    }

    protected open fun hit(x: Float, y: Float) = true

    protected open fun tapped(x: Float, y: Float) {
        onClick?.invoke()
    }
}

/** Desktop `button` / `disabled_button`: a small lit/unlit rectangle. */
open class Btn(c: Context, label: String, private val wDp: Float = 0f, private val hDp: Float = 32f) : Tappable(c) {
    var label: String = label
        set(v) {
            if (v != field) {
                field = v
                contentDescription = v
                requestLayout()
                invalidate()
            }
        }
    var on = false
        set(v) {
            if (v != field) {
                field = v
                invalidate()
            }
        }
    var usable = true
        set(v) {
            if (v != field) {
                field = v
                enabledTap = v
                invalidate()
            }
        }
    private val undo = Path()

    init {
        contentDescription = label
    }

    override fun onMeasure(w: Int, h: Int) {
        text.textSize = dp(10.5f)
        val natural = max(dp(wDp), text.measureText(label) + dp(18))
        setMeasuredDimension(resolveSize(natural.roundToInt(), w), dp(hDp).roundToInt())
    }

    override fun onDraw(c: Canvas) {
        val w = width.toFloat()
        val h = height.toFloat()
        fill.color = when {
            !usable -> pal.surface2
            on -> pal.moss
            held -> Palette.blend(pal.surface2, pal.moss, 0.35f)
            else -> pal.surface2
        }
        val inset = dp(1)
        roundRect(c, inset, inset, w - inset, h - inset, 4f, fill)
        stroke.color = pal.border
        stroke.strokeWidth = dp(1)
        roundRect(c, inset, inset, w - inset, h - inset, 4f, stroke)
        val ink = if (!usable) Palette.blend(pal.subtle, pal.surface2, 0.5f) else if (on) pal.mossFg else pal.fg
        if (label == "↺") drawUndo(c, w / 2, h / 2, ink) else c.label(label, w / 2, h / 2, 10.5f, ink)
    }

    /** The fonts lack ↺: draw it (same shape as the desktop). */
    private fun drawUndo(c: Canvas, cx: Float, cy: Float, ink: Int) {
        val rad = dp(5)
        undo.reset()
        for (i in 0..20) {
            val a = Math.toRadians(-60.0 + 280.0 * i / 20.0)
            val x = cx + cos(a).toFloat() * rad
            val y = cy + sin(a).toFloat() * rad
            if (i == 0) undo.moveTo(x, y) else undo.lineTo(x, y)
        }
        stroke.color = ink
        stroke.strokeWidth = dp(1.3f)
        c.drawPath(undo, stroke)
        val tx = cx + cos(Math.toRadians(-60.0)).toFloat() * rad
        val ty = cy + sin(Math.toRadians(-60.0)).toFloat() * rad
        undo.reset()
        undo.moveTo(tx - dp(3.5f), ty - dp(1))
        undo.lineTo(tx + dp(1.5f), ty - dp(3.5f))
        undo.lineTo(tx + dp(1.5f), ty + dp(2))
        undo.close()
        fill.color = ink
        c.drawPath(undo, fill)
    }
}

/** A [Btn] that opens a menu of [choices] (the desktop Div combo box). */
class DropDown(c: Context, private val prefix: String, private val choices: Array<String>) : Btn(c, "", 96f) {
    var selected = 0
        set(v) {
            field = v
            label = "$prefix ${choices.getOrElse(v) { "" }} ▾"
        }
    var onSelect: ((Int) -> Unit)? = null

    init {
        selected = 0
        onClick = {
            val m = PopupMenu(context, this)
            choices.forEachIndexed { i, s -> m.menu.add(0, i, i, s) }
            m.setOnMenuItemClickListener {
                selected = it.itemId
                onSelect?.invoke(it.itemId)
                true
            }
            m.show()
        }
    }
}

/** Desktop `segmented`; wraps onto more rows when the cells would get too narrow. */
class Segmented(c: Context, private val labels: Array<String>, private val minCellDp: Float = 40f) : Tappable(c) {
    var selected = 0
        set(v) {
            if (v != field) {
                field = v
                invalidate()
            }
        }
    var onSelect: ((Int) -> Unit)? = null
    private val rowDp = 28f

    init {
        contentDescription = labels.joinToString("/")
    }

    private fun perRow(width: Float): Int {
        val n = labels.size
        val fit = max(1, (width / dp(minCellDp)).toInt())
        if (fit >= n) return n
        val rows = (n + fit - 1) / fit
        return (n + rows - 1) / rows
    }

    override fun onMeasure(w: Int, h: Int) {
        var width = MeasureSpec.getSize(w)
        if (MeasureSpec.getMode(w) == MeasureSpec.UNSPECIFIED) width = (labels.size * dp(minCellDp + 8)).roundToInt()
        val per = perRow(width.toFloat())
        val rows = (labels.size + per - 1) / per
        setMeasuredDimension(width, (rows * dp(rowDp)).roundToInt())
    }

    private fun cell(i: Int, out: RectF) {
        val per = perRow(width.toFloat())
        val row = i / per
        val colN = min(per, labels.size - row * per)
        val cw = width / colN.toFloat()
        val col = i % per
        out.set(col * cw, row * dp(rowDp), (col + 1) * cw, (row + 1) * dp(rowDp))
    }

    override fun tapped(x: Float, y: Float) {
        for (i in labels.indices) {
            cell(i, r)
            if (r.contains(x, y)) {
                selected = i
                onSelect?.invoke(i)
                return
            }
        }
    }

    override fun onDraw(c: Canvas) {
        val per = perRow(width.toFloat())
        val rows = (labels.size + per - 1) / per
        for (row in 0 until rows) {
            fill.color = pal.surface2
            roundRect(c, 0f, row * dp(rowDp) + dp(2), width.toFloat(), (row + 1) * dp(rowDp) - dp(2), 4f, fill)
        }
        for (i in labels.indices) {
            cell(i, r)
            val on = i == selected
            fill.color = if (on) pal.moss else pal.surface2
            roundRect(c, r.left + dp(1), r.top + dp(3), r.right - dp(1), r.bottom - dp(3), 3f, fill)
            c.label(labels[i], r.centerX(), r.centerY(), 10f, if (on) pal.mossFg else pal.muted)
        }
    }
}

/** Desktop `knob`: vertical drag, 140 dp = full sweep. Only the dial takes the finger. */
class Knob(c: Context, val title: String, large: Boolean, private val iris: Boolean) : IkiView(c) {
    var value = 0.0
        set(v) {
            if (!dragging && v != field) {
                field = v
                invalidate()
            }
        }
    var hint = ""
        set(v) {
            if (v != field) {
                field = v
                invalidate()
            }
        }
    var onChange: ((Double) -> Unit)? = null
    private val d = if (large) 52f else 34f
    private var dragging = false
    private var lastX = 0f
    private var lastY = 0f

    init {
        contentDescription = title
    }

    override fun onMeasure(w: Int, h: Int) {
        setMeasuredDimension(resolveSize(dp(max(d + 22f, 62f)).roundToInt(), w), dp(d + 40f).roundToInt())
    }

    private val cx get() = width / 2f
    private val cy get() = dp(6) + dp(d) / 2

    @SuppressLint("ClickableViewAccessibility")
    override fun onTouchEvent(e: MotionEvent): Boolean {
        when (e.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                // desktop hit box: the dial plus 5 on each side; elsewhere the page scrolls
                if (abs(e.x - cx) > dp(d / 2 + 5) || abs(e.y - cy) > dp(d / 2 + 5)) return false
                claim()
                dragging = true
                lastX = e.x
                lastY = e.y
                invalidate()
            }
            MotionEvent.ACTION_MOVE -> if (dragging) {
                // up or right turns it up
                val dv = ((lastY - e.y) + (e.x - lastX) * 0.5f) / dp(140)
                lastX = e.x
                lastY = e.y
                val v = (value + dv).coerceIn(0.0, 1.0)
                if (v != value) {
                    dragging = false // let the setter through
                    value = v
                    dragging = true
                    onChange?.invoke(v)
                }
            }
            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> {
                dragging = false
                invalidate()
            }
        }
        return true
    }

    override fun onDraw(c: Canvas) {
        val rad = dp(d) / 2
        val cx = cx
        val cy = cy
        stroke.strokeWidth = dp(1)
        stroke.color = pal.subtle
        for (i in 0 until 11) {
            val a = Math.toRadians(-135.0 + i * 27.0)
            val sx = sin(a).toFloat()
            val sy = -cos(a).toFloat()
            c.drawLine(cx + sx * (rad + dp(3)), cy + sy * (rad + dp(3)), cx + sx * (rad + dp(6.5f)), cy + sy * (rad + dp(6.5f)), stroke)
        }
        fill.color = Palette.blend(pal.metal, pal.border, 0.5f)
        c.drawCircle(cx, cy, rad + dp(1.5f), fill)
        fill.color = pal.metal
        c.drawCircle(cx, cy, rad, fill)
        stroke.color = Palette.blend(pal.metal, pal.subtle, 0.35f)
        c.drawCircle(cx, cy, rad - dp(3), stroke)
        fill.color = pal.surface
        c.drawCircle(cx, cy, rad - dp(5), fill)
        val a = Math.toRadians(-135.0 + value * 270.0)
        val sx = sin(a).toFloat()
        val sy = -cos(a).toFloat()
        stroke.color = pal.accent(iris)
        stroke.strokeWidth = dp(if (dragging) 3f else 2.4f)
        c.drawLine(cx + sx * rad * 0.25f, cy + sy * rad * 0.25f, cx + sx * (rad - dp(4)), cy + sy * (rad - dp(4)), stroke)
        val ty = cy + rad + dp(9)
        c.label(title, cx, ty, 11f, pal.fg, v = VAlign.TOP)
        if (hint.isNotEmpty()) c.label(hint, cx, ty + dp(14), 9.5f, pal.muted, v = VAlign.TOP)
    }
}

/** Desktop `rocker`: a lever with captions; clicking toggles. */
class Rocker(
    c: Context, private val top: String, private val bottom: String, private val bottomLitWhenOn: Boolean,
) : Tappable(c) {
    var on = false
        set(v) {
            if (v != field) {
                field = v
                invalidate()
            }
        }

    init {
        contentDescription = listOf(top, bottom).filter { it.isNotEmpty() }.joinToString("/")
    }

    override fun onMeasure(w: Int, h: Int) =
        setMeasuredDimension(dp(56).roundToInt(), dp(if (bottom.isEmpty()) 50 else 64).roundToInt())

    override fun onDraw(c: Canvas) {
        val cx = width / 2f
        val bodyTop = if (bottom.isEmpty()) dp(17) else (height - dp(30)) / 2
        val litTop = if (bottomLitWhenOn) !on else on
        if (top.isNotEmpty()) {
            val col = if (litTop && bottomLitWhenOn) pal.fg else if (litTop) pal.moss else pal.subtle
            c.label(top, cx, 0f, 10.5f, col, v = VAlign.TOP)
        }
        if (bottom.isNotEmpty()) c.label(bottom, cx, height.toFloat(), 10.5f, if (on) pal.moss else pal.subtle, v = VAlign.BOTTOM)
        val l = cx - dp(12)
        val rr = cx + dp(12)
        val b = bodyTop + dp(30)
        fill.color = if (held) Palette.blend(pal.surface2, pal.moss, 0.2f) else pal.surface2
        roundRect(c, l, bodyTop, rr, b, 5f, fill)
        stroke.color = pal.border
        stroke.strokeWidth = dp(1)
        roundRect(c, l, bodyTop, rr, b, 5f, stroke)
        val mid = bodyTop + dp(15)
        val (lt, lb) = if (on) mid to b - dp(3) else bodyTop + dp(3) to mid
        fill.color = if (on) pal.moss else pal.metal
        roundRect(c, l + dp(3), lt, rr - dp(3), lb, 3f, fill)
        stroke.color = pal.subtle
        roundRect(c, l + dp(3), lt, rr - dp(3), lb, 3f, stroke)
    }
}

/** Desktop `led`. */
class Led(c: Context, private val iris: Boolean = false) : IkiView(c) {
    var on = false
        set(v) {
            if (v != field) {
                field = v
                invalidate()
            }
        }

    override fun onMeasure(w: Int, h: Int) = setMeasuredDimension(dp(14).roundToInt(), dp(14).roundToInt())

    override fun onDraw(c: Canvas) {
        val col = pal.accent(iris)
        if (on) {
            fill.color = Palette.alpha(col, 0.3f)
            c.drawCircle(width / 2f, height / 2f, dp(7), fill)
        }
        fill.color = if (on) col else pal.border
        c.drawCircle(width / 2f, height / 2f, dp(4.5f), fill)
    }
}

/** Desktop `pad`: momentary — finger down plays (or ornaments), finger up ends. */
class Pad(
    c: Context, val index: Int, val meri: Boolean, private val kana: String, private val name: String,
    private val key: String,
) : IkiView(c) {
    private val iris = meri || index % 2 == 1
    private var on = false
    private var live = false
    private var flash = 0 // 0 none, 1 atari, 2 koro
    private var down = false
    var onDown: (() -> Unit)? = null
    var onUp: (() -> Unit)? = null

    init {
        contentDescription = name
    }

    fun set(on: Boolean, live: Boolean, flash: Int) {
        if (on != this.on || live != this.live || flash != this.flash) {
            this.on = on
            this.live = live
            this.flash = flash
            invalidate()
        }
    }

    override fun onMeasure(w: Int, h: Int) =
        setMeasuredDimension(resolveSize(dp(56).roundToInt(), w), dp(66).roundToInt())

    @SuppressLint("ClickableViewAccessibility")
    override fun onTouchEvent(e: MotionEvent): Boolean {
        when (e.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                claim()
                down = true
                onDown?.invoke()
                invalidate()
            }
            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> {
                if (down) onUp?.invoke()
                down = false
                invalidate()
            }
        }
        return true
    }

    override fun onDraw(c: Canvas) {
        val w = width.toFloat()
        val h = height.toFloat() - dp(2)
        val acc = pal.accent(iris)
        val fgOn = if (iris) pal.irisFg else pal.mossFg
        var bg = if (on) acc else pal.surface
        if (down) bg = Palette.blend(bg, acc, 0.25f)
        val ink = if (on) fgOn else pal.fg
        fill.color = Palette.blend(pal.border, pal.subtle, 0.3f)
        roundRect(c, dp(2), dp(2), w - dp(2), h + dp(2), 6f, fill)
        fill.color = bg
        roundRect(c, dp(2), 0f, w - dp(2), h, 6f, fill)
        stroke.color = pal.border
        stroke.strokeWidth = dp(1)
        if (live) {
            stroke.color = Palette.blend(acc, pal.fg, 0.35f)
            stroke.strokeWidth = dp(2)
        }
        if (flash != 0) {
            stroke.color = if (flash == 2) pal.danger else pal.fg
            stroke.strokeWidth = dp(3)
        }
        val inset = stroke.strokeWidth / 2
        roundRect(c, dp(2) + inset, inset, w - dp(2) - inset, h - inset, 6f, stroke)
        fill.color = if (on) fgOn else pal.border
        c.drawCircle(w / 2, dp(8), dp(3), fill)
        c.label(kana, w / 2, dp(14), 19f, ink, v = VAlign.TOP)
        c.label(fit(name, 10.5f, w - dp(6)), w / 2, h - dp(17), 10.5f, if (on) ink else pal.muted, v = VAlign.BOTTOM)
        c.label(key, w / 2, h - dp(4), 9.5f, if (on) ink else pal.subtle, v = VAlign.BOTTOM)
    }
}

/** Engine-module bar: "label ▮▮▮▯▯ text" (desktop `output_module`). */
class Bar(c: Context, private val title: String, private val iris: Boolean, private val danger: Boolean = false) : IkiView(c) {
    private var value = 0f
    private var readout = ""

    fun set(v: Float, text: String) {
        if (v != value || text != readout) {
            value = v
            readout = text
            invalidate()
        }
    }

    override fun onMeasure(w: Int, h: Int) = setMeasuredDimension(resolveSize(dp(250).roundToInt(), w), dp(18).roundToInt())

    override fun onDraw(c: Canvas) {
        val h = height / 2f
        c.label(title, 0f, h, 10f, pal.subtle, Paint.Align.LEFT)
        val x0 = dp(50)
        val x1 = width - dp(60)
        fill.color = pal.surface2
        roundRect(c, x0, h - dp(4), x1, h + dp(4), 3f, fill)
        fill.color = if (danger) pal.danger else pal.accent(iris)
        roundRect(c, x0, h - dp(4), x0 + (x1 - x0) * value.coerceIn(0f, 1f), h + dp(4), 3f, fill)
        c.label(readout, x1 + dp(6), h, 10f, pal.muted, Paint.Align.LEFT)
    }
}

/** A text label that follows the palette. */
class Label(c: Context, s: String, sizeSp: Float, role: Role = Role.MUTED, bold: Boolean = false) : TextView(c), Themed {
    enum class Role { FG, MUTED, SUBTLE, MOSS, DANGER }

    private var pal = Palette.DAY
    var role = role
        set(v) {
            if (v != field) {
                field = v
                setTextColor(color())
            }
        }

    init {
        text = s
        textSize = sizeSp
        if (bold) setTypeface(typeface, Typeface.BOLD)
        setTextColor(color())
    }

    private fun color() = when (role) {
        Role.FG -> pal.fg
        Role.MUTED -> pal.muted
        Role.SUBTLE -> pal.subtle
        Role.MOSS -> pal.moss
        Role.DANGER -> pal.danger
    }

    override fun theme(p: Palette) {
        pal = p
        setTextColor(color())
    }

    /** setText only when it changed (these are refreshed every frame). */
    fun show(s: CharSequence) {
        if (!TextUtils.equals(text, s)) text = s
    }
}

/** Desktop `module`: a titled, bordered panel. */
class Module(c: Context, title: String?, lcd: Boolean = false) : LinearLayout(c), Themed {
    private val lcd = lcd
    private val density = c.resources.displayMetrics.density

    init {
        orientation = VERTICAL
        val d = density
        setPadding((10 * d).roundToInt(), (8 * d).roundToInt(), (10 * d).roundToInt(), (10 * d).roundToInt())
        if (title != null) {
            addView(Label(c, title.uppercase(), 10.5f, Label.Role.MUTED, bold = true).apply {
                setPadding(0, 0, 0, (4 * d).roundToInt())
            })
        }
    }

    override fun theme(p: Palette) {
        background = GradientDrawable().apply {
            setColor(if (lcd) p.lcd else p.surface)
            setStroke(density.roundToInt().coerceAtLeast(1), p.border)
            cornerRadius = 8 * density
        }
    }
}

/**
 * Wrapping row layout: children keep their natural (minimum) width, are packed
 * left to right and wrap when the row is full; each row is then stretched to the
 * full width. On a tablet this reproduces the desktop's side-by-side racks, on a
 * phone the same modules stack.
 */
class FlowLayout(c: Context, private val gapDp: Float = 10f) : ViewGroup(c) {
    class Params(val minWidthDp: Float, val grow: Boolean = true) : ViewGroup.LayoutParams(MATCH_PARENT, WRAP_CONTENT)

    private val d = c.resources.displayMetrics.density
    private val rows = ArrayList<IntArray>() // [first, last, top, height]

    fun add(v: View, minWidthDp: Float, grow: Boolean = true): FlowLayout {
        addView(v, Params(minWidthDp, grow))
        return this
    }

    private fun minW(v: View) = ((v.layoutParams as? Params)?.minWidthDp ?: 100f) * d

    override fun onMeasure(wSpec: Int, hSpec: Int) {
        val avail = MeasureSpec.getSize(wSpec) - paddingLeft - paddingRight
        val gap = gapDp * d
        rows.clear()
        var y = paddingTop.toFloat()
        var i = 0
        val vis = (0 until childCount).filter { getChildAt(it).visibility != GONE }
        while (i < vis.size) {
            var j = i
            var used = minW(getChildAt(vis[i]))
            while (j + 1 < vis.size && used + gap + minW(getChildAt(vis[j + 1])) <= avail) {
                j++
                used += gap + minW(getChildAt(vis[j]))
            }
            val sumMin = (i..j).sumOf { minW(getChildAt(vis[it])).toDouble() }.toFloat()
            val extra = max(0f, avail - used)
            var rowH = 0
            for (k in i..j) {
                val v = getChildAt(vis[k])
                val lp = v.layoutParams as? Params
                val base = minW(v)
                val w = if (lp?.grow != false) base + extra * base / sumMin else base
                val cw = min(avail.toFloat(), w).roundToInt()
                v.measure(MeasureSpec.makeMeasureSpec(cw, MeasureSpec.EXACTLY), MeasureSpec.makeMeasureSpec(0, MeasureSpec.UNSPECIFIED))
                rowH = max(rowH, v.measuredHeight)
            }
            rows.add(intArrayOf(i, j, y.roundToInt(), rowH))
            y += rowH + gap
            i = j + 1
        }
        if (rows.isNotEmpty()) y -= gap
        rowsVisible = vis
        setMeasuredDimension(MeasureSpec.getSize(wSpec), resolveSize((y + paddingBottom).roundToInt(), hSpec))
    }

    private var rowsVisible: List<Int> = emptyList()

    override fun onLayout(changed: Boolean, l: Int, t: Int, rr: Int, b: Int) {
        val gap = gapDp * d
        for (row in rows) {
            var x = paddingLeft.toFloat()
            for (k in row[0]..row[1]) {
                val v = getChildAt(rowsVisible[k])
                v.layout(x.roundToInt(), row[2], x.roundToInt() + v.measuredWidth, row[2] + v.measuredHeight)
                x += v.measuredWidth + gap
            }
        }
    }
}

/** LFO wave picker: four icon buttons (desktop `lfo_rack`). */
class WavePicker(c: Context, private val labels: Array<String>) : Tappable(c) {
    var selected = 0
        set(v) {
            if (v != field) {
                field = v
                invalidate()
            }
        }
    var onSelect: ((Int) -> Unit)? = null
    private val path = Path()

    init {
        contentDescription = "wave"
    }

    override fun onMeasure(w: Int, h: Int) = setMeasuredDimension(dp(4 * 48).roundToInt(), dp(40).roundToInt())

    override fun tapped(x: Float, y: Float) {
        val i = (x / dp(48)).toInt().coerceIn(0, labels.size - 1)
        selected = i
        onSelect?.invoke(i)
    }

    override fun onDraw(c: Canvas) {
        for (i in labels.indices) {
            val l = i * dp(48)
            val on = i == selected
            fill.color = if (on) pal.moss else pal.surface2
            roundRect(c, l, 0f, l + dp(44), dp(38), 4f, fill)
            stroke.color = pal.border
            stroke.strokeWidth = dp(1)
            roundRect(c, l, 0f, l + dp(44), dp(38), 4f, stroke)
            val ink = if (on) pal.mossFg else pal.fg
            path.reset()
            for (k in 0..28) {
                val u = k / 28f
                val yv = when (i) {
                    0 -> sin(u * 2 * PI).toFloat()
                    1 -> if (u < 0.5f) 4 * u - 1 else 3 - 4 * u
                    2 -> 2 * u - 1
                    else -> if (u < 0.5f) 1f else -1f
                }
                val px = l + dp(8) + u * dp(28)
                val py = dp(12) - yv * dp(8)
                if (k == 0) path.moveTo(px, py) else path.lineTo(px, py)
            }
            stroke.color = ink
            stroke.strokeWidth = dp(1.4f)
            c.drawPath(path, stroke)
            c.label(labels[i], l + dp(22), dp(35), 9.5f, ink, v = VAlign.BOTTOM)
        }
    }
}

/** Foli step grid: 12 cells in groups of three (desktop `foli_module`). */
class Steps(c: Context) : Tappable(c) {
    private var mask = 0
    private var current = -1
    var onToggle: ((Int) -> Unit)? = null

    init {
        contentDescription = "foli steps"
    }

    fun set(mask: Int, current: Int) {
        if (mask != this.mask || current != this.current) {
            this.mask = mask
            this.current = current
            invalidate()
        }
    }

    override fun onMeasure(w: Int, h: Int) = setMeasuredDimension(resolveSize(dp(12 * 15 + 9).roundToInt(), w), dp(30).roundToInt())

    private fun cellW() = (width - dp(9)) / 12f
    private fun cellX(i: Int) = i * cellW() + (i / 3) * dp(3)

    override fun tapped(x: Float, y: Float) {
        for (i in 0 until 12) if (x >= cellX(i) && x < cellX(i) + cellW()) onToggle?.invoke(i)
    }

    override fun onDraw(c: Canvas) {
        for (i in 0 until 12) {
            val x = cellX(i)
            val on = mask and (1 shl i) != 0
            fill.color = if (on) (if (i % 3 == 0) pal.moss else pal.iris) else pal.surface2
            roundRect(c, x + dp(1), dp(2), x + cellW() - dp(1), height - dp(2), 3f, fill)
            stroke.color = if (i == current) pal.fg else pal.border
            stroke.strokeWidth = dp(if (i == current) 2 else 1)
            roundRect(c, x + dp(1), dp(2), x + cellW() - dp(1), height - dp(2), 3f, stroke)
        }
    }
}

/** Loop mixer channel fader (desktop `mix_channel`). Takes the finger at once. */
class MixFader(c: Context, private val iris: Boolean) : IkiView(c) {
    var value = 0.9
        set(v) {
            if (!dragging && v != field) {
                field = v
                invalidate()
            }
        }
    var live = false
        set(v) {
            if (v != field) {
                field = v
                invalidate()
            }
        }
    var onChange: ((Double) -> Unit)? = null
    private var dragging = false
    private val pad get() = dp(10)

    override fun onMeasure(w: Int, h: Int) = setMeasuredDimension(dp(34).roundToInt(), resolveSize(dp(150).roundToInt(), h))

    @SuppressLint("ClickableViewAccessibility")
    override fun onTouchEvent(e: MotionEvent): Boolean {
        when (e.actionMasked) {
            MotionEvent.ACTION_DOWN, MotionEvent.ACTION_MOVE -> {
                if (e.actionMasked == MotionEvent.ACTION_DOWN) claim()
                dragging = true
                val v = faderValue(e.y, pad, height - pad)
                if (v != value) {
                    dragging = false
                    value = v
                    dragging = true
                    onChange?.invoke(v)
                }
                invalidate()
            }
            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> dragging = false
        }
        return true
    }

    override fun onDraw(c: Canvas) = paintFader(c, width / 2f, pad, height - pad, value, pal.accent(iris), live)
}

/** Distance helper for the stage and loop strips. */
internal fun dist(ax: Float, ay: Float, bx: Float, by: Float) = hypot(ax - bx, ay - by)
