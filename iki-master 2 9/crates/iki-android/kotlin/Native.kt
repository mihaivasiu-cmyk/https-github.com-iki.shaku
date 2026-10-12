package org.iki.shakuhachi

import android.content.Context

/**
 * JNI surface of the Rust kernel (`src/lib.rs` in this crate). One handle per
 * engine; every behaviour — gate, ornaments, legato, looper, envelopes, MIDI —
 * and every readout string lives in Rust. Ids are in `Ids.kt` (generated).
 */
object Native {
    init {
        System.loadLibrary("iki")
    }

    /** Starts audio (AAudio) and the controller. [silent] runs on a mute clock; [voice] 0 sampler, 1 bore, 2 both. */
    @JvmStatic external fun create(context: Context?, silent: Boolean, voice: Int): Long

    @JvmStatic external fun destroy(h: Long)

    /** App in the background: release held notes, stop loops, let go of the audio device. */
    @JvmStatic external fun pause(h: Long, paused: Boolean)

    /** Once per display frame: loads samples, pulls the engine state, pushes changed parameters. */
    @JvmStatic external fun tick(h: Long)

    @JvmStatic external fun gesture(h: Long, gesture: Int, index: Int, meriLane: Boolean)

    @JvmStatic external fun loop(h: Long, op: Int, track: Int, value: Double)

    @JvmStatic external fun set(h: Long, param: Int, value: Double)

    @JvmStatic external fun get(h: Long, param: Int): Double

    /** Readout under a knob ("64 ms", "676 Hz", "72 bpm" …). */
    @JvmStatic external fun hint(h: Long, param: Int): String

    @JvmStatic external fun text(h: Long, which: Int): String

    /** Fills [out] with [STATE_LEN] packed values. */
    @JvmStatic external fun state(h: Long, out: FloatArray)

    /** One spectrogram column: 72 rows of L then 72 of R, top = high, 0…1. */
    @JvmStatic external fun spectrum(h: Long, out: FloatArray)

    /** Envelope / filter curve; the last element is the guide line (sustain, 0 dB). */
    @JvmStatic external fun curve(h: Long, which: Int, out: FloatArray)

    /** Loop waveform peaks; returns the number of bins written (0 = empty track). */
    @JvmStatic external fun loopWave(h: Long, track: Int, out: FloatArray): Int

    /** Hardware keyboard: US key name ("a", "space", "up", "1"…). Returns true if it is a shortcut. */
    @JvmStatic external fun key(h: Long, name: String, down: Boolean): Boolean

    /** Raw bytes from a MIDI input. */
    @JvmStatic external fun midiIn(h: Long, bytes: ByteArray)

    /** Bytes for the MIDI output this frame: the Tx echo of what is played plus clip notes. Call every frame. */
    @JvmStatic external fun midiOut(h: Long, tx: Boolean): ByteArray

    /** Plays a Standard MIDI File through the instrument; returns "" or the parse error. */
    @JvmStatic external fun playMidi(h: Long, bytes: ByteArray, name: String): String

    @JvmStatic external fun stateLen(): Int
}
