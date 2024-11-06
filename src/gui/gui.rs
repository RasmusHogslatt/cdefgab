// gui.rs

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
use egui::{Margin, ScrollArea, Vec2};
use renderer::{render_score, score_info};

use egui_plot::{Line, Plot, PlotPoints};

#[derive(Clone)]
pub struct Configs {
    pub custom_tempo: usize,
    pub use_custom_tempo: bool,
    pub file_path: Option<String>,
    pub measures_per_row: usize,
    pub dashes_per_division: usize, // This parameter controls visual subdivisions per division
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
                GuitarConfig::bass_guitar(),
                GuitarConfig::twelve_string(),
            ],
            custom_tempo: 120,
            use_custom_tempo: false,
            file_path: Some("greensleeves.xml".to_owned()),
            measures_per_row: 4,
            dashes_per_division: 2,
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

    fn render_tab(&self, painter: &egui::Painter, rect: egui::Rect) {
        // Constants for rendering
        let num_strings = 6;
        let string_spacing = 20.0; // pixels between strings
        let note_spacing = 10.0; // base pixels between dashes
        let measure_spacing = 0.0; // additional spacing between measures
        let row_spacing = 20.0; // vertical spacing between rows
        let left_padding = 0.0; // Add left padding to the rendering

        if let Some(score) = &self.score {
            let total_measures = score.measures.len();
            let measures_per_row = self.configs.measures_per_row;
            let total_rows = (total_measures + measures_per_row - 1) / measures_per_row;

            // Start drawing at rect.min
            let mut y_offset = rect.min.y;

            for row in 0..total_rows {
                let measures_in_row = if (row + 1) * measures_per_row <= total_measures {
                    measures_per_row
                } else {
                    total_measures % measures_per_row
                };

                // Calculate the total width of the current row
                let mut row_width = 0.0;
                for measure_idx_in_row in 0..measures_in_row {
                    let measure_idx = row * measures_per_row + measure_idx_in_row;
                    let measure = &score.measures[measure_idx];
                    let dashes_per_division = self.configs.dashes_per_division;
                    let total_divisions = measure.positions.len();
                    let total_dashes = total_divisions * dashes_per_division;
                    row_width += total_dashes as f32 * note_spacing + measure_spacing;
                }
                // Subtract the extra measure_spacing added after the last measure
                row_width -= measure_spacing;

                // Draw strings (horizontal lines) for the current row
                for string_idx in 0..num_strings {
                    let y = y_offset + string_spacing * (string_idx as f32 + 1.0);
                    painter.line_segment(
                        [
                            egui::pos2(rect.min.x + left_padding, y),
                            egui::pos2(rect.min.x + left_padding + row_width, y),
                        ],
                        egui::Stroke::new(1.0, egui::Color32::BLACK),
                    );
                }

                // Adjust x_offset to include left_padding
                let mut x_offset = rect.min.x + left_padding;
                for measure_idx_in_row in 0..measures_in_row {
                    let measure_idx = row * measures_per_row + measure_idx_in_row;
                    let measure = &score.measures[measure_idx];

                    // Only draw vertical line at the start of the first measure in the row
                    if measure_idx_in_row == 0 {
                        painter.line_segment(
                            [
                                egui::pos2(x_offset, y_offset + string_spacing),
                                egui::pos2(
                                    x_offset,
                                    y_offset + string_spacing * (num_strings as f32),
                                ),
                            ],
                            egui::Stroke::new(1.0, egui::Color32::BLACK),
                        );
                    }

                    // Calculate total dashes per measure
                    let dashes_per_division = self.configs.dashes_per_division;
                    let total_divisions = measure.positions.len();
                    let total_dashes = total_divisions * dashes_per_division;

                    // Draw notes and dashes
                    for dash_idx in 0..total_dashes {
                        let x = x_offset + dash_idx as f32 * note_spacing;

                        // Calculate corresponding division and sub-division indices
                        let division_idx = dash_idx / dashes_per_division;

                        if division_idx < measure.positions.len() {
                            // For each note in this division
                            for note in &measure.positions[division_idx] {
                                if let (Some(string), Some(fret)) = (note.string, note.fret) {
                                    let string_idx = string - 1;
                                    let y = y_offset + string_spacing * (string_idx as f32 + 1.0);

                                    // Only draw the note at the first dash of the division
                                    if dash_idx % dashes_per_division == 0 {
                                        // Draw the fret number at (x, y)
                                        let text = fret.to_string();
                                        painter.text(
                                            egui::pos2(x, y),
                                            egui::Align2::LEFT_CENTER,
                                            text,
                                            egui::FontId::monospace(14.0),
                                            egui::Color32::BLACK,
                                        );
                                    }
                                }
                            }
                        }
                    }

                    // Move x_offset to the end of the measure
                    x_offset += total_dashes as f32 * note_spacing;

                    // Always draw vertical line at the end of measure
                    painter.line_segment(
                        [
                            egui::pos2(x_offset, y_offset + string_spacing),
                            egui::pos2(x_offset, y_offset + string_spacing * (num_strings as f32)),
                        ],
                        egui::Stroke::new(1.0, egui::Color32::BLACK),
                    );

                    x_offset += measure_spacing;
                }

                y_offset += num_strings as f32 * string_spacing + row_spacing;
            }

            // Draw the vertical line at the current measure and division
            if let (Some(current_measure), Some(current_division)) =
                (self.current_measure, self.current_division)
            {
                let row = current_measure / measures_per_row;
                let measure_idx_in_row = current_measure % measures_per_row;

                let dashes_per_division = self.configs.dashes_per_division;

                // Recalculate x_offset to find the exact position of the current division
                let mut x_offset = rect.min.x + left_padding;

                // Add the widths of the previous measures
                for idx in 0..measure_idx_in_row {
                    let measure = &score.measures[row * measures_per_row + idx];
                    let total_divisions = measure.positions.len();
                    let total_dashes = total_divisions * dashes_per_division;
                    x_offset += total_dashes as f32 * note_spacing + measure_spacing;
                }

                let x =
                    x_offset + current_division as f32 * dashes_per_division as f32 * note_spacing;

                let y_start = rect.min.y
                    + row as f32 * (num_strings as f32 * string_spacing + row_spacing)
                    + string_spacing;
                let y_end = y_start + num_strings as f32 * string_spacing - string_spacing;

                painter.line_segment(
                    [egui::pos2(x, y_start), egui::pos2(x, y_end)],
                    egui::Stroke::new(2.0, egui::Color32::RED),
                );
            }
        }
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
                            1..=8,
                        ))
                        .changed()
                    {
                        changed_rendered_score = true;
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Measure per row:");
                    if ui
                        .add(egui::Slider::new(&mut self.configs.measures_per_row, 1..=8))
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
            if let Some(score) = &self.score {
                ScrollArea::both()
                    .id_salt("tab_scroll_area")
                    .show(ui, |ui| {
                        // Wrap the content in a Frame with inner margin
                        egui::Frame::none()
                            .inner_margin(Margin {
                                left: 20.0,
                                right: 20.0, // Add a 20-pixel padding to the right
                                top: 10.0,
                                bottom: 0.0,
                            })
                            .show(ui, |ui| {
                                // Determine the desired size based on the score
                                let num_strings = 6;
                                let string_spacing = 20.0;
                                let note_spacing = 10.0; // base spacing
                                let measure_spacing = 10.0;
                                let row_spacing = 50.0;

                                let measures_per_row = self.configs.measures_per_row;
                                let dashes_per_division = self.configs.dashes_per_division;
                                let total_measures = score.measures.len();
                                let total_rows =
                                    (total_measures + measures_per_row - 1) / measures_per_row;

                                // Calculate total width
                                let total_width = (0..measures_per_row)
                                    .map(|measure_idx_in_row| {
                                        if let Some(measure) =
                                            score.measures.get(measure_idx_in_row)
                                        {
                                            let total_divisions = measure.positions.len();
                                            total_divisions
                                                * dashes_per_division
                                                * note_spacing as usize
                                        } else {
                                            0
                                        }
                                    })
                                    .sum::<usize>()
                                    as f32
                                    + measures_per_row as f32 * measure_spacing;

                                let total_height = total_rows as f32
                                    * (num_strings as f32 * string_spacing + row_spacing);

                                let desired_size = egui::Vec2::new(total_width, total_height);

                                let (rect, _response) =
                                    ui.allocate_exact_size(desired_size, egui::Sense::hover());
                                let painter = ui.painter_at(rect);
                                self.render_tab(&painter, rect);
                            });
                    });
            }
        });

        if let (Some(receiver), Some(score)) = (&self.notes_receiver, &self.score) {
            while let Ok((notes, division, measure)) = receiver.try_recv() {
                self.current_division = Some(division);
                self.current_measure = Some(measure);
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
