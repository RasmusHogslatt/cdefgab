// music_representation/musical_structures.rs

use core::fmt;
use std::collections::HashSet;

#[derive(Default, Debug, Clone)]
pub struct Score {
    pub measures: Vec<Measure>,
    pub time_signature: TimeSignature,
    pub tempo: usize,
    pub divisions_per_quarter: u8,
    pub divisions_per_measure: u8,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct Note {
    pub string: Option<u8>, // The guitar string number (e.g., 1 to 6)
    pub fret: Option<u8>,   // The fret number for the note on the guitar
    pub duration: u32,      // Duration in divisions
    pub pitch: Option<Pitch>,
}

impl fmt::Display for Note {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "String:{}, Fret: {}",
            self.string.unwrap(),
            self.fret.unwrap()
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Pitch {
    pub step: char,        // Note step (A, B, C, D, E, F, G)
    pub alter: Option<i8>, // Sharps or flats (-1 for flat, +1 for sharp)
    pub octave: u8,        // Octave number
}

#[derive(Clone, Copy, Default, Debug)]
pub struct TimeSignature {
    pub beats_per_measure: u8,
    pub beat_value: u8,
}

#[derive(Clone, Default, Debug)]
pub struct Measure {
    pub positions: Vec<HashSet<Note>>, // Use HashSet to ensure unique notes per position
}

impl Measure {
    pub fn new(total_divisions: usize) -> Self {
        Measure {
            positions: vec![HashSet::new(); total_divisions],
        }
    }
}

pub struct VoiceState {
    pub current_position: usize,
    pub prev_duration: u32,
    pub prev_is_chord: bool,
    pub first_note: bool,
}

pub fn calculate_frequency(note: &Note, scale_length: f32, capo_fret: u8) -> f32 {
    // Define the standard scale length (e.g., 25.5 inches for many guitars)
    const STANDARD_SCALE_LENGTH: f32 = 25.5;
    const MAX_FRET: u8 = 24;

    let open_string_frequencies = [329.63, 246.94, 196.00, 146.83, 110.00, 82.41];
    let string_index = (note.string.unwrap_or(1) - 1).min(5) as usize;
    let open_frequency = open_string_frequencies[string_index];

    // Effective fret number considering the capo
    let mut effective_fret = note.fret.unwrap_or(0) + capo_fret;
    if effective_fret > MAX_FRET {
        effective_fret = MAX_FRET;
    }

    // Calculate the base frequency based on the effective fret number
    let base_frequency = open_frequency * (2f32).powf(effective_fret as f32 / 12.0);

    // Adjust the frequency based on the scale length
    // Frequency is inversely proportional to scale length: f_new = f_standard * (STANDARD_L / actual_L)
    let adjusted_frequency = base_frequency * (STANDARD_SCALE_LENGTH / scale_length);

    adjusted_frequency
}
