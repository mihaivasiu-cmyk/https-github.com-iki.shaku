package org.iki.shakuhachi

import android.content.Context
import android.content.pm.PackageManager
import android.media.midi.MidiDevice
import android.media.midi.MidiDeviceInfo
import android.media.midi.MidiInputPort
import android.media.midi.MidiManager
import android.media.midi.MidiReceiver
import android.os.Handler
import android.os.Looper
import java.util.concurrent.atomic.AtomicLong

/**
 * The Android side of the desktop `midi_bus.rs`: USB / Bluetooth / virtual MIDI
 * devices through `android.media.midi`. Input from all ports or one port, one
 * output port, thru, Tx echo, and the same 50 ms echo guard so a loopback cannot
 * feed the instrument its own notes. (The desktop's four IAC/loopMIDI "cables"
 * are an OS feature Android doesn't have.)
 *
 * [sink] receives complete messages on the main thread.
 */
class MidiBus(context: Context, private val sink: (ByteArray) -> Unit) {
    data class Port(val id: String, val name: String)

    private val manager: MidiManager? =
        if (context.packageManager.hasSystemFeature(PackageManager.FEATURE_MIDI)) {
            context.getSystemService(MidiManager::class.java)
        } else {
            null
        }
    private val main = Handler(Looper.getMainLooper())
    private val devices = HashMap<Int, MidiDevice>()
    private val closers = ArrayList<AutoCloseable>()
    @Volatile private var out: MidiInputPort? = null
    private val echo = ArrayDeque<Pair<Int, Long>>()

    val available get() = manager != null
    var armed = false
        private set
    var ins: List<Port> = emptyList()
        private set
    var outs: List<Port> = emptyList()
        private set
    @Volatile var inId = "all"
        private set
    var outId = "none"
        private set
    @Volatile var thru = false
    var tx = true
    val activity = AtomicLong()
    /** The dock's status text: device names, or why MIDI is off. */
    var device = "MIDI"
        private set
    var error = false
        private set
    var onChange: (() -> Unit)? = null

    private val callback = object : MidiManager.DeviceCallback() {
        override fun onDeviceAdded(d: MidiDeviceInfo) = rescan()
        override fun onDeviceRemoved(d: MidiDeviceInfo) = rescan()
    }

    @Suppress("DEPRECATION") // getDevices(): getDevicesForTransport needs API 33
    private fun infos(): List<MidiDeviceInfo> = manager?.devices?.toList() ?: emptyList()

    private fun name(d: MidiDeviceInfo) =
        d.properties.getString(MidiDeviceInfo.PROPERTY_NAME)
            ?: d.properties.getString(MidiDeviceInfo.PROPERTY_PRODUCT)
            ?: "MIDI ${d.id}"

    private fun portName(d: MidiDeviceInfo, p: MidiDeviceInfo.PortInfo): String {
        val base = name(d)
        val same = d.ports.count { it.type == p.type } > 1
        return if (same) "$base ${p.name.ifEmpty { "${p.portNumber + 1}" }}" else base
    }

    /** `enableMidi`: open every device, listen on the chosen inputs. */
    fun enable() {
        val m = manager
        if (m == null) {
            error = true
            device = "MIDI unavailable on this device"
            onChange?.invoke()
            return
        }
        disable()
        armed = true
        error = false
        m.registerDeviceCallback(callback, main)
        rescan()
    }

    fun disable() {
        if (armed) manager?.unregisterDeviceCallback(callback)
        armed = false
        closeAll()
        device = "MIDI"
        onChange?.invoke()
    }

    private fun closeAll() {
        for (c in closers) runCatching { c.close() }
        closers.clear()
        out = null
        for (d in devices.values) runCatching { d.close() }
        devices.clear()
    }

    /** Re-enumerate ports and rebind (hot-plug, or the Rescan button). */
    fun rescan() {
        if (!armed) return
        val m = manager ?: return
        closeAll()
        val list = infos()
        ins = list.flatMap { d ->
            d.ports.filter { it.type == MidiDeviceInfo.PortInfo.TYPE_OUTPUT }.map { Port("${d.id}:${it.portNumber}", portName(d, it)) }
        }
        outs = list.flatMap { d ->
            d.ports.filter { it.type == MidiDeviceInfo.PortInfo.TYPE_INPUT }.map { Port("${d.id}:${it.portNumber}", portName(d, it)) }
        }
        if (inId != "all" && ins.none { it.id == inId }) inId = "all"
        if (outId != "none" && outs.none { it.id == outId }) outId = "none"
        device = when {
            ins.isNotEmpty() && outId != "none" -> "${ins.first().name} → ${outs.first { it.id == outId }.name}"
            ins.isNotEmpty() -> ins.first().name
            outs.isNotEmpty() -> outs.first().name
            else -> "No ports"
        }
        for (d in list) {
            m.openDevice(d, { dev ->
                if (dev == null || !armed) {
                    runCatching { dev?.close() }
                    return@openDevice
                }
                devices[d.id] = dev
                for (p in d.ports) {
                    val id = "${d.id}:${p.portNumber}"
                    if (p.type == MidiDeviceInfo.PortInfo.TYPE_OUTPUT) {
                        val port = dev.openOutputPort(p.portNumber) ?: continue
                        port.connect(Receiver(id))
                        closers.add(port)
                    } else if (id == outId) {
                        val port = dev.openInputPort(p.portNumber) ?: continue
                        out = port
                        closers.add(port)
                    }
                }
            }, main)
        }
        onChange?.invoke()
    }

    fun setIn(id: String) {
        inId = id
        onChange?.invoke()
    }

    fun setOut(id: String) {
        outId = id
        rescan()
    }

    /** `sendApp`: instrument output (Tx echo and clip notes; already gated by Tx). */
    fun send(bytes: ByteArray) {
        val port = out ?: return
        if (bytes.isEmpty()) return
        var i = 0
        synchronized(echo) {
            while (i + 2 < bytes.size) {
                remember(bytes, i)
                i += 3
            }
        }
        runCatching { port.send(bytes, 0, bytes.size) }
        activity.incrementAndGet()
    }

    private fun key(b: ByteArray, o: Int, n: Int): Int {
        var k = 0
        for (j in 0 until 3) k = (k shl 8) or (if (j < n) b[o + j].toInt() and 0xff else 0)
        return k
    }

    private fun remember(b: ByteArray, o: Int) {
        echo.addLast(key(b, o, 3) to System.nanoTime())
        while (echo.size > 48) echo.removeFirst()
    }

    private fun isEcho(b: ByteArray, o: Int, n: Int): Boolean = synchronized(echo) {
        val now = System.nanoTime()
        while (echo.isNotEmpty() && now - echo.first().second > 50_000_000L) echo.removeFirst()
        val k = key(b, o, n)
        echo.any { it.first == k }
    }

    /** Splits a buffer into complete channel messages (running status; skips realtime / sysex). */
    private inner class Receiver(private val port: String) : MidiReceiver() {
        private var status = 0
        private val msg = ByteArray(3)
        private var have = 0
        private var sysex = false

        override fun onSend(data: ByteArray, offset: Int, count: Int, timestamp: Long) {
            if (inId != "all" && inId != port) return
            for (i in offset until offset + count) {
                val b = data[i].toInt() and 0xff
                when {
                    b >= 0xf8 -> continue // realtime
                    b == 0xf0 -> sysex = true
                    b == 0xf7 -> sysex = false
                    b >= 0xf0 -> { // system common: drop and clear running status
                        status = 0
                        have = 0
                    }
                    b >= 0x80 -> {
                        sysex = false
                        status = b
                        msg[0] = b.toByte()
                        have = 1
                    }
                    sysex || status == 0 -> continue
                    else -> {
                        if (have == 0) {
                            msg[0] = status.toByte()
                            have = 1
                        }
                        msg[have++] = b.toByte()
                        val need = if (status and 0xe0 == 0xc0) 2 else 3 // program change / aftertouch: 1 data byte
                        if (have == need) {
                            deliver(msg.copyOf(need))
                            have = 0
                        }
                    }
                }
            }
        }

        private fun deliver(m: ByteArray) {
            if (isEcho(m, 0, m.size)) return
            activity.incrementAndGet()
            if (thru) out?.let { runCatching { it.send(m, 0, m.size) } }
            main.post { sink(m) }
        }
    }
}
