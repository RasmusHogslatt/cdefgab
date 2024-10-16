use musical_structures::{Measure, Score};

use crate::music_representation::*;

pub fn render_score(score: &Score, measures_per_row: usize, dashes_per_measure: usize) -> String {
    let mut rendered_output = String::new();
    for (i, measure) in score.measures.iter().enumerate() {
        if i % measures_per_row == 0 && i != 0 {
            rendered_output.push_str("\n");
        }
        rendered_output.push_str(&format!("Measure {}:\n", i + 1));
        rendered_output.push_str(&render_measure(measure, dashes_per_measure));
    }
    rendered_output
}
fn render_measure(measure: &Measure, dashes_per_measure: usize) -> String {
    let mut tab_lines = vec!["-".repeat(dashes_per_measure); 6];

    for note in &measure.notes {
        let string_index = (note.string - 1) as usize;
        let fret_position = note.fret.to_string();
        let position =
            tab_lines[string_index].len() / dashes_per_measure * note.beat_position as usize;
        tab_lines[string_index]
            .replace_range(position..position + fret_position.len(), &fret_position);
    }

    let mut rendered_measure = String::new();
    for line in &tab_lines {
        rendered_measure.push_str(&format!("{}\n", line));
    }
    rendered_measure
}
