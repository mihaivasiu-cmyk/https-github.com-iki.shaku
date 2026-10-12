//! MIDI dock (`midi-dock.tsx`): arm/disarm, status, .mid playback, virtual
//! cables, input/output routing, thru and tx.

use eframe::egui::{self, RichText, Ui, Vec2};

use super::app::{App, MidiStatus};
use super::widgets::{button, led, module};
use crate::midi_bus::{CABLE_COUNT, MidiKind};

impl App {
    pub fn midi_dock(&mut self, ui: &mut Ui) {
        let t = self.theme;
        let ctx = ui.ctx().clone();
        module(ui, &t, "Midi", 300.0, |ui| {
            let label = match self.midi.status {
                MidiStatus::On => format!("MIDI · {}", self.midi.device),
                MidiStatus::Err => "MIDI · err".to_string(),
                MidiStatus::Off => "MIDI · off".to_string(),
            };
            ui.horizontal(|ui| {
                if button(ui, &t, &label, self.midi.status == MidiStatus::On, Vec2::new(240.0, 22.0)).clicked() {
                    self.midi.open = !self.midi.open;
                }
                led(ui, &t, self.midi.snap.activity % 2 == 1, t.iris);
            });
            if !self.midi.open {
                ui.label(RichText::new("click to open routing · notes/bend/CC in, tx out").size(10.0).color(t.subtle));
                return;
            }
            ui.horizontal(|ui| {
                let armed = self.midi.status == MidiStatus::On;
                if button(ui, &t, "MIDI", armed, Vec2::new(52.0, 22.0)).clicked() {
                    if armed {
                        self.midi_disconnect();
                    } else {
                        self.midi_connect(&ctx);
                    }
                }
                let dev = self
                    .midi
                    .clip_name
                    .as_ref()
                    .map(|c| format!("♪ {}", c.0))
                    .unwrap_or_else(|| self.midi.device.clone());
                ui.add(
                    egui::Label::new(RichText::new(&dev).size(10.5).color(if self.midi.status == MidiStatus::Err {
                        t.danger
                    } else {
                        t.muted
                    }))
                    .truncate(),
                )
                .on_hover_text(dev);
            });
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.midi.path)
                        .hint_text("path/to/file.mid (or drop)")
                        .desired_width(170.0),
                );
                if button(ui, &t, ".mid", false, Vec2::new(40.0, 20.0)).clicked() {
                    let p = self.midi.path.clone();
                    self.play_midi_file(&ctx, &p);
                }
                if button(ui, &t, "phrase", false, Vec2::new(52.0, 20.0)).clicked() {
                    self.play_midi_bytes(&ctx, iki_core::shaku::midi::PHRASE_MID, "phrase");
                }
            });
            let snap = self.midi.snap.clone();
            ui.label(RichText::new("CABLES").size(9.5).color(t.subtle));
            ui.horizontal(|ui| {
                for i in 0..CABLE_COUNT {
                    let name =
                        snap.cable_names[i].trim_start_matches("IAC").trim_start_matches("Loop").trim().to_string();
                    let name = if name.is_empty() { format!("{}", i + 1) } else { name };
                    if button(ui, &t, &name, snap.cables[i], Vec2::new(52.0, 20.0))
                        .on_hover_text(&snap.cable_names[i])
                        .clicked()
                    {
                        self.midi.bus.set_cable(i, !snap.cables[i]);
                        self.midi.snap = self.midi.bus.snap();
                    }
                }
            });
            let port_name = |id: &str, list: &[crate::midi_bus::MidiPortInfo], none: &str| -> String {
                match id {
                    "all" => "All".into(),
                    "loop" => "LoopMIDI".into(),
                    "iac" => "IAC".into(),
                    "none" => none.into(),
                    other => {
                        list.iter().find(|p| p.id == other).map(|p| p.name.clone()).unwrap_or_else(|| other.into())
                    }
                }
            };
            ui.horizontal(|ui| {
                ui.label(RichText::new("In").size(10.5));
                let mut in_id = snap.in_id.clone();
                egui::ComboBox::from_id_salt("midi-in")
                    .width(200.0)
                    .selected_text(port_name(&in_id, &snap.ins, "All"))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut in_id, "all".to_string(), "All");
                        ui.selectable_value(&mut in_id, "loop".to_string(), "LoopMIDI");
                        for kind in [MidiKind::Loop, MidiKind::Iac, MidiKind::Hw] {
                            for p in snap.ins.iter().filter(|p| p.kind == kind) {
                                ui.selectable_value(&mut in_id, p.id.clone(), &p.name);
                            }
                        }
                    });
                if in_id != snap.in_id {
                    self.midi.bus.set_in(&in_id);
                    self.midi.snap = self.midi.bus.snap();
                }
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("Out").size(10.5));
                let mut out_id = snap.out_id.clone();
                egui::ComboBox::from_id_salt("midi-out")
                    .width(192.0)
                    .selected_text(port_name(&out_id, &snap.outs, "Off"))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut out_id, "none".to_string(), "Off");
                        for kind in [MidiKind::Loop, MidiKind::Iac, MidiKind::Hw] {
                            for p in snap.outs.iter().filter(|p| p.kind == kind) {
                                ui.selectable_value(&mut out_id, p.id.clone(), &p.name);
                            }
                        }
                    });
                if out_id != snap.out_id {
                    self.midi.bus.set_out(&out_id);
                    self.midi.snap = self.midi.bus.snap();
                }
            });
            ui.horizontal(|ui| {
                if button(ui, &t, "Thru", snap.thru, Vec2::new(52.0, 20.0)).clicked() {
                    self.midi.bus.set_thru(!snap.thru);
                    self.midi.snap = self.midi.bus.snap();
                }
                if button(ui, &t, "Tx", snap.tx, Vec2::new(52.0, 20.0)).clicked() {
                    self.midi.bus.set_tx(!snap.tx);
                    self.midi.snap = self.midi.bus.snap();
                }
                if button(ui, &t, "Rescan", false, Vec2::new(60.0, 20.0)).clicked() {
                    self.midi.bus.rescan();
                    self.midi.snap = self.midi.bus.snap();
                }
                ui.label(
                    RichText::new(format!("{} in · {} out", snap.ins.len(), snap.outs.len()))
                        .size(10.0)
                        .color(t.subtle),
                );
            });
        });
    }
}
