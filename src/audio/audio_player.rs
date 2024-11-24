// audio/audio_player.rs

use std::sync::Arc;

use crate::guitar::guitar::GuitarConfig;
use crate::karplus_strong::karplus_strong::KarplusStrong;
use crate::music_representation::{calculate_frequency, Note};

use kira::manager::{AudioManager, AudioManagerSettings};

use kira::sound::static_sound::{StaticSoundData, StaticSoundSettings};
use kira::Frame;
pub struct AudioPlayer {
    manager: Option<AudioManager>,
    pub sample_rate: f32,
    configs: GuitarConfig,
    pub output_signal: Vec<f32>,
}
impl AudioPlayer {
    pub fn new(configs: GuitarConfig) -> Self {
        let sample_rate = 44_100.0; // Standard sample rate

        Self {
            manager: None,
            sample_rate,
            configs,
            output_signal: Vec::new(),
        }
    }

    pub fn start(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.manager.is_none() {
            let manager = AudioManager::new(AudioManagerSettings::default())?;
            self.manager = Some(manager);
        }

        Ok(())
    }

    pub fn play_notes(&mut self, notes: &[Note], duration: f32) {
        if let Some(manager) = &mut self.manager {
            let configs = &self.configs;

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
                self.output_signal.extend_from_slice(&audio_data);

                // Limit size
                let n = 44100; // Keep last 1 second at 44.1kHz
                if self.output_signal.len() > n {
                    let remove_count = self.output_signal.len() - n;
                    self.output_signal.drain(0..remove_count);
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
                manager.play(sound).expect("Failed to play sound");
            }
        } else {
            eprintln!("AudioManager is not initialized");
        }
    }
}
