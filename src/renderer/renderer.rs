// use crate::music_representation::musical_structures::{Measure, Score, TimeSignature};

// pub fn render_score(score: &Score, measures_per_row: usize, dashes_per_beat: usize) -> String {
//     let mut rendered_output = String::new();
//     let time_signature = &score.time_signature;

//     let total_measures = score.measures.len();
//     let mut measure_index = 0;

//     while measure_index < total_measures {
//         // Process measures_per_row measures
//         let end_index = (measure_index + measures_per_row).min(total_measures);
//         // Initialize tab_lines for the row
//         let mut tab_lines: Vec<String> = vec![String::new(); 6];

//         // For each measure in the row
//         for m in measure_index..end_index {
//             let measure = &score.measures[m];
//             let measure_tab_lines = render_measure(measure, time_signature, dashes_per_beat);

//             for i in 0..6 {
//                 if m == measure_index {
//                     tab_lines[i].push('|'); // Start with '|'
//                 }
//                 tab_lines[i].push_str(&measure_tab_lines[i]);
//                 tab_lines[i].push('|'); // Add '|' after measure
//             }
//         }

//         // Append tab_lines to rendered_output
//         for line in tab_lines.iter().rev() {
//             rendered_output.push_str(&line);
//             rendered_output.push('\n');
//         }
//         rendered_output.push('\n'); // Separate rows

//         measure_index = end_index;
//     }

//     rendered_output
// }

// pub fn render_measure(
//     measure: &Measure,
//     time_signature: &TimeSignature,
//     dashes_per_beat: usize,
// ) -> Vec<String> {
//     let beats_per_measure = time_signature.beats_per_measure as usize;
//     let dashes_per_measure = dashes_per_beat * beats_per_measure;

//     let mut tab_lines: Vec<Vec<char>> = vec![vec!['-'; dashes_per_measure]; 6];

//     for note in &measure.notes {
//         let string_index = (note.string - 1) as usize;
//         let fret_str = note.fret.to_string();

//         // Compute the position in dashes
//         let position = (note.beat_position * dashes_per_beat as f32).round() as usize;

//         // Ensure position is within bounds
//         let position = if position + fret_str.len() <= dashes_per_measure {
//             position
//         } else if fret_str.len() <= dashes_per_measure {
//             dashes_per_measure - fret_str.len()
//         } else {
//             // fret_str is longer than the measure width
//             continue; // Skip this note
//         };

//         // Insert fret number into the tab line
//         for (i, c) in fret_str.chars().enumerate() {
//             if position + i < dashes_per_measure {
//                 tab_lines[string_index][position + i] = c;
//             }
//         }
//     }

//     // Build the rendered measure tab lines
//     let measure_tab_lines: Vec<String> = tab_lines
//         .iter()
//         .map(|line| line.iter().collect::<String>())
//         .collect();

//     measure_tab_lines
// }
