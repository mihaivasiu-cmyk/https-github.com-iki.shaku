package org.iki.shakuhachi

import android.app.AlertDialog
import android.content.Context
import android.graphics.Canvas
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.widget.LinearLayout
import android.widget.PopupMenu
import android.widget.ScrollView
import kotlin.math.roundToInt

/**
 * The front panel — the Android counterpart of the desktop `view.rs`, module
 * for module in the same order: header, play strip (CRT · stage · blow
 * column), note pads, the Shape rack, the Space rack, looper and MIDI dock.
 * Behaviour and readouts come from the kernel; this class only lays out,
 * forwards touches and copies state into the views once per frame.
 */
class Panel(
    private val ctx: Context,
    private val h: Long,
    private val midi: MidiBus,
    night: Boolean,
    private val onNight: (Boolean) -> Unit,
    private val openMidiFile: () -> Unit,
) {
    private val d = ctx.resources.displayMetrics.density
    val scroll = ScrollView(ctx)
    private val page = column()
    private val st = FloatArray(STATE_LEN)
    private val spec = FloatArray(Crt.ROWS * 2)
    private var frame = 0
    var night = night
        private set
    private var pal = if (night) Palette.NIGHT else Palette.DAY

    // parameter bindings, refreshed from the kernel ≈10×/s
    private val knobs = ArrayList<Pair<Knob, Int>>()
    private val segs = ArrayList<Pair<Segmented, Int>>()
    private val switches = ArrayList<Pair<Btn, Int>>()
    private val rockers = ArrayList<Pair<Rocker, Int>>()
    private val drops = ArrayList<Pair<DropDown, Int>>()

    // views updated every frame
    private lateinit var status: StatusLine
    private lateinit var banner: Label
    private lateinit var help: View
    private lateinit var themeBtn: Btn
    private lateinit var keysBtn: Btn
    private lateinit var led: Led
    private lateinit var crt: Crt
    private lateinit var stage: Stage
    private lateinit var blow: BlowButton
    private lateinit var muraiki: MuraikiButton
    private lateinit var holesText: Label
    private lateinit var playText: Label
    private val pads = Array(2) { arrayOfNulls<Pad>(6) }
    private lateinit var ampScope: Scope
    private lateinit var filtEnvScope: Scope
    private lateinit var filtScope: Scope
    private lateinit var lfoDiv: View
    private lateinit var lfoHz: Label
    private lateinit var lfoRate: View
    private lateinit var delayDiv: View
    private lateinit var delayTime: Knob
    private lateinit var delayHint: Label
    private lateinit var spaceTitle: Label
    private lateinit var phaserText: Label
    private lateinit var eOutL: Bar
    private lateinit var eOutR: Bar
    private lateinit var eBreath: Bar
    private lateinit var eComp: Bar
    private lateinit var engineText: Label
    private lateinit var clockText: Label
    private lateinit var foliOn: Btn
    private lateinit var foliBpm: Label
    private lateinit var steps: Steps
    private lateinit var tempoKnob: Knob
    private lateinit var snapText: Label
    private lateinit var barPhase: Meter
    private lateinit var freeze: Btn
    private val strips = arrayOfNulls<LoopStrip>(4)
    private val mixLabels = arrayOfNulls<Label>(4)
    private val mixFaders = arrayOfNulls<MixFader>(4)
    private val mixValues = arrayOfNulls<Label>(4)
    private val mixFx = arrayOfNulls<Btn>(4)
    private lateinit var midiHead: Btn
    private lateinit var midiLed: Led
    private lateinit var midiHint: Label
    private lateinit var midiBody: View
    private lateinit var midiArm: Btn
    private lateinit var midiDevice: Label
    private lateinit var midiIn: Btn
    private lateinit var midiOut: Btn
    private lateinit var midiThru: Btn
    private lateinit var midiTx: Btn
    private lateinit var midiCount: Label
    private var midiOpen = false
    private var midiActivity = 0L

    init {
        scroll.isFillViewport = true
        scroll.addView(page)
        page.setPadding(dp(12), dp(10), dp(12), dp(24))
        header()
        playStrip()
        keysStrip()
        page.addView(stripTitle("Shape"))
        page.addView(FlowLayout(ctx).add(envRack(), 560f).add(filterRack(), 300f).add(lfoRack(), 330f), full())
        spaceTitle = stripTitle("Space")
        page.addView(spaceTitle)
        page.addView(
            FlowLayout(ctx).add(delayModule(), 236f).add(spaceModule(), 160f).add(colorModule(), 206f)
                .add(engineModule(), 300f).add(foliModule(), 290f),
            full(),
        )
        page.addView(FlowLayout(ctx).add(loopPedal(), 560f).add(midiDock(), 300f), full())
        page.addView(
            Label(
                ctx,
                "Space gate · A–H kari · Z–N meri · ↑↓←→ step note · O octave · M muraiki · 1–4 track · R punch · T stop · U undo",
                10.5f, Label.Role.SUBTLE,
            ).apply { setPadding(0, dp(6), 0, 0) },
        )
        midi.onChange = { refreshMidi() }
        applyPalette()
        refreshControls()
    }

    // ------------------------------------------------------------------ helpers

    private fun dp(v: Number) = (v.toFloat() * d).roundToInt()

    private fun column() = LinearLayout(ctx).apply { orientation = LinearLayout.VERTICAL }

    private fun row(gapDp: Int = 0) = LinearLayout(ctx).apply {
        orientation = LinearLayout.HORIZONTAL
        gravity = Gravity.CENTER_VERTICAL
        if (gapDp > 0) {
            showDividers = LinearLayout.SHOW_DIVIDER_MIDDLE
            dividerDrawable = android.graphics.drawable.ShapeDrawable().apply {
                intrinsicWidth = dp(gapDp)
                paint.color = 0
            }
        }
    }

    private fun full() = LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT)

    private fun weight(w: Float = 1f) = LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, w)

    private fun wrap() = LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT)

    private fun sized(wDp: Int) = LinearLayout.LayoutParams(dp(wDp), ViewGroup.LayoutParams.WRAP_CONTENT)

    private fun space(hDp: Int) = View(ctx).apply { minimumHeight = dp(hDp) }

    private fun stripTitle(s: String) = Label(ctx, s.uppercase(), 12f, Label.Role.MUTED, bold = true).apply {
        setPadding(0, dp(10), 0, dp(4))
    }

    private fun small(s: String) = Label(ctx, s, 9.5f, Label.Role.SUBTLE)

    private fun knob(param: Int, title: String, large: Boolean, iris: Boolean): Knob {
        val k = Knob(ctx, title, large, iris)
        k.onChange = { v ->
            Native.set(h, param, v)
            k.hint = hintFor(param)
        }
        knobs.add(k to param)
        return k
    }

    private fun hintFor(param: Int) = when (param) {
        P_DELAY_TIME -> Native.text(h, TEXT_DELAY)
        else -> Native.hint(h, param)
    }

    private fun knobRow(vararg ks: Knob) = row().apply { for (k in ks) addView(k, weight()) }

    private fun segment(param: Int, labels: Array<String>, widthDp: Int = 0): Segmented {
        val s = Segmented(ctx, labels)
        s.onSelect = { i ->
            Native.set(h, param, i.toDouble())
            refreshControls()
        }
        s.layoutParams = if (widthDp > 0) sized(widthDp) else full()
        segs.add(s to param)
        return s
    }

    /** A lit button bound to an on/off parameter. */
    private fun switchBtn(param: Int, label: String, wDp: Float = 48f, hDp: Float = 28f): Btn {
        val b = Btn(ctx, label, wDp, hDp)
        b.onClick = {
            Native.set(h, param, if (Native.get(h, param) > 0.5) 0.0 else 1.0)
            refreshControls()
        }
        switches.add(b to param)
        return b
    }

    private fun btn(label: String, wDp: Float = 0f, hDp: Float = 32f, click: () -> Unit) =
        Btn(ctx, label, wDp, hDp).apply { onClick = click }

    private fun divSelect(param: Int): DropDown {
        val dd = DropDown(ctx, "Div", P_DELAY_DIV_LABELS)
        dd.onSelect = { i ->
            Native.set(h, param, i.toDouble())
            refreshControls()
        }
        drops.add(dd to param)
        return dd
    }

    // ------------------------------------------------------------------ header

    private fun header() {
        val flow = FlowLayout(ctx, 12f)
        val brand = row()
        brand.addView(Label(ctx, "息", 34f, Label.Role.MOSS))
        val names = column().apply { setPadding(dp(6), 0, 0, 0) }
        names.addView(Label(ctx, "Iki", 24f, Label.Role.FG))
        names.addView(Label(ctx, "1.8 shakuhachi", 11f, Label.Role.MUTED))
        brand.addView(names)
        flow.add(brand, 124f, grow = false)

        val voice = column()
        voice.addView(small("VOICE"))
        val seg = Segmented(ctx, P_VOICE_LABELS)
        seg.onSelect = { i ->
            Native.set(h, P_VOICE, i.toDouble())
            refreshControls()
        }
        segs.add(seg to P_VOICE)
        voice.addView(seg, sized(170))
        flow.add(voice, 170f, grow = false)

        flow.add(knobRow(knob(P_FLUTE_LEVEL, "Flute", false, false), knob(P_BORE_LEVEL, "Bore", false, true), knob(P_TUNE, "Tune", false, false)), 190f, grow = false)

        status = StatusLine(ctx)
        status.onClick = {
            AlertDialog.Builder(ctx).setTitle("Audio").setMessage(Native.text(h, TEXT_BACKEND))
                .setPositiveButton(android.R.string.ok, null).show()
        }
        flow.add(status, 180f)

        val right = row(6)
        themeBtn = btn("Breath", 70f, 30f) {
            night = !night
            onNight(night)
            applyPalette()
        }
        keysBtn = btn("Info", 46f, 30f) {
            help.visibility = if (help.visibility == View.VISIBLE) View.GONE else View.VISIBLE
            keysBtn.on = help.visibility == View.VISIBLE
        }
        led = Led(ctx)
        right.addView(themeBtn)
        right.addView(keysBtn)
        right.addView(led)
        flow.add(right, 146f, grow = false)
        page.addView(flow, full())

        banner = Label(ctx, "", 11.5f, Label.Role.DANGER, bold = true).apply { visibility = View.GONE }
        page.addView(banner)
        help = helpCard().apply { visibility = View.GONE }
        page.addView(help, full().apply { topMargin = dp(8) })
    }

    private fun helpCard(): View {
        val card = Module(ctx, null)
        val rows = listOf(
            "Space" to "toggle blow mode (latched drone)",
            "A S D F G H  ;  Z X C V B N" to "hold to blow; tap while blowing = atari, tap twice = koro",
            "↑ → / ↓ ←" to "next / previous note in the current lane",
            "O" to "otsu ↔ kan (octave)",
            "M" to "muraiki (breath burst)",
            "1 – 4" to "select loop track",
            "R / T / U" to "punch (rec → play → dub) · stop · undo on the selected track",
            "touch" to "drag knobs on the dial ↕ (elsewhere scrolls); open a .mid file from the MIDI dock",
        )
        for ((k, v) in rows) {
            val r = row()
            r.gravity = Gravity.TOP
            r.addView(Label(ctx, k, 12f, Label.Role.MOSS, bold = true), sized(96))
            r.addView(Label(ctx, v, 12f, Label.Role.FG), weight())
            r.setPadding(0, dp(2), 0, dp(2))
            card.addView(r, full())
        }
        return card
    }

    // ------------------------------------------------------------------ play strip

    private fun playStrip() {
        val flow = FlowLayout(ctx, 10f)
        crt = Crt(ctx)
        flow.add(crt, 360f)
        stage = Stage(ctx)
        stage.onLean = { v -> Native.set(h, P_LEAN, v) }
        stage.onNoteStart = { n -> Native.gesture(h, G_SLIDE_START, n, false) }
        stage.onNoteMove = { n -> Native.gesture(h, G_SLIDE_MOVE, n, false) }
        stage.onNoteEnd = { Native.gesture(h, G_SLIDE_END, 0, false) }
        flow.add(stage, 150f)

        val col = column()
        val top = row(6)
        top.gravity = Gravity.CENTER_VERTICAL
        blow = BlowButton(ctx)
        blow.onClick = { Native.gesture(h, G_TOGGLE_GATE, 0, false) }
        muraiki = MuraikiButton(ctx)
        muraiki.onPress = { Native.gesture(h, G_MURAIKI, 0, false) }
        val octave = Rocker(ctx, "Kan", "Otsu", true)
        octave.onClick = { Native.set(h, P_OTSU, if (Native.get(h, P_OTSU) > 0.5) 0.0 else 1.0) }
        rockers.add(octave to P_OTSU)
        top.addView(blow)
        top.addView(muraiki)
        top.addView(octave)
        col.addView(top)
        col.addView(space(4))
        col.addView(knobRow(knob(P_MERI, "Meri", true, false), knob(P_IKI, "Iki", true, true), knob(P_GAIN, "Gain", true, false)), full())
        col.addView(knobRow(knob(P_BEND, "Bend", false, true), knob(P_BEND_RANGE, "Range", false, false), knob(P_YURI, "Yuri", false, false), knob(P_YURI_RATE, "Rate", false, false), knob(P_GLIDE, "Glide", false, true)), full())
        holesText = Label(ctx, "", 10.5f)
        playText = Label(ctx, "", 10.5f)
        col.addView(holesText)
        col.addView(playText)
        flow.add(col, 226f)
        page.addView(flow, full())
    }

    // ------------------------------------------------------------------ note pads

    private fun keysStrip() {
        val kana = arrayOf("ロ", "ツ", "レ", "チ", "リ", "ヒ")
        val names = arrayOf("Ro", "Tsu", "Re", "Chi", "Ri", "Hi")
        val keys = arrayOf(arrayOf("A", "S", "D", "F", "G", "H"), arrayOf("Z", "X", "C", "V", "B", "N"))
        for (lane in 0..1) {
            val meri = lane == 1
            page.addView(
                Label(
                    ctx,
                    if (meri) "MERI · Z X C V B N · left fader leans" else "KARI · A S D F G H · tap: atari · again: koro · M: muraiki",
                    10.5f,
                ).apply { setPadding(0, dp(8), 0, dp(4)) },
            )
            val r = row(6)
            r.isMotionEventSplittingEnabled = true // a second finger on another pad = ornament
            for (i in 0..5) {
                val p = Pad(ctx, i, meri, if (meri) kana[i] + "メ" else kana[i], if (meri) "${names[i]} meri" else names[i], keys[lane][i])
                p.onDown = { Native.gesture(h, G_HIT_START, i, meri) }
                p.onUp = { Native.gesture(h, G_SLIDE_END, 0, false) }
                pads[lane][i] = p
                r.addView(p, weight())
            }
            page.addView(r, full())
        }
    }

    // ------------------------------------------------------------------ shape rack

    private fun envLane(amp: Boolean): View {
        val lane = column()
        val head = row()
        head.addView(Label(ctx, if (amp) "AMP" else "FILT", 10.5f, Label.Role.MUTED, bold = true), weight())
        head.addView(switchBtn(if (amp) P_ENV_LOOP else P_FENV_LOOP, "Loop", 48f, 26f))
        lane.addView(head, full())
        val sc = Scope(ctx, filter = false, iris = !amp)
        if (amp) ampScope = sc else filtEnvScope = sc
        lane.addView(sc, full())
        lane.addView(segment(if (amp) P_ENV_SHAPE else P_FENV_SHAPE, P_ENV_SHAPE_LABELS))
        val iris = !amp
        lane.addView(
            knobRow(
                knob(if (amp) P_ENV_A else P_FENV_A, "Attack", false, iris),
                knob(if (amp) P_ENV_D else P_FENV_D, "Decay", false, iris),
                knob(if (amp) P_ENV_S else P_FENV_S, "Sustain", false, iris),
                knob(if (amp) P_ENV_R else P_FENV_R, "Release", false, iris),
            ),
            full(),
        )
        return lane
    }

    private fun envRack() = Module(ctx, "Env").apply {
        addView(FlowLayout(ctx, 12f).add(envLane(true), 268f).add(envLane(false), 268f), full())
    }

    private fun filterRack() = Module(ctx, "Filter").apply {
        filtScope = Scope(ctx, filter = true, iris = false)
        addView(filtScope, full())
        addView(segment(P_FILT_TYPE, P_FILT_TYPE_LABELS, minCell = 34f))
        addView(segment(P_FILT_AUTO, P_FILT_AUTO_LABELS, 150))
        addView(
            knobRow(
                knob(P_FILT_CUT, "Cutoff", true, false), knob(P_FILT_RES, "Res", true, false),
                knob(P_FILT_MOTION, "Motion", false, true), knob(P_FILT_RATE, "Rate", false, true),
            ),
            full(),
        )
    }

    private fun segment(param: Int, labels: Array<String>, minCell: Float): Segmented {
        val s = Segmented(ctx, labels, minCell)
        s.onSelect = { i ->
            Native.set(h, param, i.toDouble())
            refreshControls()
        }
        s.layoutParams = full()
        segs.add(s to param)
        return s
    }

    private fun lfoRack() = Module(ctx, "Lfo").apply {
        val sync = row(8)
        sync.addView(segment(P_LFO_SYNC, arrayOf("Free", "Bars"), 100))
        lfoDiv = divSelect(P_LFO_DIV)
        sync.addView(lfoDiv)
        lfoHz = Label(ctx, "", 10.5f)
        sync.addView(lfoHz)
        addView(sync)
        val mid = row(6)
        mid.gravity = Gravity.TOP
        lfoRate = knob(P_LFO_RATE, "Rate", false, false)
        mid.addView(lfoRate)
        val wave = column()
        wave.addView(small("WAVE"))
        val picker = WavePicker(ctx, P_LFO_WAVE_LABELS)
        picker.onSelect = { i ->
            Native.set(h, P_LFO_WAVE, i.toDouble())
            refreshControls()
        }
        wavePicker = picker
        wave.addView(picker)
        mid.addView(wave)
        addView(mid)
        addView(small("DEST"))
        val dest = row()
        val lanes = listOf(Triple("Pitch", P_LFO_PITCH, P_LFO_PITCH_AMT), Triple("Filt", P_LFO_FILT, P_LFO_FILT_AMT), Triple("Amp", P_LFO_AMP, P_LFO_AMP_AMT))
        for ((i, lane) in lanes.withIndex()) {
            val (name, sw, amt) = lane
            val r = row()
            val rk = Rocker(ctx, name, "", false)
            rk.onClick = {
                Native.set(h, sw, if (Native.get(h, sw) > 0.5) 0.0 else 1.0)
                refreshControls()
            }
            rockers.add(rk to sw)
            r.addView(rk)
            r.addView(knob(amt, "Depth", false, i > 0))
            dest.addView(r, weight())
        }
        addView(dest, full())
    }

    private var wavePicker: WavePicker? = null

    // ------------------------------------------------------------------ space rack

    private fun delayModule() = Module(ctx, "Delay").apply {
        val modes = row(8)
        modes.addView(segment(P_DELAY_PING, arrayOf("Mono", "Ping"), 110))
        modes.addView(segment(P_DELAY_SYNC, arrayOf("Free", "Bars"), 110))
        addView(modes)
        delayDiv = divSelect(P_DELAY_DIV)
        addView(delayDiv, wrap().apply { topMargin = dp(4) })
        delayTime = knob(P_DELAY_TIME, "Time", false, true)
        addView(knobRow(delayTime, knob(P_DELAY_FEEDBACK, "Feedback", false, true), knob(P_DELAY_MIX, "Mix", false, true)), full())
        delayHint = Label(ctx, "", 10.5f)
        addView(delayHint)
    }

    private fun spaceModule() = Module(ctx, "Space").apply {
        addView(knobRow(knob(P_REVERB, "Mix", false, true), knob(P_REVERB_DECAY, "Decay", false, true)), full())
        val width = Segmented(ctx, arrayOf("Tight", "Wide"))
        width.onSelect = { i -> Native.set(h, P_WIDTH, if (i == 1) 0.72 else 0.1) }
        widthSeg = width
        addView(width, sized(150))
    }

    private var widthSeg: Segmented? = null

    private fun colorModule() = Module(ctx, "Color").apply {
        addView(knobRow(knob(P_TAPE, "Tape", false, false), knob(P_PHASER, "Phaser", false, true), knob(P_PHASER_DEPTH, "Depth", false, true)), full())
        phaserText = small("phaser idle")
        addView(phaserText)
    }

    /** Engine / device state at a glance (desktop `output_module`). */
    private fun engineModule() = Module(ctx, "Engine").apply {
        eOutL = Bar(ctx, "out L", false)
        eOutR = Bar(ctx, "out R", true)
        eBreath = Bar(ctx, "breath", false)
        eComp = Bar(ctx, "comp", false, danger = true)
        for (b in listOf(eOutL, eOutR, eBreath, eComp)) addView(b, full())
        engineText = Label(ctx, "", 10f, Label.Role.SUBTLE)
        clockText = Label(ctx, "", 10f, Label.Role.SUBTLE)
        addView(engineText)
        addView(clockText)
    }

    /** `foli.ts` — percussion accompaniment (triplet grid, follows the loop clock). */
    private fun foliModule() = Module(ctx, "Foli · drums").apply {
        val top = row(8)
        foliOn = switchBtn(P_FOLI_ON, "Off", 44f, 28f)
        top.addView(foliOn)
        top.addView(segment(P_FOLI_FEEL, P_FOLI_FEEL_LABELS, 150))
        addView(top)
        addView(segment(P_FOLI_KIT, P_FOLI_KIT_LABELS, 282))
        val r = row(6)
        r.addView(knob(P_FOLI_LEVEL, "Level", false, true))
        val c = column()
        foliBpm = Label(ctx, "", 10f)
        c.addView(foliBpm)
        steps = Steps(ctx)
        steps.onToggle = { i -> Native.gesture(h, G_FOLI_STEP, i, false) }
        c.addView(steps, full())
        r.addView(c, weight())
        addView(r, full())
    }

    // ------------------------------------------------------------------ looper

    private fun loopPedal() = Module(ctx, "Loop").apply {
        val clock = column()
        tempoKnob = knob(P_TEMPO, "Clock", true, false)
        clock.addView(tempoKnob, wrap())
        snapText = Label(ctx, "snap · 1 bar", 10f)
        clock.addView(snapText)
        barPhase = Meter(ctx)
        clock.addView(barPhase, sized(100))
        freeze = btn("Freeze 4", 100f, 30f) { Native.loop(h, L_FREEZE, 3, 0.0) }.apply { fireOnDown = true }
        clock.addView(freeze, wrap().apply { topMargin = dp(4) })
        clock.addView(btn("Stop all", 100f, 30f) { Native.loop(h, L_STOP_ALL, 0, 0.0) }.apply { fireOnDown = true }, wrap().apply { topMargin = dp(4) })

        val stripCol = column()
        for (t in 0..3) {
            val s = LoopStrip(ctx, t)
            s.listener = object : LoopStrip.Listener {
                override fun select() = Native.loop(h, L_SELECT, t, 0.0)
                override fun punch() = Native.loop(h, L_PUNCH, t, 0.0)
                override fun play() = Native.loop(h, L_PLAY, t, 0.0)
                override fun stop() = Native.loop(h, L_STOP, t, 0.0)
                override fun undo() = Native.loop(h, L_UNDO, t, 0.0)
                override fun trim(a: Double, b: Double) {
                    Native.loop(h, L_TRIM_IN, t, a)
                    Native.loop(h, L_TRIM_OUT, t, b)
                }
            }
            strips[t] = s
            stripCol.addView(s, full().apply { bottomMargin = dp(4) })
        }

        val mix = column()
        mix.addView(Label(ctx, "MIX", 10f))
        val chans = row(4)
        for (t in 0..3) {
            val ch = column()
            ch.gravity = Gravity.CENTER_HORIZONTAL
            val lbl = Label(ctx, "${t + 1}", 10.5f, Label.Role.SUBTLE).apply { gravity = Gravity.CENTER }
            mixLabels[t] = lbl
            ch.addView(lbl, sized(34))
            val f = MixFader(ctx, t % 2 == 1)
            f.onChange = { v -> Native.loop(h, L_LEVEL, t, v) }
            mixFaders[t] = f
            ch.addView(f, LinearLayout.LayoutParams(dp(34), dp(150)))
            val v = Label(ctx, "", 10f).apply { gravity = Gravity.CENTER }
            mixValues[t] = v
            ch.addView(v, sized(34))
            val fx = Btn(ctx, "FX", 34f, 24f)
            fx.onClick = { Native.loop(h, L_FX, t, if (st[STATE_TRACKS + t * TRACK_STRIDE + 6] > 0.5) 0.0 else 1.0) }
            mixFx[t] = fx
            ch.addView(fx)
            chans.addView(ch)
        }
        mix.addView(chans)

        // wide: clock | strips | mix (desktop); narrow: clock + mix side by side, strips below
        val body = FlowLayout(ctx, 10f)
        body.add(clock, 108f, grow = false).add(stripCol, 300f).add(mix, 160f, grow = false)
        body.addOnLayoutChangeListener { v, l, _, r, _, _, _, _, _ ->
            val wide = (r - l) >= dp(108 + 300 + 160 + 20)
            val at = (v as ViewGroup).indexOfChild(stripCol)
            if (wide != (at == 1)) v.post {
                body.removeView(stripCol)
                if (wide) body.addView(stripCol, 1, FlowLayout.Params(300f)) else body.addView(stripCol, FlowLayout.Params(300f))
            }
        }
        addView(body, full())
    }

    // ------------------------------------------------------------------ MIDI dock

    /** `midi-dock.tsx`: arm, status, .mid playback, input/output routing, thru and tx. */
    private fun midiDock() = Module(ctx, "Midi").apply {
        val head = row(6)
        midiHead = btn("MIDI · off", 240f, 30f) {
            midiOpen = !midiOpen
            refreshMidi()
        }
        midiLed = Led(ctx, iris = true)
        head.addView(midiHead, weight())
        head.addView(midiLed)
        addView(head, full())
        midiHint = small("tap to open routing · notes/bend/CC in, tx out")
        addView(midiHint)

        val body = column()
        val arm = row(6)
        midiArm = btn("MIDI", 52f, 30f) { if (midi.armed) midi.disable() else midi.enable() }
        midiDevice = Label(ctx, "MIDI", 10.5f).apply {
            maxLines = 1
            ellipsize = android.text.TextUtils.TruncateAt.END
        }
        arm.addView(midiArm)
        arm.addView(midiDevice, weight())
        body.addView(arm, full().apply { topMargin = dp(4) })
        val files = row(6)
        files.addView(btn("Open .mid", 0f, 30f) { openMidiFile() })
        files.addView(btn("phrase", 52f, 30f) { Native.gesture(h, G_PHRASE, 0, false) })
        body.addView(files, full().apply { topMargin = dp(4) })
        midiIn = btn("All ▾", 0f, 30f) { pickPort(midiIn, true) }
        midiOut = btn("Off ▾", 0f, 30f) { pickPort(midiOut, false) }
        body.addView(labelled("In", midiIn), full().apply { topMargin = dp(4) })
        body.addView(labelled("Out", midiOut), full().apply { topMargin = dp(4) })
        val flags = row(6)
        midiThru = btn("Thru", 52f, 30f) {
            midi.thru = !midi.thru
            refreshMidi()
        }
        midiTx = btn("Tx", 52f, 30f) {
            midi.tx = !midi.tx
            refreshMidi()
        }
        flags.addView(midiThru)
        flags.addView(midiTx)
        flags.addView(btn("Rescan", 60f, 30f) { midi.rescan() })
        midiCount = Label(ctx, "", 10f, Label.Role.SUBTLE)
        flags.addView(midiCount)
        body.addView(flags, full().apply { topMargin = dp(4) })
        midiBody = body
        addView(body, full())
        refreshMidi()
    }

    private fun labelled(label: String, v: View) = row(8).apply {
        addView(Label(ctx, label, 10.5f, Label.Role.FG), sized(28))
        addView(v, weight())
    }

    private fun pickPort(anchor: View, input: Boolean) {
        val m = PopupMenu(ctx, anchor)
        val ids = ArrayList<String>()
        if (input) {
            ids.add("all")
            m.menu.add(0, 0, 0, "All")
        } else {
            ids.add("none")
            m.menu.add(0, 0, 0, "Off")
        }
        for (p in if (input) midi.ins else midi.outs) {
            m.menu.add(0, ids.size, ids.size, p.name)
            ids.add(p.id)
        }
        m.setOnMenuItemClickListener {
            val id = ids[it.itemId]
            if (input) midi.setIn(id) else midi.setOut(id)
            true
        }
        m.show()
    }

    private fun portName(id: String, list: List<MidiBus.Port>, none: String) = when (id) {
        "all" -> "All"
        "none" -> none
        else -> list.firstOrNull { it.id == id }?.name ?: id
    }

    private fun refreshMidi() {
        midiHead.label = when {
            midi.error -> "MIDI · err"
            midi.armed -> "MIDI · ${midi.device}"
            else -> "MIDI · off"
        }
        midiHead.on = midi.armed
        midiHint.visibility = if (midiOpen) View.GONE else View.VISIBLE
        midiBody.visibility = if (midiOpen) View.VISIBLE else View.GONE
        midiArm.on = midi.armed
        midiIn.label = portName(midi.inId, midi.ins, "All") + " ▾"
        midiOut.label = portName(midi.outId, midi.outs, "Off") + " ▾"
        midiThru.on = midi.thru
        midiTx.on = midi.tx
        midiCount.show("${midi.ins.size} in · ${midi.outs.size} out")
        midiDevice.role = if (midi.error) Label.Role.DANGER else Label.Role.MUTED
        midiDevice.show(midi.device)
    }

    /** A parse error from a .mid file (shown in the dock like the desktop). */
    fun midiFileError(e: String) {
        midiOpen = true
        refreshMidi()
        midiDevice.role = Label.Role.DANGER
        midiDevice.show(e)
    }

    /** Keep the page clear of the status / navigation bars (edge-to-edge since targetSdk 35). */
    fun setInsets(l: Int, t: Int, r: Int, b: Int) = page.setPadding(dp(12) + l, dp(10) + t, dp(12) + r, dp(24) + b)

    // ------------------------------------------------------------------ theme

    fun applyPalette() {
        pal = if (night) Palette.NIGHT else Palette.DAY
        scroll.setBackgroundColor(pal.bg)
        scroll.applyTheme(pal)
        themeBtn.label = if (night) "Temple" else "Breath"
        themeBtn.on = night
    }

    // ------------------------------------------------------------------ per frame

    /** Knob values/hints, choices, switches, curves, conditional controls (≈10 Hz). */
    fun refreshControls() {
        for ((k, p) in knobs) {
            k.value = Native.get(h, p)
            k.hint = hintFor(p)
        }
        for ((s, p) in segs) s.selected = Native.get(h, p).roundToInt()
        for ((b, p) in switches) b.on = Native.get(h, p) > 0.5
        for ((r, p) in rockers) r.on = Native.get(h, p) > 0.5
        for ((dd, p) in drops) if (dd.selected != Native.get(h, p).roundToInt()) dd.selected = Native.get(h, p).roundToInt()
        wavePicker?.selected = Native.get(h, P_LFO_WAVE).roundToInt()
        widthSeg?.selected = if (Native.get(h, P_WIDTH) > 0.4) 1 else 0
        val delaySync = Native.get(h, P_DELAY_SYNC) > 0.5
        show(delayDiv, delaySync)
        show(delayTime, !delaySync)
        show(delayHint, delaySync)
        delayHint.show(Native.text(h, TEXT_DELAY))
        val lfoSync = Native.get(h, P_LFO_SYNC) > 0.5
        show(lfoDiv, lfoSync)
        show(lfoHz, lfoSync)
        show(lfoRate, !lfoSync)
        lfoHz.show(Native.text(h, TEXT_LFO_HZ))
        for ((sc, which) in listOf(ampScope to CURVE_AMP_ENV, filtEnvScope to CURVE_FILT_ENV, filtScope to CURVE_FILTER)) {
            Native.curve(h, which, sc.points)
            sc.invalidate()
        }
    }

    private fun show(v: View, on: Boolean) {
        val vis = if (on) View.VISIBLE else View.GONE
        if (v.visibility != vis) v.visibility = vis
    }

    /** One display frame. */
    fun frame() {
        Native.tick(h)
        Native.state(h, st)
        Native.spectrum(h, spec)
        crt.push(spec)

        val blowing = st[STATE_BLOWING] > 0.5
        val latched = st[STATE_LATCHED] > 0.5
        val meriLane = st[STATE_MERI_LANE] > 0.5
        val note = st[STATE_NOTE].toInt()
        val kari = st[STATE_KARI_NOTE].toInt()
        val meri = st[STATE_MERI_NOTE].toInt()

        crt.kana = Native.text(h, TEXT_KANA)
        crt.name = Native.text(h, TEXT_NAME)
        crt.sub = Native.text(h, TEXT_PITCH)
        crt.blowing = blowing
        crt.breath = st[STATE_BREATH]
        crt.peakL = st[STATE_PEAK_L]
        crt.peakR = st[STATE_PEAK_R]
        crt.cutText = String.format(java.util.Locale.US, "cut %.0f Hz · gr %.1f dB", st[STATE_CUTOFF], st[STATE_COMP_GR])
        led.on = latched

        for (i in 0..3) stage.holes[i] = i >= note
        stage.holes[4] = note < 5
        stage.blowing = blowing
        stage.kariNote = kari
        stage.kariActive = !meriLane
        stage.lean = Native.get(h, P_LEAN)
        stage.invalidate()
        blow.on = latched
        muraiki.lit = st[STATE_MURAIKI] > 0.5
        holesText.show(Native.text(h, TEXT_HOLES))
        playText.show(Native.text(h, TEXT_PLAY))

        val ornIndex = st[STATE_ORN_INDEX].toInt()
        val ornLane = st[STATE_ORN_LANE].toInt()
        val ornKind = if (st[STATE_ORN_KORO] > 0.5) 2 else 1
        for (lane in 0..1) {
            val sel = if (lane == 1) meri else kari
            for (i in 0..5) {
                val live = (lane == 1) == meriLane && i == note
                pads[lane][i]!!.set(i == sel, live, if (ornIndex == i && ornLane == lane) ornKind else 0)
            }
        }

        // looper
        val selTrack = st[STATE_LOOP_TRACK].toInt()
        for (t in 0..3) {
            val o = STATE_TRACKS + t * TRACK_STRIDE
            val mode = st[o].toInt()
            val s = strips[t]!!
            val waveLen = Native.loopWave(h, t, s.wave)
            s.set(waveLen, mode, st[o + 1], st[o + 2], st[o + 8], st[o + 9], st[o + 4] > 0.5, st[o + 5] > 0.5, t == selTrack, st[o + 7] > 0.5)
            val pad = st[o + 4] > 0.5
            val live = mode in 1..3
            mixLabels[t]!!.show(if (pad) "P" else "${t + 1}")
            mixLabels[t]!!.role = if (live) Label.Role.FG else Label.Role.SUBTLE
            mixFaders[t]!!.value = st[o + 3].toDouble()
            mixFaders[t]!!.live = live
            val lv = st[o + 3]
            mixValues[t]!!.show(if (lv <= 0.004f) "−∞" else "${(lv * 100).roundToInt()}")
            mixFx[t]!!.on = st[o + 6] > 0.5
        }
        val bars = maxOf(1, st[STATE_CYCLE_BARS].roundToInt())
        snapText.show("snap · $bars bar${if (bars == 1) "" else "s"}")
        barPhase.value = st[STATE_BAR_PHASE]
        freeze.on = st[STATE_PAD_ARMED] > 0.5

        // foli
        val foli = st[STATE_FOLI_ON] > 0.5
        foliOn.label = if (foli) "On" else "Off"
        foliOn.on = foli
        steps.set(st[STATE_FOLI_MASK].toInt(), st[STATE_FOLI_STEP].toInt())

        // engine
        fun f2(v: Float) = String.format(java.util.Locale.US, "%.2f", v)
        eOutL.set(st[STATE_PEAK_L], f2(st[STATE_PEAK_L]))
        eOutR.set(st[STATE_PEAK_R], f2(st[STATE_PEAK_R]))
        eBreath.set(st[STATE_BREATH], f2(st[STATE_BREATH]))
        eComp.set(st[STATE_COMP_GR] / 12f, String.format(java.util.Locale.US, "-%.1f dB", st[STATE_COMP_GR]))
        phaserText.show(if (st[STATE_PHASER_ROUTED] > 0.5) "phaser routed (L)" else "phaser idle")

        // MIDI: Tx echo + clip notes out, activity LED, playing clip name
        val out = Native.midiOut(h, midi.tx && midi.armed)
        if (out.isNotEmpty()) midi.send(out)
        val act = midi.activity.get()
        midiLed.on = act % 2 == 1L
        if (act != midiActivity) midiActivity = act

        if (frame % 6 == 0) {
            refreshControls()
            val bpm = Native.text(h, TEXT_BPM)
            spaceTitle.show("SPACE · ${bpm.uppercase()}")
            foliBpm.show("$bpm · 12/8")
            engineText.show(Native.text(h, TEXT_ENGINE))
            clockText.show(Native.text(h, TEXT_CLOCK))
            clockText.role = if (st[STATE_CLOCK_LATE] > 0.5) Label.Role.DANGER else Label.Role.SUBTLE
            if (st[STATE_CLIP] > 0.5) {
                midiDevice.role = Label.Role.MUTED
                midiDevice.show("♪ " + Native.text(h, TEXT_CLIP))
            } else if (!midi.error && midiDevice.text.startsWith("♪")) {
                midiDevice.show(midi.device)
            }
        }
        if (frame % 15 == 0) {
            val problem = if (st[STATE_PROBLEM] > 0.5) Native.text(h, TEXT_PROBLEM) else ""
            status.status = Native.text(h, TEXT_STATUS)
            status.problem = problem.isNotEmpty()
            banner.show(if (problem.isEmpty()) "" else "⚠ $problem")
            show(banner, problem.isNotEmpty())
        }
        frame++
    }
}

/** A thin progress bar (the loop clock's bar phase). */
class Meter(c: Context) : IkiView(c) {
    var value = 0f
        set(v) {
            if (v != field) {
                field = v
                invalidate()
            }
        }

    override fun onMeasure(w: Int, h: Int) = setMeasuredDimension(resolveSize(dp(100).roundToInt(), w), dp(12).roundToInt())

    override fun onDraw(c: Canvas) {
        val y = height / 2f
        fill.color = pal.surface2
        roundRect(c, 0f, y - dp(3), width.toFloat(), y + dp(3), 3f, fill)
        fill.color = pal.moss
        roundRect(c, 0f, y - dp(3), width * value.coerceIn(0f, 1f), y + dp(3), 3f, fill)
    }
}
