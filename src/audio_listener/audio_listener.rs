// audio_listener.rs

use crate::music_representation::musical_structures::{calculate_frequency, Note};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream};
use rustfft::{num_complex::Complex, FftPlanner};
use std::sync::{mpsc::Sender, Arc, Mutex};

pub struct AudioListener {
    stream: Option<Stream>,
    match_result_sender: Arc<Sender<f32>>,
    expected_notes: Arc<Mutex<Option<Vec<Note>>>>,
    pub sample_rate: f32,
    input_buffer: Arc<Mutex<Vec<f32>>>,
    // New fields for storing time-domain signals
    pub input_signal_history: Arc<Mutex<Vec<Vec<f32>>>>,
    pub expected_signal_history: Arc<Mutex<Vec<Vec<f32>>>>,
}

impl AudioListener {
    pub fn new(
        match_result_sender: Sender<f32>,
        expected_notes: Arc<Mutex<Option<Vec<Note>>>>,
    ) -> Self {
        // Initialize the sample rate
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .expect("No input device available");
        let config = device.default_input_config().unwrap();
        let sample_rate = config.sample_rate().0 as f32;

        // Initialize the input buffer
        let input_buffer = Arc::new(Mutex::new(Vec::new()));

        // Initialize signal histories
        let input_signal_history = Arc::new(Mutex::new(Vec::new()));
        let expected_signal_history = Arc::new(Mutex::new(Vec::new()));

        Self {
            stream: None, // We'll set this in the start method
            match_result_sender: Arc::new(match_result_sender),
            expected_notes,
            sample_rate,
            input_buffer,
            // Initialize new fields
            input_signal_history,
            expected_signal_history,
        }
    }

    pub fn start(&mut self) {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .expect("No input device available");
        let config = device.default_input_config().unwrap();

        // Clone fields to move into the closure
        let sample_rate = self.sample_rate;
        let match_result_sender = Arc::clone(&self.match_result_sender);
        let expected_notes = Arc::clone(&self.expected_notes);
        let input_buffer = Arc::clone(&self.input_buffer);
        let input_signal_history = Arc::clone(&self.input_signal_history);
        let expected_signal_history = Arc::clone(&self.expected_signal_history);

        let stream = match config.sample_format() {
            SampleFormat::F32 => device
                .build_input_stream(
                    &config.into(),
                    move |data: &[f32], _| {
                        // Call the processing function
                        Self::process_audio_input(
                            data,
                            sample_rate,
                            &match_result_sender,
                            &expected_notes,
                            &input_buffer,
                            &input_signal_history,
                            &expected_signal_history,
                        );
                    },
                    |err| eprintln!("Stream error: {}", err),
                    None,
                )
                .expect("Failed to build input stream"),
            _ => panic!("Unsupported sample format"),
        };

        self.stream = Some(stream);

        if let Some(ref stream) = self.stream {
            stream.play().expect("Failed to start audio stream");
        }
    }

    fn process_audio_input(
        data: &[f32],
        sample_rate: f32,
        match_result_sender: &Arc<Sender<f32>>,
        expected_notes: &Arc<Mutex<Option<Vec<Note>>>>,
        input_buffer: &Arc<Mutex<Vec<f32>>>,
        input_signal_history: &Arc<Mutex<Vec<Vec<f32>>>>,
        expected_signal_history: &Arc<Mutex<Vec<Vec<f32>>>>,
    ) {
        // Append incoming data to the input buffer
        {
            let mut buffer = input_buffer.lock().unwrap();
            buffer.extend_from_slice(data);
        }

        // Check if we have enough data to process (e.g., 2048 samples for FFT)
        const FRAME_SIZE: usize = 2048 * 2;
        const HOP_SIZE: usize = 512; // For overlapping frames

        loop {
            let mut buffer = input_buffer.lock().unwrap();

            if buffer.len() < FRAME_SIZE {
                break;
            }

            let mut input_signal = buffer[..FRAME_SIZE].to_vec();
            // Remove the processed samples, keeping the overlap
            buffer.drain(..HOP_SIZE);
            drop(buffer); // Release the lock

            // Get the expected notes
            let expected_notes_lock = expected_notes.lock().unwrap();
            if expected_notes_lock.is_none() {
                // No expected notes, skip processing
                continue;
            }
            let expected_notes_clone = expected_notes_lock.clone();
            drop(expected_notes_lock); // Release the lock

            // Generate expected signal
            let expected_signal =
                Self::generate_expected_signal(&expected_notes_clone, sample_rate, FRAME_SIZE);
            if let Some(mut expected_signal) = expected_signal {
                println!(
                    "Raw MIC: {:?}, Raw notes: {:?}",
                    input_signal.len(),
                    expected_signal.len()
                );
                // Normalize in time domain
                input_signal = normalize_signal(&input_signal);
                expected_signal = normalize_signal(&expected_signal);

                // Perform FFT
                let input_spectrum = Self::compute_fft_magnitude(&input_signal);
                let expected_spectrum = Self::compute_fft_magnitude(&expected_signal);

                // Normalize frequency domain
                let input_spectrum = normalize_signal(&input_spectrum);
                let expected_spectrum = normalize_signal(&expected_spectrum);

                println!(
                    "MIC spectra: {:?}, Notes spectra: {:?}",
                    input_spectrum.len(),
                    expected_spectrum.len()
                );
                // Compute similarity
                let similarity =
                    Self::compute_cosine_similarity(&input_spectrum, &expected_spectrum);

                // Send similarity value
                match_result_sender.send(similarity).ok();

                // Store time-domain signals in Mutex Vec<f32> sent to gui
                {
                    let mut input_signal_hist = input_signal_history.lock().unwrap();
                    let mut expected_signal_hist = expected_signal_history.lock().unwrap();

                    input_signal_hist.push(input_spectrum.clone());
                    expected_signal_hist.push(expected_spectrum.clone());

                    // Limit history size
                    const MAX_HISTORY_LENGTH: usize = 100;
                    if input_signal_hist.len() > MAX_HISTORY_LENGTH {
                        input_signal_hist.remove(0);
                        expected_signal_hist.remove(0);
                    }
                }
            } else {
                // No expected signal to compare
                match_result_sender.send(0.0).ok();
            }
        }
    }

    fn generate_expected_signal(
        expected_notes: &Option<Vec<Note>>,
        sample_rate: f32,
        num_samples: usize,
    ) -> Option<Vec<f32>> {
        if let Some(notes) = expected_notes {
            let mut signal = vec![0.0; num_samples];
            for note in notes {
                if let (Some(string), Some(fret)) = (note.string, note.fret) {
                    let frequency = calculate_frequency(string, fret);
                    for i in 0..num_samples {
                        let t = i as f32 / sample_rate;
                        signal[i] += (2.0 * std::f32::consts::PI * frequency * t).sin();
                    }
                }
            }
            Some(signal)
        } else {
            None
        }
    }

    fn compute_fft_magnitude(signal: &[f32]) -> Vec<f32> {
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(signal.len());
        let mut buffer: Vec<Complex<f32>> =
            signal.iter().map(|&s| Complex { re: s, im: 0.0 }).collect();
        fft.process(&mut buffer);
        buffer.iter().map(|c| c.norm()).collect()
    }

    fn compute_cosine_similarity(spectrum1: &[f32], spectrum2: &[f32]) -> f32 {
        let dot_product: f32 = spectrum1
            .iter()
            .zip(spectrum2.iter())
            .map(|(a, b)| a * b)
            .sum();
        let magnitude1: f32 = spectrum1.iter().map(|a| a * a).sum::<f32>().sqrt();
        let magnitude2: f32 = spectrum2.iter().map(|b| b * b).sum::<f32>().sqrt();
        if magnitude1 > 0.0 && magnitude2 > 0.0 {
            dot_product / (magnitude1 * magnitude2)
        } else {
            0.0
        }
    }
}
pub fn normalize_signal(signal: &Vec<f32>) -> Vec<f32> {
    if signal.is_empty() {
        return signal.clone();
    }
    let max_abs = signal.iter().fold(0.0_f32, |max, &x| max.max(x.abs()));
    if max_abs == 0.0 {
        signal.clone() // Avoid division by zero
    } else {
        signal.iter().map(|&x| x / max_abs).collect()
    }
}
