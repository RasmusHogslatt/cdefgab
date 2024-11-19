// gui.rs

use crate::audio::audio_listener::AudioListener;
use crate::audio::audio_player::AudioPlayer;
use crate::guitar::guitar::{GuitarConfig, GuitarType};
use crate::music_representation::{Measure, Note, Score, Technique};
use crate::renderer::renderer::{score_info, Renderer};
use crate::time_scrubber::time_scrubber::TimeScrubber;

use eframe::egui;
use egui::epaint::{PathStroke, QuadraticBezierShape};
use egui::{Margin, ScrollArea, Vec2};
use egui_file::FileDialog;
use egui_plot::{Line, Plot, PlotPoints};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver},
    Arc, Mutex,
};
use std::{env, thread};
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

#[derive(Clone)]
pub struct Configs {
    pub custom_tempo: usize,
    pub use_custom_tempo: bool,
    pub file_path: Option<PathBuf>,
    pub measures_per_row: usize,
    pub dashes_per_division: usize,
    pub guitar_configs: Vec<GuitarConfig>,
    pub active_guitar: usize,
}

pub struct DisplayMetrics {
    pub total_score_time: f32,
}

impl Configs {
    pub fn new() -> Self {
        Self {
            active_guitar: 0,
            guitar_configs: vec![
                GuitarConfig::custom(
                    0.996, // decay
                    0.5,   // string_damping
                    100.0, // body_resonance
                    0.5,   // body_damping
                    0.7,   // string_tension
                    25.5,  // scale_length
                    0,     // capo_fret
                    0.5,   // volume
                ),
                GuitarConfig::acoustic(),
                GuitarConfig::electric(),
                GuitarConfig::classical(),
                GuitarConfig::bass_guitar(),
                GuitarConfig::twelve_string(),
            ],
            custom_tempo: 120,
            use_custom_tempo: false,
            file_path: Some(PathBuf::from("test_music.xml")),
            measures_per_row: 4,
            dashes_per_division: 2,
        }
    }
}

pub struct TabApp {
    score: Option<Score>,
    renderer: Renderer,
    playback_handle: Option<thread::JoinHandle<()>>,
    notes_receiver: Option<Receiver<(Vec<Note>, usize, usize)>>, // Notes, current division, current measure
    is_playing: bool,
    stop_flag: Arc<AtomicBool>,
    configs: Configs,
    display_metrics: DisplayMetrics,
    previous_notes: Option<Vec<Note>>,
    current_notes: Option<Vec<Note>>,
    audio_player: AudioPlayer,
    audio_listener: AudioListener,
    match_result_receiver: Receiver<bool>,
    expected_notes: Arc<Mutex<Option<Vec<Note>>>>,
    is_match: bool,
    input_chroma_history: Arc<Mutex<Vec<Vec<f32>>>>,
    expected_chroma_history: Arc<Mutex<Vec<Vec<f32>>>>,
    input_signal_history: Arc<Mutex<Vec<Vec<f32>>>>,
    current_measure: Option<usize>,
    current_division: Option<usize>,
    last_division: Option<usize>,
    matching_threshold: Arc<Mutex<f32>>,
    silence_threshold: Arc<Mutex<f32>>,
    open_file_dialog: Option<FileDialog>,
}

impl TabApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let configs = Configs::new();
        let display_metrics = DisplayMetrics {
            total_score_time: 0.0,
        };
        let file_path = configs.file_path.clone();
        let score = match &file_path {
            Some(path) => Score::parse_from_musicxml(path).ok(),
            None => None,
        };
        let renderer = Renderer::new(configs.measures_per_row, configs.dashes_per_division);

        let stop_flag = Arc::new(AtomicBool::new(false));

        let audio_player_configs = configs.guitar_configs[configs.active_guitar].clone();
        let audio_player =
            AudioPlayer::new(audio_player_configs).expect("Failed to initialize AudioPlayer");
        audio_player.start().expect("Failed to start AudioPlayer");

        let (match_result_sender, match_result_receiver) = mpsc::channel();
        let expected_notes = Arc::new(Mutex::new(None));

        let matching_threshold = Arc::new(Mutex::new(0.8)); // Default value
        let silence_threshold = Arc::new(Mutex::new(0.01)); // Default value
        let mut audio_listener = AudioListener::new(
            match_result_sender.clone(),
            expected_notes.clone(),
            matching_threshold.clone(),
            silence_threshold.clone(),
        )
        .expect("Failed to initialize AudioListener");
        audio_listener
            .start()
            .expect("Failed to start AudioListener");

        let input_chroma_history = audio_listener.input_chroma_history.clone();
        let expected_chroma_history = audio_listener.expected_chroma_history.clone();
        let input_signal_history = audio_listener.input_signal_history.clone();

        Self {
            score,
            renderer,
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
            last_division: None,
            matching_threshold,
            silence_threshold,
            open_file_dialog: None,
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
        let configs = self.configs.guitar_configs[self.configs.active_guitar].clone();
        self.audio_player.update_configs(configs);
    }

    fn render_tab(&self, painter: &egui::Painter, rect: egui::Rect) {
        // Constants for rendering
        let num_strings = 6;
        let string_spacing = 20.0; // pixels between strings
        let note_spacing = 10.0; // base pixels between dashes
        let measure_spacing = 10.0; // spacing between measures
        let row_spacing = 50.0; // vertical spacing between rows

        if let Some(score) = &self.score {
            let total_measures = score.measures.len();
            let measures_per_row = self.configs.measures_per_row;
            let total_rows = (total_measures + measures_per_row - 1) / measures_per_row;

            // Start drawing at rect.min, which already includes the padding
            let mut y_offset = rect.min.y;

            for row in 0..total_rows {
                let measures_in_row = if (row + 1) * measures_per_row <= total_measures {
                    measures_per_row
                } else {
                    total_measures % measures_per_row
                };

                // Calculate the total width of the current row
                let row_width = self.calculate_row_width(
                    &score,
                    row,
                    measures_in_row,
                    note_spacing,
                    measure_spacing,
                );

                // Draw strings (horizontal lines) for the current row
                self.draw_strings(
                    painter,
                    rect.min.x, // Start from rect.min.x, padding is already included
                    y_offset,
                    row_width,
                    num_strings,
                    string_spacing,
                );

                // Adjust x_offset to start from rect.min.x
                let mut x_offset = rect.min.x;
                for measure_idx_in_row in 0..measures_in_row {
                    let measure_idx = row * measures_per_row + measure_idx_in_row;
                    let measure = &score.measures[measure_idx];

                    // Determine if we need to draw the starting vertical line
                    let draw_start_line = measure_idx_in_row == 0;

                    // Draw measure
                    self.draw_measure(
                        painter,
                        measure,
                        x_offset,
                        y_offset,
                        num_strings,
                        string_spacing,
                        note_spacing,
                        draw_start_line,
                    );

                    // Move x_offset to the end of the measure
                    let total_dashes = measure.positions.len() * self.configs.dashes_per_division;
                    x_offset += total_dashes as f32 * note_spacing + measure_spacing;
                }

                y_offset += num_strings as f32 * string_spacing + row_spacing;
            }

            // Draw the playback position indicator (if applicable)
            if let (Some(current_measure), Some(current_division)) =
                (self.current_measure, self.current_division)
            {
                self.draw_playback_indicator(
                    painter,
                    rect.min.x,
                    rect.min.y,
                    current_measure,
                    current_division,
                    measures_per_row,
                    &score,
                    num_strings,
                    string_spacing,
                    note_spacing,
                    measure_spacing,
                    row_spacing,
                );
            }
        }
    }

    /// Calculates the total width of a row of measures.
    fn calculate_row_width(
        &self,
        score: &Score,
        row: usize,
        measures_in_row: usize,
        note_spacing: f32,
        measure_spacing: f32,
    ) -> f32 {
        let measures_per_row = self.configs.measures_per_row;
        let dashes_per_division = self.configs.dashes_per_division;
        let mut row_width = 0.0;

        for measure_idx_in_row in 0..measures_in_row {
            let measure_idx = row * measures_per_row + measure_idx_in_row;
            let measure = &score.measures[measure_idx];
            let total_divisions = measure.positions.len();
            let total_dashes = total_divisions * dashes_per_division;
            row_width += total_dashes as f32 * note_spacing + measure_spacing;
        }
        // Subtract the extra measure_spacing added after the last measure
        row_width -= measure_spacing;

        row_width
    }

    /// Draws the strings for a row.
    fn draw_strings(
        &self,
        painter: &egui::Painter,
        x_start: f32,
        y_offset: f32,
        row_width: f32,
        num_strings: usize,
        string_spacing: f32,
    ) {
        for string_idx in 0..num_strings {
            let y = y_offset + string_spacing * (string_idx as f32 + 1.0);
            painter.line_segment(
                [egui::pos2(x_start, y), egui::pos2(x_start + row_width, y)],
                egui::Stroke::new(1.0, egui::Color32::BLACK),
            );
        }
    }

    fn draw_measure(
        &self,
        painter: &egui::Painter,
        measure: &Measure,
        x_offset: f32,
        y_offset: f32,
        num_strings: usize,
        string_spacing: f32,
        note_spacing: f32,
        draw_start_line: bool,
    ) {
        let dashes_per_division = self.configs.dashes_per_division;
        let total_divisions = measure.positions.len();
        let total_dashes = total_divisions * dashes_per_division;

        // Draw vertical line at the start of the measure if needed
        if draw_start_line {
            painter.line_segment(
                [
                    egui::pos2(x_offset, y_offset + string_spacing),
                    egui::pos2(x_offset, y_offset + string_spacing * (num_strings as f32)),
                ],
                egui::Stroke::new(1.0, egui::Color32::BLACK),
            );
        }

        // Store positions of notes for drawing techniques
        let mut note_positions: Vec<(egui::Pos2, &Note)> = Vec::new();

        // Draw notes and collect positions
        for division_idx in 0..total_divisions {
            let position_in_dashes = division_idx * dashes_per_division;
            let x = x_offset + position_in_dashes as f32 * note_spacing;

            // Draw notes in this division
            for note in &measure.positions[division_idx] {
                if let (Some(string), Some(fret)) = (note.string, note.fret) {
                    let string_idx = string - 1;
                    let y = y_offset + string_spacing * (string_idx as f32 + 1.0);

                    // Draw the fret number at (x, y)
                    let text = fret.to_string();
                    painter.text(
                        egui::pos2(x, y),
                        egui::Align2::LEFT_CENTER,
                        text,
                        egui::FontId::monospace(14.0),
                        egui::Color32::BLACK,
                    );

                    // Store the position and note
                    note_positions.push((egui::pos2(x, y), note));
                }
            }
        }

        // After drawing notes, draw hammer-on and pull-off arcs
        if note_positions.len() >= 2 {
            for i in 0..note_positions.len() - 1 {
                let (current_pos, current_note) = note_positions[i];
                let (next_pos, next_note) = note_positions[i + 1];

                // Only draw if the notes are on the same string and the next note has a technique
                if current_note.string == next_note.string {
                    match next_note.technique {
                        Technique::HammerOn | Technique::PullOff => {
                            // Draw an arc between current_pos and next_pos
                            let control_point = egui::pos2(
                                (current_pos.x + next_pos.x) / 2.0,
                                current_pos.y - 20.0, // Adjust as needed
                            );

                            // Construct the QuadraticBezierShape
                            let bezier = QuadraticBezierShape {
                                points: [current_pos, control_point, next_pos],
                                closed: false,
                                fill: egui::Color32::TRANSPARENT,
                                stroke: PathStroke::new(1.0, egui::Color32::BLACK),
                            };

                            // Add the shape to the painter
                            painter.add(egui::Shape::QuadraticBezier(bezier));

                            // Optionally, label the technique
                            let label = match next_note.technique {
                                Technique::HammerOn => "H",
                                Technique::PullOff => "P",
                                _ => "",
                            };
                            painter.text(
                                egui::pos2(control_point.x, control_point.y - 5.0),
                                egui::Align2::CENTER_BOTTOM,
                                label,
                                egui::FontId::monospace(12.0),
                                egui::Color32::BLACK,
                            );
                        }
                        _ => {}
                    }
                }
            }
        }

        // Draw vertical line at the end of the measure
        let x_end = x_offset + total_dashes as f32 * note_spacing;
        painter.line_segment(
            [
                egui::pos2(x_end, y_offset + string_spacing),
                egui::pos2(x_end, y_offset + string_spacing * (num_strings as f32)),
            ],
            egui::Stroke::new(1.0, egui::Color32::BLACK),
        );
    }

    /// Draws the playback position indicator (red vertical line).
    fn draw_playback_indicator(
        &self,
        painter: &egui::Painter,
        x_start: f32,
        y_start: f32,
        current_measure: usize,
        current_division: usize,
        measures_per_row: usize,
        score: &Score,
        num_strings: usize,
        string_spacing: f32,
        note_spacing: f32,
        measure_spacing: f32,
        row_spacing: f32,
    ) {
        let dashes_per_division = self.configs.dashes_per_division;

        let row = current_measure / measures_per_row;
        let measure_idx_in_row = current_measure % measures_per_row;

        // Recalculate x_offset to find the exact position of the current division
        let mut x_offset = x_start;

        // Add the widths of the previous measures in the row
        for idx in 0..measure_idx_in_row {
            let measure = &score.measures[row * measures_per_row + idx];
            let total_divisions = measure.positions.len();
            let total_dashes = total_divisions * dashes_per_division;
            x_offset += total_dashes as f32 * note_spacing + measure_spacing;
        }

        // Add the positions within the current measure
        let x = x_offset + current_division as f32 * dashes_per_division as f32 * note_spacing;

        let y_offset = y_start + row as f32 * (num_strings as f32 * string_spacing + row_spacing);
        let y_top = y_offset + string_spacing;
        let y_bottom = y_top + string_spacing * (num_strings as f32 - 1.0);

        painter.line_segment(
            [egui::pos2(x, y_top), egui::pos2(x, y_bottom)],
            egui::Stroke::new(2.0, egui::Color32::RED),
        );
    }

    fn handle_playback_messages(&mut self) {
        if let (Some(receiver), Some(score)) = (&self.notes_receiver, &self.score) {
            while let Ok((notes, division, measure)) = receiver.try_recv() {
                self.last_division = self.current_division.clone();
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
                    let duration = seconds_per_division * notes[0].duration as f32;
                    self.audio_player.play_notes(&notes, duration);
                }
            }
        }
    }

    fn handle_match_results(&mut self) {
        while let Ok(is_match) = self.match_result_receiver.try_recv() {
            if self.is_playing
                && self.current_notes.is_some()
                && self.current_division != self.last_division
            {
                self.is_match = is_match;
            } else {
                self.is_match = false;
            }
        }
    }

    fn update_display_metrics(&mut self) {
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
    }

    fn render_plots(&self, ui: &mut egui::Ui) {
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
                .view_aspect(2.0)
                .include_y(-1.1)
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
    }

    fn render_tab_view(&self, ui: &mut egui::Ui) {
        ui.heading("Tablature");
        if let Some(score) = &self.score {
            ScrollArea::both()
                .id_salt("tab_scroll_area")
                .show(ui, |ui| {
                    // Wrap the content in a Frame with inner margin
                    egui::Frame::none()
                        .inner_margin(Margin::same(20.0)) // Add 20.0 padding to all sides
                        .show(ui, |ui| {
                            // Determine the desired size based on the score
                            let desired_size = self.calculate_tab_size(score);
                            let (rect, _response) =
                                ui.allocate_exact_size(desired_size, egui::Sense::hover());
                            let painter = ui.painter_at(rect);
                            self.render_tab(&painter, rect);
                        });
                });
        }
    }

    fn calculate_tab_size(&self, score: &Score) -> Vec2 {
        let num_strings = 6;
        let string_spacing = 20.0;
        let note_spacing = 10.0;
        let measure_spacing = 10.0;
        let row_spacing = 50.0;

        let measures_per_row = self.configs.measures_per_row;
        let dashes_per_division = self.configs.dashes_per_division;
        let total_measures = score.measures.len();
        let total_rows = (total_measures + measures_per_row - 1) / measures_per_row;

        // Calculate total width
        let total_width = (0..measures_per_row)
            .map(|measure_idx_in_row| {
                if let Some(measure) = score.measures.get(measure_idx_in_row) {
                    let total_divisions = measure.positions.len();
                    total_divisions * dashes_per_division * note_spacing as usize
                } else {
                    0
                }
            })
            .sum::<usize>() as f32
            + measures_per_row as f32 * measure_spacing;

        let total_height = total_rows as f32 * (num_strings as f32 * string_spacing + row_spacing);

        // Add padding to the total size (20.0 pixels on each side)
        let padding = 40.0; // 20.0 pixels on each side
        Vec2::new(total_width + padding, total_height + padding)
    }
}

impl eframe::App for TabApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_playback_messages();
        self.handle_match_results();
        self.update_display_metrics();

        let mut changed_config = false;
        let mut changed_rendered_score = false;

        egui::SidePanel::left("left_panel").show(ctx, |ui| {
            self.ui_playback_controls(ui, &mut changed_config);
            self.ui_guitar_settings(ui, &mut changed_config);
            self.ui_render_settings(ui, &mut changed_rendered_score);
            self.ui_audio_matching_settings(ui);
            self.ui_current_notes(ui);
            self.ui_match_status(ui);
        });

        egui::Window::new("Input plot")
            .fixed_size(Vec2::new(400.0, 400.0))
            .show(ctx, |ui| {
                self.render_plots(ui);
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

            self.render_tab_view(ui);
        });

        if changed_config {
            self.update_audio_player_configs();
        }

        // Handle file dialog
        if self.open_file_dialog.is_some() {
            // Create a temporary variable to hold the selected file
            let selected_file = {
                // Limit the scope of the mutable borrow
                let dialog = self.open_file_dialog.as_mut().unwrap();
                if dialog.show(ctx).selected() {
                    dialog.path().map(|p| p.to_path_buf())
                } else {
                    None
                }
            };

            // Now we can safely borrow `self` mutably again
            if let Some(file) = selected_file {
                // Close the dialog
                self.open_file_dialog = None;

                // Stop any existing playback
                self.stop_playback();

                // Update the configs.file_path
                self.configs.file_path = Some(file.clone());

                // Reload the score
                match Score::parse_from_musicxml(&file) {
                    Ok(new_score) => {
                        self.score = Some(new_score);
                        self.update_display_metrics();
                        // Reset any necessary state
                        self.current_measure = None;
                        self.current_division = None;
                        self.last_division = None;
                    }
                    Err(err) => {
                        // Handle parse error
                        self.score = None;
                        // Show error message
                        eprintln!("Error parsing MusicXML file: {}", err);
                    }
                }
            }
        }

        // Check if playback has finished
        if self.is_playing && self.stop_flag.load(Ordering::Relaxed) {
            self.is_playing = false;
            self.current_notes = None;
            self.previous_notes = None;
            self.is_match = false;
        }

        ctx.request_repaint();
    }
}

impl TabApp {
    fn ui_audio_matching_settings(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.heading("Audio Matching Settings");

            // Matching Threshold
            ui.horizontal(|ui| {
                ui.label("Matching Threshold:");
                let mut matching_threshold = *self.matching_threshold.lock().unwrap();
                if ui
                    .add(egui::Slider::new(&mut matching_threshold, 0.0..=1.0).step_by(0.01))
                    .changed()
                {
                    *self.matching_threshold.lock().unwrap() = matching_threshold;
                }
            });

            // Silence Threshold
            ui.horizontal(|ui| {
                ui.label("Silence Threshold:");
                let mut silence_threshold = *self.silence_threshold.lock().unwrap();
                if ui
                    .add(egui::Slider::new(&mut silence_threshold, 0.0..=0.7).step_by(0.001))
                    .changed()
                {
                    *self.silence_threshold.lock().unwrap() = silence_threshold;
                }
            });
        });
    }

    fn ui_playback_controls(&mut self, ui: &mut egui::Ui, changed_config: &mut bool) {
        ui.group(|ui| {
            ui.heading("Playback Controls");
            ui.horizontal(|ui| {
                if ui.button("Play").clicked() {
                    self.start_playback();
                }
                if ui.button("Stop").clicked() {
                    self.stop_playback();
                }
                if ui.button("Open File").clicked() {
                    // Stop any existing playback
                    self.stop_playback();

                    // Filter for .xml files
                    let filter = Box::new({
                        let ext = Some(OsStr::new("xml"));
                        move |path: &Path| -> bool { path.extension() == ext }
                    });

                    // Set the initial directory to the current working directory
                    let current_dir = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
                    let mut dialog =
                        FileDialog::open_file(Some(current_dir)).show_files_filter(filter);
                    dialog.open();
                    self.open_file_dialog = Some(dialog);
                }
            });
            ui.horizontal(|ui| {
                ui.label("Volume:");
                let active_guitar_config =
                    &mut self.configs.guitar_configs[self.configs.active_guitar];
                if ui
                    .add(
                        egui::Slider::new(&mut active_guitar_config.volume, 0.0..=1.0)
                            .step_by(0.01),
                    )
                    .changed()
                {
                    *changed_config = true;
                }
            });
            ui.horizontal(|ui| {
                ui.label("Set custom tempo:");
                ui.checkbox(&mut self.configs.use_custom_tempo, "");
            });
            if self.configs.use_custom_tempo {
                if ui
                    .add(egui::Slider::new(&mut self.configs.custom_tempo, 1..=240))
                    .changed()
                {
                    *changed_config = true;
                }
            }
            ui.label(format!(
                "Total score time: {:.2} seconds",
                self.display_metrics.total_score_time
            ));
            ui.label("Capo fret:");
            let active_guitar_config = &mut self.configs.guitar_configs[self.configs.active_guitar];
            if ui
                .add(egui::Slider::new(&mut active_guitar_config.capo_fret, 0..=24).text("Fret"))
                .changed()
            {
                *changed_config = true;
            }
        });
    }

    fn ui_guitar_settings(&mut self, ui: &mut egui::Ui, changed_config: &mut bool) {
        ui.group(|ui| {
            ui.heading("Guitar Profile");
            egui::ComboBox::from_label("Guitar Type")
                .selected_text(format!(
                    "{}",
                    self.configs.guitar_configs[self.configs.active_guitar].name
                ))
                .show_ui(ui, |ui| {
                    for (index, guitar) in self.configs.guitar_configs.iter().enumerate() {
                        let checked = index == self.configs.active_guitar;
                        if ui
                            .selectable_label(checked, format!("{}", &guitar.name))
                            .clicked()
                        {
                            self.configs.active_guitar = index;
                            *changed_config = true;
                        }
                    }
                });

            if let GuitarType::Custom = self.configs.guitar_configs[self.configs.active_guitar].name
            {
                egui::Grid::new("custom_guitar_config")
                    .num_columns(2)
                    .show(ui, |ui| {
                        let custom_config =
                            &mut self.configs.guitar_configs[self.configs.active_guitar];

                        ui.label("Decay:");
                        if ui
                            .add(
                                egui::Slider::new(&mut custom_config.decay, 0.9..=1.0)
                                    .step_by(0.001),
                            )
                            .changed()
                        {
                            *changed_config = true;
                        }
                        ui.end_row();

                        ui.label("String Damping:");
                        if ui
                            .add(
                                egui::Slider::new(&mut custom_config.string_damping, 0.0..=1.0)
                                    .step_by(0.001),
                            )
                            .changed()
                        {
                            *changed_config = true;
                        }
                        ui.end_row();

                        ui.label("Body Damping:");
                        if ui
                            .add(
                                egui::Slider::new(&mut custom_config.body_damping, 0.0..=1.0)
                                    .step_by(0.001),
                            )
                            .changed()
                        {
                            *changed_config = true;
                        }
                        ui.end_row();

                        ui.label("Body Resonance:");
                        if ui
                            .add(
                                egui::Slider::new(&mut custom_config.body_resonance, 0.0..=500.0)
                                    .step_by(0.1),
                            )
                            .changed()
                        {
                            *changed_config = true;
                        }
                        ui.end_row();

                        ui.label("String Tension:");
                        if ui
                            .add(
                                egui::Slider::new(&mut custom_config.string_tension, 0.0..=1.0)
                                    .step_by(0.001),
                            )
                            .changed()
                        {
                            *changed_config = true;
                        }
                        ui.end_row();

                        ui.label("Scale Length [inch]:");
                        if ui
                            .add(
                                egui::Slider::new(&mut custom_config.scale_length, 10.0..=50.0)
                                    .step_by(0.1),
                            )
                            .changed()
                        {
                            *changed_config = true;
                        }
                        ui.end_row();
                    });
            }
        });
    }

    fn ui_render_settings(&mut self, ui: &mut egui::Ui, changed_rendered_score: &mut bool) {
        ui.group(|ui| {
            ui.heading("Render Settings");
            ui.horizontal(|ui| {
                ui.label("Dashes per division:");
                if ui
                    .add(egui::Slider::new(
                        &mut self.configs.dashes_per_division,
                        1..=8,
                    ))
                    .changed()
                {
                    self.renderer.dashes_per_division = self.configs.dashes_per_division;
                    *changed_rendered_score = true;
                }
            });
            ui.horizontal(|ui| {
                ui.label("Measures per row:");
                if ui
                    .add(egui::Slider::new(&mut self.configs.measures_per_row, 1..=8))
                    .changed()
                {
                    self.renderer.measures_per_row = self.configs.measures_per_row;
                    *changed_rendered_score = true;
                }
            });
        });
    }

    fn ui_current_notes(&self, ui: &mut egui::Ui) {
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
    }

    fn ui_match_status(&self, ui: &mut egui::Ui) {
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
    }
}
