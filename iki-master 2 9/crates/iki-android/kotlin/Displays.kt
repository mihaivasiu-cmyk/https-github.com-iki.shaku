package org.iki.shakuhachi

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.LinearGradient
import android.graphics.Paint
import android.graphics.Path
import android.graphics.Rect
import android.graphics.Shader
import android.os.SystemClock
import android.view.MotionEvent
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.max
import kotlin.math.min
import kotlin.math.roundToInt
import kotlin.math.sin

// The painted panels of the desktop `view.rs`, `scopes.rs` and `loop_view.rs`.

/** The CRT: note kana/name, pitch readout, level meters, stereo spectrogram. */
class Crt(c: Context) : IkiView(c) {
    var kana = "ロ"
    var name = "Ro"
    var sub = ""
    var blowing = false
    var breath = 0f
    var peakL = 0f
    var peakR = 0f
    var cutText = ""

    // spectrogram history: ROWS values per channel per column, like the desktop texture
    private val bmp = Bitmap.createBitmap(COLS, ROWS * 2 + GAP, Bitmap.Config.ARGB_8888)
    private val column = IntArray(ROWS * 2 + GAP)
    private var col = 0
    private val bitmapPaint = Paint(Paint.FILTER_BITMAP_FLAG)
    private val src = Rect()
    private val dst = Rect()

    init {
        contentDescription = "crt"
        val silent = IntArray(ROWS * 2 + GAP)
        for (y in 0 until ROWS) {
            silent[y] = 0xff241a10.toInt()
            silent[ROWS + GAP + y] = 0xff2e163a.toInt()
        }
        for (x in 0 until COLS) bmp.setPixels(silent, 0, 1, x, 0, 1, silent.size)
    }

    override fun theme(p: Palette) {
        super.theme(p)
        for (x in 0 until COLS) for (y in ROWS until ROWS + GAP) bmp.setPixel(x, y, p.lcd)
    }

    /** One column: ROWS values for L then ROWS for R, 0…1, top = high. */
    fun push(v: FloatArray) {
        for (y in 0 until ROWS) {
            val l = v[y]
            val r = v[ROWS + y]
            column[y] = (0xff shl 24) or ((36 + l * 214).toInt() shl 16) or ((26 + l * 168).toInt() shl 8) or (16 + l * 36).toInt()
            column[ROWS + GAP + y] =
                (0xff shl 24) or ((46 + r * 130).toInt() shl 16) or ((22 + r * 48).toInt() shl 8) or (58 + r * 168).toInt()
        }
        for (y in ROWS until ROWS + GAP) column[y] = pal.lcd
        bmp.setPixels(column, 0, 1, col, 0, 1, column.size)
        col = (col + 1) % COLS
        invalidate()
    }

    override fun onMeasure(w: Int, h: Int) = setMeasuredDimension(resolveSize(dp(470).roundToInt(), w), dp(300).roundToInt())

    override fun onDraw(c: Canvas) {
        val w = width.toFloat()
        val h = height.toFloat()
        fill.color = pal.lcd
        roundRect(c, 0f, 0f, w, h, 8f, fill)
        stroke.color = pal.border
        stroke.strokeWidth = dp(1)
        roundRect(c, dp(0.5f), dp(0.5f), w - dp(0.5f), h - dp(0.5f), 8f, stroke)
        // head: kana, name, sub
        text.textSize = dp(46)
        val kw = text.measureText(kana)
        c.label(kana, dp(16), dp(8), 46f, if (blowing) pal.moss else pal.iris, Paint.Align.LEFT, VAlign.TOP)
        val x = dp(16) + kw + dp(14)
        val narrow = w < dp(440)
        val meterW = if (narrow) dp(60) else dp(84)
        val meterL = w - dp(34) - meterW
        val textW = meterL - dp(42) - x
        c.label(fit(name, 19f, textW), x, dp(18), 19f, pal.fg, Paint.Align.LEFT, VAlign.TOP)
        c.label(fit(sub, 11.5f, if (narrow) w - x - dp(12) else textW), x, dp(if (narrow) 62 else 44), 11.5f, pal.muted, Paint.Align.LEFT, VAlign.TOP)
        meter(c, meterL, meterW, dp(16), "breath", breath, pal.moss)
        meter(c, meterL, meterW, dp(28), "out L", peakL, pal.moss)
        meter(c, meterL, meterW, dp(40), "out R", peakR, pal.iris)
        c.label(cutText, w - dp(12), dp(if (narrow) 50 else 56), 9f, pal.subtle, Paint.Align.RIGHT, VAlign.TOP)
        // spectrogram, oldest column on the left
        val top = dp(if (narrow) 82 else 76)
        val sl = dp(12).roundToInt()
        val sr = (w - dp(12)).roundToInt()
        val st = top.roundToInt()
        val sb = (h - dp(26)).roundToInt()
        val split = sl + ((sr - sl) * (COLS - col) / COLS.toFloat()).roundToInt()
        src.set(col, 0, COLS, bmp.height)
        dst.set(sl, st, split, sb)
        c.drawBitmap(bmp, src, dst, bitmapPaint)
        if (col > 0) {
            src.set(0, 0, col, bmp.height)
            dst.set(split, st, sr, sb)
            c.drawBitmap(bmp, src, dst, bitmapPaint)
        }
        c.label("L", sl + dp(8), st + dp(4), 11f, 0xd8f0c43a.toInt(), Paint.Align.LEFT, VAlign.TOP)
        c.label("R", sl + dp(8), (st + sb) / 2f + dp(4), 11f, 0xe6c4a4e8.toInt(), Paint.Align.LEFT, VAlign.TOP)
        c.label(
            fit("A S D F G H · Z X C V B N · SPACE BLOW · O OCTAVE · M MURAIKI", 9.5f, w - dp(24)),
            dp(12), h - dp(8), 9.5f, pal.subtle, Paint.Align.LEFT, VAlign.BOTTOM,
        )
    }

    private fun meter(c: Canvas, l: Float, mw: Float, y: Float, label: String, v: Float, col: Int) {
        c.label(label, l - dp(6), y + dp(3), 9f, pal.subtle, Paint.Align.RIGHT)
        fill.color = pal.surface2
        roundRect(c, l, y, l + mw, y + dp(6), 2f, fill)
        fill.color = col
        roundRect(c, l, y, l + mw * v.coerceIn(0f, 1f), y + dp(6), 2f, fill)
    }

    companion object {
        const val ROWS = 72
        private const val COLS = 360
        private const val GAP = 2
    }
}

/**
 * The stage (desktop `stage`): lean fader (meri lean, left), the flute body,
 * and the kari note fader (right). The faders take the finger at once; a drag
 * that starts on the flute scrolls the page.
 */
class Stage(c: Context) : IkiView(c) {
    var lean = 0.0
    var kariNote = 0
    var kariActive = true
    var blowing = false
    val holes = BooleanArray(5) { true }
    var onLean: ((Double) -> Unit)? = null
    var onNoteStart: ((Int) -> Unit)? = null
    var onNoteMove: ((Int) -> Unit)? = null
    var onNoteEnd: (() -> Unit)? = null

    private var drag = 0 // 0 none, 1 lean, 2 note
    private var noteDrag = -1
    private var bamboo: LinearGradient? = null
    private var bambooKey = ""

    init {
        contentDescription = "stage"
    }

    override fun onMeasure(w: Int, h: Int) = setMeasuredDimension(resolveSize(dp(250).roundToInt(), w), dp(300).roundToInt())

    // desktop geometry on a 250-wide stage, squeezed horizontally when narrower
    private fun sx(v: Float) = v * width / dp(250)
    private val top get() = dp(30) // room for the breath wisps above the flute
    private val bot get() = height - dp(8)
    private val pad get() = min((bot - top) * 0.08f, dp(28))
    private val leanX get() = max(sx(dp(38)), dp(36)) // leave room for the メリ / カリ captions
    private val bodyX get() = sx(dp(118))
    private val noteX get() = sx(dp(186))

    private fun noteAt(y: Float) = (faderValue(y, top + pad, bot - pad) * 5).roundToInt().coerceIn(0, 5)

    @SuppressLint("ClickableViewAccessibility")
    override fun onTouchEvent(e: MotionEvent): Boolean {
        when (e.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                drag = when {
                    e.x < (leanX + bodyX) / 2 - sx(dp(4)) -> 1
                    e.x > (bodyX + noteX) / 2 + sx(dp(4)) -> 2
                    else -> return false // the flute: let the page scroll
                }
                claim()
                if (drag == 1) {
                    lean = faderValue(e.y, top + pad, bot - pad)
                    onLean?.invoke(lean)
                } else {
                    noteDrag = noteAt(e.y)
                    kariNote = noteDrag
                    onNoteStart?.invoke(noteDrag)
                }
                invalidate()
            }
            MotionEvent.ACTION_MOVE -> {
                if (drag == 1) {
                    lean = faderValue(e.y, top + pad, bot - pad)
                    onLean?.invoke(lean)
                } else if (drag == 2) {
                    val n = noteAt(e.y)
                    if (n != noteDrag) {
                        noteDrag = n
                        kariNote = n
                        onNoteMove?.invoke(n)
                    }
                }
                invalidate()
            }
            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> {
                if (drag == 2) onNoteEnd?.invoke()
                drag = 0
                noteDrag = -1
            }
        }
        return true
    }

    override fun onDraw(c: Canvas) {
        val t0 = top
        val b0 = bot
        val h = b0 - t0
        val p = pad
        // lean fader
        paintFader(c, leanX, t0 + p, b0 - p, lean, pal.iris, lean > 0)
        c.label("メリ", leanX - dp(13), b0 - p - (h - 2 * p) * 0.92f, 10f, pal.subtle, Paint.Align.RIGHT)
        c.label("カリ", leanX - dp(13), b0 - p - (h - 2 * p) * 0.08f, 10f, pal.subtle, Paint.Align.RIGHT)

        // flute body
        val bw = min(dp(40), sx(dp(40)))
        val l = bodyX - bw / 2
        val rr = bodyX + bw / 2
        val night = pal.night
        val dark = if (night) 0xff4a3610.toInt() else 0xff8a6a18.toInt()
        val lit = if (night) 0xffb08c3a.toInt() else 0xfff0c85c.toInt()
        val edge = if (night) 0xff302208.toInt() else 0xff5a4010.toInt()
        if (blowing) {
            fill.color = Palette.alpha(pal.moss, 0.22f)
            roundRect(c, l - dp(6), t0 - dp(6), rr + dp(6), b0 + dp(6), 24f, fill)
        }
        val key = "$l $rr $night"
        if (bamboo == null || key != bambooKey) {
            bamboo = LinearGradient(
                l, 0f, rr, 0f, intArrayOf(dark, lit, Palette.blend(lit, dark, 0.2f), edge),
                floatArrayOf(0f, 0.42f, 0.58f, 1f), Shader.TileMode.CLAMP,
            )
            bambooKey = key
        }
        fill.color = 0xff000000.toInt()
        fill.shader = bamboo
        roundRect(c, l, t0, rr, b0, 20f, fill)
        fill.shader = null
        stroke.color = 0x5a14100c
        stroke.strokeWidth = dp(1)
        roundRect(c, l, t0, rr, b0, 20f, stroke)
        fill.color = 0x73282018
        for (y in floatArrayOf(0.18f, 0.40f, 0.62f, 0.84f)) {
            val cy = t0 + h * y
            roundRect(c, l - dp(3), cy - dp(3.5f), rr + dp(3), cy + dp(3.5f), 4f, fill)
        }
        hole(c, t0 + h * 0.08f, holes[4], dp(13))
        for ((i, y) in listOf(3 to 0.31f, 2 to 0.45f, 1 to 0.59f, 0 to 0.73f)) hole(c, t0 + h * y, holes[i], dp(15))
        if (blowing) {
            val tt = SystemClock.uptimeMillis() / 1000f
            for ((k, dx) in listOf(0 to 0f, 1 to -8f, 2 to 7f)) {
                val ph = ((tt / 1.6f) + k * 0.125f) % 1f
                val y = t0 - dp(4) - ph * dp(22)
                fill.color = Palette.alpha(pal.iris, (1 - ph) * 0.55f)
                roundRect(c, bodyX + dp(dx) - dp(2.5f), y - dp(9), bodyX + dp(dx) + dp(2.5f), y + dp(9), 3f, fill)
            }
            postInvalidateOnAnimation()
        }

        // note fader with the kana ladder
        paintFader(c, noteX, t0 + p, b0 - p, kariNote / 5.0, pal.moss, kariActive)
        val kana = arrayOf("ロ", "ツ", "レ", "チ", "リ", "ヒ")
        for (i in 0..5) {
            val y = b0 - p - (b0 - t0 - 2 * p) * i / 5f
            c.label(kana[i], noteX + dp(24), y, 12f, if (kariActive && i == kariNote) pal.fg else pal.subtle, Paint.Align.LEFT)
        }
    }

    private fun hole(c: Canvas, y: Float, covered: Boolean, rad: Float) {
        val rr = min(rad, sx(rad))
        fill.color = 0x3c000000
        c.drawCircle(bodyX, y, rr + dp(1), fill)
        fill.color = if (covered) pal.hole else pal.moss
        c.drawCircle(bodyX, y, rr, fill)
        if (!covered) {
            stroke.strokeWidth = dp(1)
            stroke.color = Palette.blend(pal.moss, 0xffffffff.toInt(), 0.4f)
            c.drawCircle(bodyX, y, rr - dp(3), stroke)
        }
    }
}

/** The Blow / Gate disc: a tap toggles the latched drone (desktop: click). */
class BlowButton(c: Context) : Tappable(c) {
    var on = false
        set(v) {
            if (v != field) {
                field = v
                invalidate()
            }
        }
    private val wave = Path()

    init {
        contentDescription = "Blow"
    }

    override fun onMeasure(w: Int, h: Int) = setMeasuredDimension(dp(92).roundToInt(), dp(92).roundToInt())

    override fun hit(x: Float, y: Float) = dist(x, y, width / 2f, height / 2f) <= dp(46)

    override fun onDraw(c: Canvas) {
        val cx = width / 2f
        val cy = height / 2f
        if (on) {
            fill.color = Palette.alpha(pal.moss, 0.25f)
            c.drawCircle(cx, cy, dp(46), fill)
        }
        fill.color = if (on) pal.moss else if (held) Palette.blend(pal.surface, pal.moss, 0.2f) else pal.surface
        c.drawCircle(cx, cy, dp(42), fill)
        stroke.color = if (on) Palette.blend(pal.moss, pal.fg, 0.3f) else pal.border
        stroke.strokeWidth = dp(1)
        c.drawCircle(cx, cy, dp(42), stroke)
        val ink = if (on) pal.mossFg else pal.fg
        stroke.color = ink
        stroke.strokeWidth = dp(1.6f)
        for ((k, w) in listOf(-9f to 26f, 0f to 32f, 9f to 22f)) {
            wave.reset()
            for (i in 0..16) {
                val u = i / 16f
                val x = cx - dp(w) / 2 + u * dp(w)
                val y = cy - dp(14) + dp(k) * 0.7f + sin(u * 2 * PI).toFloat() * dp(2)
                if (i == 0) wave.moveTo(x, y) else wave.lineTo(x, y)
            }
            c.drawPath(wave, stroke)
        }
        c.label(if (on) "Gate" else "Blow", cx, cy + dp(16), 13f, ink)
    }
}

/** 風 Muraiki: fires on finger down (desktop: on press). */
class MuraikiButton(c: Context) : IkiView(c) {
    var lit = false
        set(v) {
            if (v != field) {
                field = v
                invalidate()
            }
        }
    var onPress: (() -> Unit)? = null

    init {
        contentDescription = "Muraiki"
    }

    override fun onMeasure(w: Int, h: Int) = setMeasuredDimension(dp(80).roundToInt(), dp(80).roundToInt())

    @SuppressLint("ClickableViewAccessibility")
    override fun onTouchEvent(e: MotionEvent): Boolean {
        if (e.actionMasked == MotionEvent.ACTION_DOWN) {
            claim()
            onPress?.invoke()
        }
        return true
    }

    override fun onDraw(c: Canvas) {
        val m = dp(5)
        if (lit) {
            fill.color = Palette.alpha(pal.iris, 0.3f)
            roundRect(c, 0f, 0f, width.toFloat(), height.toFloat(), 8f, fill)
        }
        fill.color = if (lit) pal.iris else pal.surface
        roundRect(c, m, m, width - m, height - m, 4f, fill)
        stroke.color = pal.border
        stroke.strokeWidth = dp(1)
        roundRect(c, m, m, width - m, height - m, 4f, stroke)
        val ink = if (lit) pal.irisFg else pal.fg
        c.label("風", width / 2f, height / 2f - dp(8), 24f, ink)
        c.label("Muraiki", width / 2f, height - m - dp(8), 10f, ink, v = VAlign.BOTTOM)
    }
}

/** Envelope or filter-response scope (desktop `env_scope` / `filt_scope`). */
class Scope(c: Context, private val filter: Boolean, private val iris: Boolean) : IkiView(c) {
    /** Filled by [Native.curve]; the last element is the guide line. */
    val points = FloatArray(161)
    private val line = Path()
    private val area = Path()

    override fun onMeasure(w: Int, h: Int) = setMeasuredDimension(resolveSize(dp(268).roundToInt(), w), dp(64).roundToInt())

    override fun onDraw(c: Canvas) {
        val w = width.toFloat()
        val h = height.toFloat()
        fill.color = pal.lcd
        roundRect(c, 0f, 0f, w, h, 4f, fill)
        val n = points.size - 1
        fun y(v: Float) = if (filter) h - v * (h - dp(4)) - dp(2) else h - dp(3) - v * (h - dp(6))
        line.reset()
        area.reset()
        area.moveTo(0f, h)
        for (i in 0 until n) {
            val x = w * i / (n - 1f)
            val yy = y(points[i])
            if (i == 0) line.moveTo(x, yy) else line.lineTo(x, yy)
            area.lineTo(x, yy)
        }
        area.lineTo(w, h)
        area.close()
        val ink = if (filter) pal.iris else pal.accent(iris)
        fill.color = Palette.alpha(if (filter) pal.moss else ink, if (filter) 0.18f else 0.16f)
        c.drawPath(area, fill)
        stroke.color = if (filter) Palette.blend(pal.lcd, pal.subtle, 0.3f) else Palette.alpha(pal.subtle, 0.35f)
        stroke.strokeWidth = dp(1)
        val gy = y(points[n])
        c.drawLine(0f, gy, w, gy, stroke)
        stroke.color = ink
        stroke.strokeWidth = dp(if (filter) 1.7f else 1.6f)
        c.drawPath(line, stroke)
    }
}

/**
 * One looper track (desktop `loop_strip` + `loop_wave`): number, zooming trim
 * waveform, punch ring, ▶ ■ ↺. A horizontal drag on the waveform moves the
 * nearer trim handle (the view follows the trims and scrolls at the edges);
 * a vertical one scrolls the page.
 */
class LoopStrip(c: Context, val track: Int) : IkiView(c) {
    interface Listener {
        fun select()
        fun punch()
        fun play()
        fun stop()
        fun undo()
        fun trim(a: Double, b: Double)
    }

    var listener: Listener? = null
    val wave = FloatArray(384)
    private var waveLen = 0
    private var mode = 0 // 0 idle, 1 rec, 2 play, 3 dub, 4 stop
    private var progress = 0.0
    private var length = 0f
    private var snapIn = 0.0
    private var snapOut = 1.0
    private var pad = false
    private var pending = false
    private var selected = false
    private var canUndo = false

    // view window / drag state (desktop LoopWaveUi)
    private var viewA = 0.0
    private var viewB = 1.0
    private var trimIn = 0.0
    private var trimOut = 1.0
    private var drag = -1 // -1 none, 0 in handle, 1 out handle
    private var lastX = 0f
    private var downX = 0f
    private var downY = 0f
    private var deciding = false
    private var press = 0 // which button is held: 1 punch, 2 play, 3 stop, 4 undo
    private var firedOnDown = false
    private val top = Path()
    private val bot = Path()

    init {
        contentDescription = "loop track ${track + 1}"
    }

    fun set(
        waveLen: Int, mode: Int, progress: Float, length: Float, trimIn: Float, trimOut: Float,
        pad: Boolean, pending: Boolean, selected: Boolean, canUndo: Boolean,
    ) {
        this.waveLen = waveLen
        this.mode = mode
        this.progress = progress.toDouble()
        this.length = length
        snapIn = trimIn.toDouble()
        snapOut = trimOut.toDouble()
        this.pad = pad
        this.pending = pending
        this.selected = selected
        this.canUndo = canUndo
        step()
        invalidate()
    }

    private val empty get() = mode == 0 && waveLen == 0
    private val playing get() = mode == 2 || mode == 3
    private val canPlay get() = !empty

    override fun onMeasure(w: Int, h: Int) = setMeasuredDimension(resolveSize(dp(500).roundToInt(), w), dp(62).roundToInt())

    // geometry
    private val waveL get() = dp(20)
    private val waveR get() = width - dp(96)
    private val ringX get() = width - dp(64)
    private val btnX get() = width - dp(30)

    private fun button(y: Float): Int = when {
        y < dp(3 + 17 + 1) -> 2
        y < dp(3 + 2 * 19 + 1) -> 3
        else -> 4
    }

    /** Per frame: follow the snapshot, ease the view window, auto-scroll while dragging at an edge. */
    private fun step() {
        if (drag >= 0) {
            val w = (waveR - waveL).coerceAtLeast(1f)
            val u = ((lastX - waveL) / w).toDouble().coerceIn(0.0, 1.0)
            val span = (viewB - viewA).coerceAtLeast(1e-4)
            val edge = 0.14
            if (u < edge) {
                val s = ((edge - u) / edge) * span * 0.08
                viewA = (viewA - s).coerceAtLeast(0.0)
                viewB = viewA + span
            } else if (u > 1 - edge) {
                val s = ((u - (1 - edge)) / edge) * span * 0.08
                viewB = (viewB + s).coerceAtMost(1.0)
                viewA = viewB - span
            }
            val p = viewA + u * (viewB - viewA)
            if (drag == 1) trimOut = p.coerceAtLeast(trimIn + 0.012).coerceAtMost(1.0)
            else trimIn = p.coerceAtMost(trimOut - 0.012).coerceAtLeast(0.0)
            listener?.trim(trimIn, trimOut)
        } else {
            trimIn = snapIn
            trimOut = snapOut
            val span = (trimOut - trimIn).coerceAtLeast(0.05)
            val padW = (span * 0.28).coerceAtLeast(0.05)
            var a = trimIn - padW
            var b = trimOut + padW
            if (a < 0) {
                b = (b - a).coerceAtMost(1.0)
                a = 0.0
            }
            if (b > 1) {
                a = (a - (b - 1)).coerceAtLeast(0.0)
                b = 1.0
            }
            viewA += (a - viewA) * 0.28
            viewB += (b - viewB) * 0.28
        }
    }

    @SuppressLint("ClickableViewAccessibility")
    override fun onTouchEvent(e: MotionEvent): Boolean {
        val x = e.x
        when (e.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                listener?.select()
                downX = x
                downY = e.y
                lastX = x
                press = when {
                    x >= btnX - dp(13) -> button(e.y)
                    abs(x - ringX) <= dp(27) -> 1
                    else -> 0
                }
                deciding = press == 0 && !empty && x in waveL..waveR
                // record / play / stop act the instant they are touched
                firedOnDown = false
                when (press) {
                    1 -> { listener?.punch(); firedOnDown = true }
                    2 -> if (canPlay) { listener?.play(); firedOnDown = true }
                    3 -> { listener?.stop(); firedOnDown = true }
                }
                invalidate()
            }
            MotionEvent.ACTION_MOVE -> {
                lastX = x
                if (deciding && abs(x - downX) > slop && abs(x - downX) > abs(e.y - downY)) {
                    deciding = false
                    claim()
                    val w = (waveR - waveL).coerceAtLeast(1f)
                    val p = viewA + ((downX - waveL) / w).coerceIn(0f, 1f) * (viewB - viewA)
                    drag = if (abs(p - trimIn) > abs(p - trimOut)) 1 else 0
                }
                if (press != 0 && (abs(x - downX) > slop || abs(e.y - downY) > slop)) {
                    press = 0
                    invalidate()
                }
            }
            MotionEvent.ACTION_UP -> {
                if (!firedOnDown && press == 4 && (canUndo || mode != 0)) listener?.undo()
                firedOnDown = false
                press = 0
                drag = -1
                deciding = false
                invalidate()
            }
            MotionEvent.ACTION_CANCEL -> {
                press = 0
                drag = -1
                deciding = false
                invalidate()
            }
        }
        return true
    }

    private fun sampleWave(t: Double): Float {
        val n = waveLen
        if (n < 2) return 0f
        val x = t.coerceIn(0.0, 1.0) * (n - 1)
        val i = min(x.toInt(), n - 2)
        val f = (x - i).toFloat()
        return wave[i] * (1 - f) + wave[i + 1] * f
    }

    override fun onDraw(c: Canvas) {
        val w = width.toFloat()
        val h = height.toFloat()
        fill.color = if (selected) Palette.blend(pal.surface, pal.moss, 0.08f) else pal.surface2
        roundRect(c, 0f, 0f, w, h, 6f, fill)
        stroke.strokeWidth = dp(1)
        if (selected) {
            stroke.color = Palette.alpha(pal.moss, 0.6f)
            roundRect(c, dp(0.5f), dp(0.5f), w - dp(0.5f), h - dp(0.5f), 6f, stroke)
        }
        if (pending) {
            val blink = (SystemClock.uptimeMillis() / 125) % 2 == 0L
            stroke.color = if (blink) pal.iris else Palette.alpha(pal.iris, 0.3f)
            stroke.strokeWidth = dp(1.5f)
            roundRect(c, dp(0.75f), dp(0.75f), w - dp(0.75f), h - dp(0.75f), 6f, stroke)
        }
        if (pad) {
            stroke.color = Palette.alpha(pal.iris, 0.45f)
            stroke.strokeWidth = dp(1)
            roundRect(c, dp(1.5f), dp(1.5f), w - dp(1.5f), h - dp(1.5f), 6f, stroke)
        }
        c.label(if (pad) "P" else "${track + 1}", dp(10), h / 2, 12f, pal.subtle)

        // waveform
        val wl = waveL
        val wr = waveR
        val wt = dp(4)
        val wb = h - dp(4)
        val mid = h / 2
        val h2 = (wb - wt) / 2
        fill.color = pal.lcd
        roundRect(c, wl, wt, wr, wb, 4f, fill)
        c.save()
        c.clipRect(wl, wt, wr, wb)
        val span = (viewB - viewA).coerceAtLeast(1e-4)
        fun toX(q: Double) = wl + ((q - viewA) / span * (wr - wl)).toFloat()
        val ink = if (track % 2 == 1) pal.iris else pal.moss
        if (waveLen < 2) {
            stroke.color = Palette.alpha(pal.subtle, 0.35f)
            stroke.strokeWidth = dp(1)
            c.drawLine(wl, mid, wr, mid, stroke)
            if (empty) c.label("empty", (wl + wr) / 2, mid, 10f, pal.subtle)
        } else {
            val steps = max(48, ((wr - wl) / dp(2)).toInt())
            top.reset()
            bot.reset()
            for (i in 0..steps) {
                val q = viewA + i.toDouble() / steps * span
                val x = wl + i.toFloat() / steps * (wr - wl)
                val amp = min(1f, sampleWave(q) * 1.6f) * (h2 - dp(3))
                if (i == 0) top.moveTo(x, mid - amp) else top.lineTo(x, mid - amp)
                if (i == 0) bot.moveTo(x, mid + amp) else bot.lineTo(x, mid + amp)
            }
            val area = Path(top)
            for (i in steps downTo 0) {
                val q = viewA + i.toDouble() / steps * span
                val x = wl + i.toFloat() / steps * (wr - wl)
                area.lineTo(x, mid + min(1f, sampleWave(q) * 1.6f) * (h2 - dp(3)))
            }
            area.close()
            fill.color = Palette.alpha(ink, 0.22f)
            c.drawPath(area, fill)
            stroke.color = Palette.alpha(ink, 0.9f)
            stroke.strokeWidth = dp(1.1f)
            c.drawPath(top, stroke)
            c.drawPath(bot, stroke)
            val ix = toX(trimIn)
            val ox = toX(trimOut)
            fill.color = 0x282a2214
            if (ix > wl) c.drawRect(wl, wt, ix, wb, fill)
            if (ox < wr) c.drawRect(ox, wt, wr, wb, fill)
            for ((x, active) in listOf(ix to (drag == 0), ox to (drag == 1))) {
                fill.color = if (active) pal.iris else Palette.alpha(pal.fg, 0.7f)
                roundRect(c, x - dp(3), wt + dp(2), x + dp(3), wb - dp(2), 2f, fill)
            }
        }
        if (mode in 1..3) {
            val x = toX(progress)
            stroke.color = Palette.alpha(pal.fg, 0.85f)
            stroke.strokeWidth = dp(1)
            c.drawLine(x, wt, x, wb, stroke)
        }
        c.restore()

        // punch ring
        val cx = ringX
        val col = when (mode) {
            1 -> pal.danger
            3 -> pal.iris
            else -> pal.moss
        }
        fill.color = if (press == 1) Palette.blend(pal.surface, col, 0.12f) else pal.surface
        c.drawCircle(cx, mid, dp(25), fill)
        stroke.strokeWidth = dp(3)
        stroke.color = pal.border
        c.drawCircle(cx, mid, dp(23), stroke)
        val frac = when (mode) {
            0 -> 0f
            1 -> 0.92f
            else -> progress.toFloat()
        }
        if (frac > 0.001f) {
            stroke.color = col
            r.set(cx - dp(23), mid - dp(23), cx + dp(23), mid + dp(23))
            c.drawArc(r, -90f, 360f * frac.coerceAtMost(1f), false, stroke)
        }
        if (mode == 1) {
            fill.color = pal.danger
            c.drawCircle(cx, mid - dp(10), dp(3), fill)
        }
        val cap = arrayOf("Rec", "Play", "Dub", "Play", "Play")[mode.coerceIn(0, 4)]
        c.label(cap, cx, mid - dp(3), 11f, pal.fg)
        c.label(if (mode == 0) "—" else String.format(java.util.Locale.US, "%.1fs", length), cx, mid + dp(9), 9f, pal.muted)

        // ▶ ■ ↺
        smallButton(c, 0, "▶", playing, canPlay, press == 2)
        smallButton(c, 1, "■", false, true, press == 3)
        smallButton(c, 2, "↺", false, canUndo || mode != 0, press == 4)
    }

    private fun smallButton(c: Canvas, i: Int, s: String, on: Boolean, usable: Boolean, down: Boolean) {
        val l = btnX - dp(13)
        val t = dp(3) + i * dp(19)
        val rr = btnX + dp(13)
        val b = t + dp(17)
        fill.color = when {
            !usable -> pal.surface2
            on -> pal.moss
            down -> Palette.blend(pal.surface2, pal.moss, 0.35f)
            else -> pal.surface2
        }
        roundRect(c, l, t, rr, b, 4f, fill)
        stroke.color = pal.border
        stroke.strokeWidth = dp(1)
        roundRect(c, l, t, rr, b, 4f, stroke)
        val ink = if (!usable) Palette.blend(pal.subtle, pal.surface2, 0.5f) else if (on) pal.mossFg else pal.fg
        if (s == "↺") {
            val cx = btnX
            val cy = (t + b) / 2
            stroke.color = ink
            stroke.strokeWidth = dp(1.3f)
            r.set(cx - dp(5), cy - dp(5), cx + dp(5), cy + dp(5))
            c.drawArc(r, -60f, 280f, false, stroke)
        } else {
            c.label(s, btnX, (t + b) / 2, 10.5f, ink)
        }
    }
}

/** Header status line with its underline (desktop header); red when audio has a problem. */
class StatusLine(c: Context) : Tappable(c) {
    var status = ""
        set(v) {
            if (v != field) {
                field = v
                invalidate()
            }
        }
    var problem = false
        set(v) {
            if (v != field) {
                field = v
                invalidate()
            }
        }

    override fun onMeasure(w: Int, h: Int) = setMeasuredDimension(resolveSize(dp(300).roundToInt(), w), dp(30).roundToInt())

    override fun onDraw(c: Canvas) {
        val w = width.toFloat()
        stroke.color = pal.border
        stroke.strokeWidth = dp(1)
        c.drawLine(0f, height - dp(4), w, height - dp(4), stroke)
        c.label(fit(status, 10.5f, w), w, height - dp(10), 10.5f, if (problem) pal.danger else pal.muted, Paint.Align.RIGHT, VAlign.BOTTOM)
    }
}
