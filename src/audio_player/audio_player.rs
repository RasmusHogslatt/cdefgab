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
            // Pass the scale_length and capo_fret to the calculate_frequency function
            let frequency = calculate_frequency(
                note,
                guitar_config.scale_length,
                guitar_config.capo_fret, // New parameter
            );

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
    Bass,
    TwelveString,
}

impl fmt::Display for GuitarType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GuitarType::Custom => write!(f, "Custom"),
            GuitarType::Acoustic => write!(f, "Acoustic"),
            GuitarType::Classical => write!(f, "Classical"),
            GuitarType::Electric => write!(f, "Electric"),
            GuitarType::Bass => write!(f, "Bass"),
            GuitarType::TwelveString => write!(f, "Twelve string"),
        }
    }
}

#[derive(Clone)]
pub struct GuitarConfig {
    pub decay: f32,
    pub string_damping: f32,
    pub body_resonance: f32,
    pub body_damping: f32,
    pub string_tension: f32,
    pub scale_length: f32,
    pub capo_fret: u8, // New Parameter: Fret number where capo is placed (0 = no capo)
    pub name: GuitarType,
}

impl GuitarConfig {
    pub fn acoustic() -> Self {
        Self {
            name: GuitarType::Acoustic,
            decay: 0.995,          // Medium sustain typical for acoustic guitars
            string_damping: 0.4,   // Moderate string damping
            body_resonance: 150.0, // Prominent body resonance around 150 Hz
            body_damping: 0.2,     // Low body damping for richer resonance
            string_tension: 0.8,   // High tension for steel strings
            scale_length: 25.5,    // Common scale length for acoustic guitars
            capo_fret: 0,
        }
    }

    pub fn electric() -> Self {
        Self {
            name: GuitarType::Electric,
            decay: 0.999,         // Longer sustain due to pickups and solid body
            string_damping: 0.1,  // Less string damping
            body_resonance: 70.0, // Minimal body resonance in solid bodies
            body_damping: 0.8,    // High body damping
            string_tension: 0.8,  // Similar tension to acoustic steel strings
            scale_length: 25.5,   // Common scale length (Fender style)
            capo_fret: 0,
        }
    }

    pub fn classical() -> Self {
        Self {
            name: GuitarType::Classical,
            decay: 0.990,          // Shorter sustain due to nylon strings
            string_damping: 0.6,   // Higher string damping
            body_resonance: 120.0, // Body resonance typical around 120 Hz
            body_damping: 0.3,     // Moderate body damping
            string_tension: 0.5,   // Lower tension for nylon strings
            scale_length: 25.6,    // Standard scale length for classical guitars
            capo_fret: 0,
        }
    }

    pub fn bass_guitar() -> Self {
        Self {
            name: GuitarType::Bass,
            decay: 0.997,        // Long sustain typical for bass guitars
            string_damping: 0.3, // Less string damping
            body_resonance: 0.0, // Minimal body resonance
            body_damping: 0.9,   // High body damping
            string_tension: 0.9, // Very high string tension
            scale_length: 34.0,  // Standard long scale length for bass guitars
            capo_fret: 0,
        }
    }

    pub fn twelve_string() -> Self {
        Self {
            name: GuitarType::TwelveString,
            decay: 0.994,          // Slightly shorter sustain due to extra strings
            string_damping: 0.5,   // Slightly higher string damping
            body_resonance: 150.0, // Similar to acoustic guitars
            body_damping: 0.2,     // Low body damping
            string_tension: 0.9,   // Higher tension due to additional strings
            scale_length: 25.5,    // Common scale length
            capo_fret: 0,
        }
    }

    pub fn custom(
        decay: f32,
        string_damping: f32,
        body_resonance: f32,
        body_damping: f32,
        string_tension: f32,
        scale_length: f32,
        capo_fret: u8, // Allow setting capo_fret
    ) -> Self {
        // Validate capo_fret to prevent unrealistic values
        let validated_capo_fret = capo_fret.min(24); // Assuming a maximum of 24 frets

        GuitarConfig {
            decay,
            string_damping,
            body_resonance,
            body_damping,
            string_tension,
            scale_length,
            capo_fret: validated_capo_fret,
            name: GuitarType::Custom,
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

            let tension_effect = config.string_tension * white;
            let filtered =
                config.string_damping * prev + (1.0 - config.string_damping) * tension_effect;
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
