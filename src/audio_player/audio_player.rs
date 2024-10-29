// audio_player.rs

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream};
use rand::random;
use std::sync::{Arc, Mutex};

use crate::gui::gui::Configs;
use crate::music_representation::musical_structures::{calculate_frequency, Note};

pub struct AudioPlayer {
    stream: Stream,
    active_notes: Arc<Mutex<Vec<KarplusStrong>>>,
    sample_rate: f32,
    volume: Arc<Mutex<f32>>,
    pub seconds_per_division: f32,
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
        let seconds_per_division = 0.5;
        Self {
            stream,
            active_notes,
            sample_rate,
            volume,
            seconds_per_division,
        }
    }

    pub fn start(&self) {
        self.stream.play().expect("Failed to start audio stream");
    }

    pub fn update_seconds_per_division(&mut self, tempo: f32, divisions_per_quarter: f32) {
        let seconds_per_beat = 60.0 / tempo;
        self.seconds_per_division = seconds_per_beat / divisions_per_quarter;
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

    pub fn play_notes_with_config(
        &self,
        notes: &[Note],
        config: &Configs,
        volume: f32,
        seconds_per_division: f32,
    ) {
        // Update volume
        self.set_volume(volume);

        let mut active_notes = self.active_notes.lock().unwrap();
        for note in notes {
            let frequency = calculate_frequency(note);

            let ks = KarplusStrong::new(
                frequency,
                seconds_per_division * note.duration as f32,
                self.sample_rate,
                config.decay,
            );
            active_notes.push(ks);
        }
    }

    /// Sets a new decay parameter for all active notes.
    pub fn set_decay(&self, new_decay: f32) {
        let mut active_notes = self.active_notes.lock().unwrap();
        for ks in active_notes.iter_mut() {
            ks.decay = new_decay;
        }
    }

    /// Sets a new volume parameter.
    pub fn set_volume(&self, new_volume: f32) {
        let mut vol = self.volume.lock().unwrap();
        *vol = new_volume;
    }
}

/// Make KarplusStrong public
pub struct KarplusStrong {
    pub buffer: Vec<f32>,
    pub position: usize,
    pub remaining_samples: usize,
    pub decay: f32,
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
