// renderer.rs

use crate::music_representation::musical_structures::{Measure, Score};

pub fn score_info(score: &Score) -> String {
    let info = format!(
        "Time signature: {}/{}\n\
        Tempo: {}\n\
        Divisions per quarter note: {}\n\
        Divisions per measure: {}\n\
        Number of measures: {}",
        score.time_signature.beats_per_measure,
        score.time_signature.beat_value,
        score.tempo,
        score.divisions_per_quarter,
        score.divisions_per_measure,
        score.measures.len(),
    );
    info
}

pub fn render_score(score: &Score, measures_per_row: usize, dashes_per_division: usize) -> String {
    let mut rendered_output = String::new();
    let total_measures = score.measures.len();
    let mut measure_index = 0;

    while measure_index < total_measures {
        let end_index = (measure_index + measures_per_row).min(total_measures);
        let tab_lines = render_measure_row(
            &score.measures[measure_index..end_index],
            dashes_per_division,
        );
        for line in tab_lines.iter() {
            rendered_output.push_str(&line);
            rendered_output.push('\n');
        }
        rendered_output.push('\n');
        measure_index = end_index;
    }

    rendered_output
}

fn render_measure_row(measures: &[Measure], dashes_per_division: usize) -> Vec<String> {
    let mut tab_lines: Vec<String> = vec![String::new(); 6];

    for measure in measures {
        let measure_tab_lines = render_measure(measure, dashes_per_division);
        for i in 0..6 {
            tab_lines[i].push('|');
            tab_lines[i].push_str(&measure_tab_lines[i]);
            tab_lines[i].push('|');
        }
    }

    tab_lines
}

pub fn render_measure(measure: &Measure, dashes_per_division: usize) -> Vec<String> {
    let total_divisions = measure.positions.len();
    let dashes_per_measure = dashes_per_division * total_divisions;
    let mut tab_lines: Vec<Vec<char>> = vec![vec!['-'; dashes_per_measure]; 6];

    for (division_index, notes) in measure.positions.iter().enumerate() {
        let position_in_dashes = division_index * dashes_per_division;
        for note in notes.values() {
            if let (Some(string), Some(fret)) = (note.string, note.fret) {
                insert_note_into_tab_line(
                    &mut tab_lines,
                    string,
                    fret,
                    position_in_dashes,
                    dashes_per_measure,
                );
            }
        }
    }

    tab_lines
        .iter()
        .map(|line| line.iter().collect::<String>())
        .collect()
}

fn insert_note_into_tab_line(
    tab_lines: &mut [Vec<char>],
    string: u8,
    fret: u8,
    position_in_dashes: usize,
    dashes_per_measure: usize,
) {
    let string_index = (string - 1) as usize;
    let fret_str = fret.to_string();
    let position = if position_in_dashes + fret_str.len() <= dashes_per_measure {
        position_in_dashes
    } else if fret_str.len() <= dashes_per_measure {
        dashes_per_measure - fret_str.len()
    } else {
        return;
    };

    for (i, c) in fret_str.chars().enumerate() {
        if position + i < dashes_per_measure {
            tab_lines[string_index][position + i] = c;
        }
    }
}
