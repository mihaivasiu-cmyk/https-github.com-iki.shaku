//! A small re-implementation of the Web Audio `AudioParam` automation timeline.
//!
//! The web engine drives almost everything through `setTargetAtTime`,
//! `linearRampToValueAtTime`, `setValueCurveAtTime`, `setValueAtTime` and
//! `cancelScheduledValues`. Porting those semantics lets the engine logic stay
//! a near line-for-line translation. Evaluation is per sample, in order, and
//! allocation-free after construction.

use crate::shaku::env_shape::ENV_CURVE_N;

#[derive(Clone, Copy, Debug)]
enum Ev {
    Set {
        t: f64,
        v: f32,
    },
    /// Linear ramp that *ends* at `t`.
    Ramp {
        t: f64,
        v: f32,
    },
    Target {
        t: f64,
        v: f32,
        tau: f32,
    },
    Curve {
        t: f64,
        dur: f64,
    },
}

impl Ev {
    fn key(&self) -> f64 {
        match *self {
            Ev::Set { t, .. } | Ev::Ramp { t, .. } | Ev::Target { t, .. } | Ev::Curve { t, .. } => t,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Active {
    Hold,
    Ramp { v0: f32, t0: f64, v1: f32, t1: f64 },
    Target { v: f32, coef: f32 },
    Curve { t0: f64, dur: f64 },
}

/// Pending automation events per parameter; the queue is allocated at this
/// size up front and drops its oldest event instead of growing.
const MAX_EVENTS: usize = 64;

#[derive(Clone, Debug)]
pub struct Param {
    value: f32,
    events: Vec<Ev>,
    active: Active,
    last_t: f64,
    sr: f32,
    /// Only one value-curve can be pending/active at a time (the engine never schedules two).
    curve: [f32; ENV_CURVE_N],
    pending_curve: [f32; ENV_CURVE_N],
}

impl Param {
    pub fn new(value: f32, sr: f32) -> Self {
        Self {
            value,
            events: Vec::with_capacity(MAX_EVENTS),
            active: Active::Hold,
            last_t: 0.0,
            sr,
            curve: [0.0; ENV_CURVE_N],
            pending_curve: [0.0; ENV_CURVE_N],
        }
    }

    /// `param.value` (read).
    #[inline]
    pub fn value(&self) -> f32 {
        self.value
    }

    /// Back to a static `value` with nothing scheduled, keeping the event
    /// queue's storage (the engine reuses voices on its audio thread).
    pub fn reset(&mut self, value: f32, now: f64) {
        self.events.clear();
        self.set_now(value, now);
    }

    /// `param.value = v` — immediate, no automation event.
    pub fn set_now(&mut self, v: f32, now: f64) {
        self.value = v;
        self.active = Active::Hold;
        self.last_t = now;
    }

    fn insert(&mut self, ev: Ev) {
        if self.events.len() >= MAX_EVENTS {
            // Keep realtime-safe: drop the oldest event rather than grow unbounded.
            self.events.remove(0);
        }
        let k = ev.key();
        let pos = self.events.iter().position(|e| e.key() > k).unwrap_or(self.events.len());
        self.events.insert(pos, ev);
    }

    pub fn set_value_at(&mut self, v: f32, t: f64) {
        self.insert(Ev::Set { t, v });
    }

    pub fn linear_ramp_to(&mut self, v: f32, t: f64) {
        self.insert(Ev::Ramp { t, v });
    }

    pub fn set_target_at(&mut self, v: f32, t: f64, tau: f32) {
        self.insert(Ev::Target { t, v, tau: tau.max(1e-5) });
    }

    pub fn set_value_curve_at(&mut self, curve: &[f32; ENV_CURVE_N], t: f64, dur: f64) {
        self.pending_curve = *curve;
        self.insert(Ev::Curve { t, dur: dur.max(1e-4) });
    }

    /// `cancelScheduledValues(t)`: drop every event at or after `t`, freezing any
    /// running ramp/curve that would have ended after `t`.
    pub fn cancel(&mut self, t: f64) {
        self.events.retain(|e| e.key() < t);
        match self.active {
            Active::Ramp { t1, .. } if t1 >= t => self.active = Active::Hold,
            Active::Curve { t0, dur } if t0 + dur >= t => self.active = Active::Hold,
            _ => {}
        }
    }

    #[inline]
    fn blocking(&self) -> bool {
        matches!(self.active, Active::Ramp { .. } | Active::Curve { .. })
    }

    /// Advance to time `t` (seconds) and return the value there. Call once per sample.
    #[inline]
    pub fn tick(&mut self, t: f64) -> f32 {
        if self.events.is_empty() {
            match self.active {
                Active::Hold => return self.value,
                Active::Target { v, coef } => {
                    self.value += (v - self.value) * coef;
                    return self.value;
                }
                _ => {}
            }
        }
        self.tick_slow(t)
    }

    fn tick_slow(&mut self, t: f64) -> f32 {
        loop {
            // Finish a blocking segment whose end has passed.
            match self.active {
                Active::Ramp { v1, t1, .. } if t >= t1 => {
                    self.value = v1;
                    self.last_t = t1;
                    self.active = Active::Hold;
                }
                Active::Curve { t0, dur } if t >= t0 + dur => {
                    self.value = self.curve[ENV_CURVE_N - 1];
                    self.last_t = t0 + dur;
                    self.active = Active::Hold;
                }
                _ => {}
            }
            let Some(&ev) = self.events.first() else { break };
            let due = match ev {
                Ev::Ramp { .. } => !self.blocking(),
                other => other.key() <= t,
            };
            if !due {
                break;
            }
            self.events.remove(0);
            match ev {
                Ev::Set { t: t0, v } => {
                    self.value = v;
                    self.active = Active::Hold;
                    self.last_t = t0;
                }
                Ev::Target { t: t0, v, tau } => {
                    let coef = 1.0 - (-1.0 / (tau * self.sr)).exp();
                    self.active = Active::Target { v, coef };
                    self.last_t = t0;
                }
                Ev::Curve { t: t0, dur } => {
                    self.curve = self.pending_curve;
                    self.active = Active::Curve { t0, dur };
                    self.last_t = t0;
                }
                Ev::Ramp { t: t1, v } => {
                    let t0 = self.last_t.min(t);
                    if t1 <= t0 {
                        self.value = v;
                        self.last_t = t1.max(t0);
                        self.active = Active::Hold;
                    } else {
                        self.active = Active::Ramp { v0: self.value, t0, v1: v, t1 };
                    }
                }
            }
        }
        match self.active {
            Active::Hold => {}
            Active::Target { v, coef } => self.value += (v - self.value) * coef,
            Active::Ramp { v0, t0, v1, t1 } => {
                let f = ((t - t0) / (t1 - t0)).clamp(0.0, 1.0) as f32;
                self.value = v0 + (v1 - v0) * f;
            }
            Active::Curve { t0, dur } => {
                if t >= t0 {
                    let x = ((t - t0) / dur).clamp(0.0, 1.0) * (ENV_CURVE_N - 1) as f64;
                    let i = (x as usize).min(ENV_CURVE_N - 2);
                    let f = (x - i as f64) as f32;
                    self.value = self.curve[i] + (self.curve[i + 1] - self.curve[i]) * f;
                }
            }
        }
        self.value
    }

    /// True when the value is static (no pending events and no moving segment).
    pub fn is_idle(&self) -> bool {
        self.events.is_empty() && matches!(self.active, Active::Hold)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(p: &mut Param, sr: f32, from: f64, secs: f64) -> f32 {
        let n = (secs * sr as f64) as usize;
        let mut v = 0.0;
        for i in 0..n {
            v = p.tick(from + i as f64 / sr as f64);
        }
        v
    }

    #[test]
    fn target_converges() {
        let sr = 48000.0;
        let mut p = Param::new(0.0, sr);
        p.set_target_at(1.0, 0.0, 0.1);
        let v = run(&mut p, sr, 0.0, 0.1);
        assert!((v - (1.0 - (-1.0f32).exp())).abs() < 0.01, "{v}");
    }

    #[test]
    fn ramp_chain() {
        let sr = 1000.0;
        let mut p = Param::new(0.5, sr);
        p.cancel(0.0);
        p.set_value_at(0.5, 0.0);
        p.linear_ramp_to(1.0, 0.1);
        p.linear_ramp_to(0.0, 0.2);
        assert!((run(&mut p, sr, 0.0, 0.05) - 0.75).abs() < 0.02);
        assert!((run(&mut p, sr, 0.05, 0.1) - 0.5).abs() < 0.03);
        assert!(run(&mut p, sr, 0.15, 0.1).abs() < 1e-6);
    }

    #[test]
    fn curve_holds_last() {
        let sr = 1000.0;
        let mut p = Param::new(0.0, sr);
        let mut c = [0f32; ENV_CURVE_N];
        for (i, x) in c.iter_mut().enumerate() {
            *x = i as f32 / (ENV_CURVE_N - 1) as f32;
        }
        p.set_value_curve_at(&c, 0.0, 0.1);
        assert!((run(&mut p, sr, 0.0, 0.05) - 0.49).abs() < 0.03);
        assert!((run(&mut p, sr, 0.05, 0.2) - 1.0).abs() < 1e-6);
    }
}
