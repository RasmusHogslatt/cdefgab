// gui.rs

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
        Arc, Mutex,
    },
    thread,
};

use crate::{
    audio_listener::audio_listener::{AudioListener, SimilarityMetric},
    audio_player::audio_player::AudioPlayer,
    music_representation::musical_structures::{Note, Score},
    renderer::*,
    time_scrubber::time_scrubber::TimeScrubber,
};
use eframe::egui;
use egui::{ScrollArea, Vec2};
use renderer::{render_score, score_info};

// Import the plot module from egui_plot
use egui_plot::{Line, Plot, PlotPoints};

pub struct Configs {
    pub custom_tempo: usize,
    pub use_custom_tempo: bool,
    pub file_path: Option<String>,
    pub measures_per_row: usize,
    pub dashes_per_division: usize,
    pub decay: f32,
    pub volume: f32,
    pub matching_threshold: f32,
}

pub struct DisplayMetrics {
    pub total_score_time: f32,
}

impl Configs {
    pub fn new() -> Self {
        Self {
            custom_tempo: 120,
            use_custom_tempo: false,
            file_path: Some("greensleeves.xml".to_owned()),
            measures_per_row: 4,
            dashes_per_division: 4,
            decay: 0.996,
            volume: 0.5,
            matching_threshold: 0.8, // Set default threshold to 0.8
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
    pub display_metrics: DisplayMetrics,
    pub previous_notes: Option<Vec<Note>>,
    pub current_notes: Option<Vec<Note>>,
    pub audio_player: AudioPlayer,
    pub audio_listener: AudioListener,
    pub match_result_receiver: Receiver<f32>,
    pub expected_notes: Arc<Mutex<Option<Vec<Note>>>>,
    pub similarity: f32,
    pub is_match: bool,
    pub matching_threshold: Arc<Mutex<f32>>,
    // New fields for accessing chroma and signal feature histories
    pub input_chroma_history: Arc<Mutex<Vec<Vec<f32>>>>,
    pub expected_chroma_history: Arc<Mutex<Vec<Vec<f32>>>>,
    pub input_signal_history: Arc<Mutex<Vec<Vec<f32>>>>,
    pub expected_signal_history: Arc<Mutex<Vec<Vec<f32>>>>,
}

impl TabApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let configs = Configs::new();
        let display_metrics = DisplayMetrics {
            total_score_time: 0.0,
        };
        let file_path = configs.file_path.clone().unwrap_or_default();
        let score = Score::parse_from_musicxml(file_path).expect("Failed to parse MusicXML");
        let tab_text = render_score(
            &score,
            configs.measures_per_row,
            configs.dashes_per_division,
        );

        // Initialize the stop flag
        let stop_flag = Arc::new(AtomicBool::new(false));

        let audio_player = AudioPlayer::new();
        audio_player.start();

        let (match_result_sender, match_result_receiver) = mpsc::channel();
        let expected_notes = Arc::new(Mutex::new(None));
        let matching_threshold = Arc::new(Mutex::new(configs.matching_threshold));
        let mut audio_listener = AudioListener::new(
            match_result_sender.clone(),
            expected_notes.clone(),
            SimilarityMetric::DTW, // Use DTW metric
        );
        audio_listener.start();

        // Clone the chroma and signal feature histories before moving audio_listener
        let input_chroma_history = audio_listener.input_chroma_history.clone();
        let expected_chroma_history = audio_listener.expected_chroma_history.clone();
        let input_signal_history = audio_listener.input_signal_history.clone();
        let expected_signal_history = audio_listener.expected_signal_history.clone();

        Self {
            score: Some(score),
            tab_text: Some(tab_text),
            playback_handle: None,
            notes_receiver: None,
            is_playing: false,
            stop_flag,
            configs,
            display_metrics,
            previous_notes: None,
            current_notes: None,
            audio_player,
            audio_listener,
            match_result_receiver,
            expected_notes,
            similarity: 0.0,
            is_match: false,
            matching_threshold,
            // Initialize new fields
            input_chroma_history,
            expected_chroma_history,
            input_signal_history,
            expected_signal_history,
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

            let tempo: Option<usize> = if self.configs.use_custom_tempo {
                Some(self.configs.custom_tempo)
            } else {
                Some(score.tempo)
            };
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
            self.similarity = 0.0;
            self.is_match = false;
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

            ui.separator();
            ui.heading("Audio Settings");

            ui.horizontal(|ui| {
                ui.label("Decay:");
                ui.add(egui::Slider::new(&mut self.configs.decay, 0.9..=1.0).step_by(0.001));
            });

            ui.horizontal(|ui| {
                ui.label("Volume:");
                ui.add(egui::Slider::new(&mut self.configs.volume, 0.0..=1.0).step_by(0.01));
            });

            ui.horizontal(|ui| {
                ui.label("Matching Threshold:");
                ui.add(
                    egui::Slider::new(&mut self.configs.matching_threshold, 0.0..=1.0)
                        .step_by(0.01),
                );
            });

            ui.separator();
            ui.heading("Score Info");

            match (&self.configs.use_custom_tempo, &self.score) {
                (true, Some(score)) => {
                    let seconds_per_beat = 60.0 / self.configs.custom_tempo as f32;
                    let seconds_per_division =
                        seconds_per_beat / score.divisions_per_quarter as f32;
                    self.display_metrics.total_score_time = score.measures.len() as f32
                        * seconds_per_division
                        * score.divisions_per_measure as f32;
                }
                (false, Some(score)) => {
                    let seconds_per_beat = 60.0 / score.tempo as f32;
                    let seconds_per_division =
                        seconds_per_beat / score.divisions_per_quarter as f32;
                    self.display_metrics.total_score_time = score.measures.len() as f32
                        * seconds_per_division
                        * score.divisions_per_measure as f32;
                }
                _ => {}
            }

            ui.label(format!(
                "Total score time: {:.2} seconds",
                self.display_metrics.total_score_time
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

            ui.separator();

            // Display similarity and match status only if a similarity score has been received
            if self.is_playing && self.current_notes.is_some() {
                ui.label(format!("Similarity: {:.3}", self.similarity));
                ui.label(format!("Note Matched: {}", self.is_match));
            }
        });

        // Window for Chroma Feature Plot
        egui::Window::new("Chroma Plot").show(ctx, |ui| {
            ui.heading("Live Chroma Feature Plot");

            // Access the chroma feature histories
            let input_chroma_hist = self.input_chroma_history.lock().unwrap();
            let expected_chroma_hist = self.expected_chroma_history.lock().unwrap();

            if !input_chroma_hist.is_empty() && !expected_chroma_hist.is_empty() {
                // Use the latest chroma features
                let input_chroma = &input_chroma_hist[input_chroma_hist.len() - 1];
                let expected_chroma = &expected_chroma_hist[expected_chroma_hist.len() - 1];

                // Create plot points for chroma features
                let input_points: PlotPoints = input_chroma
                    .iter()
                    .enumerate()
                    .map(|(i, &y)| [i as f64, y as f64])
                    .collect();

                let expected_points: PlotPoints = expected_chroma
                    .iter()
                    .enumerate()
                    .map(|(i, &y)| [i as f64, y as f64])
                    .collect();

                // Create lines
                let input_line = Line::new(input_points).name("Input Chroma");
                let expected_line = Line::new(expected_points).name("Expected Chroma");

                // Plot the lines
                Plot::new("chroma_plot")
                    .legend(egui_plot::Legend::default())
                    .show(ui, |plot_ui| {
                        plot_ui.line(input_line);
                        plot_ui.line(expected_line);
                    });
            } else {
                ui.label("No chroma data to display yet.");
            }
        });

        // Window for Time-Domain Signal Plot
        egui::Window::new("Time-Domain Plot").show(ctx, |ui| {
            ui.heading("Live Time-Domain Signal Plot");

            // Access the raw signal histories
            let input_signal_hist = self.input_signal_history.lock().unwrap();
            let expected_signal_hist = self.expected_signal_history.lock().unwrap();

            if !input_signal_hist.is_empty() && !expected_signal_hist.is_empty() {
                // Use the latest raw signals
                let input_signal = &input_signal_hist[input_signal_hist.len() - 1];
                let expected_signal = &expected_signal_hist[expected_signal_hist.len() - 1];

                // Create plot points for raw signals
                let input_points: PlotPoints = input_signal
                    .iter()
                    .enumerate()
                    .map(|(i, &y)| [i as f64, y as f64])
                    .collect();

                let expected_points: PlotPoints = expected_signal
                    .iter()
                    .enumerate()
                    .map(|(i, &y)| [i as f64, y as f64])
                    .collect();

                // Create lines
                let input_line = Line::new(input_points).name("Input Signal");
                let expected_line = Line::new(expected_points).name("Expected Signal");
                let difference_signal: Vec<f32> = input_signal
                    .into_iter()
                    .zip(expected_signal)
                    .map(|(a, b)| a - b)
                    .collect();
                let difference_points: PlotPoints = difference_signal
                    .iter()
                    .enumerate()
                    .map(|(i, &y)| [i as f64, y as f64])
                    .collect();
                let difference_line = Line::new(difference_points).name("Difference");

                // Plot the lines
                Plot::new("time_domain_plot")
                    .legend(egui_plot::Legend::default())
                    .show(ui, |plot_ui| {
                        plot_ui.line(input_line);
                        plot_ui.line(expected_line);
                        // plot_ui.line(difference_line);
                    });
            } else {
                ui.label("No time-domain data to display yet.");
            }
        });

        // Central panel to display the tabs and other information
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Parsed Score Info");
            if let Some(score) = &self.score {
                ScrollArea::vertical()
                    .id_salt("score_info_scroll_area")
                    .show(ui, |ui| {
                        ui.monospace(score_info(&score));
                    });
            }
            ui.heading("Tablature");
            if let Some(tab_text) = &self.tab_text {
                ScrollArea::vertical()
                    .id_salt("tab_scroll_area")
                    .show(ui, |ui| {
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
                    self.current_notes = Some(notes.clone());

                    // Update expected notes for the AudioListener
                    let mut expected_notes = self.expected_notes.lock().unwrap();
                    *expected_notes = Some(notes.clone());

                    // Reset similarity computation flag
                    {
                        let mut computed = self.audio_listener.similarity_computed.lock().unwrap();
                        *computed = false;
                    }

                    // Play the notes
                    self.audio_player.play_notes_with_config(
                        &notes,
                        self.configs.decay,
                        self.configs.volume,
                    );
                }
            }
            // Request repaint to update the UI
            ctx.request_repaint();
        }

        // Receive similarity result from AudioListener
        while let Ok(similarity) = self.match_result_receiver.try_recv() {
            if self.current_notes.is_some() && self.is_playing {
                self.similarity = similarity;
                self.is_match = self.similarity >= self.configs.matching_threshold;
                println!(
                    "Similarity: {:.3}, Match: {}",
                    self.similarity, self.is_match
                );
            } else {
                self.similarity = 0.0;
                self.is_match = false;
            }
        }

        // Update the AudioListener's decay based on the GUI setting
        {
            let decay = self.configs.decay;
            self.audio_listener.set_decay(decay);
        }

        // Update the AudioPlayer's decay and volume based on the GUI settings
        {
            let decay = self.configs.decay;
            let volume = self.configs.volume;
            self.audio_player.set_decay(decay);
            self.audio_player.set_volume(volume);
        }

        // Update the matching threshold in the listener
        {
            let mut threshold = self.matching_threshold.lock().unwrap();
            *threshold = self.configs.matching_threshold;
        }

        // Check if playback has finished
        if self.is_playing && self.stop_flag.load(Ordering::Relaxed) {
            self.is_playing = false;
            self.current_notes = None;
            self.previous_notes = None;
            self.similarity = 0.0;
            self.is_match = false;
        }
    }
}
