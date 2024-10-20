use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::music_representation::musical_structures::{Note, Score};

pub struct TimeScrubber {
    pub start_time: Option<Instant>,
    pub total_duration: Option<Duration>,
    pub elapsed_since_start: Duration,
    pub seconds_per_division: f32,
    pub seconds_per_beat: f32,
}

impl TimeScrubber {
    pub fn new(score: &Score, tempo: Option<usize>) -> Self {
        let mut seconds_per_beat = 60.0 / score.tempo as f32;
        let mut seconds_per_division = seconds_per_beat / score.divisions_per_quarter as f32;
        if let Some(custom_tempo) = tempo {
            seconds_per_beat = 60.0 / custom_tempo as f32;
            seconds_per_division = seconds_per_beat / score.divisions_per_quarter as f32;
        }
        let total_duration: Duration = Duration::from_secs_f32(
            score.measures.len() as f32 * seconds_per_division * score.divisions_per_measure as f32,
        );

        println!("Total duration: {} seconds", total_duration.as_secs_f32());
        Self {
            start_time: None,
            total_duration: Some(total_duration),
            elapsed_since_start: Duration::ZERO,
            seconds_per_division,
            seconds_per_beat,
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

    pub fn simulate_playback(
        &mut self,
        score: &Score,
        tx: Sender<Vec<Note>>,
        stop_flag: Arc<AtomicBool>,
    ) {
        self.start();

        let seconds_per_measure = self.seconds_per_division * score.divisions_per_measure as f32;
        println!("Seconds per measure: {}", seconds_per_measure);

        match self.total_duration {
            Some(total_duration) => {
                let mut current_measure: usize = 0;
                let mut current_division: usize = 0;
                let mut last_sent_measure: Option<usize> = None;
                let mut last_sent_division: Option<usize> = None;

                // Loop until the elapsed time exceeds the total duration or all measures are played
                while self.elapsed().as_secs_f32() < total_duration.as_secs_f32()
                    && current_measure < score.measures.len()
                    && !stop_flag.load(Ordering::Relaxed)
                {
                    let elapsed = self.elapsed().as_secs_f32();

                    // Calculate which measure and division we are currently in
                    let total_divisions_elapsed =
                        (elapsed / self.seconds_per_division).floor() as usize;
                    current_measure =
                        total_divisions_elapsed / score.divisions_per_measure as usize;
                    current_division =
                        total_divisions_elapsed % score.divisions_per_measure as usize;

                    if current_measure >= score.measures.len() {
                        break; // Prevent out-of-bounds access
                    }

                    // Only send notes if we have not already sent for this measure and division
                    if Some(current_measure) != last_sent_measure
                        || Some(current_division) != last_sent_division
                    {
                        let measure = &score.measures[current_measure];
                        let notes_map = &measure.positions[current_division];

                        // Convert HashMap<NoteKey, Note> to Vec<Note>
                        let notes: Vec<Note> = notes_map.values().cloned().collect();
                        println!("{}", current_division);
                        // Send the notes to the receiver
                        if tx.send(notes).is_err() {
                            println!("Receiver has been dropped. Stopping playback.");
                            break;
                        }

                        // Update last sent measure and division
                        last_sent_measure = Some(current_measure);
                        last_sent_division = Some(current_division);
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
