// main.rs
mod music_representation;
mod renderer;
mod time_scrubber;
use std::env;
use std::sync::mpsc;
use std::thread;

use music_representation::musical_structures::{Note, Score};
use renderer::renderer::{render_score, score_info};
use time_scrubber::time_scrubber::TimeScrubber;

fn main() {
    // Get the MusicXML file path from command-line arguments or use a default

    let file_path = "silent.xml";

    // Parse the MusicXML file
    let score = Score::parse_from_musicxml(file_path).expect("Failed to parse MusicXML");
    score_info(&score);
    // Render the score
    let measures_per_row = 4;
    let dashes_per_division = 4; // Adjust as needed
    let rendered_output = render_score(&score, measures_per_row, dashes_per_division);
    println!("{}", rendered_output);

    // Set up the time scrubber and playback simulation
    let (tx, rx) = mpsc::channel();
    let mut scrubber = TimeScrubber::new(&score);

    // Start the playback in a separate thread
    thread::spawn(move || {
        scrubber.simulate_playback(&score, tx);
    });

    // Receive and play notes as they are sent
    for received_notes in rx {
        play_notes(received_notes);
    }
}

fn play_notes(notes: Vec<Note>) {
    if !notes.is_empty() {
        println!("Notes to play:");
    }
    for note in notes {
        if let (Some(string), Some(fret)) = (note.string, note.fret) {
            println!("String: {}, Fret: {}", string, fret);
        }
    }
}
