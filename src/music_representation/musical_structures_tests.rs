// Add this to your musical_structures.rs file

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::{
        music_representation::musical_structures::{
            calculate_string_and_fret, parse_note, Measure, Pitch, Score,
        },
        renderer::renderer::score_info,
    };

    #[test]
    fn test_score_parsing() {
        // Assuming you have a test MusicXML file named "test_musicxml.xml"
        let file_path = "silent.xml".to_string();
        let score_result = Score::parse_from_musicxml(file_path);
        assert!(score_result.is_ok(), "Failed to parse MusicXML file");

        let score = score_result.unwrap();
        println!("{}", score_info(&score));
        assert_eq!(score.time_signature.beats_per_measure, 3);
        assert_eq!(score.time_signature.beat_value, 4);
        assert_eq!(score.tempo, 120);
        assert_eq!(score.divisions_per_quarter, 2);
        assert_eq!(score.measures.len(), 48);
    }

    #[test]
    fn test_measure_creation() {
        let divisions_per_measure = 4;
        let measure = Measure::new(divisions_per_measure);

        assert_eq!(
            measure.positions.len(),
            divisions_per_measure,
            "Measure does not have the correct number of positions"
        );

        for position in measure.positions {
            assert!(
                position.is_empty(),
                "Position should be initialized as empty"
            );
        }
    }

    #[test]
    fn test_note_parsing_with_technical_info() {
        // Simulate a note with technical info
        let note_node_xml = r#"
            <note>
                <voice>1</voice>
                <duration>1</duration>
                <notations>
                    <technical>
                        <string>1</string>
                        <fret>3</fret>
                    </technical>
                </notations>
            </note>
        "#;

        let doc = roxmltree::Document::parse(note_node_xml).unwrap();
        let note_node = doc.root_element();

        let mut voice_states = HashMap::new();
        let mut measure: Measure = Measure::new(4);

        let result: Result<(), String> = parse_note(note_node, &mut voice_states, &mut measure);
        assert!(result.is_ok(), "Failed to parse note with technical info");

        let position: &std::collections::HashMap<
            crate::music_representation::musical_structures::NoteKey,
            crate::music_representation::musical_structures::Note,
        > = &measure.positions[0];
        assert_eq!(position.len(), 1, "Incorrect number of notes in position");

        let note: &crate::music_representation::musical_structures::Note =
            position.values().next().unwrap();
        assert_eq!(note.string, Some(1));
        assert_eq!(note.fret, Some(3));
    }

    #[test]
    fn test_note_parsing_without_technical_info() {
        // Simulate a note without technical info but with pitch
        let note_node_xml = r#"
            <note>
                <voice>1</voice>
                <duration>1</duration>
                <pitch>
                    <step>E</step>
                    <octave>4</octave>
                </pitch>
            </note>
        "#;

        let doc = roxmltree::Document::parse(note_node_xml).unwrap();
        let note_node = doc.root_element();

        let mut voice_states = HashMap::new();
        let mut measure = Measure::new(4);

        let result = parse_note(note_node, &mut voice_states, &mut measure);
        assert!(
            result.is_ok(),
            "Failed to parse note without technical info"
        );

        let position = &measure.positions[0];
        assert_eq!(position.len(), 1, "Incorrect number of notes in position");

        let note = position.values().next().unwrap();
        // Depending on your calculate_string_and_fret implementation,
        // adjust the expected string and fret.
        assert_eq!(note.string, Some(1)); // Assuming high E string
        assert_eq!(note.fret, Some(0)); // Open string
    }

    #[test]
    fn test_calculate_string_and_fret() {
        let pitch = Pitch {
            step: 'E',
            alter: None,
            octave: 4,
        };

        let result = calculate_string_and_fret(&pitch);
        assert!(result.is_some(), "Failed to calculate string and fret");

        let (string, fret) = result.unwrap();
        assert_eq!(string, 1); // High E string
        assert_eq!(fret, 0); // Open string
    }
}
