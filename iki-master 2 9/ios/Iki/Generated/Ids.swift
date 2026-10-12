// GENERATED from crates/iki-android/src/params.rs — do not edit.
// Regenerate: cargo run -p iki-android --example gen_ids
// Ids are Int32 to match the C ABI; STATE_* are Int (array indices).

// Gestures
let G_HIT_START: Int32 = 0
let G_SLIDE_START: Int32 = 1
let G_SLIDE_MOVE: Int32 = 2
let G_SLIDE_END: Int32 = 3
let G_TOGGLE_GATE: Int32 = 4
let G_MURAIKI: Int32 = 5
let G_RELEASE_ALL: Int32 = 6
let G_FOLI_STEP: Int32 = 7
let G_PHRASE: Int32 = 8

// Looper ops
let L_PUNCH: Int32 = 0
let L_PLAY: Int32 = 1
let L_STOP: Int32 = 2
let L_STOP_ALL: Int32 = 3
let L_UNDO: Int32 = 4
let L_FREEZE: Int32 = 5
let L_LEVEL: Int32 = 6
let L_FX: Int32 = 7
let L_SELECT: Int32 = 8
let L_TRIM_IN: Int32 = 9
let L_TRIM_OUT: Int32 = 10

// Texts
let TEXT_KANA: Int32 = 0
let TEXT_NAME: Int32 = 1
let TEXT_PITCH: Int32 = 2
let TEXT_BACKEND: Int32 = 3
let TEXT_PROBLEM: Int32 = 4
let TEXT_STATUS: Int32 = 5
let TEXT_HOLES: Int32 = 6
let TEXT_PLAY: Int32 = 7
let TEXT_ENGINE: Int32 = 8
let TEXT_CLOCK: Int32 = 9
let TEXT_DELAY: Int32 = 10
let TEXT_LFO_HZ: Int32 = 11
let TEXT_BPM: Int32 = 12
let TEXT_CLIP: Int32 = 13

// Curves
let CURVE_AMP_ENV: Int32 = 0
let CURVE_FILT_ENV: Int32 = 1
let CURVE_FILTER: Int32 = 2

// Continuous parameters (0…1)
let P_MERI: Int32 = 0
let P_LEAN: Int32 = 1
let P_IKI: Int32 = 2
let P_GAIN: Int32 = 3
let P_DELAY_TIME: Int32 = 4
let P_DELAY_FEEDBACK: Int32 = 5
let P_DELAY_MIX: Int32 = 6
let P_TEMPO: Int32 = 7
let P_REVERB: Int32 = 8
let P_REVERB_DECAY: Int32 = 9
let P_PHASER: Int32 = 10
let P_PHASER_DEPTH: Int32 = 11
let P_TAPE: Int32 = 12
let P_WIDTH: Int32 = 13
let P_LFO_RATE: Int32 = 14
let P_LFO_PITCH_AMT: Int32 = 15
let P_LFO_FILT_AMT: Int32 = 16
let P_LFO_AMP_AMT: Int32 = 17
let P_FILT_CUT: Int32 = 18
let P_FILT_RES: Int32 = 19
let P_FILT_MOTION: Int32 = 20
let P_FILT_RATE: Int32 = 21
let P_ENV_A: Int32 = 22
let P_ENV_D: Int32 = 23
let P_ENV_S: Int32 = 24
let P_ENV_R: Int32 = 25
let P_FENV_A: Int32 = 26
let P_FENV_D: Int32 = 27
let P_FENV_S: Int32 = 28
let P_FENV_R: Int32 = 29
let P_FOLI_LEVEL: Int32 = 30
let P_FLUTE_LEVEL: Int32 = 31
let P_BORE_LEVEL: Int32 = 32
let P_BEND: Int32 = 33
let P_BEND_RANGE: Int32 = 34
let P_YURI: Int32 = 35
let P_YURI_RATE: Int32 = 36
let P_GLIDE: Int32 = 37
let P_TUNE: Int32 = 38
let P_DELAY_TONE: Int32 = 50
let P_DELAY_LOWCUT: Int32 = 51
let P_REVERB_SIZE: Int32 = 52
let P_REVERB_PRE: Int32 = 53
let P_REVERB_DAMP: Int32 = 54

// Switches (0/1)
let P_OTSU: Int32 = 40
let P_DELAY_PING: Int32 = 41
let P_DELAY_SYNC: Int32 = 42
let P_LFO_SYNC: Int32 = 43
let P_LFO_PITCH: Int32 = 44
let P_LFO_FILT: Int32 = 45
let P_LFO_AMP: Int32 = 46
let P_ENV_LOOP: Int32 = 47
let P_FENV_LOOP: Int32 = 48
let P_FOLI_ON: Int32 = 49

// Choices (index)
let P_FILT_TYPE: Int32 = 60
let P_FILT_AUTO: Int32 = 61
let P_ENV_SHAPE: Int32 = 62
let P_FENV_SHAPE: Int32 = 63
let P_LFO_WAVE: Int32 = 64
let P_DELAY_DIV: Int32 = 65
let P_LFO_DIV: Int32 = 66
let P_VOICE: Int32 = 67
let P_FOLI_FEEL: Int32 = 68
let P_FOLI_KIT: Int32 = 69
let P_REVERB_TYPE: Int32 = 70

// Packed state indices
let STATE_BLOWING: Int = 0
let STATE_LATCHED: Int = 1
let STATE_NOTE: Int = 2
let STATE_MERI_LANE: Int = 3
let STATE_KARI_NOTE: Int = 4
let STATE_MERI_NOTE: Int = 5
let STATE_OTSU: Int = 6
let STATE_HZ: Int = 7
let STATE_BREATH: Int = 8
let STATE_PEAK_L: Int = 9
let STATE_PEAK_R: Int = 10
let STATE_COMP_GR: Int = 11
let STATE_CUTOFF: Int = 12
let STATE_LATENCY_MS: Int = 13
let STATE_LOADED: Int = 14
let STATE_ORN_LANE: Int = 15
let STATE_ORN_INDEX: Int = 16
let STATE_ORN_KORO: Int = 17
let STATE_MURAIKI: Int = 18
let STATE_LOOP_TRACK: Int = 19
let STATE_BPM: Int = 20
let STATE_BAR_PHASE: Int = 21
let STATE_CYCLE_BARS: Int = 22
let STATE_PAD_ARMED: Int = 23
let STATE_FOLI_ON: Int = 24
let STATE_FOLI_STEP: Int = 25
let STATE_FOLI_MASK: Int = 26
let STATE_BORE: Int = 27
let STATE_PROBLEM: Int = 28
let STATE_PHASER_ROUTED: Int = 29
let STATE_CLOCK_LATE: Int = 30
let STATE_CLIP: Int = 31
let STATE_TRACKS: Int = 32
let TRACK_STRIDE: Int = 10
let STATE_LEN: Int = 80

// Choice labels
let P_FILT_TYPE_LABELS: [String] = ["Lp", "Hp", "Bp", "Notch", "Peak", "LoSh", "HiSh", "All"]
let P_FILT_AUTO_LABELS: [String] = ["Off", "Env", "Sweep"]
let P_ENV_SHAPE_LABELS: [String] = ["Lin", "Exp", "Punch", "Trap", "Fold"]
let P_FENV_SHAPE_LABELS: [String] = ["Lin", "Exp", "Punch", "Trap", "Fold"]
let P_LFO_WAVE_LABELS: [String] = ["Sine", "Tri", "Saw", "Sqr"]
let P_DELAY_DIV_LABELS: [String] = ["1/16", "1/8", "1/4", "1/2", "1", "2", "4", "6", "8"]
let P_LFO_DIV_LABELS: [String] = ["1/16", "1/8", "1/4", "1/2", "1", "2", "4", "6", "8"]
let P_VOICE_LABELS: [String] = ["Flute", "Bore", "Both"]
let P_FOLI_FEEL_LABELS: [String] = ["Work", "Dance", "Pulse"]
let P_FOLI_KIT_LABELS: [String] = ["90 Gm", "110 Cm", "110 Em", "140 Cm", "140 D", "160 Am", "160 Gm"]
let P_REVERB_TYPE_LABELS: [String] = ["Custom", "Room", "Hall", "Plate", "Slap"]
