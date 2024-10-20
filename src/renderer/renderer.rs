// renderer.rs

use crate::music_representation::musical_structures::{Measure, Score};

pub fn score_info(score: &Score) {
    println!("----- SCORE INFO -----");
    println!(
        "Time signatuare: {}/{}",
        score.time_signature.beats_per_measure, score.time_signature.beat_value
    );
    println!("Tempo: {}", score.tempo);
    println!(
        "Divisions per quarter note: {}",
        score.divisions_per_quarter
    );
    println!("Divisions per measure: {}", score.divisions_per_measure);
    println!("Number of measures: {}", score.measures.len());
    println!("----------------------");
}

pub fn render_score(score: &Score, measures_per_row: usize, dashes_per_division: usize) -> String {
    let mut rendered_output = String::new();

    let total_measures = score.measures.len();
    let mut measure_index = 0;

    while measure_index < total_measures {
        // Process measures_per_row measures
        let end_index = (measure_index + measures_per_row).min(total_measures);
        // Initialize tab_lines for the row
        let mut tab_lines: Vec<String> = vec![String::new(); 6];

        // For each measure in the row
        for m in measure_index..end_index {
            let measure = &score.measures[m];
            let measure_tab_lines = render_measure(measure, dashes_per_division);

            for i in 0..6 {
                if m == measure_index {
                    tab_lines[i].push('|'); // Start with '|'
                }
                tab_lines[i].push_str(&measure_tab_lines[i]);
                tab_lines[i].push('|'); // Add '|' after measure
            }
        }

        // Append tab_lines to rendered_output
        for line in tab_lines.iter() {
            rendered_output.push_str(&line);
            rendered_output.push('\n');
        }
        rendered_output.push('\n'); // Separate rows

        measure_index = end_index;
    }

    rendered_output
}

pub fn render_measure(measure: &Measure, dashes_per_division: usize) -> Vec<String> {
    let total_divisions = measure.positions.len();
    let dashes_per_measure = dashes_per_division * total_divisions;

    let mut tab_lines: Vec<Vec<char>> = vec![vec!['-'; dashes_per_measure]; 6];

    for (division_index, notes) in measure.positions.iter().enumerate() {
        let position_in_dashes = division_index * dashes_per_division;

        for note in notes {
            if let (Some(string), Some(fret)) = (note.1.string, note.1.fret) {
                let string_index = (string - 1) as usize;
                let fret_str = fret.to_string();

                // Ensure position is within bounds
                let position = if position_in_dashes + fret_str.len() <= dashes_per_measure {
                    position_in_dashes
                } else if fret_str.len() <= dashes_per_measure {
                    dashes_per_measure - fret_str.len()
                } else {
                    // fret_str is longer than the measure width
                    continue; // Skip this note
                };

                // Insert fret number into the tab line
                for (i, c) in fret_str.chars().enumerate() {
                    if position + i < dashes_per_measure {
                        tab_lines[string_index][position + i] = c;
                    }
                }
            }
        }
    }

    // Build the rendered measure tab lines
    let measure_tab_lines: Vec<String> = tab_lines
        .iter()
        .map(|line| line.iter().collect::<String>())
        .collect();

    measure_tab_lines
}
