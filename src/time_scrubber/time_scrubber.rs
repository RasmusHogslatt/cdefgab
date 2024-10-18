// use std::sync::mpsc::Sender;
// use std::time::{Duration, Instant};

// use crate::music_representation::musical_structures::{Note, Score};

// pub struct TimeScrubber {
//     pub start_time: Instant,
//     pub stop_time: Option<Instant>,
//     pub current_time: Duration,
// }

// impl TimeScrubber {
//     pub fn new() -> Self {
//         Self {
//             start_time: Instant::now(),
//             stop_time: None,
//             current_time: Duration::new(0, 0),
//         }
//     }

//     pub fn start(&mut self) {
//         self.start_time = Instant::now();
//         self.current_time = Duration::new(0, 0);
//     }

//     pub fn stop(&mut self) {
//         self.stop_time = Some(Instant::now());
//     }

//     pub fn set_current_time(&mut self, new_time: Duration) {
//         self.current_time = new_time;
//     }

//     pub fn simulate_playback(&mut self, score: &Score, tx: Sender<Vec<Note>>) {
//         for measure in &score.measures {
//             let measure_duration = Duration::from_secs_f32(
//                 60.0 / score.tempo * score.time_signature.beats_per_measure as f32,
//             );

//             let mut current_beat = 0.0;
//             for note in &measure.notes {
//                 let delay = note.beat_position - current_beat;
//                 if delay > 0.0 {
//                     std::thread::sleep(Duration::from_secs_f32(60.0 / score.tempo * delay));
//                     current_beat = note.beat_position;
//                 }
//                 let mut simultaneous_notes: Vec<Note> = vec![*note];
//                 for next_note in &measure.notes {
//                     if next_note.beat_position == note.beat_position
//                         && next_note.string != note.string
//                     {
//                         simultaneous_notes.push(*next_note);
//                     }
//                 }
//                 tx.send(simultaneous_notes).unwrap();
//             }
//         }
//     }
// }
