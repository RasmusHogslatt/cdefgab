use core::fmt;
use std::io::Read;
use std::{collections::HashSet, fs::File};

use roxmltree::{Document, Node};

use regex::Regex;
use std::collections::HashMap;

pub struct VoiceState {
    current_position: usize,
    prev_duration: u32,
    prev_is_chord: bool,
    first_note: bool,
}

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
    pub positions: Vec<HashSet<Note>>, // Use HashMap to ensure unique notes per position
}

impl Measure {
    pub fn new(total_divisions: usize) -> Self {
        Measure {
            positions: vec![HashSet::new(); total_divisions],
        }
    }
}

impl Score {
    pub fn parse_from_musicxml(file_path: String) -> Result<Score, String> {
        // Read the MusicXML file content
        let mut file = File::open(&file_path).map_err(|e| e.to_string())?;
        let mut xml_content = String::new();
        file.read_to_string(&mut xml_content)
            .map_err(|e| e.to_string())?;

        // Remove the DTD declaration from the XML content
        let dtd_regex = Regex::new(r"(?s)<!DOCTYPE.*?>").unwrap();
        let xml_content = dtd_regex.replace(&xml_content, "").to_string();

        // Parse the XML content
        let doc = Document::parse(&xml_content).map_err(|e| e.to_string())?;
        let root = doc.root_element();

        // Extract score metadata
        let (divisions_per_quarter, time_signature, tempo) = extract_score_metadata(&root);

        // Calculate divisions per measure
        let divisions_per_measure = calculate_divisions_per_measure(
            time_signature.beats_per_measure,
            divisions_per_quarter,
            time_signature.beat_value,
        );

        // Parse measures
        let measures = parse_measures(&root, divisions_per_measure)?;

        Ok(Score {
            measures,
            time_signature,
            tempo,
            divisions_per_quarter,
            divisions_per_measure: divisions_per_measure as u8,
        })
    }
}

pub fn extract_score_metadata(root: &Node) -> (u8, TimeSignature, usize) {
    let divisions_per_quarter = root
        .descendants()
        .find(|n| n.has_tag_name("divisions"))
        .and_then(|n| n.text().map(|t| t.parse::<u8>().unwrap_or(1)))
        .unwrap_or(1);

    let beats_per_measure = root
        .descendants()
        .find(|n| n.has_tag_name("time"))
        .and_then(|n| {
            n.descendants()
                .find(|m| m.has_tag_name("beats"))
                .and_then(|b| b.text().map(|t| t.parse::<u8>().unwrap_or(0)))
        })
        .unwrap_or(4);

    let beat_value = root
        .descendants()
        .find(|n| n.has_tag_name("time"))
        .and_then(|n| {
            n.descendants()
                .find(|m| m.has_tag_name("beat-type"))
                .and_then(|b| b.text().map(|t| t.parse::<u8>().unwrap_or(0)))
        })
        .unwrap_or(4);

    let tempo = root
        .descendants()
        .find(|n| n.has_tag_name("sound") && n.attribute("tempo").is_some())
        .and_then(|n| {
            n.attribute("tempo")
                .map(|t| t.parse::<usize>().unwrap_or(120))
        })
        .unwrap_or(120);

    let time_signature = TimeSignature {
        beats_per_measure,
        beat_value,
    };

    (divisions_per_quarter, time_signature, tempo)
}
pub fn calculate_divisions_per_measure(
    beats_per_measure: u8,
    divisions_per_quarter: u8,
    beat_value: u8,
) -> usize {
    (beats_per_measure as usize) * (divisions_per_quarter as usize) * 4 / (beat_value as usize)
}
pub fn parse_measures(root: &Node, divisions_per_measure: usize) -> Result<Vec<Measure>, String> {
    let mut measures = Vec::new();

    for part in root.children().filter(|n| n.has_tag_name("part")) {
        for measure_node in part.children().filter(|n| n.has_tag_name("measure")) {
            let measure = parse_measure(measure_node, divisions_per_measure)?;
            measures.push(measure);
        }
    }

    Ok(measures)
}
pub fn parse_measure(measure_node: Node, divisions_per_measure: usize) -> Result<Measure, String> {
    let mut measure = Measure::new(divisions_per_measure);
    let mut voice_states: HashMap<u8, VoiceState> = HashMap::new();

    for note_node in measure_node.children().filter(|n| n.has_tag_name("note")) {
        parse_note(note_node, &mut voice_states, &mut measure)?;
    }

    Ok(measure)
}
pub fn parse_note(
    note_node: Node,
    voice_states: &mut HashMap<u8, VoiceState>,
    measure: &mut Measure,
) -> Result<(), String> {
    let voice = note_node
        .children()
        .find(|n| n.has_tag_name("voice"))
        .and_then(|n| n.text().map(|t| t.parse::<u8>().unwrap_or(1)))
        .unwrap_or(1);

    let voice_state = voice_states.entry(voice).or_insert(VoiceState {
        current_position: 0,
        prev_duration: 0,
        prev_is_chord: false,
        first_note: true,
    });

    let pitch = extract_pitch(&note_node);
    let duration = note_node
        .children()
        .find(|n| n.has_tag_name("duration"))
        .and_then(|n| n.text().map(|t| t.parse::<u32>().unwrap_or(0)))
        .unwrap_or(1);

    let (string, fret) = extract_technical_info(&note_node, &pitch);

    let is_chord = note_node.children().any(|n| n.has_tag_name("chord"));

    let note = Note {
        string,
        fret,
        duration,
        pitch,
    };

    if !voice_state.first_note {
        if !voice_state.prev_is_chord || !is_chord {
            voice_state.current_position += voice_state.prev_duration as usize;
        }
    }

    if voice_state.current_position >= measure.positions.len() {
        measure
            .positions
            .resize_with(voice_state.current_position + 1, HashSet::new);
    }

    if let (Some(_), Some(_)) = (note.string, note.fret) {
        measure.positions[voice_state.current_position].insert(note);
    }

    voice_state.first_note = false;
    voice_state.prev_duration = duration;
    voice_state.prev_is_chord = is_chord;

    Ok(())
}
pub fn extract_pitch(note_node: &Node) -> Option<Pitch> {
    if let Some(pitch_node) = note_node.children().find(|n| n.has_tag_name("pitch")) {
        let step = pitch_node
            .children()
            .find(|n| n.has_tag_name("step"))
            .and_then(|n| n.text().map(|t| t.chars().next().unwrap_or('C')))
            .unwrap_or('C');

        let octave = pitch_node
            .children()
            .find(|n| n.has_tag_name("octave"))
            .and_then(|n| n.text().map(|t| t.parse::<u8>().unwrap_or(4)))
            .unwrap_or(4);

        let alter = pitch_node
            .children()
            .find(|n| n.has_tag_name("alter"))
            .and_then(|n| n.text().map(|t| t.parse::<i8>().ok()))
            .flatten();

        Some(Pitch {
            step,
            alter,
            octave,
        })
    } else {
        None
    }
}
pub fn extract_technical_info(note_node: &Node, pitch: &Option<Pitch>) -> (Option<u8>, Option<u8>) {
    let technical = note_node
        .children()
        .find(|n| n.has_tag_name("notations"))
        .and_then(|n| n.children().find(|n| n.has_tag_name("technical")));

    let string = technical
        .and_then(|n| n.children().find(|n| n.has_tag_name("string")))
        .and_then(|n| n.text())
        .and_then(|t| t.parse::<u8>().ok());

    let fret = technical
        .and_then(|n| n.children().find(|n| n.has_tag_name("fret")))
        .and_then(|n| n.text())
        .and_then(|t| t.parse::<u8>().ok());

    if string.is_some() && fret.is_some() {
        (string, fret)
    } else if let Some(ref p) = pitch {
        calculate_string_and_fret(p).map_or((None, None), |(s, f)| (Some(s), Some(f)))
    } else {
        (None, None)
    }
}

pub fn calculate_string_and_fret(pitch: &Pitch) -> Option<(u8, u8)> {
    // Define standard tuning pitches for each string
    let string_pitches = [
        Pitch {
            step: 'E',
            alter: None,
            octave: 4,
        }, // 1st string (high E)
        Pitch {
            step: 'B',
            alter: None,
            octave: 3,
        }, // 2nd string
        Pitch {
            step: 'G',
            alter: None,
            octave: 3,
        }, // 3rd string
        Pitch {
            step: 'D',
            alter: None,
            octave: 3,
        }, // 4th string
        Pitch {
            step: 'A',
            alter: None,
            octave: 2,
        }, // 5th string
        Pitch {
            step: 'E',
            alter: None,
            octave: 2,
        }, // 6th string (low E)
    ];

    // Attempt to find a string and fret combination
    for (i, open_string_pitch) in string_pitches.iter().enumerate() {
        if let Some(fret) = calculate_fret(open_string_pitch, pitch) {
            if fret <= 24 {
                // Assuming 24 frets maximum
                return Some((i as u8 + 1, fret));
            }
        }
    }
    None
}

pub fn calculate_fret(open_string_pitch: &Pitch, note_pitch: &Pitch) -> Option<u8> {
    let open_midi = pitch_to_midi(open_string_pitch);
    let note_midi = pitch_to_midi(note_pitch);
    if note_midi >= open_midi {
        Some(note_midi - open_midi)
    } else {
        None
    }
}

pub fn pitch_to_midi(pitch: &Pitch) -> u8 {
    let step_to_semitone = |step: char| match step {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => 0,
    };
    let semitone = step_to_semitone(pitch.step) + pitch.alter.unwrap_or(0);
    (pitch.octave * 12) + semitone as u8
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
