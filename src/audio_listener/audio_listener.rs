use crate::audio_player::audio_player::KarplusStrong;
use crate::music_representation::musical_structures::Note;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream};
use rustfft::{num_complex::Complex, FftPlanner};
use std::sync::{mpsc::Sender, Arc, Mutex};

pub struct AudioListener {
    stream: Stream,
    match_result_sender: Sender<bool>,
    expected_notes: Arc<Mutex<Option<Vec<Note>>>>,
    sample_rate: f32,
    matching_threshold: Arc<Mutex<f32>>,
    input_buffer: Arc<Mutex<Vec<f32>>>,
}

impl AudioListener {
    pub fn new(
        match_result_sender: Sender<bool>,
        expected_notes: Arc<Mutex<Option<Vec<Note>>>>,
        matching_threshold: Arc<Mutex<f32>>,
    ) -> Self {
        // Initialize the audio input stream
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .expect("No input device available");
        let config = device.default_input_config().unwrap();
        let sample_rate = config.sample_rate().0 as f32;

        let expected_notes_clone = expected_notes.clone();
        let match_result_sender_clone = match_result_sender.clone();
        let matching_threshold_clone = matching_threshold.clone();
        let input_buffer = Arc::new(Mutex::new(Vec::new()));
        let input_buffer_clone = input_buffer.clone();

        let stream = match config.sample_format() {
            SampleFormat::F32 => device
                .build_input_stream(
                    &config.into(),
                    move |data: &[f32], _| {
                        Self::process_audio_input(
                            data,
                            sample_rate,
                            &match_result_sender_clone,
                            &expected_notes_clone,
                            &matching_threshold_clone,
                            &input_buffer_clone,
                        );
                    },
                    |err| eprintln!("Stream error: {}", err),
                    None,
                )
                .unwrap(),
            _ => panic!("Unsupported sample format"),
        };

        Self {
            stream,
            match_result_sender,
            expected_notes,
            sample_rate,
            matching_threshold,
            input_buffer,
        }
    }

    pub fn start(&self) {
        self.stream.play().expect("Failed to start audio stream");
    }

    fn process_audio_input(
        data: &[f32],
        sample_rate: f32,
        match_result_sender: &Sender<bool>,
        expected_notes: &Arc<Mutex<Option<Vec<Note>>>>,
        matching_threshold: &Arc<Mutex<f32>>,
        input_buffer: &Arc<Mutex<Vec<f32>>>,
    ) {
        // Append incoming data to the input buffer
        {
            let mut buffer = input_buffer.lock().unwrap();
            buffer.extend_from_slice(data);
        }

        // Check if we have enough data to process
        const REQUIRED_SAMPLES: usize = 44100; // 1 second of audio at 44.1kHz
        let buffer_length = {
            let buffer = input_buffer.lock().unwrap();
            buffer.len()
        };

        if buffer_length >= REQUIRED_SAMPLES {
            // Clone the input buffer for processing
            let input_signal = {
                let mut buffer = input_buffer.lock().unwrap();
                let signal = buffer.drain(..REQUIRED_SAMPLES).collect::<Vec<f32>>();
                signal
            };

            // Generate expected signal
            let expected_signal =
                Self::generate_expected_signal(expected_notes, sample_rate, REQUIRED_SAMPLES);

            if let Some(expected_signal) = expected_signal {
                // Preprocess signals (e.g., normalization)
                let input_signal = Self::normalize_signal(&input_signal);
                let expected_signal = Self::normalize_signal(&expected_signal);

                // Compute Euclidean distance
                let distance = Self::compute_euclidean_distance(&input_signal, &expected_signal);

                // Get matching threshold
                let threshold = *matching_threshold.lock().unwrap();

                // Determine if it's a match
                let is_match = distance <= threshold;

                // Send match result
                match_result_sender.send(is_match).ok();
            } else {
                // No expected notes to compare
                match_result_sender.send(false).ok();
            }
        }
    }

    fn generate_expected_signal(
        expected_notes: &Arc<Mutex<Option<Vec<Note>>>>,
        sample_rate: f32,
        num_samples: usize,
    ) -> Option<Vec<f32>> {
        let expected_notes = expected_notes.lock().unwrap();
        if let Some(expected_notes) = &*expected_notes {
            let mut signal = vec![0.0; num_samples];
            for note in expected_notes {
                if let (Some(string), Some(fret)) = (note.string, note.fret) {
                    let frequency = Self::calculate_frequency(string, fret);
                    let duration_seconds = 0.5;
                    let decay = 0.996; // Use default decay or get from config
                    let mut ks =
                        KarplusStrong::new(frequency, duration_seconds, sample_rate, decay);

                    // Generate samples for the note
                    for i in 0..num_samples {
                        if let Some(sample) = ks.next_sample() {
                            signal[i] += sample;
                        } else {
                            break;
                        }
                    }
                }
            }
            let sum: f32 = signal.iter().sum();
            println!("Sum of signal for debug: {:?}", sum);
            Some(signal)
        } else {
            None
        }
    }

    fn compute_euclidean_distance(signal1: &[f32], signal2: &[f32]) -> f32 {
        let sum_of_squares = signal1
            .iter()
            .zip(signal2.iter())
            .map(|(a, b)| (a - b).powi(2))
            .sum::<f32>();
        sum_of_squares.sqrt()
    }

    fn normalize_signal(signal: &[f32]) -> Vec<f32> {
        let max_amplitude = signal
            .iter()
            .cloned()
            .fold(0. / 0., f32::max)
            .abs()
            .max(signal.iter().cloned().fold(0. / 0., f32::min).abs());

        if max_amplitude > 0.0 {
            signal.iter().map(|&s| s / max_amplitude).collect()
        } else {
            signal.to_vec()
        }
    }

    fn calculate_frequency(string: u8, fret: u8) -> f32 {
        // Same as in AudioPlayer
        let open_string_frequencies = [329.63, 246.94, 196.00, 146.83, 110.00, 82.41];
        let string_index = (string - 1).min(5) as usize;
        let open_frequency = open_string_frequencies[string_index];
        let frequency = open_frequency * (2f32).powf(fret as f32 / 12.0);
        frequency
    }
}
