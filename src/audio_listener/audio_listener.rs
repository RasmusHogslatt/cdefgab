use crate::music_representation::musical_structures::{calculate_frequency, Note};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream};
use std::sync::{mpsc::Sender, Arc, Mutex};

/// Enum representing the available similarity metrics.
/// You can keep this if you plan to use different metrics in the future.
/// Alternatively, it can be removed if not needed.
#[derive(Clone, Copy)]
pub enum SimilarityMetric {
    Placeholder, // Placeholder metric
}

impl SimilarityMetric {
    /// Computes the similarity between two feature vectors based on the selected metric.
    /// Currently, it returns a constant value as a placeholder.
    fn compute_similarity(&self, _a: &[f32], _b: &[f32]) -> f32 {
        match self {
            SimilarityMetric::Placeholder => 1.0, // Always returns maximum similarity
        }
    }
}

pub struct AudioListener {
    stream: Option<Stream>,
    match_result_sender: Arc<Sender<f32>>,
    expected_notes: Arc<Mutex<Option<Vec<Note>>>>,
    pub sample_rate: f32,
    input_buffer: Arc<Mutex<Vec<f32>>>,
    // Fields for storing time-domain signal histories
    pub input_signal_history: Arc<Mutex<Vec<Vec<f32>>>>,
    pub expected_signal_history: Arc<Mutex<Vec<Vec<f32>>>>,
    pub similarity_metric: Arc<Mutex<SimilarityMetric>>, // Current similarity metric
}

impl AudioListener {
    pub fn new(
        match_result_sender: Sender<f32>,
        expected_notes: Arc<Mutex<Option<Vec<Note>>>>,
        initial_metric: SimilarityMetric, // New parameter for initial similarity metric
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
        let similarity_metric = Arc::new(Mutex::new(initial_metric));

        Self {
            stream: None, // We'll set this in the start method
            match_result_sender: Arc::new(match_result_sender),
            expected_notes,
            sample_rate,
            input_buffer,
            input_signal_history,
            expected_signal_history,
            similarity_metric, // Initialize similarity metric
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
        let similarity_metric = Arc::clone(&self.similarity_metric);

        let stream = match config.sample_format() {
            SampleFormat::F32 => device
                .build_input_stream(
                    &config.into(),
                    move |data: &[f32], _| {
                        // Call the processing function with new parameters
                        Self::process_audio_input(
                            data,
                            sample_rate,
                            &match_result_sender,
                            &expected_notes,
                            &input_buffer,
                            &input_signal_history,
                            &expected_signal_history,
                            &similarity_metric,
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

    /// Processes incoming audio data and sends a placeholder similarity score.
    fn process_audio_input(
        data: &[f32],
        _sample_rate: f32,
        match_result_sender: &Arc<Sender<f32>>,
        expected_notes: &Arc<Mutex<Option<Vec<Note>>>>,
        input_buffer: &Arc<Mutex<Vec<f32>>>,
        input_signal_history: &Arc<Mutex<Vec<Vec<f32>>>>,
        expected_signal_history: &Arc<Mutex<Vec<Vec<f32>>>>,
        similarity_metric: &Arc<Mutex<SimilarityMetric>>, // New parameter
    ) {
        // Append incoming data to the input buffer
        {
            let mut buffer = input_buffer.lock().unwrap();
            buffer.extend_from_slice(data);
        }

        // Define frame and hop sizes
        const FRAME_SIZE: usize = 4096; // e.g., 4096 samples (~0.09 seconds at 44.1kHz)
        const HOP_SIZE: usize = 512; // Overlap of FRAME_SIZE - HOP_SIZE

        loop {
            let mut buffer = input_buffer.lock().unwrap();

            if buffer.len() < FRAME_SIZE {
                break;
            }

            // Extract the current frame
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

            // Generate expected signal
            let expected_signal =
                Self::generate_expected_signal(&expected_notes_clone, _sample_rate, FRAME_SIZE);
            if let Some(expected_signal) = expected_signal {
                println!(
                    "Raw MIC: {}, Raw notes: {}",
                    input_signal.len(),
                    expected_signal.len()
                );

                // Store time-domain signals in Mutex Vecs sent to GUI
                {
                    let mut input_signal_hist = input_signal_history.lock().unwrap();
                    let mut expected_signal_hist = expected_signal_history.lock().unwrap();

                    input_signal_hist.push(input_signal.clone());
                    expected_signal_hist.push(expected_signal.clone());

                    // Limit history size
                    const MAX_HISTORY_LENGTH: usize = 100;
                    if input_signal_hist.len() > MAX_HISTORY_LENGTH {
                        input_signal_hist.remove(0);
                        expected_signal_hist.remove(0);
                    }
                }

                // Acquire the current similarity metric
                let metric = {
                    let metric_lock = similarity_metric.lock().unwrap();
                    *metric_lock
                };

                // Placeholder similarity computation
                let similarity = metric.compute_similarity(&input_signal, &expected_signal);

                // Send similarity value
                match_result_sender.send(similarity).ok();
            } else {
                // No expected signal to compare
                match_result_sender.send(0.0).ok();
            }
        }
    }

    /// Generates the expected audio signal based on predefined notes.
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
}
