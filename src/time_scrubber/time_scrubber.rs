// time_scrubber.rs

use crate::music_representation::{Measure, Note, Score};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub struct TimeScrubber {
    start_time: Option<Instant>,
    total_duration: Option<Duration>,
    elapsed_since_start: Duration,
    seconds_per_division: f32,
    current_division: Option<usize>,
    current_measure: Option<usize>,
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

        Self {
            start_time: None,
            total_duration: Some(total_duration),
            elapsed_since_start: Duration::ZERO,
            seconds_per_division,
            current_division: None,
            current_measure: None,
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

    pub fn simulate_playback(
        &mut self,
        score: &Score,
        tx_notes: Sender<(Vec<Note>, usize, usize)>,
        stop_flag: Arc<AtomicBool>,
    ) {
        self.start();

        if let Some(total_duration) = self.total_duration {
            let total_duration_f32 = total_duration.as_secs_f32();
            let mut last_sent_measure: Option<usize> = None;
            let mut last_sent_division: Option<usize> = None;

            while self.elapsed().as_secs_f32() < total_duration_f32
                && !stop_flag.load(Ordering::Relaxed)
            {
                let elapsed = self.elapsed().as_secs_f32();
                let (current_measure, current_division) = self.calculate_current_time(
                    elapsed,
                    score.divisions_per_measure as usize,
                    score.measures.len(),
                );

                self.current_division = Some(current_division);
                self.current_measure = Some(current_measure);

                if current_measure >= score.measures.len() {
                    break;
                }

                if Some(current_measure) != last_sent_measure
                    || Some(current_division) != last_sent_division
                {
                    self.send_notes(
                        &score.measures[current_measure],
                        current_division,
                        current_measure,
                        &tx_notes,
                    );

                    last_sent_measure = Some(current_measure);
                    last_sent_division = Some(current_division);
                }
            }
        } else {
            eprintln!("Can't simulate as total_duration is not set.");
        }

        self.stop();
    }

    fn calculate_current_time(
        &self,
        elapsed: f32,
        divisions_per_measure: usize,
        total_measures: usize,
    ) -> (usize, usize) {
        let total_divisions_elapsed = (elapsed / self.seconds_per_division).floor() as usize;
        let current_measure = total_divisions_elapsed / divisions_per_measure;
        let current_division = total_divisions_elapsed % divisions_per_measure;
        (current_measure.min(total_measures - 1), current_division)
    }

    fn send_notes(
        &self,
        measure: &Measure,
        current_division: usize,
        current_measure: usize,
        tx: &Sender<(Vec<Note>, usize, usize)>,
    ) {
        let notes_map = &measure.positions[current_division];
        let notes: Vec<Note> = notes_map.iter().cloned().collect();

        if tx.send((notes, current_division, current_measure)).is_err() {
            eprintln!("Receiver has been dropped. Stopping playback.");
        }
    }
}
