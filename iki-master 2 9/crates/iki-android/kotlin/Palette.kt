// GENERATED from crates/iki-core/src/shaku/design.rs — do not edit.
// Regenerate: cargo run -p iki-android --example gen_ids
package org.iki.shakuhachi

import kotlin.math.roundToInt

/** Chassis colours — "Breath" (day) and "Temple" (night); the same tokens as the desktop and plugin themes. */
class Palette private constructor(val night: Boolean) {
    val moss = 0xffe0b020.toInt()
    val mossFg = 0xff1a1406.toInt()
    val iris = 0xff8b6bb5.toInt()
    val irisFg = 0xfffaf7ff.toInt()
    val danger = 0xffc45c4a.toInt()
    val bg = if (night) 0xff16120c.toInt() else 0xfffbf8f0.toInt()
    val surface = if (night) 0xff1c1812.toInt() else 0xffffffff.toInt()
    val surface2 = if (night) 0xff241e16.toInt() else 0xfff4efe6.toInt()
    val fg = if (night) 0xfff3ead8.toInt() else 0xff2a2214.toInt()
    val muted = if (night) 0xffc4b090.toInt() else 0xff6e5a38.toInt()
    val subtle = if (night) 0xff9a8868.toInt() else 0xff8a7450.toInt()
    val border = if (night) 0xff3a3224.toInt() else 0xffe4d8c0.toInt()
    val hole = if (night) 0xff0e0c08.toInt() else 0xff3a3020.toInt()
    val lcd = if (night) 0xff14110c.toInt() else 0xfffffaf2.toInt()
    val metal = if (night) 0xff2a2418.toInt() else 0xffefe8dc.toInt()

    fun accent(iris: Boolean) = if (iris) this.iris else moss

    companion object {
        val DAY = Palette(false)
        val NIGHT = Palette(true)

        /** Linear blend of two ARGB colours, like the desktop `blend`. */
        fun blend(a: Int, b: Int, t: Float): Int {
            val k = t.coerceIn(0f, 1f)
            fun ch(shift: Int): Int {
                val x = (a ushr shift) and 0xff
                val y = (b ushr shift) and 0xff
                return (x + (y - x) * k).roundToInt() shl shift
            }
            return ch(24) or ch(16) or ch(8) or ch(0)
        }

        /** Scale the alpha (egui `gamma_multiply` on an opaque colour). */
        fun alpha(c: Int, a: Float): Int = ((((c ushr 24) and 0xff) * a).roundToInt() shl 24) or (c and 0xffffff)

        /** Motion timings in milliseconds, from the shared design tokens. */
        const val HOVER_MS = 80
        const val SWITCH_MS = 120
        const val KNOB_MS = 90
        const val HOLE_MS = 50
    }
}
