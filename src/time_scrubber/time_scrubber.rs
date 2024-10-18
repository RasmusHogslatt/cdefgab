// time_scrubber.rs

use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use crate::music_representation::musical_structures::{Note, Score};

pub struct TimeScrubber {
    pub start_time: Instant,
    pub stop_time: Option<Instant>,
    pub current_time: Duration,
}

impl TimeScrubber {
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
            stop_time: None,
            current_time: Duration::new(0, 0),
        }
    }

    pub fn start(&mut self) {
        self.start_time = Instant::now();
        self.current_time = Duration::new(0, 0);
    }

    pub fn stop(&mut self) {
        self.stop_time = Some(Instant::now());
    }

    pub fn set_current_time(&mut self, new_time: Duration) {
        self.current_time = new_time;
    }

    pub fn simulate_playback(&mut self, score: &Score, tx: Sender<Vec<Note>>) {
        let seconds_per_division = score.seconds_per_division;

        for measure in &score.measures {
            let total_divisions = measure.positions.len();

            for division_index in 0..total_divisions {
                let notes = &measure.positions[division_index];
                if notes.is_empty() {
                    continue; // Skip if no notes at this division
                }

                let target_time =
                    Duration::from_secs_f32(seconds_per_division * division_index as f32);
                if self.current_time < target_time {
                    let sleep_duration = target_time - self.current_time;
                    std::thread::sleep(sleep_duration);
                    self.current_time = target_time;
                }

                // Send notes at this division
                tx.send(notes.clone()).unwrap();
            }
        }
    }
}
