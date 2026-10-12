//! Port of `src/lib/shaku/midi-bus.ts` on top of `midir` (ALSA / CoreMIDI / WinMM).
//!
//! Ports are classified like the web version (IAC driver, loopMIDI/LoopBe
//! virtual cables, hardware). Input can be filtered to one port or a class,
//! output goes to one port or fans out over up to four virtual "cables"
//! (each on its own MIDI channel), with thru and a 50 ms echo guard so a
//! loopback cable cannot feed the instrument its own notes.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use midir::{MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};

pub const CABLE_COUNT: usize = 4;
const CLIENT: &str = "Iki";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MidiKind {
    Iac,
    Loop,
    Hw,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MidiPortInfo {
    pub id: String,
    pub name: String,
    pub kind: MidiKind,
    pub short: String,
}

fn kind_of(name: &str) -> MidiKind {
    let n = name.to_lowercase();
    let iac = n.split(|c: char| !c.is_alphanumeric()).any(|w| w == "iac") || n.contains("iac driver");
    if iac {
        MidiKind::Iac
    } else if n.contains("loopmidi") || n.contains("loop midi") || n.contains("loopbe") {
        MidiKind::Loop
    } else {
        MidiKind::Hw
    }
}

fn bus_number(name: &str) -> Option<String> {
    let n = name.to_lowercase();
    let at = n.find("bus")?;
    let digits: String = n[at + 3..].trim_start().chars().take_while(|c| c.is_ascii_digit()).collect();
    (!digits.is_empty()).then_some(digits)
}

fn short_of(name: &str, kind: MidiKind) -> String {
    match kind {
        MidiKind::Iac => bus_number(name).map(|b| format!("IAC {b}")).unwrap_or_else(|| "IAC".into()),
        MidiKind::Loop => bus_number(name).map(|b| format!("Loop {b}")).unwrap_or_else(|| "Loop".into()),
        MidiKind::Hw => name.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(18).collect(),
    }
}

fn info(names: Vec<String>) -> Vec<MidiPortInfo> {
    let mut seen: HashMap<String, usize> = HashMap::new();
    names
        .into_iter()
        .map(|name| {
            let n = seen.entry(name.clone()).or_insert(0);
            *n += 1;
            let id = if *n > 1 { format!("{name} #{n}") } else { name.clone() };
            let kind = kind_of(&name);
            let short = short_of(&name, kind);
            MidiPortInfo { id, name, kind, short }
        })
        .collect()
}

fn in_ports() -> Vec<MidiPortInfo> {
    let Ok(mi) = MidiInput::new(CLIENT) else { return Vec::new() };
    info(mi.ports().iter().filter_map(|p| mi.port_name(p).ok()).collect())
}

fn out_ports() -> Vec<MidiPortInfo> {
    let Ok(mo) = MidiOutput::new(CLIENT) else { return Vec::new() };
    info(mo.ports().iter().filter_map(|p| mo.port_name(p).ok()).collect())
}

#[derive(Clone, Debug)]
pub struct MidiBusSnap {
    pub in_id: String,
    pub out_id: String,
    pub thru: bool,
    pub tx: bool,
    pub cables: [bool; CABLE_COUNT],
    pub cable_names: Vec<String>,
    pub ins: Vec<MidiPortInfo>,
    pub outs: Vec<MidiPortInfo>,
    pub activity: u64,
}

struct BusState {
    armed: bool,
    in_id: String,
    out_id: String,
    thru: bool,
    tx: bool,
    iac: bool,
    cables: [bool; CABLE_COUNT],
    cable_out: [Option<String>; CABLE_COUNT],
    echo: Vec<([u8; 3], Instant)>,
    activity: u64,
    ins: Vec<MidiPortInfo>,
    outs: Vec<MidiPortInfo>,
    conns: HashMap<String, MidiOutputConnection>,
}

impl BusState {
    fn accept(&self, port: &MidiPortInfo) -> bool {
        match self.in_id.as_str() {
            "all" => true,
            "iac" => port.kind == MidiKind::Iac,
            "loop" => port.kind == MidiKind::Loop,
            id => port.id == id,
        }
    }

    fn remember(&mut self, data: &[u8]) {
        self.echo.push((key_of(data), Instant::now()));
        if self.echo.len() > 48 {
            let drop = self.echo.len() - 48;
            self.echo.drain(..drop);
        }
    }

    fn is_echo(&mut self, data: &[u8]) -> bool {
        let now = Instant::now();
        self.echo.retain(|e| now.duration_since(e.1).as_millis() < 50);
        let k = key_of(data);
        self.echo.iter().any(|e| e.0 == k)
    }

    fn send_to(&mut self, id: Option<&str>, data: &[u8], tag_echo: bool) {
        let Some(id) = id else { return };
        if !self.armed || id == "none" || id.is_empty() {
            return;
        }
        if !self.conns.contains_key(id) {
            let Some(port) = self.outs.iter().position(|p| p.id == id) else { return };
            let Ok(mo) = MidiOutput::new(CLIENT) else { return };
            let ports = mo.ports();
            let Some(p) = ports.get(port) else { return };
            match mo.connect(p, "iki-out") {
                Ok(c) => {
                    self.conns.insert(id.to_string(), c);
                }
                Err(_) => return,
            }
        }
        if tag_echo {
            self.remember(data);
        }
        if let Some(c) = self.conns.get_mut(id)
            && c.send(data).is_ok()
        {
            self.activity += 1;
        }
    }

    fn map_cables(&mut self) {
        let iac: Vec<&MidiPortInfo> = self.outs.iter().filter(|p| p.kind == MidiKind::Iac).collect();
        let lp: Vec<&MidiPortInfo> = self.outs.iter().filter(|p| p.kind == MidiKind::Loop).collect();
        let pool = if iac.is_empty() { lp } else { iac };
        for i in 0..CABLE_COUNT {
            self.cable_out[i] = pool.get(i).or(pool.first()).map(|p| p.id.clone());
        }
    }

    fn prefer_virtual(&mut self) {
        let iac_out: Vec<String> = self.outs.iter().filter(|p| p.kind == MidiKind::Iac).map(|p| p.id.clone()).collect();
        let iac_in: Vec<String> = self.ins.iter().filter(|p| p.kind == MidiKind::Iac).map(|p| p.id.clone()).collect();
        let loop_out: Vec<String> =
            self.outs.iter().filter(|p| p.kind == MidiKind::Loop).map(|p| p.id.clone()).collect();
        if self.out_id == "none" {
            if iac_out.len() >= 2 {
                self.out_id = iac_out[1].clone();
            } else if let Some(o) = iac_out.first().or(loop_out.first()) {
                self.out_id = o.clone();
            }
        }
        if self.in_id == "all" && !iac_in.is_empty() && iac_out.len() >= 2 {
            self.in_id = iac_in[0].clone();
        }
    }
}

fn key_of(data: &[u8]) -> [u8; 3] {
    [data.first().copied().unwrap_or(0), data.get(1).copied().unwrap_or(0), data.get(2).copied().unwrap_or(0)]
}

pub type HwSink = Arc<dyn Fn(Vec<u8>) + Send + Sync>;

pub struct MidiBus {
    state: Arc<Mutex<BusState>>,
    inputs: Vec<MidiInputConnection<()>>,
    sink: Option<HwSink>,
}

impl Default for MidiBus {
    fn default() -> Self {
        Self::new()
    }
}

impl MidiBus {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(BusState {
                armed: false,
                in_id: "all".into(),
                out_id: "none".into(),
                thru: false,
                tx: true,
                iac: false,
                cables: [true, false, false, false],
                cable_out: [None, None, None, None],
                echo: Vec::new(),
                activity: 0,
                ins: Vec::new(),
                outs: Vec::new(),
                conns: HashMap::new(),
            })),
            inputs: Vec::new(),
            sink: None,
        }
    }

    /// `enableMidi` + `attachAccess`. Returns the status line the dock shows.
    pub fn enable(&mut self, sink: HwSink) -> Result<String, String> {
        MidiInput::new(CLIENT).map_err(|e| format!("MIDI unavailable: {e}"))?;
        self.disable();
        self.sink = Some(sink);
        self.rescan();
        let st = self.state.lock().unwrap();
        let lp = st.outs.iter().find(|p| p.kind == MidiKind::Loop);
        Ok(match (st.ins.first(), lp) {
            (Some(i), Some(l)) => format!("{} → {}", i.short, l.short),
            (Some(i), None) => i.name.clone(),
            (None, Some(l)) => l.name.clone(),
            (None, None) => "No ports".into(),
        })
    }

    /// Re-enumerate ports and rebind inputs (the web `onstatechange`).
    pub fn rescan(&mut self) {
        let Some(sink) = self.sink.clone() else { return };
        self.inputs.clear();
        let ins = in_ports();
        let outs = out_ports();
        {
            let mut st = self.state.lock().unwrap();
            st.armed = true;
            st.ins = ins.clone();
            st.outs = outs;
            st.conns.clear();
            st.prefer_virtual();
            st.map_cables();
            st.iac = st.ins.iter().any(|p| p.kind == MidiKind::Iac) || st.outs.iter().any(|p| p.kind == MidiKind::Iac);
        }
        for (idx, port) in ins.into_iter().enumerate() {
            let Ok(mi) = MidiInput::new(CLIENT) else { continue };
            let ports = mi.ports();
            let Some(p) = ports.get(idx) else { continue };
            let state = self.state.clone();
            let sink = sink.clone();
            let conn = mi.connect(
                p,
                "iki-in",
                move |_ts, data, _| {
                    let forward = {
                        let Ok(mut st) = state.lock() else { return };
                        if !st.accept(&port) || st.is_echo(data) {
                            false
                        } else {
                            st.activity += 1;
                            if st.thru {
                                let out = st.out_id.clone();
                                st.send_to(Some(&out), data, false);
                            }
                            true
                        }
                    };
                    if forward {
                        sink(data.to_vec());
                    }
                },
                (),
            );
            if let Ok(c) = conn {
                self.inputs.push(c);
            }
        }
    }

    /// `detachAccess`.
    pub fn disable(&mut self) {
        self.inputs.clear();
        self.sink = None;
        let mut st = self.state.lock().unwrap();
        st.armed = false;
        st.iac = false;
        st.conns.clear();
    }

    /// `sendApp`: instrument output, fanned out over the enabled cables.
    pub fn send_app(&self, data: &[u8]) {
        send_app_shared(&self.state, data);
    }

    /// A cloneable sender for other threads (the clip player).
    pub fn sender(&self) -> impl Fn(&[u8]) + Send + Sync + 'static {
        let st = self.state.clone();
        move |d: &[u8]| send_app_shared(&st, d)
    }

    pub fn set_in(&self, id: &str) {
        self.state.lock().unwrap().in_id = id.to_string();
    }
    pub fn set_out(&self, id: &str) {
        self.state.lock().unwrap().out_id = id.to_string();
    }
    pub fn set_thru(&self, on: bool) {
        self.state.lock().unwrap().thru = on;
    }
    pub fn set_tx(&self, on: bool) {
        self.state.lock().unwrap().tx = on;
    }
    pub fn set_cable(&self, i: usize, on: bool) {
        if i < CABLE_COUNT {
            self.state.lock().unwrap().cables[i] = on;
        }
    }

    pub fn snap(&self) -> MidiBusSnap {
        let st = self.state.lock().unwrap();
        let cable_names = (0..CABLE_COUNT)
            .map(|i| {
                st.cable_out[i]
                    .as_ref()
                    .and_then(|id| st.outs.iter().find(|o| &o.id == id))
                    .map(|p| p.short.clone())
                    .unwrap_or_else(|| format!("Bus {}", i + 1))
            })
            .collect();
        MidiBusSnap {
            in_id: st.in_id.clone(),
            out_id: st.out_id.clone(),
            thru: st.thru,
            tx: st.tx,
            cables: st.cables,
            cable_names,
            ins: st.ins.clone(),
            outs: st.outs.clone(),
            activity: st.activity,
        }
    }
}

fn send_app_shared(state: &Arc<Mutex<BusState>>, data: &[u8]) {
    let Ok(mut st) = state.lock() else { return };
    if !st.tx || data.is_empty() {
        return;
    }
    let status = data[0] & 0xf0;
    let mut any = false;
    for i in 0..CABLE_COUNT {
        if !st.cables[i] {
            continue;
        }
        let mut bytes = data.to_vec();
        if matches!(status, 0x80 | 0x90 | 0xb0 | 0xe0) {
            bytes[0] = status | i as u8;
        }
        let target = st.cable_out[i].clone().unwrap_or_else(|| st.out_id.clone());
        st.send_to(Some(&target), &bytes, true);
        any = true;
    }
    if !any {
        let out = st.out_id.clone();
        st.send_to(Some(&out), data, true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify() {
        assert_eq!(kind_of("IAC Driver Bus 2"), MidiKind::Iac);
        assert_eq!(short_of("IAC Driver Bus 2", MidiKind::Iac), "IAC 2");
        assert_eq!(kind_of("loopMIDI Port"), MidiKind::Loop);
        assert_eq!(kind_of("Midi Through Port-0"), MidiKind::Hw);
        assert_eq!(kind_of("Brilliac Synth"), MidiKind::Hw);
    }
}
