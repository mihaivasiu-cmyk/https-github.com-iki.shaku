// GENERATED from crates/iki-android/src/params.rs — do not edit.
// Regenerate: cargo run -p iki-android --example gen_ids
@file:Suppress("unused")

package org.iki.shakuhachi

// Gestures
const val G_HIT_START = 0
const val G_SLIDE_START = 1
const val G_SLIDE_MOVE = 2
const val G_SLIDE_END = 3
const val G_TOGGLE_GATE = 4
const val G_MURAIKI = 5
const val G_RELEASE_ALL = 6
const val G_FOLI_STEP = 7
const val G_PHRASE = 8

// Looper ops
const val L_PUNCH = 0
const val L_PLAY = 1
const val L_STOP = 2
const val L_STOP_ALL = 3
const val L_UNDO = 4
const val L_FREEZE = 5
const val L_LEVEL = 6
const val L_FX = 7
const val L_SELECT = 8
const val L_TRIM_IN = 9
const val L_TRIM_OUT = 10

// Texts
const val TEXT_KANA = 0
const val TEXT_NAME = 1
const val TEXT_PITCH = 2
const val TEXT_BACKEND = 3
const val TEXT_PROBLEM = 4
const val TEXT_STATUS = 5
const val TEXT_HOLES = 6
const val TEXT_PLAY = 7
const val TEXT_ENGINE = 8
const val TEXT_CLOCK = 9
const val TEXT_DELAY = 10
const val TEXT_LFO_HZ = 11
const val TEXT_BPM = 12
const val TEXT_CLIP = 13

// Curves
const val CURVE_AMP_ENV = 0
const val CURVE_FILT_ENV = 1
const val CURVE_FILTER = 2

// Continuous parameters (0…1)
const val P_MERI = 0
const val P_LEAN = 1
const val P_IKI = 2
const val P_GAIN = 3
const val P_DELAY_TIME = 4
const val P_DELAY_FEEDBACK = 5
const val P_DELAY_MIX = 6
const val P_TEMPO = 7
const val P_REVERB = 8
const val P_REVERB_DECAY = 9
const val P_PHASER = 10
const val P_PHASER_DEPTH = 11
const val P_TAPE = 12
const val P_WIDTH = 13
const val P_LFO_RATE = 14
const val P_LFO_PITCH_AMT = 15
const val P_LFO_FILT_AMT = 16
const val P_LFO_AMP_AMT = 17
const val P_FILT_CUT = 18
const val P_FILT_RES = 19
const val P_FILT_MOTION = 20
const val P_FILT_RATE = 21
const val P_ENV_A = 22
const val P_ENV_D = 23
const val P_ENV_S = 24
const val P_ENV_R = 25
const val P_FENV_A = 26
const val P_FENV_D = 27
const val P_FENV_S = 28
const val P_FENV_R = 29
const val P_FOLI_LEVEL = 30
const val P_FLUTE_LEVEL = 31
const val P_BORE_LEVEL = 32
const val P_BEND = 33
const val P_BEND_RANGE = 34
const val P_YURI = 35
const val P_YURI_RATE = 36
const val P_GLIDE = 37
const val P_TUNE = 38
const val P_DELAY_TONE = 50
const val P_DELAY_LOWCUT = 51
const val P_REVERB_SIZE = 52
const val P_REVERB_PRE = 53
const val P_REVERB_DAMP = 54

// Switches (0/1)
const val P_OTSU = 40
const val P_DELAY_PING = 41
const val P_DELAY_SYNC = 42
const val P_LFO_SYNC = 43
const val P_LFO_PITCH = 44
const val P_LFO_FILT = 45
const val P_LFO_AMP = 46
const val P_ENV_LOOP = 47
const val P_FENV_LOOP = 48
const val P_FOLI_ON = 49

// Choices (index)
const val P_FILT_TYPE = 60
const val P_FILT_AUTO = 61
const val P_ENV_SHAPE = 62
const val P_FENV_SHAPE = 63
const val P_LFO_WAVE = 64
const val P_DELAY_DIV = 65
const val P_LFO_DIV = 66
const val P_VOICE = 67
const val P_FOLI_FEEL = 68
const val P_FOLI_KIT = 69
const val P_REVERB_TYPE = 70

// Packed state indices
const val STATE_BLOWING = 0
const val STATE_LATCHED = 1
const val STATE_NOTE = 2
const val STATE_MERI_LANE = 3
const val STATE_KARI_NOTE = 4
const val STATE_MERI_NOTE = 5
const val STATE_OTSU = 6
const val STATE_HZ = 7
const val STATE_BREATH = 8
const val STATE_PEAK_L = 9
const val STATE_PEAK_R = 10
const val STATE_COMP_GR = 11
const val STATE_CUTOFF = 12
const val STATE_LATENCY_MS = 13
const val STATE_LOADED = 14
const val STATE_ORN_LANE = 15
const val STATE_ORN_INDEX = 16
const val STATE_ORN_KORO = 17
const val STATE_MURAIKI = 18
const val STATE_LOOP_TRACK = 19
const val STATE_BPM = 20
const val STATE_BAR_PHASE = 21
const val STATE_CYCLE_BARS = 22
const val STATE_PAD_ARMED = 23
const val STATE_FOLI_ON = 24
const val STATE_FOLI_STEP = 25
const val STATE_FOLI_MASK = 26
const val STATE_BORE = 27
const val STATE_PROBLEM = 28
const val STATE_PHASER_ROUTED = 29
const val STATE_CLOCK_LATE = 30
const val STATE_CLIP = 31
const val STATE_TRACKS = 32
const val TRACK_STRIDE = 10
const val STATE_LEN = 80

// Choice labels
val P_FILT_TYPE_LABELS = arrayOf("Lp", "Hp", "Bp", "Notch", "Peak", "LoSh", "HiSh", "All")
val P_FILT_AUTO_LABELS = arrayOf("Off", "Env", "Sweep")
val P_ENV_SHAPE_LABELS = arrayOf("Lin", "Exp", "Punch", "Trap", "Fold")
val P_FENV_SHAPE_LABELS = arrayOf("Lin", "Exp", "Punch", "Trap", "Fold")
val P_LFO_WAVE_LABELS = arrayOf("Sine", "Tri", "Saw", "Sqr")
val P_DELAY_DIV_LABELS = arrayOf("1/16", "1/8", "1/4", "1/2", "1", "2", "4", "6", "8")
val P_LFO_DIV_LABELS = arrayOf("1/16", "1/8", "1/4", "1/2", "1", "2", "4", "6", "8")
val P_VOICE_LABELS = arrayOf("Flute", "Bore", "Both")
val P_FOLI_FEEL_LABELS = arrayOf("Work", "Dance", "Pulse")
val P_FOLI_KIT_LABELS = arrayOf("90 Gm", "110 Cm", "110 Em", "140 Cm", "140 D", "160 Am", "160 Gm")
val P_REVERB_TYPE_LABELS = arrayOf("Custom", "Room", "Hall", "Plate", "Slap")
