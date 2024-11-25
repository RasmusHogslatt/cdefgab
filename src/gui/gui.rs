// gui.rs

// use crate::audio::audio_listener::AudioListener;
use crate::audio::audio_player::AudioPlayer;
use crate::guitar::guitar::{GuitarConfig, GuitarType};
use crate::music_representation::{Measure, Note, Score, Technique};
use crate::renderer::renderer::{score_info, Renderer};

use eframe::egui;
use egui::epaint::{PathStroke, QuadraticBezierShape};
use egui::{Margin, ScrollArea, Vec2};
use instant::Instant;

use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::{channel, Sender};

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
    is_playing: bool,
    configs: Configs,
    display_metrics: DisplayMetrics,
    previous_notes: Option<Vec<Note>>,
    current_notes: Option<Vec<Note>>,
    audio_player: AudioPlayer,
    is_match: bool,
    last_division: Option<usize>,
    plot_length: usize,
    plot_frequency_range: (usize, usize),
    score_channel: (Sender<Score>, Receiver<Score>),
    playback_start_time: Option<Instant>,
    current_time: f32,
    current_measure_index: usize,
    current_division_index: usize,
    tempo: usize,
    last_played_measure_index: Option<usize>,
    last_played_division_index: Option<usize>,
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

        let audio_player_configs = configs.guitar_configs[configs.active_guitar].clone();
        let audio_player = AudioPlayer::new(audio_player_configs);

        let score_channel = channel();
        Self {
            score,
            renderer,
            is_playing: false,
            configs,
            display_metrics,
            previous_notes: None,
            current_notes: None,
            audio_player,
            is_match: false,
            last_division: None,
            plot_length: 2048,
            plot_frequency_range: (50, 7500),
            score_channel,
            playback_start_time: None,
            current_time: 0.0,
            current_measure_index: 0,
            current_division_index: 0,
            tempo: 120,
            last_played_measure_index: None,
            last_played_division_index: None,
        }
    }

    fn update_playback(&mut self) {
        if let Some(playback_start_time) = self.playback_start_time {
            let elapsed = playback_start_time.elapsed().as_secs_f32();
            self.current_time = elapsed;

            if let Some(score) = &self.score {
                let seconds_per_beat = 60.0 / self.tempo as f32;
                let seconds_per_division = seconds_per_beat / score.divisions_per_quarter as f32;
                let total_divisions_passed = (elapsed / seconds_per_division) as usize;

                let mut divisions_accum = 0;
                let mut measure_found = false;
                for (measure_idx, measure) in score.measures.iter().enumerate() {
                    let measure_divisions = measure.positions.len();
                    if divisions_accum + measure_divisions > total_divisions_passed {
                        self.current_measure_index = measure_idx;
                        self.current_division_index = total_divisions_passed - divisions_accum;
                        measure_found = true;
                        break;
                    } else {
                        divisions_accum += measure_divisions;
                    }
                }

                if measure_found {
                    // Check if we've moved to a new division
                    if Some(self.current_measure_index) != self.last_played_measure_index
                        || Some(self.current_division_index) != self.last_played_division_index
                    {
                        let measure = &score.measures[self.current_measure_index];
                        if self.current_division_index < measure.positions.len() {
                            let notes = measure.positions[self.current_division_index].clone();

                            if !notes.is_empty() {
                                let duration = seconds_per_division * notes[0].duration as f32;
                                self.audio_player.play_notes(&notes, duration);

                                self.previous_notes = self.current_notes.take();
                                self.current_notes = Some(notes.clone());
                            }
                            // Update the last played indices
                            self.last_played_measure_index = Some(self.current_measure_index);
                            self.last_played_division_index = Some(self.current_division_index);
                        }
                    }
                } else {
                    self.stop_playback();
                }
            }
        }
    }

    fn start_playback(&mut self) {
        if self.is_playing {
            return;
        }

        if let Some(score) = &self.score {
            // Start the audio player
            if let Err(e) = self.audio_player.start() {
                eprintln!("Failed to start AudioPlayer: {}", e);
                return;
            }

            self.is_playing = true;
            self.playback_start_time = Some(Instant::now());
            self.current_time = 0.0;
            self.current_measure_index = 0;
            self.current_division_index = 0;

            // Use custom tempo if set
            self.tempo = if self.configs.use_custom_tempo {
                self.configs.custom_tempo
            } else {
                score.tempo
            };
        }
    }

    fn stop_playback(&mut self) {
        if self.is_playing {
            self.is_playing = false;
            self.playback_start_time = None;
            self.current_time = 0.0;
            self.current_measure_index = 0;
            self.current_division_index = 0;
            self.current_notes = None;
            self.previous_notes = None;
            self.is_match = false;
            self.last_played_measure_index = None;
            self.last_played_division_index = None;
        }
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
            if self.is_playing {
                self.draw_playback_indicator(
                    painter,
                    rect.min.x,
                    rect.min.y,
                    self.current_measure_index,
                    self.current_division_index,
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
        if self.is_playing {
            self.update_playback();
        }
        self.update_display_metrics();

        let mut changed_config = false;
        let mut changed_rendered_score = false;

        // Check if a new score has been received
        if let Ok(new_score) = self.score_channel.1.try_recv() {
            self.score = Some(new_score);
            // Reset any necessary state
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
            self.last_division = None;
            // Reset other relevant state variables
        }

        egui::Window::new("Input plot")
            .fixed_size(Vec2::new(800.0, 800.0))
            .show(ctx, |ui| {
                // self.render_plots(ui);
                ui.label("TODO");
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
