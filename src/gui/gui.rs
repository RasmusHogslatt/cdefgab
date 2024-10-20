use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
        Arc,
    },
    thread,
};

use crate::{
    music_representation::musical_structures::{self, Note, Score},
    renderer::*,
    time_scrubber::time_scrubber::TimeScrubber,
};
use eframe::egui;
use egui::ScrollArea;
use renderer::render_score;

pub struct Configs {
    pub custom_tempo: Option<u8>,
    pub file_path: Option<String>,
    pub measures_per_row: usize,
    pub dashes_per_division: usize,
}

impl Configs {
    pub fn new() -> Self {
        Self {
            custom_tempo: None,
            file_path: Some("silent.xml".to_owned()),
            measures_per_row: 4,
            dashes_per_division: 4,
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

            self.playback_handle = Some(thread::spawn(move || {
                let mut scrubber = TimeScrubber::new(&score);
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
