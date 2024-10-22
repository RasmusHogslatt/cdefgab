// Add this to your time_scrubber.rs file

#[cfg(test)]
mod tests {

    use crate::{
        music_representation::musical_structures::{Measure, Note, NoteKey, Score, TimeSignature},
        time_scrubber::time_scrubber::TimeScrubber,
    };
    use std::{
        sync::{atomic::AtomicBool, mpsc::channel, Arc},
        time::Duration,
    };

    #[test]
    fn test_time_scrubber_initialization() {
        let score = create_test_score();
        let time_scrubber = TimeScrubber::new(&score, None);

        assert_eq!(time_scrubber.seconds_per_division, 0.5);
        assert!(time_scrubber.total_duration.is_some());
        assert_eq!(time_scrubber.total_duration.unwrap().as_secs_f32(), 8.0);
    }

    #[test]
    fn test_calculate_current_time() {
        let score = create_test_score();
        let time_scrubber = TimeScrubber::new(&score, None);

        // Elapsed time at 0 seconds
        let (measure_idx, division_idx) =
            time_scrubber.calculate_current_time(0.0, 4, score.measures.len());
        assert_eq!(measure_idx, 0);
        assert_eq!(division_idx, 0);

        // Elapsed time at 2 seconds
        let (measure_idx, division_idx) =
            time_scrubber.calculate_current_time(2.0, 4, score.measures.len());
        assert_eq!(measure_idx, 1);
        assert_eq!(division_idx, 0);

        // Elapsed time at 7.5 seconds
        let (measure_idx, division_idx) =
            time_scrubber.calculate_current_time(7.5, 4, score.measures.len());
        assert_eq!(measure_idx, 3);
        assert_eq!(division_idx, 3);
    }

    #[test]
    fn test_start_stop_elapsed() {
        let score = create_test_score();
        let mut time_scrubber = TimeScrubber::new(&score, None);

        time_scrubber.start();
        std::thread::sleep(Duration::from_millis(100));
        time_scrubber.stop();

        let elapsed = time_scrubber.elapsed();
        assert!(
            elapsed >= Duration::from_millis(100),
            "Elapsed time should be at least 100ms"
        );
    }

    #[test]
    fn test_simulate_playback() {
        let score = create_test_score();
        let mut time_scrubber = TimeScrubber::new(&score, None);
        let (tx, rx) = channel();
        let stop_flag = Arc::new(AtomicBool::new(false));

        let handle = std::thread::spawn(move || {
            time_scrubber.simulate_playback(&score, tx, stop_flag.clone());
        });

        // Collect notes sent during playback
        let mut received_notes = Vec::new();
        while let Ok(notes) = rx.recv_timeout(Duration::from_secs(1)) {
            received_notes.push(notes);
        }

        handle.join().unwrap();

        assert!(
            !received_notes.is_empty(),
            "Should have received notes during playback"
        );
    }

    // Helper function to create a test score
    fn create_test_score() -> Score {
        let time_signature = TimeSignature {
            beats_per_measure: 4,
            beat_value: 4,
        };

        let divisions_per_quarter = 1;
        let divisions_per_measure = 4;

        let note = Note {
            string: Some(1),
            fret: Some(0),
        };
        let note_key = NoteKey { string: 1, fret: 0 };

        // Create measures with one note per measure
        let measures = (0..4)
            .map(|_| {
                let mut measure = Measure::new(divisions_per_measure);
                measure.positions[0].insert(note_key.clone(), note.clone());
                measure
            })
            .collect();

        Score {
            measures,
            time_signature,
            tempo: 120,
            divisions_per_quarter,
            divisions_per_measure: divisions_per_measure as u8,
        }
    }
}
