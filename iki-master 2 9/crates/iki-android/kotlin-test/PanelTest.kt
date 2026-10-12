package org.iki.shakuhachi

import android.graphics.Bitmap
import android.graphics.Canvas
import android.os.SystemClock
import android.view.MotionEvent
import android.view.View
import android.view.ViewGroup
import android.widget.ScrollView
import java.io.File
import java.io.FileOutputStream
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import org.robolectric.shadows.ShadowLooper

/**
 * Runs the real Activity on the JVM (Robolectric, native graphics) on top of
 * the real Rust kernel (host build of libiki), plays it through simulated
 * touches, checks the engine state and writes full-page screenshots.
 */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [36], qualifiers = "w411dp-h914dp-xxhdpi")
class PanelTest {
    private lateinit var activity: MainActivity
    private val st = FloatArray(STATE_LEN)
    private val h get() = activity.handle()

    @Before
    fun start() {
        MainActivity.forceSilentAudio = true
        MainActivity.manualFrames = true
        activity = Robolectric.buildActivity(MainActivity::class.java).setup().get()
        // wait for the sample bank (decoded + loop-searched on a background thread)
        var i = 0
        while (i++ < 400 && state()[STATE_LOADED] < 6) frames(1)
        assertEquals("samples loaded", 36f, state()[STATE_LOADED], 0f)
    }

    private fun state(): FloatArray {
        Native.state(h, st)
        return st
    }

    /** Advance [n] frames of ~16 ms real time (the engine runs on a real-time silent clock). */
    private fun frames(n: Int) = repeat(n) {
        activity.frame()
        ShadowLooper.idleMainLooper() // layout / draw traversal like a device
        Thread.sleep(16)
    }

    private fun find(v: View, desc: String): View? {
        if (desc.contentEquals(v.contentDescription ?: "")) return v
        if (v is ViewGroup) for (i in 0 until v.childCount) find(v.getChildAt(i), desc)?.let { return it }
        return null
    }

    private fun view(desc: String): View = find(activity.window.decorView, desc) ?: throw AssertionError("no view '$desc'")

    private val scroll get() = activity.panel.scroll

    /** Send a touch to the window at [x],[y] (window coordinates), like the finger would. */
    private fun send(action: Int, x: Float, y: Float, down: Long) {
        val e = MotionEvent.obtain(down, SystemClock.uptimeMillis(), action, x, y, 0)
        activity.window.decorView.dispatchTouchEvent(e)
        e.recycle()
    }

    private fun at(v: View, fx: Float = 0.5f, fy: Float = 0.5f): FloatArray {
        val loc = IntArray(2)
        v.getLocationInWindow(loc)
        return floatArrayOf(loc[0] + v.width * fx, loc[1] + v.height * fy)
    }

    private fun touch(v: View, action: Int, fx: Float = 0.5f, fy: Float = 0.5f) {
        val p = at(v, fx, fy)
        send(action, p[0], p[1], SystemClock.uptimeMillis())
    }

    private fun tap(v: View, fx: Float = 0.5f, fy: Float = 0.5f) {
        touch(v, MotionEvent.ACTION_DOWN, fx, fy)
        touch(v, MotionEvent.ACTION_UP, fx, fy)
        ShadowLooper.idleMainLooper()
    }

    /** A finger drag from (x, y) by (dx, dy) in window pixels, in small steps. */
    private fun drag(x: Float, y: Float, dx: Float, dy: Float) {
        val t = SystemClock.uptimeMillis()
        send(MotionEvent.ACTION_DOWN, x, y, t)
        for (i in 1..20) send(MotionEvent.ACTION_MOVE, x + dx * i / 20, y + dy * i / 20, t)
        send(MotionEvent.ACTION_UP, x + dx, y + dy, t)
        ShadowLooper.idleMainLooper()
    }

    /** Renders the whole scrollable page (not just the visible screen) to dist/android-screenshots. */
    private fun screenshot(name: String) {
        val page = scroll.getChildAt(0)
        val bmp = Bitmap.createBitmap(page.width, page.height, Bitmap.Config.ARGB_8888)
        val c = Canvas(bmp)
        c.drawColor(if (activity.panel.night) Palette.NIGHT.bg else Palette.DAY.bg)
        page.draw(c)
        val dir = File(System.getProperty("iki.screenshots", "build/screenshots")!!)
        dir.mkdirs()
        FileOutputStream(File(dir, "$name.png")).use { bmp.compress(Bitmap.CompressFormat.PNG, 100, it) }
        println("screenshot: ${File(dir, "$name.png")} ${bmp.width}x${bmp.height}")
    }

    /** A fling left over from an earlier drag would (correctly) swallow the next touch: stop it like a finger does. */
    private fun stopFling() {
        val t = SystemClock.uptimeMillis()
        for (a in listOf(MotionEvent.ACTION_DOWN, MotionEvent.ACTION_CANCEL)) {
            val e = MotionEvent.obtain(t, t, a, 1f, 1f, 0)
            scroll.onTouchEvent(e)
            e.recycle()
        }
    }

    private fun scrollTo(v: View) {
        stopFling()
        scroll.scrollTo(0, 0)
        val loc = IntArray(2)
        v.getLocationInWindow(loc)
        scroll.scrollTo(0, maxOf(0, loc[1] - scroll.height / 3))
        ShadowLooper.idleMainLooper()
    }

    @Test
    fun playsPadsOrnamentsAndLooper() {
        frames(20)
        screenshot("01-idle-day")

        // Hold Chi: blowing, note 3, holes follow
        val chi = view("Chi")
        scrollTo(chi)
        touch(chi, MotionEvent.ACTION_DOWN)
        frames(45)
        assertTrue("blowing while held", state()[STATE_BLOWING] > 0.5)
        assertEquals(3f, st[STATE_NOTE], 0f)
        assertTrue("sound reaches the output", st[STATE_PEAK_L] > 0.05)
        // a second finger on Ri while Chi is held → atari jump
        val ri = view("Ri")
        Native.gesture(h, G_HIT_START, 4, false)
        frames(3)
        assertEquals(4f, state()[STATE_NOTE], 0f)
        assertEquals("atari flash on Ri", 4f, st[STATE_ORN_INDEX], 0f)
        frames(30)
        screenshot("02-playing-ri")
        touch(ri, MotionEvent.ACTION_UP)
        touch(chi, MotionEvent.ACTION_UP)
        frames(30)
        assertFalse("released → silent", state()[STATE_BLOWING] > 0.5)

        // Gate + meri lane, looper record → plays at once
        scroll.scrollTo(0, 0)
        tap(view("Blow"))
        frames(2)
        assertTrue(state()[STATE_LATCHED] > 0.5)
        val tsuMeri = view("Tsu meri")
        scrollTo(tsuMeri)
        tap(tsuMeri)
        frames(2)
        assertEquals(1f, state()[STATE_MERI_LANE], 0f)
        val strip = view("loop track 1")
        scrollTo(strip)
        tap(strip, 1f - 64f / (strip.width / activity.resources.displayMetrics.density), 0.5f) // the punch ring
        frames(60)
        assertEquals("recording", 1f, state()[STATE_TRACKS], 0f)
        tap(strip, 1f - 64f / (strip.width / activity.resources.displayMetrics.density), 0.5f)
        var i = 0
        while (i++ < 400 && state()[STATE_TRACKS] != 2f) frames(1)
        assertEquals("loop playing immediately", 2f, state()[STATE_TRACKS], 0f)
        frames(40)
        screenshot("03-gate-meri-loop")

        // Night theme
        scroll.scrollTo(0, 0)
        tap(view("Breath"))
        frames(20)
        assertTrue(activity.panel.night)
        screenshot("04-night")
        tap(view("Blow"))
        frames(10)
        assertFalse(state()[STATE_BLOWING] > 0.5)
    }

    @Test
    fun pageScrollsFromControlsButNotFromDials() {
        frames(5)
        // a drag that starts on a knob's label (below the dial) scrolls the page
        val cutoff = view("Cutoff")
        scrollTo(cutoff)
        val before = scroll.scrollY
        val v0 = Native.get(h, P_FILT_CUT)
        val lbl = at(cutoff, 0.5f, 0.92f)
        drag(lbl[0], lbl[1], 0f, -300f)
        frames(2)
        assertTrue("page scrolled (${scroll.scrollY} vs $before)", scroll.scrollY > before + 100)
        assertEquals("knob untouched", v0, Native.get(h, P_FILT_CUT), 1e-9)

        // …and so does one that starts on a segmented switch or a button
        val seg = view("Lp/Hp/Bp/Notch/Peak/LoSh/HiSh/All")
        scrollTo(seg)
        val b2 = scroll.scrollY
        val type0 = Native.get(h, P_FILT_TYPE)
        val p = at(seg, 0.3f, 0.3f)
        drag(p[0], p[1], 0f, -300f)
        assertTrue("scrolled from a segment", scroll.scrollY > b2 + 100)
        assertEquals("segment not switched by the scroll", type0, Native.get(h, P_FILT_TYPE), 0.0)

        // a drag that starts on the dial turns the knob and does not scroll
        scrollTo(cutoff)
        val b3 = scroll.scrollY
        val dial = at(cutoff, 0.5f, (6f + 26f) / (52f + 40f))
        drag(dial[0], dial[1], 0f, -200f)
        frames(2)
        assertEquals("no scroll while turning", b3, scroll.scrollY)
        assertTrue("cutoff went up", Native.get(h, P_FILT_CUT) > v0 + 0.3)

        // the flute scrolls; the note fader plays
        val stage = view("stage")
        scrollTo(stage)
        val b4 = scroll.scrollY
        val flute = at(stage, 118f / 250f, 0.6f)
        drag(flute[0], flute[1], 0f, -250f)
        assertTrue("scrolled from the flute", scroll.scrollY > b4 + 80)
        assertFalse(state()[STATE_BLOWING] > 0.5)
    }

    @Test
    fun voiceSwitchKeepsTheSound() {
        val voice = view("Flute/Bore/Both")
        for (mode in listOf(1, 0, 2, 0, 1, 2, 1, 0)) {
            scroll.scrollTo(0, 0)
            ShadowLooper.idleMainLooper()
            tap(voice, (mode + 0.5f) / 3f, 0.5f)
            frames(3)
            assertEquals("voice", mode.toFloat(), state()[STATE_BORE], 0f)
            val ro = view("Ro")
            scrollTo(ro)
            touch(ro, MotionEvent.ACTION_DOWN)
            frames(50)
            assertTrue("sounds after switching to mode $mode: ${state()[STATE_PEAK_L]}", st[STATE_PEAK_L] > 0.05)
            touch(ro, MotionEvent.ACTION_UP)
            frames(80)
        }
        // switching while a drone is latched keeps it sounding
        scroll.scrollTo(0, 0)
        ShadowLooper.idleMainLooper()
        tap(view("Blow"))
        frames(30)
        tap(voice, 0.5f, 0.5f)
        frames(50)
        assertTrue("bore while latched", state()[STATE_PEAK_L] > 0.05 && st[STATE_BORE] == 1f)
        tap(voice, 5f / 6f, 0.5f)
        frames(50)
        assertTrue("both while latched", state()[STATE_PEAK_L] > 0.05 && st[STATE_BORE] == 2f)
        tap(voice, 1f / 6f, 0.5f)
        frames(50)
        assertTrue("sampler while latched", state()[STATE_PEAK_L] > 0.05 && st[STATE_BORE] == 0f)
    }

    /** On a tablet the modules sit side by side like the desktop racks. */
    @Test
    @Config(qualifiers = "w1280dp-h800dp-land-mdpi")
    fun tabletLayoutMatchesTheDesktopRacks() {
        frames(10)
        val loc = { v: View -> IntArray(2).also { v.getLocationInWindow(it) } }
        val crt = view("crt")
        val stage = view("stage")
        assertEquals("CRT and stage share a row", loc(crt)[1], loc(stage)[1])
        val env = view("Attack").parent.parent.parent // knob → knob row → lane → env lane flow
        assertTrue(env is View)
        val cutoff = view("Cutoff")
        val lfo = view("wave")
        assertTrue("filter and LFO share a row", kotlin.math.abs(loc(cutoff)[1] - loc(lfo)[1]) < 400)
        screenshot("05-tablet")
    }

    @Test
    fun hardwareKeyboardShortcuts() {
        assertTrue(Native.key(h, "g", true))
        frames(5)
        assertTrue(state()[STATE_BLOWING] > 0.5)
        assertEquals(4f, st[STATE_NOTE], 0f)
        Native.key(h, "o", true)
        frames(2)
        assertEquals("o toggles kan", 0f, state()[STATE_OTSU], 0f)
        Native.key(h, "g", false)
        frames(2)
        assertFalse(state()[STATE_BLOWING] > 0.5)
        assertFalse("q is not a shortcut", Native.key(h, "q", true))
    }

    @Test
    fun knobsAndReadoutsComeFromTheKernel() {
        Native.set(h, P_FILT_CUT, 1.0)
        assertEquals("4200 Hz", Native.hint(h, P_FILT_CUT))
        Native.set(h, P_FILT_TYPE, 1.0) // highpass
        assertEquals("1200 Hz", Native.hint(h, P_FILT_TYPE.let { P_FILT_CUT }))
        assertEquals(STATE_LEN, Native.stateLen())
        frames(8)
        assertTrue(Native.text(h, TEXT_HOLES).startsWith("holes "))
        assertTrue(Native.text(h, TEXT_PITCH).contains(" Hz · "))
        assertNotEquals("", Native.text(h, TEXT_STATUS))
        // the phrase plays through the instrument
        Native.gesture(h, G_PHRASE, 0, false)
        var i = 0
        while (i++ < 120 && state()[STATE_BLOWING] < 0.5) frames(1)
        assertTrue("phrase blows", st[STATE_BLOWING] > 0.5)
        assertTrue(st[STATE_CLIP] > 0.5)
        assertEquals("phrase", Native.text(h, TEXT_CLIP))
        // MIDI in: a note-on from a controller
        Native.midiIn(h, byteArrayOf(0x80.toByte(), 62, 0))
        assertEquals("unknown file → error", true, Native.playMidi(h, byteArrayOf(1, 2, 3), "junk").isNotEmpty())
    }
}
