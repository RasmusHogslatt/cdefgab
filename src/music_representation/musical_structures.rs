pub struct Score {
    pub measures: Vec<Measure>,
    pub time_signature: TimeSignature,
    pub tempo: f32,
}

pub struct Measure {
    pub notes: Vec<Note>,
}

#[derive(Clone, Copy)]
pub struct Note {
    pub string: u8,
    pub fret: u8,
    pub duration: f32,
    pub beat_position: f32,
}

#[derive(Clone, Copy)]
pub struct TimeSignature {
    pub beats_per_measure: u8,
    pub beat_value: u8,
}

impl Score {
    pub fn parse_from_musicxml(file_path: &str) -> Result<Self, String> {
        // TODO: Implement parsing
        Ok(Score {
            measures: vec![],
            time_signature: TimeSignature {
                beats_per_measure: 4,
                beat_value: 4,
            },
            tempo: 120.0,
        })
    }
}
