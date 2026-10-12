//! The audio-thread path must never allocate (hosts run it on a realtime
//! thread). Counts allocations made by this thread while playing blocks with
//! MIDI, parameter changes, host clock and editor gestures.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use iki_core::params::*;
use iki_core::shaku::fingering::Voicing;
use iki_plugin::instrument::{Gesture, Instrument};

struct Counting;

thread_local! {
    static COUNT: Cell<Option<usize>> = const { Cell::new(None) };
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        // Counting is paused while reporting, so the report's own allocations don't count.
        let counted = COUNT.try_with(|c| c.take()).ok().flatten();
        if let Some(n) = counted {
            if n < 3 {
                eprintln!(
                    "allocation #{n} ({} bytes) on the audio path:\n{}",
                    l.size(),
                    std::backtrace::Backtrace::force_capture()
                );
            }
            COUNT.with(|c| c.set(Some(n + 1)));
        }
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
}

#[global_allocator]
static A: Counting = Counting;

fn play(inst: &mut Instrument, t: usize, beat: &mut f64) {
    let (mut l, mut r) = ([0f32; 256], [0f32; 256]);
    inst.param(P_FILT_CUT, 0.3 + (t % 7) as f64 * 0.05);
    inst.param(P_VOICE, ((t / 40) % 3) as f64);
    inst.param(P_FOLI_ON, 1.0);
    inst.param(P_DELAY_SYNC, 1.0);
    inst.host_clock(Some(120.0), Some(*beat), true);
    match t % 24 {
        0 => inst.midi(&[0x90, 62 + (t % 5) as u8 * 2, 90]),
        3 => inst.midi(&[0xe0, 0, (t % 64) as u8]),
        5 => inst.midi(&[0xb0, 1, 70]),
        6 => inst.midi(&[0xb0, 64, 127]),
        9 => inst.midi(&[0x80, 62 + (t % 5) as u8 * 2, 0]),
        10 => inst.midi(&[0xb0, 64, 0]),
        12 => inst.gesture(Gesture::HitStart(3, Voicing::Kari)),
        14 => inst.gesture(Gesture::HitStart(4, Voicing::Meri)),
        16 => inst.gesture(Gesture::SlideEnd),
        18 => inst.gesture(Gesture::ToggleGate),
        20 => inst.gesture(Gesture::Muraiki),
        22 => inst.gesture(Gesture::ToggleGate),
        _ => {}
    }
    inst.render(&mut l[..100], &mut r[..100]);
    inst.render(&mut l[100..], &mut r[100..]);
    inst.end_block();
    let _ = inst.perf();
    *beat += 256.0 / 48000.0 * 2.0;
}

#[test]
fn process_path_does_not_allocate() {
    let (mut inst, _scope) = Instrument::new(48000.0, 256);
    let mut beat = 0.0;
    for t in 0..400 {
        play(&mut inst, t, &mut beat); // warm-up: first-time growth is allowed
    }
    COUNT.with(|c| c.set(Some(0)));
    for t in 400..2400 {
        play(&mut inst, t, &mut beat);
    }
    let n = COUNT.with(|c| c.replace(None)).unwrap_or(0);
    assert_eq!(n, 0, "allocations on the audio path");
}
