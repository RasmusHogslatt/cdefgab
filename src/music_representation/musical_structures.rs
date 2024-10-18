use std::fs::File;
use std::io::Read;

extern crate roxmltree;
use roxmltree::Document;

extern crate regex;
use regex::Regex;

#[derive(Default, Debug)]
pub struct Score {
    pub measures: Vec<Measure>,
    pub time_signature: TimeSignature,
    pub tempo: u8,
    pub divisions_per_quarter: u8,
    pub seconds_per_beat: f32,     // seconds_per_beat = 60 / tempo
    pub seconds_per_division: f32, // seconds_per_beat / divisions_per_quarter
}

#[derive(Clone, Debug)]
pub struct Note {
    pub pitch: Option<Pitch>, // Some notes might not have a pitch (e.g., rests)
    pub duration: u32,        // Duration of the note in divisions
    pub note_type: String,    // Note type (e.g., quarter, eighth)
    pub voice: u8,            // Voice number to distinguish different voices
    pub stem_direction: Option<String>, // Direction of the stem ("up" or "down")
    pub techniques: Vec<Technique>, // List of techniques used on the note (e.g., hammer-on, pull-off)
    pub string: Option<u8>,         // The guitar string number (e.g., 1 to 6)
    pub fret: Option<u8>,           // The fret number for the note on the guitar
    pub is_chord: bool,             // Whether the note is part of a chord
}

#[derive(Clone, Copy, Debug)]
pub struct Pitch {
    pub step: char,        // Note step (A, B, C, D, E, F, G)
    pub alter: Option<i8>, // Sharps or flats (-1 for flat, +1 for sharp)
    pub octave: u8,        // Octave number
}

#[derive(Clone, Copy, Debug)]
pub enum Technique {
    HammerOn,
    PullOff,
    Slide,
    Bend,
}

#[derive(Clone, Copy, Default, Debug)]
pub struct TimeSignature {
    pub beats_per_measure: u8,
    pub beat_value: u8,
}

#[derive(Clone, Default, Debug)]
pub struct Measure {
    pub positions: Vec<Vec<Note>>, // Group notes by their division positions
}

impl Measure {
    pub fn new(total_divisions: usize) -> Self {
        Measure {
            positions: vec![vec![]; total_divisions], // Initialize with empty vectors for each division
        }
    }
}

impl Score {
    pub fn parse_from_musicxml(file_path: &str) -> Result<Score, String> {
        // Read the MusicXML file content
        let mut file = File::open(file_path).map_err(|e| e.to_string())?;
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
            .and_then(|n| n.attribute("tempo").map(|t| t.parse::<u8>().unwrap_or(120)))
            .unwrap_or(120);

        // Calculate seconds per beat and per division
        let seconds_per_beat = 60.0 / tempo as f32;
        let seconds_per_division = seconds_per_beat / divisions_per_quarter as f32;

        let time_signature = TimeSignature {
            beats_per_measure,
            beat_value,
        };

        // Calculate total divisions in a measure
        let total_divisions = (beats_per_measure as usize) * (divisions_per_quarter as usize);

        let mut measures = Vec::new();

        // Iterate over parts and extract guitar parts
        for part in root.children().filter(|n| n.has_tag_name("part")) {
            for measure_node in part.children().filter(|n| n.has_tag_name("measure")) {
                // Create a new Measure with total divisions
                let mut measure = Measure::new(total_divisions);
                let mut current_position = 0;

                // Parse each note within the measure
                for note in measure_node.children().filter(|n| n.has_tag_name("note")) {
                    // Determine if this note is part of a chord
                    let is_chord = note.children().any(|n| n.has_tag_name("chord"));

                    // Extract the pitch, duration, string, and fret for each note
                    let pitch = if let Some(pitch_node) =
                        note.children().find(|n| n.has_tag_name("pitch"))
                    {
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
                    };

                    let duration = note
                        .children()
                        .find(|n| n.has_tag_name("duration"))
                        .and_then(|n| n.text().map(|t| t.parse::<u32>().unwrap_or(0)))
                        .unwrap_or(0);

                    let voice = note
                        .children()
                        .find(|n| n.has_tag_name("voice"))
                        .and_then(|n| n.text().map(|t| t.parse::<u8>().unwrap_or(1)))
                        .unwrap_or(1);

                    // Extract the string and fret for each note
                    let technical = note
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

                    // If string and fret are not provided, calculate them from pitch
                    let (string, fret) = if let (Some(s), Some(f)) = (string, fret) {
                        (Some(s), Some(f))
                    } else if let Some(ref p) = pitch {
                        calculate_string_and_fret(p)
                            .map_or((None, None), |(s, f)| (Some(s), Some(f)))
                    } else {
                        (None, None)
                    };

                    // Create the Note struct
                    let note = Note {
                        pitch: pitch.clone(),
                        duration,
                        note_type: note
                            .children()
                            .find(|n| n.has_tag_name("type"))
                            .and_then(|n| n.text().map(|t| t.to_string()))
                            .unwrap_or("quarter".to_string()),
                        voice,
                        stem_direction: note
                            .children()
                            .find(|n| n.has_tag_name("stem"))
                            .and_then(|n| n.text().map(|t| t.to_string())),
                        techniques: vec![], // Not implemented for simplicity
                        string,
                        fret,
                        is_chord,
                    };

                    // Add the note to the appropriate position in the measure
                    if current_position < measure.positions.len() {
                        measure.positions[current_position].push(note);
                    } else {
                        // Handle cases where current_position exceeds the measure length
                        println!(
                            "Warning: Note at position {} exceeds measure length {}",
                            current_position,
                            measure.positions.len()
                        );
                    }

                    // Update current_position if not a chord note
                    if !is_chord {
                        current_position += duration as usize;
                    }
                }

                measures.push(measure);
            }
        }

        Ok(Score {
            measures,
            time_signature,
            tempo,
            divisions_per_quarter,
            seconds_per_beat,
            seconds_per_division,
        })
    }

    pub fn print_score_as_tablature(&self, measures_per_row: usize, chars_per_division: usize) {
        let mut current_measure_index = 0;
        let total_measures = self.measures.len();

        // Iterate through all measures
        while current_measure_index < total_measures {
            let end_measure_index = (current_measure_index + measures_per_row).min(total_measures);

            // Print the set of measures per row
            for string_number in 1..=6 {
                for measure_index in current_measure_index..end_measure_index {
                    // Print each measure for the current string
                    self.print_measure(
                        &self.measures[measure_index],
                        string_number,
                        chars_per_division,
                    );
                    print!("  "); // Separate measures
                }
                println!();
            }
            println!(); // Newline after each row of measures

            current_measure_index = end_measure_index;
        }
    }

    fn print_measure(&self, measure: &Measure, string_number: u8, chars_per_division: usize) {
        let total_divisions = measure.positions.len();
        let total_chars = total_divisions * chars_per_division;
        let mut string_representation = vec!['-'; total_chars];

        for (division, notes) in measure.positions.iter().enumerate() {
            for note in notes {
                if let (Some(note_string), Some(fret)) = (note.string, note.fret) {
                    if note_string == string_number {
                        let position = division * chars_per_division;
                        let fret_str = fret.to_string();
                        for (i, ch) in fret_str.chars().enumerate() {
                            if position + i < string_representation.len() {
                                string_representation[position + i] = ch;
                            }
                        }
                    }
                }
            }
        }

        let string_output: String = string_representation.into_iter().collect();
        print!("{}|", string_output);
    }
}

// Helper functions to calculate string and fret from pitch

fn calculate_string_and_fret(pitch: &Pitch) -> Option<(u8, u8)> {
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

fn calculate_fret(open_string_pitch: &Pitch, note_pitch: &Pitch) -> Option<u8> {
    let open_midi = pitch_to_midi(open_string_pitch);
    let note_midi = pitch_to_midi(note_pitch);
    if note_midi >= open_midi {
        Some(note_midi - open_midi)
    } else {
        None
    }
}

fn pitch_to_midi(pitch: &Pitch) -> u8 {
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

// Main function for testing

fn main() {
    let score = Score::parse_from_musicxml("path_to_your_musicxml_file.xml").unwrap();

    // Print the parsed score for debugging
    // println!("{:#?}", score);

    // Print the tablature with specified dashes per division
    let dashes_per_quarter = 3; // As per your requirement
    score.print_score_as_tablature(4, 1); // Set chars_per_division to 1
}
