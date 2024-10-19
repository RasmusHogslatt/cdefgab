use std::sync::mpsc::Sender;
use std::thread;
use std::time::{Duration, Instant};

use crate::music_representation::musical_structures::{Note, Score};

pub struct TimeScrubber {
    pub start_time: Option<Instant>,
    pub total_duration: Option<Duration>,
    pub elapsed_since_start: Duration,
}

impl TimeScrubber {
    pub fn new(score: &Score) -> Self {
        let total_duration: Duration = Duration::from_secs_f32(
            score.measures.len() as f32
                * score.seconds_per_division
                * score.divisions_per_measure as f32,
        );
        println!("Total duration: {} seconds", total_duration.as_secs_f32());
        Self {
            start_time: None,
            total_duration: Some(total_duration),
            elapsed_since_start: Duration::ZERO,
        }
    }

    pub fn start(&mut self) {
        if self.start_time.is_none() && self.total_duration.is_some() {
            self.start_time = Some(Instant::now());
        }
    }

    pub fn stop(&mut self) {
        if let Some(start) = self.start_time {
            self.elapsed_since_start += start.elapsed();
            self.start_time = None;
        }
    }

    pub fn elapsed(&self) -> Duration {
        match self.start_time {
            Some(start) => self.elapsed_since_start + start.elapsed(),
            None => self.elapsed_since_start,
        }
    }

    pub fn reset(&mut self) {
        self.start_time = None;
        self.elapsed_since_start = Duration::ZERO;
    }

    pub fn set_elapsed(&mut self, new_elapsed: Duration) {
        self.elapsed_since_start = new_elapsed;
        if let Some(start) = self.start_time {
            self.start_time = Some(Instant::now() - self.elapsed_since_start);
        }
    }

    pub fn simulate_playback(&mut self, score: &Score, tx: Sender<Vec<Note>>) {
        // Start playback
        self.start();
        let seconds_per_division = score.seconds_per_division;
        let seconds_per_measure = seconds_per_division * score.divisions_per_measure as f32;
        println!("Seconds per measure: {}", seconds_per_measure);

        match self.total_duration {
            Some(total_duration) => {
                let start_instant = Instant::now();
                let mut current_measure: usize = 0;
                let mut current_division: usize = 0;

                // Loop until the elapsed time exceeds the total duration or all measures are played
                while self.elapsed().as_secs_f32() < total_duration.as_secs_f32()
                    && current_measure < score.measures.len()
                {
                    let elapsed = self.elapsed().as_secs_f32();

                    // Calculate which measure and division we are currently in
                    let total_divisions_elapsed = (elapsed / seconds_per_division).floor() as usize;
                    current_measure =
                        total_divisions_elapsed / score.divisions_per_measure as usize;
                    current_division =
                        total_divisions_elapsed % score.divisions_per_measure as usize;

                    if current_measure >= score.measures.len() {
                        break; // Prevent out-of-bounds access
                    }

                    // println!(
                    //     "Current measure: {}, Current division: {}",
                    //     current_measure, current_division
                    // );

                    let measure = &score.measures[current_measure];
                    let notes_map = &measure.positions[current_division];

                    // Convert HashMap<NoteKey, Note> to Vec<Note>
                    let notes: Vec<Note> = notes_map.values().cloned().collect();

                    // println!("Number of notes at position: {}", notes.len());
                    // for note in &notes {
                    //     println!("{}", note);
                    // }

                    // Send the notes to the receiver
                    if tx.send(notes).is_err() {
                        println!("Receiver has been dropped. Stopping playback.");
                        break;
                    }

                    // Sleep for a short duration to prevent tight looping
                    thread::sleep(Duration::from_millis(10));
                }
            }
            None => {
                println!("Can't simulate as total_duration is not set.");
            }
        }

        // Stop playback after all notes are played
        self.stop();
    }
}
