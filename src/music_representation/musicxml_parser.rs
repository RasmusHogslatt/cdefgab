// musicxml_parser.rs

use regex::Regex;
use roxmltree::{Document, Node};

use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use crate::music_representation::utils::{calculate_divisions_per_measure, extract_score_metadata};
use crate::music_representation::{Measure, Note, Pitch, Score, Technique, VoiceState};

impl Score {
    pub fn parse_from_musicxml_str(xml_content: &str) -> Result<Score, String> {
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
    pub fn parse_from_musicxml<P: AsRef<Path>>(file_path: P) -> Result<Score, String> {
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

fn parse_measures(root: &Node, divisions_per_measure: usize) -> Result<Vec<Measure>, String> {
    let mut measures = Vec::new();

    for part in root.children().filter(|n| n.has_tag_name("part")) {
        for measure_node in part.children().filter(|n| n.has_tag_name("measure")) {
            let measure = parse_measure(measure_node, divisions_per_measure)?;
            measures.push(measure);
        }
    }

    Ok(measures)
}

fn parse_measure(measure_node: Node, divisions_per_measure: usize) -> Result<Measure, String> {
    let mut measure = Measure::new(divisions_per_measure);
    let mut voice_states: HashMap<u8, VoiceState> = HashMap::new();

    for note_node in measure_node.children().filter(|n| n.has_tag_name("note")) {
        parse_note(note_node, &mut voice_states, &mut measure)?;
    }

    Ok(measure)
}

fn parse_note(
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

    let technique = extract_technique(&note_node);

    let note = Note {
        string,
        fret,
        duration,
        pitch,
        technique,
    };

    if !voice_state.first_note {
        if !voice_state.prev_is_chord || !is_chord {
            voice_state.current_position += voice_state.prev_duration as usize;
        }
    }

    if voice_state.current_position >= measure.positions.len() {
        measure
            .positions
            .resize_with(voice_state.current_position + 1, Vec::new);
    }

    if let (Some(_), Some(_)) = (note.string, note.fret) {
        measure.positions[voice_state.current_position].push(note);
    }

    voice_state.first_note = false;
    voice_state.prev_duration = duration;
    voice_state.prev_is_chord = is_chord;

    Ok(())
}

fn extract_pitch(note_node: &Node) -> Option<Pitch> {
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

fn extract_technical_info(note_node: &Node, pitch: &Option<Pitch>) -> (Option<u8>, Option<u8>) {
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

fn extract_technique(note_node: &Node) -> Technique {
    if let Some(notations) = note_node.children().find(|n| n.has_tag_name("notations")) {
        if let Some(technical) = notations.children().find(|n| n.has_tag_name("technical")) {
            for technique_node in technical.children() {
                match technique_node.tag_name().name() {
                    "hammer-on" => {
                        println!("Found hammer on");
                        return Technique::HammerOn;
                    }
                    "pull-off" => {
                        println!("Found pull-off");
                        return Technique::PullOff;
                    }
                    _ => {}
                }
            }
        }
    }
    Technique::None
}

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
        let fret = note_midi - open_midi;
        if fret <= 24 {
            Some(fret as u8)
        } else {
            None
        }
    } else {
        None
    }
}

fn pitch_to_midi(pitch: &Pitch) -> u16 {
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
    let semitone = step_to_semitone(pitch.step) as i16 + pitch.alter.unwrap_or(0) as i16;
    let octave = pitch.octave as u16;
    let midi_note = (octave * 12) as i16 + semitone;
    midi_note as u16
}
