// audio/audio_player.rs

use crate::guitar::guitar::GuitarConfig;
use crate::karplus_strong::karplus_strong::KarplusStrong;
use crate::music_representation::{calculate_frequency, Note};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream};
use std::sync::{Arc, Mutex};

pub struct AudioPlayer {
    stream: Option<Stream>,
    active_notes: Arc<Mutex<Vec<KarplusStrong>>>,
    sample_rate: f32,
    configs: Arc<Mutex<GuitarConfig>>,
}

impl AudioPlayer {
    pub fn new(configs: GuitarConfig) -> Result<Self, Box<dyn std::error::Error>> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or("No output device available")?;
        let config = device.default_output_config()?;
        let sample_rate = config.sample_rate().0 as f32;
        let channels = config.channels() as usize;

        let active_notes = Arc::new(Mutex::new(Vec::new()));
        let active_notes_clone = Arc::clone(&active_notes);

        // Wrap configs in Arc<Mutex<...>> for thread-safe shared access
        let configs = Arc::new(Mutex::new(configs));
        let configs_clone = Arc::clone(&configs);

        let stream = match config.sample_format() {
            SampleFormat::F32 => device.build_output_stream(
                &config.into(),
                move |data: &mut [f32], _| {
                    if let Err(e) = AudioPlayer::write_data(
                        data,
                        channels,
                        &active_notes_clone,
                        &configs_clone,
                        sample_rate,
                    ) {
                        eprintln!("Error in audio output stream: {}", e);
                    }
                },
                |err| eprintln!("Stream error: {}", err),
                None,
            )?,
            _ => return Err("Unsupported sample format".into()),
        };

        Ok(Self {
            stream: Some(stream),
            active_notes,
            sample_rate,
            configs,
        })
    }

    pub fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(ref stream) = self.stream {
            stream.play()?;
        }
        Ok(())
    }

    fn write_data(
        output: &mut [f32],
        channels: usize,
        active_notes: &Arc<Mutex<Vec<KarplusStrong>>>,
        configs: &Arc<Mutex<GuitarConfig>>,
        sample_rate: f32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut active_notes = active_notes.lock().unwrap();
        let configs = configs.lock().unwrap();

        for frame in output.chunks_mut(channels) {
            let mut value = 0.0;

            // Sum samples from all active notes
            active_notes.retain_mut(|note| {
                if let Some(sample) = note.next_sample(&configs, sample_rate) {
                    value += sample;
                    true
                } else {
                    false
                }
            });

            // Apply volume
            value *= configs.volume;

            // Prevent clipping
            value = value.clamp(-1.0, 1.0);

            for sample in frame.iter_mut() {
                *sample = value;
            }
        }

        Ok(())
    }

    pub fn play_notes(&self, notes: &[Note], duration: f32) {
        let configs = self.configs.lock().unwrap();

        let mut active_notes = self.active_notes.lock().unwrap();
        for note in notes {
            let frequency = calculate_frequency(note, configs.scale_length, configs.capo_fret);
            let ks = KarplusStrong::new(frequency, duration, self.sample_rate, &configs);
            active_notes.push(ks);
        }
    }

    pub fn update_configs(&mut self, configs: GuitarConfig) {
        let mut guard = self.configs.lock().unwrap();
        *guard = configs;
    }
}
