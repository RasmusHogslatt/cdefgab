// audio/audio_player.rs

use crate::guitar::guitar::GuitarConfig;
use crate::karplus_strong::karplus_strong::KarplusStrong;
use crate::music_representation::{calculate_frequency, Note};
use std::sync::{Arc, Mutex};

use kira::manager::{AudioManager, AudioManagerSettings};

use kira::sound::static_sound::{StaticSoundData, StaticSoundSettings};
use kira::Frame;

pub struct AudioPlayer {
    // #[cfg(not(target_arch = "wasm32"))]
    manager: Mutex<AudioManager>,
    // #[cfg(target_arch = "wasm32")]
    // manager: Mutex<AudioManager<WebAudioBackend>>,
    pub sample_rate: f32,
    configs: Arc<Mutex<GuitarConfig>>,
    pub output_signal: Arc<Mutex<Vec<f32>>>,
}

impl AudioPlayer {
    pub fn new(configs: GuitarConfig) -> Result<Self, Box<dyn std::error::Error>> {
        // #[cfg(not(target_arch = "wasm32"))]
        let manager = AudioManager::new(AudioManagerSettings::default())?;

        // #[cfg(target_arch = "wasm32")]
        // let manager = AudioManager::<WebAudioBackend>::new(AudioManagerSettings::default())?;

        // Since kira doesn't expose the sample rate, we'll use the common sample rate.
        let sample_rate = 44_100.0; // Standard sample rate

        let configs = Arc::new(Mutex::new(configs));
        let output_signal = Arc::new(Mutex::new(Vec::new()));

        Ok(Self {
            manager: Mutex::new(manager),
            sample_rate,
            configs,
            output_signal,
        })
    }

    pub fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        // No action needed for kira
        Ok(())
    }

    pub fn play_notes(&self, notes: &[Note], duration: f32) {
        let configs = self.configs.lock().unwrap();

        for note in notes {
            let frequency = calculate_frequency(note, configs.scale_length, configs.capo_fret);
            let mut karplus_strong =
                KarplusStrong::new(frequency, duration, self.sample_rate, &configs);
            let mut audio_data = karplus_strong.generate_audio_data();

            // Apply volume
            for sample in &mut audio_data {
                *sample *= configs.volume;
            }

            // Collect output_signal for plotting
            {
                let mut output_signal = self.output_signal.lock().unwrap();
                output_signal.extend_from_slice(&audio_data);

                // Limit size
                let n = 44100; // Keep last 1 second at 44.1kHz
                if output_signal.len() > n {
                    let remove_count = output_signal.len() - n;
                    output_signal.drain(0..remove_count);
                }
            }

            // Convert audio_data (Vec<f32>) to frames (Vec<Frame>)
            let frames: Vec<Frame> = audio_data
                .iter()
                .map(|&sample| Frame::from_mono(sample))
                .collect();

            // Convert Vec<Frame> into Arc<[Frame]>
            let frames_arc = Arc::from(frames.into_boxed_slice());

            // Create a StaticSoundData by initializing its fields
            let sound = StaticSoundData {
                sample_rate: self.sample_rate as u32,
                frames: frames_arc,
                settings: StaticSoundSettings::default(),
                slice: None,
            };

            // Play the sound
            let mut manager = self.manager.lock().unwrap();
            manager.play(sound).expect("Failed to play sound");
        }
    }

    pub fn update_configs(&mut self, configs: GuitarConfig) {
        let mut guard = self.configs.lock().unwrap();
        *guard = configs;
    }
}
