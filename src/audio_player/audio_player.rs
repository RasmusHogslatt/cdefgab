// audio_player.rs

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream};
use rand::random;
use std::f32::consts::PI;
use std::fmt;
use std::sync::{Arc, Mutex};

use crate::gui::gui::Configs;
use crate::music_representation::musical_structures::{calculate_frequency, Note};

pub struct AudioPlayer {
    stream: Stream,
    active_notes: Arc<Mutex<Vec<KarplusStrong>>>,
    pub sample_rate: f32,
    volume: f32,
    pub seconds_per_division: f32,
    pub configs: Arc<Mutex<Configs>>,
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

        // Wrap configs in Arc<Mutex<...>> for thread-safe shared access
        let configs = Arc::new(Mutex::new(configs));
        let configs_clone = configs.clone();

        let stream = match config.sample_format() {
            SampleFormat::F32 => device
                .build_output_stream(
                    &config.into(),
                    {
                        move |data: &mut [f32], _| {
                            AudioPlayer::write_data(
                                data,
                                channels,
                                &active_notes_clone,
                                &configs_clone, // Pass the Arc<Mutex<Configs>>
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

    /// Static method to write audio data
    fn write_data(
        output: &mut [f32],
        channels: usize,
        active_notes: &Arc<Mutex<Vec<KarplusStrong>>>,
        configs: &Arc<Mutex<Configs>>,
        sample_rate: f32,
    ) {
        let mut active_notes = active_notes.lock().unwrap();
        let configs = configs.lock().unwrap(); // Lock to access current configs
        let guitar_config = &configs.guitar_configs[configs.active_guitar];

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

            // Apply volume from configs
            value *= configs.volume;

            // Prevent clipping
            value = value.clamp(-1.0, 1.0);

            for sample in frame.iter_mut() {
                *sample = value;
            }
        }
    }

    pub fn play_notes_with_config(&self, notes: &[Note], seconds_per_division: f32) {
        let configs = self.configs.lock().unwrap();
        let guitar_config = &configs.guitar_configs[configs.active_guitar];

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

    /// Updates the AudioPlayer's configurations.
    pub fn update_configs(&mut self, configs: Configs) {
        let mut guard = self.configs.lock().unwrap();
        *guard = configs;
        self.volume = guard.volume;
    }
}

#[derive(Default, Clone, Debug)]
pub enum GuitarType {
    #[default]
    Custom,
    Acoustic,
    Classical,
    Electric,
}

impl fmt::Display for GuitarType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GuitarType::Custom => write!(f, "Custom"),
            GuitarType::Acoustic => write!(f, "Acoustic"),
            GuitarType::Classical => write!(f, "Classical"),
            GuitarType::Electric => write!(f, "Electric"),
        }
    }
}

#[derive(Clone)]
pub struct GuitarConfig {
    pub decay: f32,
    pub string_damping: f32,
    pub body_resonance: f32,
    pub body_damping: f32,
    pub pickup_position: f32,
    pub name: GuitarType,
}

impl GuitarConfig {
    pub fn acoustic() -> Self {
        GuitarConfig {
            decay: 0.998,
            string_damping: 0.2,
            body_resonance: 100.0,
            body_damping: 0.1,
            pickup_position: 0.85,
            name: GuitarType::Acoustic,
        }
    }

    pub fn electric() -> Self {
        GuitarConfig {
            decay: 0.995,
            string_damping: 0.1,
            body_resonance: 150.0,
            body_damping: 0.3,
            pickup_position: 0.8,
            name: GuitarType::Electric,
        }
    }

    pub fn classical() -> Self {
        GuitarConfig {
            decay: 0.997,
            string_damping: 0.3,
            body_resonance: 90.0,
            body_damping: 0.05,
            pickup_position: 0.85,
            name: GuitarType::Classical,
        }
    }
}

pub struct KarplusStrong {
    pub buffer: Vec<f32>,
    pub position: usize,
    pub remaining_samples: usize,
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
        }
    }

    pub fn next_sample(&mut self, config: &GuitarConfig, sample_rate: f32) -> Option<f32> {
        if self.remaining_samples == 0 {
            return None;
        }

        let current_value = self.buffer[self.position];
        let next_index = (self.position + 1) % self.buffer.len();
        let next_value = self.buffer[next_index];

        // Use config.decay instead of self.decay
        let string_sample = config.decay
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
