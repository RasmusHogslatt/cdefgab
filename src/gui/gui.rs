use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
        Arc,
    },
    thread,
};

use crate::{
    audio_listener::audio_listener::AudioListener,
    audio_player::audio_player::{AudioPlayer, GuitarConfig},
    music_representation::musical_structures::{Note, Score},
    renderer::*,
    time_scrubber::time_scrubber::TimeScrubber,
};
use eframe::egui;
use egui::{ScrollArea, Vec2};
use renderer::{render_score, score_info};

use egui_plot::{Line, Plot, PlotPoints};

#[derive(Clone)]
pub struct Configs {
    pub custom_tempo: usize,
    pub use_custom_tempo: bool,
    pub file_path: Option<String>,
    pub measures_per_row: usize,
    pub dashes_per_division: usize,
    pub volume: f32,
    pub guitar_configs: Vec<GuitarConfig>,
    pub active_guitar: usize,
}

pub struct DisplayMetrics {
    pub total_score_time: f32,
}

impl Configs {
    pub fn new() -> Self {
        Self {
            volume: 0.5,
            active_guitar: 0,
            guitar_configs: vec![
                GuitarConfig::custom(
                    0.996, // decay
                    0.5,   // string_damping
                    100.0, // body_resonance
                    0.5,   // body_damping
                    0.7,   // string_tension
                    25.5,  // scale_length
                    0,     // capo_fret: No capo by default
                ),
                GuitarConfig::acoustic(),
                GuitarConfig::electric(),
                GuitarConfig::classical(),
            ],
            custom_tempo: 120,
            use_custom_tempo: false,
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
    notes_receiver: Option<Receiver<(Vec<Note>, usize, usize)>>, // Notes, current division, current measure
    is_playing: bool,
    stop_flag: Arc<AtomicBool>,
    pub configs: Configs,
    pub display_metrics: DisplayMetrics,
    pub previous_notes: Option<Vec<Note>>,
    pub current_notes: Option<Vec<Note>>,
    pub audio_player: AudioPlayer,
    pub audio_listener: AudioListener,
    pub match_result_receiver: Receiver<bool>,
    pub expected_notes: Arc<std::sync::Mutex<Option<Vec<Note>>>>,
    pub is_match: bool,
    pub input_chroma_history: Arc<std::sync::Mutex<Vec<Vec<f32>>>>,
    pub expected_chroma_history: Arc<std::sync::Mutex<Vec<Vec<f32>>>>,
    pub input_signal_history: Arc<std::sync::Mutex<Vec<Vec<f32>>>>,
    pub current_measure: Option<usize>,
    pub current_division: Option<usize>,
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

        let stop_flag = Arc::new(AtomicBool::new(false));

        let audio_player_configs = configs.clone();
        let audio_player = AudioPlayer::new(audio_player_configs);
        audio_player.start();

        let (match_result_sender, match_result_receiver) = mpsc::channel();
        let expected_notes = Arc::new(std::sync::Mutex::new(None));

        let mut audio_listener =
            AudioListener::new(match_result_sender.clone(), expected_notes.clone());
        audio_listener.start();

        let input_chroma_history = audio_listener.input_chroma_history.clone();
        let expected_chroma_history = audio_listener.expected_chroma_history.clone();
        let input_signal_history = audio_listener.input_signal_history.clone();

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
            is_match: false,
            input_chroma_history,
            expected_chroma_history,
            input_signal_history,
            current_measure: None,
            current_division: None,
        }
    }

    fn start_playback(&mut self) {
        if self.is_playing {
            return;
        }

        if let Some(score) = &self.score {
            let score = score.clone();
            let (tx_notes, rx_notes) = mpsc::channel();
            self.notes_receiver = Some(rx_notes);

            self.stop_flag.store(false, Ordering::Relaxed);
            let stop_flag = self.stop_flag.clone();

            let tempo = if self.configs.use_custom_tempo {
                Some(self.configs.custom_tempo)
            } else {
                Some(score.tempo)
            };
            self.playback_handle = Some(thread::spawn(move || {
                let mut scrubber = TimeScrubber::new(&score, tempo);

                scrubber.simulate_playback(&score, tx_notes, stop_flag);
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
            self.is_match = false;
        }
    }

    fn update_audio_player_configs(&mut self) {
        self.audio_player.update_configs(self.configs.clone());
    }
}

impl eframe::App for TabApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut changed_config = false;
        let mut changed_rendered_score = false;
        egui::SidePanel::left("left_panel").show(ctx, |ui| {
            ui.group(|ui| {
                ui.heading("Playback controls");
                ui.horizontal(|ui| {
                    if ui.button("Play").clicked() {
                        self.start_playback();
                    }
                    if ui.button("Stop").clicked() {
                        self.stop_playback();
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Volume:");
                    if ui
                        .add(egui::Slider::new(&mut self.configs.volume, 0.0..=1.0).step_by(0.01))
                        .changed()
                    {
                        changed_config = true;
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Set custom tempo");
                    ui.checkbox(&mut self.configs.use_custom_tempo, "");
                });
                if self.configs.use_custom_tempo {
                    if ui
                        .add(egui::Slider::new(&mut self.configs.custom_tempo, 1..=240))
                        .changed()
                    {
                        self.update_audio_player_configs();
                    }
                }
                ui.label(format!(
                    "Total score time: {:.2} seconds",
                    self.display_metrics.total_score_time
                ));
                ui.label("Capo fret:");
                if ui
                    .add(
                        egui::Slider::new(
                            &mut self.configs.guitar_configs[self.configs.active_guitar].capo_fret,
                            0..=24,
                        )
                        .text("Fret"),
                    )
                    .changed()
                {
                    changed_config = true;
                }
            });

            ui.group(|ui| {
                ui.heading("Guitar profile");
                egui::ComboBox::new("guitar_selection", "Guitar type")
                    .selected_text(format!(
                        "{}",
                        self.configs.guitar_configs[self.configs.active_guitar].name
                    ))
                    .show_ui(ui, |ui| {
                        for (index, guitar) in self.configs.guitar_configs.iter().enumerate() {
                            // Check if this guitar is currently selected
                            let checked = index == self.configs.active_guitar;
                            if ui
                                .selectable_label(checked, format!("{}", &guitar.name))
                                .clicked()
                            {
                                self.configs.active_guitar = index;
                                changed_config = true;
                            }
                        }
                    });

                if self.configs.active_guitar == 0 {
                    egui::Grid::new("custom_guitar_config")
                        .num_columns(2)
                        .show(ui, |ui| {
                            ui.label("Decay:");
                            if ui
                                .add(
                                    egui::Slider::new(
                                        &mut self.configs.guitar_configs[0].decay,
                                        0.9..=1.0,
                                    )
                                    .step_by(0.001),
                                )
                                .changed()
                            {
                                changed_config = true;
                            }
                            ui.end_row();

                            ui.label("String damping:");
                            if ui
                                .add(
                                    egui::Slider::new(
                                        &mut self.configs.guitar_configs[0].string_damping,
                                        0.0..=1.0,
                                    )
                                    .step_by(0.001),
                                )
                                .changed()
                            {
                                changed_config = true;
                            }
                            ui.end_row();

                            ui.label("Body damping:");
                            if ui
                                .add(
                                    egui::Slider::new(
                                        &mut self.configs.guitar_configs[0].body_damping,
                                        0.0..=1.0,
                                    )
                                    .step_by(0.001),
                                )
                                .changed()
                            {
                                changed_config = true;
                            }
                            ui.end_row();

                            ui.label("Body resonance:");
                            if ui
                                .add(
                                    egui::Slider::new(
                                        &mut self.configs.guitar_configs[0].body_resonance,
                                        0.0..=500.0,
                                    )
                                    .step_by(0.1),
                                )
                                .changed()
                            {
                                changed_config = true;
                            }
                            ui.end_row();

                            ui.label("String tension:");
                            if ui
                                .add(
                                    egui::Slider::new(
                                        &mut self.configs.guitar_configs[0].string_tension,
                                        0.0..=1.0,
                                    )
                                    .step_by(0.001),
                                )
                                .changed()
                            {
                                changed_config = true;
                            }
                            ui.end_row();

                            ui.label("Scale length [inch]:");
                            if ui
                                .add(
                                    egui::Slider::new(
                                        &mut self.configs.guitar_configs[0].scale_length,
                                        10.0..=50.0,
                                    )
                                    .step_by(0.1),
                                )
                                .changed()
                            {
                                changed_config = true;
                            }
                            ui.end_row();
                        });
                }
            });

            ui.group(|ui| {
                ui.heading("Render settings");
                ui.horizontal(|ui| {
                    ui.label("Dashes per division:");
                    if ui
                        .add(egui::Slider::new(
                            &mut self.configs.dashes_per_division,
                            3..=8,
                        ))
                        .changed()
                    {
                        changed_rendered_score = true;
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Measure per row:");
                    if ui
                        .add(egui::Slider::new(&mut self.configs.measures_per_row, 3..=8))
                        .changed()
                    {
                        changed_rendered_score = true;
                    }
                });
            });

            if changed_rendered_score {
                if let Some(score) = &self.score {
                    self.tab_text = Some(render_score(
                        &score,
                        self.configs.measures_per_row,
                        self.configs.dashes_per_division,
                    ));
                }
            }

            if let Some(score) = &self.score {
                let cfg = &self.configs;
                let seconds_per_beat = if cfg.use_custom_tempo {
                    60.0 / cfg.custom_tempo as f32
                } else {
                    60.0 / score.tempo as f32
                };
                let seconds_per_division = seconds_per_beat / score.divisions_per_quarter as f32;
                self.display_metrics.total_score_time = score.measures.len() as f32
                    * seconds_per_division
                    * score.divisions_per_measure as f32;
            }

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

            // Display match status
            if self.is_playing && self.current_notes.is_some() {
                ui.label(format!("Note Matched: {}", self.is_match));
            }
            ui.separator();
            ui.heading("Playback Timing");

            ui.label(format!(
                "Current Measure: {}",
                self.current_measure.unwrap_or(0)
            ));
            ui.label(format!(
                "Current Division: {}",
                self.current_division.unwrap_or(0)
            ));
        });

        egui::Window::new("Input plot")
            .fixed_size(Vec2::new(400.0, 400.0))
            .show(ctx, |ui| {
                ui.heading("Live Time-Domain Signal Plot");

                // Access the raw signal histories
                let input_signal_hist = self.input_signal_history.lock().unwrap();

                if !input_signal_hist.is_empty() {
                    // Use the latest raw signals
                    let input_signal = &input_signal_hist[input_signal_hist.len() - 1];

                    // Create plot points for raw signals
                    let input_points: PlotPoints = input_signal
                        .iter()
                        .enumerate()
                        .map(|(i, &y)| [i as f64, y as f64])
                        .collect();

                    // Create lines
                    let input_line = Line::new(input_points).name("Input Signal");

                    // Plot the lines with fixed y-axis limits
                    Plot::new("time_domain_plot")
                        .legend(egui_plot::Legend::default())
                        .view_aspect(2.0) // Adjust aspect ratio as needed
                        .include_y(-1.1) // Since we normalized per frame
                        .include_y(1.1)
                        .show(ui, |plot_ui| {
                            plot_ui.line(input_line);
                        });
                } else {
                    ui.label("No time-domain data to display yet.");
                }

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

        if let (Some(receiver), Some(score)) = (&self.notes_receiver, &self.score) {
            while let Ok((notes, division, measure)) = receiver.try_recv() {
                if !notes.is_empty() {
                    // Update previous and current notes
                    self.previous_notes = self.current_notes.take();
                    self.current_notes = Some(notes.clone());

                    // Update expected notes for the AudioListener
                    let mut expected_notes = self.expected_notes.lock().unwrap();
                    *expected_notes = Some(notes.clone());
                    let seconds_per_division = {
                        let cfg = &self.configs;
                        if cfg.use_custom_tempo {
                            60.0 / cfg.custom_tempo as f32 / score.divisions_per_quarter as f32
                        } else {
                            60.0 / score.tempo as f32 / score.divisions_per_quarter as f32
                        }
                    };
                    self.display_metrics.total_score_time = score.measures.len() as f32
                        * seconds_per_division
                        * score.divisions_per_measure as f32;

                    // Play the notes
                    self.audio_player
                        .play_notes_with_config(&notes, self.audio_player.seconds_per_division);
                }
                println!("Division: {}, Measure: {}", division, measure);
            }
            ctx.request_repaint();
        }

        // Receive match result from AudioListener
        while let Ok(is_match) = self.match_result_receiver.try_recv() {
            if self.is_playing && self.current_notes.is_some() {
                self.is_match = is_match;
            } else {
                self.is_match = false;
            }
        }

        if changed_config {
            self.update_audio_player_configs();
        }

        // Check if playback has finished
        if self.is_playing && self.stop_flag.load(Ordering::Relaxed) {
            self.is_playing = false;
            self.current_notes = None;
            self.previous_notes = None;
            self.is_match = false;
        }
    }
}
