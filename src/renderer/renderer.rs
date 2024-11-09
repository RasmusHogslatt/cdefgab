// renderer.rs

use crate::music_representation::Score;

// The Renderer struct encapsulates rendering logic
pub struct Renderer {
    pub measures_per_row: usize,
    pub dashes_per_division: usize,
}

impl Renderer {
    pub fn new(measures_per_row: usize, dashes_per_division: usize) -> Self {
        Self {
            measures_per_row,
            dashes_per_division,
        }
    }
}

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
