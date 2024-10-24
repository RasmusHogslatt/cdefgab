use crate::music_representation::musical_structures::{calculate_frequency, Note};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream};
use rustfft::{num_complex::Complex, FftPlanner};
use std::sync::{mpsc::Sender, Arc, Mutex};

pub struct AudioListener {
    stream: Option<Stream>,
    match_result_sender: Arc<Sender<f32>>,
    expected_notes: Arc<Mutex<Option<Vec<Note>>>>,
    sample_rate: f32,
    input_buffer: Arc<Mutex<Vec<f32>>>,
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

        Self {
            stream: None, // We'll set this in the start method
            match_result_sender: Arc::new(match_result_sender),
            expected_notes,
            sample_rate,
            input_buffer,
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
    ) {
        // Append incoming data to the input buffer
        {
            let mut buffer = input_buffer.lock().unwrap();
            buffer.extend_from_slice(data);
        }

        // Check if we have enough data to process (e.g., 2048 samples for FFT)
        const FRAME_SIZE: usize = 2048;
        const HOP_SIZE: usize = 512; // For overlapping frames

        loop {
            let mut buffer = input_buffer.lock().unwrap();

            if buffer.len() < FRAME_SIZE {
                break;
            }

            let input_signal = buffer[..FRAME_SIZE].to_vec();
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

            // Perform FFT on the input signal
            let input_spectrum = Self::compute_fft_magnitude(&input_signal);

            // Generate expected signal
            let expected_signal =
                Self::generate_expected_signal(&expected_notes_clone, sample_rate, FRAME_SIZE);

            if let Some(expected_signal) = expected_signal {
                // Compute FFT of expected signal
                let expected_spectrum = Self::compute_fft_magnitude(&expected_signal);

                // Normalize spectra
                let input_spectrum = Self::normalize_spectrum(&input_spectrum);
                let expected_spectrum = Self::normalize_spectrum(&expected_spectrum);

                // Compute similarity
                let similarity =
                    Self::compute_cosine_similarity(&input_spectrum, &expected_spectrum);

                // Send similarity value
                match_result_sender.send(similarity).ok();
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

    fn normalize_spectrum(spectrum: &[f32]) -> Vec<f32> {
        let max_value = spectrum.iter().cloned().fold(0.0, f32::max);
        if max_value > 0.0 {
            spectrum.iter().map(|&v| v / max_value).collect()
        } else {
            spectrum.to_vec()
        }
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
