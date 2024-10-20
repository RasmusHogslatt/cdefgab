use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
        Arc,
    },
    thread,
};

use crate::{
    music_representation::musical_structures::{Note, Score},
    renderer::*,
    time_scrubber::{self, time_scrubber::TimeScrubber},
};
use eframe::egui;
use egui::ScrollArea;
use renderer::render_score;

pub struct Configs {
    pub custom_tempo: usize,
    pub use_custom_tempo: bool,
    pub file_path: Option<String>,
    pub measures_per_row: usize,
    pub dashes_per_division: usize,
    pub total_score_time: f32,
}

impl Configs {
    pub fn new() -> Self {
        Self {
            custom_tempo: 120,
            use_custom_tempo: false,
            file_path: Some("silent.xml".to_owned()),
            measures_per_row: 4,
            dashes_per_division: 4,
            total_score_time: 0.0,
        }
    }
}

pub struct TabApp {
    score: Option<Score>,
    tab_text: Option<String>,
    playback_handle: Option<thread::JoinHandle<()>>,
    notes_receiver: Option<Receiver<Vec<Note>>>,
    is_playing: bool,
    stop_flag: Arc<AtomicBool>,
    pub configs: Configs,
    pub previous_notes: Option<Vec<Note>>,
    pub current_notes: Option<Vec<Note>>,
}

impl TabApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let configs = Configs::new();
        let file_path = configs.file_path.clone().unwrap_or_default();
        let score = Score::parse_from_musicxml(file_path).expect("Failed to parse MusicXML");
        let tab_text = render_score(
            &score,
            configs.measures_per_row,
            configs.dashes_per_division,
        );

        // Initialize the stop flag
        let stop_flag = Arc::new(AtomicBool::new(false));

        Self {
            score: Some(score),
            tab_text: Some(tab_text),
            playback_handle: None,
            notes_receiver: None,
            is_playing: false,
            stop_flag,
            configs,
            previous_notes: None,
            current_notes: None,
        }
    }

    fn start_playback(&mut self) {
        if self.is_playing {
            return;
        }

        if let Some(score) = &self.score {
            let score = score.clone(); // Clone to move into the thread
            let (tx, rx) = mpsc::channel();
            self.notes_receiver = Some(rx);
            self.stop_flag.store(false, Ordering::Relaxed);
            let stop_flag = self.stop_flag.clone();

            let mut tempo: Option<usize> = Some(score.tempo);
            if self.configs.use_custom_tempo {
                tempo = Some(self.configs.custom_tempo);
            }
            self.playback_handle = Some(thread::spawn(move || {
                let mut scrubber = TimeScrubber::new(&score, tempo);
                scrubber.simulate_playback(&score, tx, stop_flag);
            }));

            self.is_playing = true;
        }
    }

    fn stop_playback(&mut self) {
        if self.is_playing {
            self.stop_flag.store(true, Ordering::Relaxed);
            if let Some(handle) = self.playback_handle.take() {
                let _ = handle.join();
            }
            self.is_playing = false;
            self.current_notes = None;
            self.previous_notes = None;
        }
    }
}

impl eframe::App for TabApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Left panel with playback controls
        egui::SidePanel::left("left_panel").show(ctx, |ui| {
            ui.heading("Playback Controls");

            if ui.button("Play").clicked() {
                self.start_playback();
            }

            if ui.button("Stop").clicked() {
                self.stop_playback();
            }

            ui.heading("Settings");
            ui.checkbox(&mut self.configs.use_custom_tempo, "Custom tempo");
            if self.configs.use_custom_tempo {
                ui.add(egui::Slider::new(&mut self.configs.custom_tempo, 1..=240));
            }
            ui.heading("Score info");

            match (self.configs.use_custom_tempo, self.score.clone().unwrap()) {
                (true, score) => {
                    let seconds_per_beat = 60.0 / self.configs.custom_tempo as f32;
                    let seconds_per_division =
                        seconds_per_beat / score.divisions_per_quarter as f32;
                    self.configs.total_score_time = score.measures.len() as f32
                        * seconds_per_division
                        * score.divisions_per_measure as f32;
                }
                (false, score) => {
                    let seconds_per_beat = 60.0 / score.tempo as f32;
                    let seconds_per_division =
                        seconds_per_beat / score.divisions_per_quarter as f32;
                    self.configs.total_score_time = score.measures.len() as f32
                        * seconds_per_division
                        * score.divisions_per_measure as f32;
                }
            }

            ui.label(format!(
                "Total score time: {}",
                self.configs.total_score_time
            ));

            ui.separator();

            ui.label("Currently Playing Notes:");
            if let Some(current_notes) = &self.current_notes {
                for note in current_notes.iter() {
                    if let (Some(string), Some(fret)) = (note.string, note.fret) {
                        ui.label(format!("String: {}, Fret: {}", string, fret));
                    }
                }
            }

            ui.separator();

            ui.label("Previous Notes:");
            if let Some(previous_notes) = &self.previous_notes {
                for note in previous_notes.iter() {
                    if let (Some(string), Some(fret)) = (note.string, note.fret) {
                        ui.label(format!("String: {}, Fret: {}", string, fret));
                    }
                }
            }
        });

        // Central panel to display the tabs
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Tablature");
            if let Some(tab_text) = &self.tab_text {
                ScrollArea::vertical().show(ui, |ui| {
                    ui.monospace(tab_text);
                });
            }
        });

        // Receive notes from the playback thread without blocking
        if let Some(receiver) = &self.notes_receiver {
            while let Ok(notes) = receiver.try_recv() {
                if !notes.is_empty() {
                    // Update previous and current notes
                    self.previous_notes = self.current_notes.take();
                    self.current_notes = Some(notes);
                }
            }
            // Request repaint to update the UI
            ctx.request_repaint();
        }

        // Check if playback has finished
        if self.is_playing && self.stop_flag.load(Ordering::Relaxed) {
            self.is_playing = false;
            self.current_notes = None;
            self.previous_notes = None;
        }
    }
}
