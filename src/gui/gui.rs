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
use egui::{mutex::Mutex, ScrollArea};
use renderer::render_score;

pub struct TabApp {
    score: Option<Score>,
    tab_text: Option<String>,
    playback_handle: Option<thread::JoinHandle<()>>,
    notes_receiver: Option<Receiver<Vec<Note>>>,
    playing_notes: Arc<Mutex<Vec<Note>>>,
    is_playing: bool,
    stop_flag: Arc<AtomicBool>,
}

impl TabApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let file_path = "greensleeves.xml";
        let score = Score::parse_from_musicxml(file_path).expect("Failed to parse MusicXML");

        // Render the tab text
        let measures_per_row = 4;
        let dashes_per_division = 4; // Adjust as needed
        let tab_text = render_score(&score, measures_per_row, dashes_per_division);

        // Initialize the stop flag
        let stop_flag = Arc::new(AtomicBool::new(false));

        Self {
            score: Some(score),
            tab_text: Some(tab_text),
            playback_handle: None,
            notes_receiver: None,
            playing_notes: Arc::new(Mutex::new(Vec::new())),
            is_playing: false,
            stop_flag,
        }
    }
}

impl TabApp {
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
            self.playing_notes.lock().clear();
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

            ui.heading("Currently Playing Notes:");
            let playing_notes = self.playing_notes.lock();
            for note in playing_notes.iter() {
                if let (Some(string), Some(fret)) = (note.string, note.fret) {
                    ui.label(format!("String: {}, Fret: {}", string, fret));
                }
            }
        });

        // Central panel to display the tabs
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Tabs");
            if let Some(tab_text) = &self.tab_text {
                ScrollArea::vertical().show(ui, |ui| {
                    ui.monospace(tab_text);
                });
            }
        });

        // Receive notes from the playback thread without blocking
        if let Some(receiver) = &self.notes_receiver {
            while let Ok(notes) = receiver.try_recv() {
                let mut playing_notes = self.playing_notes.lock();
                *playing_notes = notes;
            }
            // Request repaint to update the UI
            ctx.request_repaint();
        }

        // Check if playback has finished
        if self.is_playing && self.stop_flag.load(Ordering::Relaxed) {
            self.is_playing = false;
            self.playing_notes.lock().clear();
        }
    }
}
