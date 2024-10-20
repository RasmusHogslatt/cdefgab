use crate::{
    music_representation::musical_structures::{self, Score},
    renderer::*,
};
use eframe::egui;
use egui::ScrollArea;

pub struct TabApp {
    score: Option<Score>,
    tab_text: Option<String>,
}

impl Default for TabApp {
    fn default() -> Self {
        Self {
            score: None,
            tab_text: None,
        }
    }
}

impl TabApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Default::default()
    }
}

impl eframe::App for TabApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        egui::SidePanel::left("left_panel").show(ctx, |ui| {
            ui.heading("Properties");
        });
        egui::SidePanel::right("right_panel").show(ctx, |ui| {
            ui.heading("Tabs");
        });
    }
}
