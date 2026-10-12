package org.iki.shakuhachi

import android.app.Activity
import android.content.Intent
import android.content.res.Configuration
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.provider.OpenableColumns
import android.view.Choreographer
import android.view.KeyEvent
import android.view.WindowInsets
import android.view.WindowInsetsController
import android.view.WindowManager

/**
 * The Android app shell — the counterpart of the desktop `app.rs`: engine
 * lifecycle, preferences (theme, voice), hardware keyboard, MIDI and .mid
 * files, and the per-display-frame loop that drives [Panel].
 */
class MainActivity : Activity(), Choreographer.FrameCallback {
    companion object {
        /** Tests set this to run the engine on a silent clock. */
        @JvmField var forceSilentAudio = false

        /** Tests drive [frame] themselves (no Choreographer loop). */
        @JvmField var manualFrames = false

        private const val OPEN_MIDI = 1
    }

    private var h = 0L
    private var running = false
    lateinit var panel: Panel
        private set
    private lateinit var midi: MidiBus

    // ------------------------------------------------------------------ lifecycle

    override fun onCreate(saved: Bundle?) {
        super.onCreate(saved)
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        val prefs = getPreferences(MODE_PRIVATE)
        val systemNight = (resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK) == Configuration.UI_MODE_NIGHT_YES
        h = Native.create(applicationContext, forceSilentAudio, prefs.getInt("voice", 0))
        midi = MidiBus(this) { bytes -> if (h != 0L) Native.midiIn(h, bytes) }
        panel = Panel(this, h, midi, prefs.getBoolean("temple", systemNight), { savePrefs(); systemBars() }, ::pickMidiFile)
        setContentView(panel.scroll)
        panel.scroll.setOnApplyWindowInsetsListener { _, insets ->
            if (Build.VERSION.SDK_INT >= 30) {
                val b = insets.getInsets(WindowInsets.Type.systemBars() or WindowInsets.Type.displayCutout())
                panel.setInsets(b.left, b.top, b.right, b.bottom)
            } else {
                @Suppress("DEPRECATION")
                panel.setInsets(insets.systemWindowInsetLeft, insets.systemWindowInsetTop, insets.systemWindowInsetRight, insets.systemWindowInsetBottom)
            }
            insets
        }
        systemBars()
        frame()
        openIntent(intent)
    }

    override fun onStart() {
        super.onStart()
        if (h != 0L) Native.pause(h, false)
    }

    override fun onResume() {
        super.onResume()
        running = true
        if (!manualFrames) Choreographer.getInstance().postFrameCallback(this)
    }

    override fun onPause() {
        super.onPause()
        running = false
        // Don't leave a held note sounding behind a dialog or the recents screen.
        Native.gesture(h, G_RELEASE_ALL, 0, false)
        savePrefs()
    }

    override fun onStop() {
        super.onStop()
        // In the background: stop loops and give the audio device back.
        if (h != 0L) Native.pause(h, true)
    }

    override fun onDestroy() {
        super.onDestroy()
        midi.disable()
        Native.destroy(h)
        h = 0
    }

    override fun doFrame(frameTimeNanos: Long) {
        if (!running || h == 0L) return
        frame()
        Choreographer.getInstance().postFrameCallback(this)
    }

    /** One display frame (tests call it directly). */
    fun frame() {
        if (h != 0L) panel.frame()
    }

    /** Engine handle (tests). */
    fun handle() = h

    private fun savePrefs() {
        getPreferences(MODE_PRIVATE).edit()
            .putBoolean("temple", panel.night)
            .putInt("voice", Native.get(h, P_VOICE).toInt())
            .apply()
    }

    private fun systemBars() {
        window.decorView.setBackgroundColor(if (panel.night) Palette.NIGHT.bg else Palette.DAY.bg)
        if (Build.VERSION.SDK_INT >= 30) {
            val light = WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS or WindowInsetsController.APPEARANCE_LIGHT_NAVIGATION_BARS
            window.insetsController?.setSystemBarsAppearance(if (panel.night) 0 else light, light)
        }
    }

    // ------------------------------------------------------------------ keyboard

    private fun keyName(code: Int): String? = when (code) {
        in KeyEvent.KEYCODE_A..KeyEvent.KEYCODE_Z -> ('a' + (code - KeyEvent.KEYCODE_A)).toString()
        in KeyEvent.KEYCODE_1..KeyEvent.KEYCODE_4 -> ('1' + (code - KeyEvent.KEYCODE_1)).toString()
        KeyEvent.KEYCODE_SPACE -> "space"
        KeyEvent.KEYCODE_DPAD_UP, KeyEvent.KEYCODE_DPAD_RIGHT -> "up"
        KeyEvent.KEYCODE_DPAD_DOWN, KeyEvent.KEYCODE_DPAD_LEFT -> "down"
        else -> null
    }

    override fun onKeyDown(keyCode: Int, event: KeyEvent): Boolean {
        val name = keyName(keyCode)
        if (name != null && !event.isCtrlPressed && !event.isAltPressed && !event.isMetaPressed) {
            if (event.repeatCount == 0) Native.key(h, name, true)
            return true
        }
        return super.onKeyDown(keyCode, event)
    }

    override fun onKeyUp(keyCode: Int, event: KeyEvent): Boolean {
        val name = keyName(keyCode) ?: return super.onKeyUp(keyCode, event)
        Native.key(h, name, false)
        return true
    }

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (!hasFocus && h != 0L) Native.gesture(h, G_RELEASE_ALL, 0, false)
    }

    // ------------------------------------------------------------------ .mid files

    private fun pickMidiFile() {
        val i = Intent(Intent.ACTION_OPEN_DOCUMENT).addCategory(Intent.CATEGORY_OPENABLE).setType("*/*")
            .putExtra(Intent.EXTRA_MIME_TYPES, arrayOf("audio/midi", "audio/x-midi", "audio/mid", "application/x-midi"))
        @Suppress("DEPRECATION")
        runCatching { startActivityForResult(i, OPEN_MIDI) }.onFailure { panel.midiFileError("no file picker: ${it.message}") }
    }

    @Deprecated("Activity result API needs androidx; this app has no dependencies")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        @Suppress("DEPRECATION")
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode == OPEN_MIDI && resultCode == RESULT_OK) data?.data?.let { playMidi(it) }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        openIntent(intent)
    }

    private fun openIntent(i: Intent?) {
        if (i?.action == Intent.ACTION_VIEW) i.data?.let { playMidi(it) }
    }

    private fun playMidi(uri: Uri) {
        val name = runCatching {
            contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { c ->
                if (c.moveToFirst()) c.getString(0) else null
            }
        }.getOrNull() ?: uri.lastPathSegment ?: "clip"
        val bytes = runCatching {
            contentResolver.openInputStream(uri)?.use { s -> s.readBytes().takeIf { it.size <= 4 shl 20 } }
        }.getOrNull()
        val err = if (bytes == null) "$name: cannot read (or larger than 4 MB)" else Native.playMidi(h, bytes, name.substringBeforeLast('.'))
        if (err.isNotEmpty()) panel.midiFileError(err)
    }
}
