pub mod gui {
    use std::{
        sync::{
            atomic::{AtomicBool, Ordering},
            mpsc::{self, Receiver},
            Arc, Mutex,
        },
        thread,
    };

    use crate::{
        audio_listener::audio_listener::audio_listener::AudioListener,
        audio_player::audio_player::audio_player::AudioPlayer,
        music_representation::musical_structures::{Note, Score},
        renderer::*,
        time_scrubber::time_scrubber::TimeScrubber,
    };
    use eframe::egui;
    use egui::ScrollArea;
    use renderer::{render_score, score_info};

    pub struct Configs {
        pub custom_tempo: usize,
        pub use_custom_tempo: bool,
        pub file_path: Option<String>,
        pub measures_per_row: usize,
        pub dashes_per_division: usize,
        pub decay: f32,
        pub volume: f32,
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
        pub match_result_receiver: Receiver<bool>,
        pub expected_notes: Arc<Mutex<Option<Vec<Note>>>>,
        pub note_matched: bool,
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
            let audio_listener = AudioListener::new(match_result_sender, expected_notes.clone());
            audio_listener.start();

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
                note_matched: false,
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
                    ui.add(eframe::egui::Slider::new(
                        &mut self.configs.decay,
                        0.9..=1.0,
                    ));
                });

                ui.horizontal(|ui| {
                    ui.label("Volume:");
                    ui.add(eframe::egui::Slider::new(
                        &mut self.configs.volume,
                        0.0..=1.0,
                    ));
                });
                ui.heading("Score info");

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
                    "Total score time: {}",
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
                if self.is_playing {
                    ui.label(format!("Note Matched: {}", self.note_matched));
                }
            });

            // Central panel to display the tabs
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

            // Receive match result from AudioListener
            while let Ok(matched) = self.match_result_receiver.try_recv() {
                self.note_matched = matched;
                // Update UI or state based on match result
                println!("Match {}", self.note_matched);
            }

            // Check if playback has finished
            if self.is_playing && self.stop_flag.load(Ordering::Relaxed) {
                self.is_playing = false;
                self.current_notes = None;
                self.previous_notes = None;
            }
        }
    }
}
