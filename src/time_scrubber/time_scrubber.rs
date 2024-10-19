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
                let mut current_measure: usize = 0;
                let mut current_division: usize = 0;

                while self.elapsed().as_secs_f32() < total_duration.as_secs_f32() {
                    // Start new measure
                    if self.elapsed().as_secs_f32() % seconds_per_measure == 0.0 {
                        current_measure += 1;
                        println!("Current measure: {}", current_measure);
                    }

                    if self.elapsed().as_secs_f32() % seconds_per_division == 0.0 {
                        match current_division < score.divisions_per_measure as usize - 1 {
                            true => current_division += 1,
                            false => current_division = 0,
                        }
                        println!("Current division: {}", current_division);
                        let measure = &score.measures[current_measure];
                        let notes = &measure.positions[current_division];
                        println!("{:#?}", notes.len());
                        for note in notes {
                            println!("{}", note);
                        }
                    }
                }
            }
            None => {
                println!("Can't simulate as total_duration is not set.");
            }
        }

        // while self.elapsed_since_start < self.total_duration {}
        // for measure in &score.measures {
        //     println!("{:#?}", self.elapsed_since_start);
        //     for division_index in 0..score.divisions_per_measure {
        //         let notes = &measure.positions[division_index as usize];
        //         let division_time = seconds_per_division * division_index as f32;

        //         // Calculate the time to wait until the next division
        //         let target_time = Duration::from_secs_f32(division_time);

        //         if let Some(start_time) = self.start_time {
        //             let elapsed = start_time.elapsed();

        //             println!(
        //                 "Division Index: {}, Target Time: {:.3}, Elapsed Time: {:.3}, Sleep Duration: {:.3}",
        //                 division_index,
        //                 target_time.as_secs_f32(),
        //                 elapsed.as_secs_f32(),
        //                 if elapsed < target_time {
        //                     (target_time - elapsed).as_secs_f32()
        //                 } else {
        //                     0.0
        //                 }
        //             );

        //             if elapsed < target_time {
        //                 let sleep_duration = target_time - elapsed;
        //                 thread::sleep(sleep_duration);
        //             }

        //             // Send notes at this division if there are any
        //             if !notes.is_empty() {
        //                 tx.send(notes.clone()).unwrap();
        //             }
        //         } else {
        //             // If start_time is not set, start it now
        //             self.start_time = Some(Instant::now());
        //         }
        //     }
        // }

        // // Stop playback after all notes are played
        self.stop();
    }
}
