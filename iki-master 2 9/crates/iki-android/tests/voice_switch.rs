use iki::*;
use std::time::Duration;

fn run(i: &mut Iki, ms: u64) -> f32 {
    let mut peak = 0f32;
    for _ in 0..ms / 16 {
        std::thread::sleep(Duration::from_millis(16));
        i.tick();
        peak = peak.max(i.ctl.snap.peak_l);
    }
    i.ctl.snap.peak_l
}

#[test]
fn toggling_voice_keeps_audio() {
    let mut i = Iki::new(true, iki_core::engine::VoiceMode::Sampler);
    run(&mut i, 400); // samples load
    let hit = |i: &mut Iki, down: bool| {
        if down { i.ctl.on_hit_start(0, iki_core::shaku::fingering::Voicing::Kari) } else { i.ctl.on_slide_end() }
    };
    hit(&mut i, true);
    println!("sampler: {}", run(&mut i, 600));
    hit(&mut i, false);
    run(&mut i, 1500);
    set_param(&mut i.ctl, P_VOICE, 1.0);
    hit(&mut i, true);
    println!("bore: {}", run(&mut i, 900));
    hit(&mut i, false);
    run(&mut i, 1500);
    set_param(&mut i.ctl, P_VOICE, 2.0);
    hit(&mut i, true);
    println!("both: {}", run(&mut i, 900));
    hit(&mut i, false);
    run(&mut i, 1500);
    set_param(&mut i.ctl, P_VOICE, 0.0);
    hit(&mut i, true);
    println!("sampler again: {}", run(&mut i, 600));
}
