use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream};
use rand::random;
use std::sync::{Arc, Mutex};

use crate::music_representation::musical_structures::Note;

pub struct AudioPlayer {
    stream: Stream,
    active_notes: Arc<Mutex<Vec<KarplusStrong>>>,
    sample_rate: f32,
    volume: Arc<Mutex<f32>>,
}

impl AudioPlayer {
    pub fn new() -> Self {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .expect("No output device available");
        let config = device.default_output_config().unwrap();
        let sample_rate = config.sample_rate().0 as f32;
        let channels = config.channels() as usize;

        let active_notes = Arc::new(Mutex::new(Vec::new()));
        let active_notes_clone = active_notes.clone();

        let volume = Arc::new(Mutex::new(0.5)); // Default volume
        let volume_clone = volume.clone();

        let stream = match config.sample_format() {
            SampleFormat::F32 => device
                .build_output_stream(
                    &config.into(),
                    move |data: &mut [f32], _| {
                        Self::write_data(data, channels, &active_notes_clone, &volume_clone);
                    },
                    |err| eprintln!("Stream error: {}", err),
                    None,
                )
                .unwrap(),
            _ => panic!("Unsupported sample format"),
        };

        Self {
            stream,
            active_notes,
            sample_rate,
            volume,
        }
    }

    pub fn start(&self) {
        self.stream.play().expect("Failed to start audio stream");
    }

    fn write_data(
        output: &mut [f32],
        channels: usize,
        active_notes: &Arc<Mutex<Vec<KarplusStrong>>>,
        volume: &Arc<Mutex<f32>>,
    ) {
        let mut active_notes = active_notes.lock().unwrap();
        let volume = *volume.lock().unwrap();

        for frame in output.chunks_mut(channels) {
            let mut value = 0.0;

            // Sum samples from all active notes
            active_notes.retain_mut(|note| {
                if let Some(sample) = note.next_sample() {
                    value += sample;
                    true
                } else {
                    false
                }
            });

            // Apply volume
            value *= volume;

            // Prevent clipping
            value = value.clamp(-1.0, 1.0);

            for sample in frame.iter_mut() {
                *sample = value;
            }
        }
    }

    fn calculate_frequency(string: u8, fret: u8) -> f32 {
        let open_string_frequencies = [329.63, 246.94, 196.00, 146.83, 110.00, 82.41];
        let string_index = (string - 1).min(5) as usize;
        let open_frequency = open_string_frequencies[string_index];
        let frequency = open_frequency * (2f32).powf(fret as f32 / 12.0);
        frequency
    }

    pub fn play_notes_with_config(&self, notes: &[Note], decay: f32, volume: f32) {
        // Update volume
        {
            let mut vol = self.volume.lock().unwrap();
            *vol = volume;
        }

        let mut active_notes = self.active_notes.lock().unwrap();
        for note in notes {
            if let (Some(string), Some(fret)) = (note.string, note.fret) {
                let frequency = Self::calculate_frequency(string, fret);
                let duration_seconds = 0.5;
                let ks = KarplusStrong::new(frequency, duration_seconds, self.sample_rate, decay);
                active_notes.push(ks);
            }
        }
    }
}

pub struct KarplusStrong {
    buffer: Vec<f32>,
    position: usize,
    remaining_samples: usize,
    decay: f32,
}

impl KarplusStrong {
    pub fn new(frequency: f32, duration_seconds: f32, sample_rate: f32, decay: f32) -> Self {
        let buffer_length = (sample_rate / frequency).ceil() as usize;
        let mut buffer = Vec::with_capacity(buffer_length);

        for _ in 0..buffer_length {
            buffer.push(random::<f32>() * 2.0 - 1.0);
        }

        let remaining_samples = (duration_seconds * sample_rate) as usize;
        KarplusStrong {
            buffer,
            position: 0,
            remaining_samples,
            decay,
        }
    }

    pub fn next_sample(&mut self) -> Option<f32> {
        if self.remaining_samples == 0 {
            return None;
        }

        let current_value = self.buffer[self.position];
        let next_index = (self.position + 1) % self.buffer.len();
        let next_value = self.buffer[next_index];

        let new_value = self.decay * 0.5 * (current_value + next_value);

        self.buffer[self.position] = new_value;
        self.position = next_index;
        self.remaining_samples -= 1;

        Some(current_value)
    }
}
