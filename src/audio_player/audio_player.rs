// audio_player.rs

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream};
use rand::random;
use std::f32::consts::PI;
use std::sync::{Arc, Mutex};

use crate::gui::gui::Configs;
use crate::music_representation::musical_structures::{calculate_frequency, Note};

pub struct AudioPlayer {
    stream: Stream,
    active_notes: Arc<Mutex<Vec<KarplusStrong>>>,
    pub sample_rate: f32,
    volume: f32, // Changed from Arc<Mutex<f32>> to f32
    pub seconds_per_division: f32,
    pub configs: Configs, // Now stores its own Configs
}

impl AudioPlayer {
    pub fn new(configs: Configs) -> Self {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .expect("No output device available");
        let config = device.default_output_config().unwrap();
        let sample_rate = config.sample_rate().0 as f32;
        let channels = config.channels() as usize;

        let active_notes = Arc::new(Mutex::new(Vec::new()));
        let active_notes_clone = Arc::clone(&active_notes);

        let volume = configs.volume;

        let configs_clone = configs.clone();

        let stream = match config.sample_format() {
            SampleFormat::F32 => device
                .build_output_stream(
                    &config.into(),
                    {
                        // Clone necessary Arcs and configurations for the closure
                        move |data: &mut [f32], _| {
                            AudioPlayer::write_data(
                                data,
                                channels,
                                &active_notes_clone,
                                volume,
                                &configs_clone,
                                sample_rate,
                            );
                        }
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
            configs,
        }
    }

    pub fn start(&self) {
        self.stream.play().expect("Failed to start audio stream");
    }

    pub fn update_seconds_per_division(&mut self, tempo: f32, divisions_per_quarter: f32) {
        let seconds_per_beat = 60.0 / tempo;
        self.seconds_per_division = seconds_per_beat / divisions_per_quarter;
    }

    /// Static method to write audio data
    fn write_data(
        output: &mut [f32],
        channels: usize,
        active_notes: &Arc<Mutex<Vec<KarplusStrong>>>,
        volume: f32,
        configs: &Configs,
        sample_rate: f32,
    ) {
        let mut active_notes = active_notes.lock().unwrap();
        let guitar_config = &configs.custom_guitar_config;

        for frame in output.chunks_mut(channels) {
            let mut value = 0.0;

            // Sum samples from all active notes
            active_notes.retain_mut(|note| {
                if let Some(sample) = note.next_sample(guitar_config, sample_rate) {
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

    pub fn play_notes_with_config(&self, notes: &[Note], seconds_per_division: f32) {
        let guitar_config = &self.configs.custom_guitar_config;

        let mut active_notes = self.active_notes.lock().unwrap();
        for note in notes {
            let frequency = calculate_frequency(note);

            let ks = KarplusStrong::new(
                frequency,
                seconds_per_division * note.duration as f32,
                self.sample_rate,
                guitar_config,
            );
            active_notes.push(ks);
        }
    }

    /// Sets a new decay parameter for all active notes.
    pub fn set_decay(&mut self, new_decay: f32) {
        let mut active_notes = self.active_notes.lock().unwrap();
        for ks in active_notes.iter_mut() {
            ks.decay = new_decay;
        }
    }

    /// Sets a new volume parameter.
    pub fn set_volume(&mut self, new_volume: f32) {
        self.volume = new_volume;
    }

    /// Updates the AudioPlayer's configurations.
    pub fn update_configs(&mut self, configs: Configs) {
        self.configs = configs;
        self.volume = self.configs.volume;
    }
}

#[derive(Clone)]
pub struct GuitarConfig {
    pub decay: f32,
    pub string_damping: f32,
    pub body_resonance: f32,
    pub body_damping: f32,
    pub pickup_position: f32,
}

impl GuitarConfig {
    pub fn acoustic() -> Self {
        GuitarConfig {
            decay: 0.998,
            string_damping: 0.2,
            body_resonance: 100.0,
            body_damping: 0.1,
            pickup_position: 0.85,
        }
    }
}

pub struct KarplusStrong {
    pub buffer: Vec<f32>,
    pub position: usize,
    pub remaining_samples: usize,
    pub decay: f32,
}

impl KarplusStrong {
    pub fn new(
        frequency: f32,
        duration_seconds: f32,
        sample_rate: f32,
        config: &GuitarConfig,
    ) -> Self {
        let buffer_length = (sample_rate / frequency).ceil() as usize;
        let mut buffer = Vec::with_capacity(buffer_length);

        let mut prev = 0.0;
        for _ in 0..buffer_length {
            let white = random::<f32>() * 2.0 - 1.0;
            // Lowpass filter the white noise
            let filtered = config.string_damping * prev + (1.0 - config.string_damping) * white;
            buffer.push(filtered);
            prev = filtered;
        }

        let remaining_samples = (duration_seconds * sample_rate) as usize;
        KarplusStrong {
            buffer,
            position: 0,
            remaining_samples,
            decay: config.decay,
        }
    }

    pub fn next_sample(&mut self, config: &GuitarConfig, sample_rate: f32) -> Option<f32> {
        if self.remaining_samples == 0 {
            return None;
        }

        let current_value = self.buffer[self.position];
        let next_index = (self.position + 1) % self.buffer.len();
        let next_value = self.buffer[next_index];

        let string_sample = self.decay
            * (config.string_damping * current_value + (1.0 - config.string_damping) * next_value);

        let body_freq = 2.0 * PI * config.body_resonance / sample_rate;

        let resonated = string_sample * body_freq.sin();
        let body_sample = resonated * (1.0 - config.body_damping);

        self.buffer[self.position] = string_sample;
        self.position = next_index;
        self.remaining_samples -= 1;

        Some(string_sample * 0.7 + body_sample * 0.3)
    }
}
