mod music_representation;
mod renderer;
mod time_scrubber;

use music_representation::*;
use musical_structures::{Note, Score};
use renderer::renderer::render_score;
use renderer::*;
use std::sync::mpsc;
use std::thread;
use time_scrubber::time_scrubber::TimeScrubber;

fn main() {
    let score =
        Score::parse_from_musicxml("path/to/your/music.xml").expect("Failed to parse MusicXML");
    let rendered_output = render_score(&score, 4, 16);
    println!("{}", rendered_output);

    let (tx, rx) = mpsc::channel();
    let mut scrubber = TimeScrubber::new();

    thread::spawn(move || {
        scrubber.simulate_playback(&score, tx);
    });

    for received_notes in rx {
        play_notes(received_notes);
    }
}

fn play_notes(notes: Vec<Note>) {
    println!("Notes to play:");
    for note in notes {
        println!("String: {}, Fret: {}", note.string, note.fret);
    }
}
