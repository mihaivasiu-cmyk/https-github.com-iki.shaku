//! Native egui front panel.

pub mod app;
mod loop_view;
mod midi_view;
mod view;

pub use app::App;
// The look (theme, widgets, scopes, panels) is shared with the plugin editor.
pub use iki_ui::{scopes, theme, widgets};
