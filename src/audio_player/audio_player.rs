pub mod audio_player {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use cpal::{SampleFormat, Stream};

    use crate::music_representation::musical_structures::Note;
    use std::collections::VecDeque;

    use std::f32::consts::PI;
    use std::sync::{Arc, Mutex};

    pub struct AudioPlayer {
        stream: Stream,
        notes_queue: Arc<Mutex<VecDeque<(f32, f32)>>>, // (frequency, duration)
    }

    impl AudioPlayer {
        pub fn new() -> Self {
            // Initialize the audio output stream
            let host = cpal::default_host();
            let device = host
                .default_output_device()
                .expect("No output device available");
            let config = device.default_output_config().unwrap();
            let sample_rate = config.sample_rate().0 as f32;
            let channels = config.channels() as usize;

            let notes_queue = Arc::new(Mutex::new(VecDeque::new()));
            let notes_queue_clone = notes_queue.clone();

            let stream = match config.sample_format() {
                SampleFormat::F32 => device
                    .build_output_stream(
                        &config.into(),
                        move |data: &mut [f32], _| {
                            Self::write_data(data, channels, sample_rate, &notes_queue_clone);
                        },
                        |err| eprintln!("Stream error: {}", err),
                        None,
                    )
                    .unwrap(),
                _ => panic!("Unsupported sample format"),
            };

            Self {
                stream,
                notes_queue,
            }
        }

        pub fn start(&self) {
            self.stream.play().expect("Failed to start audio stream");
        }

        fn write_data(
            output: &mut [f32],
            channels: usize,
            sample_rate: f32,
            notes_queue: &Arc<Mutex<VecDeque<(f32, f32)>>>,
        ) {
            let mut notes_queue = notes_queue.lock().unwrap();

            let mut sample_clock = 0f32;
            for frame in output.chunks_mut(channels) {
                let value = if let Some((frequency, duration_samples)) = notes_queue.front_mut() {
                    let value = (sample_clock * *frequency * 2.0 * PI / sample_rate).sin();
                    sample_clock += 1.0;
                    *duration_samples -= 1.0;
                    if *duration_samples <= 0.0 {
                        notes_queue.pop_front();
                        sample_clock = 0.0;
                    }
                    value
                } else {
                    0.0
                };

                for sample in frame.iter_mut() {
                    *sample = value;
                }
            }
        }

        fn calculate_frequency(string: u8, fret: u8) -> f32 {
            // Standard tuning frequencies for open strings EADGBE
            let open_string_frequencies = [329.63, 246.94, 196.00, 146.83, 110.00, 82.41];
            let string_index = (string - 1).min(5) as usize; // Ensure index is within bounds
            let open_frequency = open_string_frequencies[string_index];
            // Each fret increases the frequency by a semitone (approximately 2^(1/12))
            let frequency = open_frequency * (2f32).powf(fret as f32 / 12.0);
            frequency
        }

        pub fn play_notes(&self, notes: &[Note]) {
            let mut notes_queue = self.notes_queue.lock().unwrap();
            for note in notes {
                if let (Some(string), Some(fret)) = (note.string, note.fret) {
                    let frequency = Self::calculate_frequency(string, fret);
                    // Assume a fixed duration for simplicity, e.g., 0.5 seconds
                    let duration_seconds = 0.5;
                    let sample_rate = 44100.0; // Adjust according to your configuration
                    let duration_samples = duration_seconds * sample_rate;
                    notes_queue.push_back((frequency, duration_samples));
                }
            }
        }
    }
}
