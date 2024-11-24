// gui.rs

// use crate::audio::audio_listener::AudioListener;
use crate::audio::audio_player::AudioPlayer;
use crate::guitar::guitar::{GuitarConfig, GuitarType};
use crate::music_representation::{Measure, Note, Score, Technique};
use crate::renderer::renderer::{score_info, Renderer};
use crate::time_scrubber::time_scrubber::TimeScrubber;

use eframe::egui;
use egui::epaint::{PathStroke, QuadraticBezierShape};
use egui::{Margin, ScrollArea, Vec2};
use egui_plot::{Line, Plot, PlotBounds, PlotPoints};
use rustfft::num_complex::Complex;
use rustfft::FftPlanner;

use std::path::PathBuf;
use std::sync::mpsc::{channel, Sender};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver},
    Arc, Mutex,
};
use std::thread;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;
#[cfg(target_arch = "wasm32")]
use web_sys::{Event, HtmlInputElement};
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
            file_path: Some(PathBuf::from("silent.xml")),
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
    match_result_receiver: Receiver<bool>,
    expected_notes: Arc<Mutex<Option<Vec<Note>>>>,
    is_match: bool,
    output_signal: Arc<Mutex<Vec<f32>>>,
    current_measure: Option<usize>,
    current_division: Option<usize>,
    last_division: Option<usize>,
    plot_length: usize,
    plot_frequency_range: (usize, usize),
    score_channel: (Sender<Score>, Receiver<Score>),
}
#[cfg(not(target_arch = "wasm32"))]
fn execute<F>(f: F)
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    // Spawn a new thread to run the future

    use futures::executor::block_on;
    std::thread::spawn(move || {
        // Run the future to completion
        block_on(f);
    });
}
#[cfg(target_arch = "wasm32")]
fn execute<F: std::future::Future<Output = ()> + 'static>(f: F) {
    wasm_bindgen_futures::spawn_local(f);
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

        let (_match_result_sender, match_result_receiver) = mpsc::channel();
        let expected_notes = Arc::new(Mutex::new(None));

        let output_signal_history = audio_player.output_signal.clone();
        let score_channel = channel();
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
            match_result_receiver,
            expected_notes,
            is_match: false,
            current_measure: None,
            current_division: None,
            last_division: None,
            output_signal: output_signal_history,
            plot_length: 2048,
            plot_frequency_range: (50, 7500),
            score_channel,
        }
    }

    fn render_plots(&mut self, ui: &mut egui::Ui) {
        let output_signal = self.output_signal.lock().unwrap();
        let len = output_signal.len();

        if len > 0 {
            let n = self.plot_length.min(len);

            let start = len - n;
            let output_slice = &output_signal[start..];

            // Normalize the time-domain signal (optional)
            let max_amplitude = output_slice
                .iter()
                .map(|&x| x.abs())
                .fold(0.0_f32, f32::max);
            let normalized_output: Vec<f32> = if max_amplitude > 0.0 {
                output_slice.iter().map(|&x| x / max_amplitude).collect()
            } else {
                output_slice.to_vec()
            };
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label("Samples:");
                    ui.add(egui::Slider::new(&mut self.plot_length, 256..=16384).step_by(256.0));
                });

                // Plot Time-Domain Signal
                let plot_points: PlotPoints = (0..n)
                    .map(|i| [i as f64, normalized_output[i] as f64])
                    .collect();

                let line = Line::new(plot_points);

                Plot::new("Time Domain")
                    .view_aspect(3.0)
                    .include_y(-1.1)
                    .include_y(1.1)
                    .include_x(0.0)
                    .include_x(n as f64)
                    .x_axis_label("Sample Index")
                    .y_axis_label("Amplitude")
                    .show(ui, |plot_ui| {
                        plot_ui.line(line);
                    });
            });

            // Compute FFT
            let mut planner = FftPlanner::new();
            let fft = planner.plan_fft_forward(n);

            // Prepare complex input
            let mut input: Vec<Complex<f32>> = normalized_output
                .iter()
                .map(|&x| Complex { re: x, im: 0.0 })
                .collect();

            // Apply Hanning window
            for (i, sample) in input.iter_mut().enumerate() {
                let multiplier =
                    0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / n as f32).cos());
                sample.re *= multiplier;
            }

            // Perform FFT in-place
            fft.process(&mut input);

            // Compute magnitude spectrum in dB
            let epsilon = 1e-10_f64; // Small value to prevent log(0)
            let magnitude_spectrum_db: Vec<f64> = input
                .iter()
                .take(n / 2) // Only need first half of spectrum
                .map(|c| {
                    let mag = c.norm() as f64 + epsilon;
                    20.0 * mag.log10()
                })
                .collect();

            // Find the maximum magnitude in dB
            let max_db = magnitude_spectrum_db
                .iter()
                .cloned()
                .fold(f64::MIN, f64::max);

            // Normalize the magnitude spectrum so that the maximum is at 0 dB
            let normalized_magnitude_spectrum_db: Vec<f64> = magnitude_spectrum_db
                .iter()
                .map(|&db| db - max_db)
                .collect();

            // Prepare frequency axis
            let sample_rate = self.audio_player.sample_rate;
            let freq_resolution = sample_rate as f64 / n as f64;
            let frequencies: Vec<f64> = (0..n / 2).map(|i| i as f64 * freq_resolution).collect();

            // Prepare data points for plotting
            let spectrum_points_db: PlotPoints = frequencies
                .iter()
                .zip(normalized_magnitude_spectrum_db.iter())
                .map(|(&freq, &mag_db)| [freq, mag_db])
                .collect();

            let spectrum_line_db = Line::new(spectrum_points_db);

            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label("Frequency range (low, high):");
                    ui.horizontal(|ui| {
                        let max_low = self.plot_frequency_range.1 - 50;
                        let low_max = self.plot_frequency_range.0 + 50;
                        ui.add(egui::Slider::new(
                            &mut self.plot_frequency_range.0,
                            0..=max_low,
                        ));
                        ui.add(egui::Slider::new(
                            &mut self.plot_frequency_range.1,
                            low_max..=7500,
                        ));
                    });
                });

                // Plot Frequency-Domain Signal in dB
                Plot::new("Frequency-Amplitude")
                    .view_aspect(2.0)
                    .allow_scroll(false)
                    .allow_zoom(false)
                    .include_y(0.0)
                    .include_x(0.0)
                    .include_x(sample_rate as f64 / 2.0)
                    .x_axis_label("Frequency [Hz]")
                    .y_axis_label("Amplitude [dB]")
                    .label_formatter(|name, value| {
                        if !name.is_empty() {
                            format!("{}: {:.2} Hz, {:.2} dB", name, value.x, value.y)
                        } else {
                            format!("{:.2} Hz, {:.2} dB", value.x, value.y)
                        }
                    })
                    .show(ui, |plot_ui| {
                        // Define fixed bounds for the plot
                        let y_min = -200.0; // Set the lower bound of the y-axis
                        let y_max = 10.0; // Set the upper bound of the y-axis

                        // Apply the fixed bounds to the plot
                        plot_ui.set_plot_bounds(PlotBounds::from_min_max(
                            [self.plot_frequency_range.0 as f64, y_min],
                            [self.plot_frequency_range.1 as f64, y_max],
                        ));

                        plot_ui.line(spectrum_line_db);
                    });
            });
        } else {
            ui.label("No data to display");
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

        // Check if a new score has been received
        if let Ok(new_score) = self.score_channel.1.try_recv() {
            self.score = Some(new_score);
            // Reset any necessary state
            self.current_measure = None;
            self.current_division = None;
            self.last_division = None;
            // Any other state resets
        }

        egui::SidePanel::left("left_panel").show(ctx, |ui| {
            self.ui_playback_controls(ui, &mut changed_config);
            self.ui_guitar_settings(ui, &mut changed_config);
            self.ui_render_settings(ui, &mut changed_rendered_score);
            self.ui_current_notes(ui);
            if ui.button("Open File").clicked() {
                self.stop_playback();
                #[cfg(not(target_arch = "wasm32"))]
                {
                    let sender = self.score_channel.0.clone();
                    let task = rfd::AsyncFileDialog::new()
                        .add_filter("MusicXML", &["xml"])
                        .pick_file();
                    let ctx = ui.ctx().clone();

                    execute(async move {
                        if let Some(file) = task.await {
                            let data = file.read().await;
                            let xml_string = String::from_utf8_lossy(&data).to_string();

                            if let Ok(new_score) = Score::parse_from_musicxml_str(&xml_string) {
                                let _ = sender.send(new_score);
                            }
                        }
                        ctx.request_repaint();
                    });
                }

                #[cfg(target_arch = "wasm32")]
                {
                    use wasm_bindgen::prelude::*;
                    use wasm_bindgen::JsCast;
                    use web_sys::{Event, HtmlInputElement};

                    let document = web_sys::window().unwrap().document().unwrap();
                    let input = document.create_element("input").unwrap();
                    input.set_attribute("type", "file").unwrap();
                    input.set_attribute("accept", ".xml").unwrap();
                    input.set_attribute("style", "display: none;").unwrap();
                    let input: HtmlInputElement = input.dyn_into().unwrap();

                    let sender = self.score_channel.0.clone();
                    let ctx = ui.ctx().clone();

                    let closure = Closure::wrap(Box::new(move |event: Event| {
                        let input: HtmlInputElement = event.target().unwrap().dyn_into().unwrap();
                        if let Some(files) = input.files() {
                            if let Some(file) = files.get(0) {
                                let file_reader = web_sys::FileReader::new().unwrap();
                                let fr_c = file_reader.clone();
                                let sender_clone = sender.clone(); // Clone sender here
                                let ctx_clone = ctx.clone(); // Clone ctx if needed in inner closure
                                let onloadend = Closure::wrap(Box::new(move |_event: Event| {
                                    let result = fr_c.result().unwrap();
                                    let array = js_sys::Uint8Array::new(&result);
                                    let data = array.to_vec();
                                    let xml_string = String::from_utf8_lossy(&data).to_string();

                                    if let Ok(new_score) =
                                        Score::parse_from_musicxml_str(&xml_string)
                                    {
                                        let _ = sender_clone.send(new_score);
                                    }
                                    ctx_clone.request_repaint();
                                })
                                    as Box<dyn FnMut(_)>);

                                file_reader.set_onloadend(Some(onloadend.as_ref().unchecked_ref()));
                                file_reader.read_as_array_buffer(&file).unwrap();
                                onloadend.forget();
                            }
                        }
                    }) as Box<dyn FnMut(_)>);

                    input.set_onchange(Some(closure.as_ref().unchecked_ref()));
                    closure.forget();

                    // Add the input to the DOM and trigger the click
                    document.body().unwrap().append_child(&input).unwrap();
                    input.click();
                }
            }
        });
        if let Ok(new_score) = self.score_channel.1.try_recv() {
            self.score = Some(new_score);
            self.stop_playback(); // If you have a method to stop playback
            self.current_measure = None;
            self.current_division = None;
            self.last_division = None;
            // Reset other relevant state variables
        }

        egui::Window::new("Input plot")
            .fixed_size(Vec2::new(800.0, 800.0))
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

        // // Handle file dialog
        // if self.open_file_dialog.is_some() {
        //     // Create a temporary variable to hold the selected file
        //     let selected_file = {
        //         // Limit the scope of the mutable borrow
        //         let dialog = self.open_file_dialog.as_mut().unwrap();
        //         if dialog.show(ctx).selected() {
        //             dialog.path().map(|p| p.to_path_buf())
        //         } else {
        //             None
        //         }
        //     };

        //     // Now we can safely borrow `self` mutably again
        //     if let Some(file) = selected_file {
        //         // Close the dialog
        //         self.open_file_dialog = None;

        //         // Stop any existing playback
        //         self.stop_playback();

        //         // Update the configs.file_path
        //         self.configs.file_path = Some(file.clone());

        //         // Reload the score
        //         match Score::parse_from_musicxml(&file) {
        //             Ok(new_score) => {
        //                 self.score = Some(new_score);
        //                 self.update_display_metrics();
        //                 // Reset any necessary state
        //                 self.current_measure = None;
        //                 self.current_division = None;
        //                 self.last_division = None;
        //             }
        //             Err(err) => {
        //                 // Handle parse error
        //                 self.score = None;
        //                 // Show error message
        //                 eprintln!("Error parsing MusicXML file: {}", err);
        //             }
        //         }
        //     }
        // }

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
                    ui.label(format!(
                        "String: {}, Fret: {}",
                        string,
                        fret + self.configs.guitar_configs[self.configs.active_guitar].capo_fret
                    ));
                }
            }
        }

        ui.separator();

        ui.label("Previous Notes:");
        if let Some(previous_notes) = &self.previous_notes {
            for note in previous_notes.iter() {
                if let (Some(string), Some(fret)) = (note.string, note.fret) {
                    ui.label(format!(
                        "String: {}, Fret: {}",
                        string,
                        fret + self.configs.guitar_configs[self.configs.active_guitar].capo_fret
                    ));
                }
            }
        }
    }
}
