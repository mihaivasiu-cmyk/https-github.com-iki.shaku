//! Worst-case realtime factor: every effect on, loops playing, voice blowing.
use iki_core::audio::render_offline;
use iki_core::engine::{Cmd, Engine, FxSet};
fn main() {
    let mut e = Engine::new(48000.0);
    e.handle(Cmd::Bank(iki_core::shaku::samples::load_bank().unwrap()));
    for s in [
        FxSet::Tape(1.0),
        FxSet::Phaser(1.0),
        FxSet::DelayMix(1.0),
        FxSet::DelayPing(true),
        FxSet::Reverb(1.0),
        FxSet::LfoPitch(true),
        FxSet::LfoFilt(true),
        FxSet::LfoAmp(true),
        FxSet::EnvLoop(true),
        FxSet::FenvLoop(true),
    ] {
        e.handle(Cmd::Fx(s));
    }
    for t in 0..3 {
        e.handle(Cmd::LoopFx(t, true));
    }
    e.handle(Cmd::LoopPunch(0));
    e.handle(Cmd::Blow(true));
    let mut out = Vec::new();
    render_offline(&mut e, 3.5, &mut out);
    e.handle(Cmd::LoopPunch(0));
    out.clear();
    let t0 = std::time::Instant::now();
    render_offline(&mut e, 20.0, &mut out);
    let el = t0.elapsed().as_secs_f64();
    println!(
        "20 s of worst-case audio in {:.3} s → {:.0}× realtime ({:.2}% of one core)",
        el,
        20.0 / el,
        el / 20.0 * 100.0
    );
}
