// audio_listener.rs

use crate::music_representation::musical_structures::{calculate_frequency, Note};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream};
use rustfft::{num_complex::Complex, FftPlanner};
use std::sync::{mpsc::Sender, Arc, Mutex};

/// Enum representing the available similarity metrics.
#[derive(Clone, Copy)]
pub enum SimilarityMetric {
    Cosine,
    Pearson,
    Euclidean,
    // Add more metrics here as needed
}

impl SimilarityMetric {
    /// Computes the similarity between two feature vectors based on the selected metric.
    fn compute_similarity(&self, a: &[f32], b: &[f32]) -> f32 {
        match self {
            SimilarityMetric::Cosine => compute_cosine_similarity(a, b),
            SimilarityMetric::Pearson => compute_pearson_correlation(a, b),
            SimilarityMetric::Euclidean => compute_euclidean_distance(a, b),
            // Add more metrics here as needed
        }
    }
}

/// Computes Cosine Similarity between two vectors.
fn compute_cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let magnitude_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let magnitude_b: f32 = b.iter().map(|y| y * y).sum::<f32>().sqrt();
    if magnitude_a > 0.0 && magnitude_b > 0.0 {
        dot_product / (magnitude_a * magnitude_b)
    } else {
        0.0
    }
}

/// Computes Pearson Correlation between two vectors.
fn compute_pearson_correlation(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len();
    if n == 0 {
        return 0.0;
    }
    let sum_a: f32 = a.iter().sum();
    let sum_b: f32 = b.iter().sum();
    let sum_a_sq: f32 = a.iter().map(|x| x * x).sum();
    let sum_b_sq: f32 = b.iter().map(|x| x * x).sum();
    let sum_ab: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();

    let numerator = sum_ab - (sum_a * sum_b) / n as f32;
    let denominator =
        ((sum_a_sq - (sum_a * sum_a) / n as f32) * (sum_b_sq - (sum_b * sum_b) / n as f32)).sqrt();

    if denominator != 0.0 {
        numerator / denominator
    } else {
        0.0
    }
}

/// Computes Euclidean Distance between two vectors.
fn compute_euclidean_distance(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y).powi(2))
        .sum::<f32>()
        .sqrt()
}

pub struct AudioListener {
    stream: Option<Stream>,
    match_result_sender: Arc<Sender<f32>>,
    expected_notes: Arc<Mutex<Option<Vec<Note>>>>,
    pub sample_rate: f32,
    input_buffer: Arc<Mutex<Vec<f32>>>,
    // Fields for storing time-domain and feature histories
    pub input_signal_history: Arc<Mutex<Vec<Vec<f32>>>>,
    pub expected_signal_history: Arc<Mutex<Vec<Vec<f32>>>>,
    pub mfcc_history: Arc<Mutex<Vec<Vec<f32>>>>, // Input MFCC features
    pub expected_mfcc_history: Arc<Mutex<Vec<Vec<f32>>>>, // Expected MFCC features
    pub zcr_history: Arc<Mutex<Vec<f32>>>,       // Input ZCR features
    pub expected_zcr_history: Arc<Mutex<Vec<f32>>>, // Expected ZCR features
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
        let mfcc_history = Arc::new(Mutex::new(Vec::new()));
        let expected_mfcc_history = Arc::new(Mutex::new(Vec::new()));
        let zcr_history = Arc::new(Mutex::new(Vec::new()));
        let expected_zcr_history = Arc::new(Mutex::new(Vec::new()));
        let similarity_metric = Arc::new(Mutex::new(initial_metric));

        Self {
            stream: None, // We'll set this in the start method
            match_result_sender: Arc::new(match_result_sender),
            expected_notes,
            sample_rate,
            input_buffer,
            input_signal_history,
            expected_signal_history,
            mfcc_history,
            expected_mfcc_history,
            zcr_history,
            expected_zcr_history,
            similarity_metric, // Initialize similarity metric
        }
    }

    /// Sets a new similarity metric at runtime.
    pub fn set_similarity_metric(&self, new_metric: SimilarityMetric) {
        let mut metric = self.similarity_metric.lock().unwrap();
        *metric = new_metric;
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
        let mfcc_history = Arc::clone(&self.mfcc_history);
        let expected_mfcc_history = Arc::clone(&self.expected_mfcc_history);
        let zcr_history = Arc::clone(&self.zcr_history);
        let expected_zcr_history = Arc::clone(&self.expected_zcr_history);
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
                            &mfcc_history,
                            &zcr_history,
                            &expected_mfcc_history,
                            &expected_zcr_history,
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

    /// Processes incoming audio data, computes MFCC and ZCR, compares with expected notes,
    /// and sends the similarity score.
    fn process_audio_input(
        data: &[f32],
        sample_rate: f32,
        match_result_sender: &Arc<Sender<f32>>,
        expected_notes: &Arc<Mutex<Option<Vec<Note>>>>,
        input_buffer: &Arc<Mutex<Vec<f32>>>,
        input_signal_history: &Arc<Mutex<Vec<Vec<f32>>>>,
        expected_signal_history: &Arc<Mutex<Vec<Vec<f32>>>>,
        mfcc_history: &Arc<Mutex<Vec<Vec<f32>>>>,
        zcr_history: &Arc<Mutex<Vec<f32>>>,
        expected_mfcc_history: &Arc<Mutex<Vec<Vec<f32>>>>,
        expected_zcr_history: &Arc<Mutex<Vec<f32>>>,
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

        // Parameters for MFCC
        const NUM_MEL_FILTERS: usize = 26;
        const NUM_MFCC: usize = 13;

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
                Self::generate_expected_signal(&expected_notes_clone, sample_rate, FRAME_SIZE);
            if let Some(expected_signal) = expected_signal {
                println!(
                    "Raw MIC: {}, Raw notes: {}",
                    input_signal.len(),
                    expected_signal.len()
                );

                // Normalize in time domain
                let normalized_input_signal = normalize_signal(&input_signal);
                let normalized_expected_signal = normalize_signal(&expected_signal);

                // Compute MFCC features
                let input_mfcc = Self::compute_mfcc(
                    &normalized_input_signal,
                    sample_rate,
                    NUM_MEL_FILTERS,
                    NUM_MFCC,
                );
                let expected_mfcc = Self::compute_mfcc(
                    &normalized_expected_signal,
                    sample_rate,
                    NUM_MEL_FILTERS,
                    NUM_MFCC,
                );

                // Compute ZCR features
                let input_zcr = Self::compute_zcr(&normalized_input_signal);
                let expected_zcr = Self::compute_zcr(&normalized_expected_signal);

                // Combine MFCC and ZCR features
                let combined_input_features = [input_mfcc.clone(), vec![input_zcr]].concat();
                let combined_expected_features =
                    [expected_mfcc.clone(), vec![expected_zcr]].concat();

                // Acquire the current similarity metric
                let metric = {
                    let metric_lock = similarity_metric.lock().unwrap();
                    *metric_lock
                };

                // Compute similarity using the selected metric
                let similarity = metric
                    .compute_similarity(&combined_input_features, &combined_expected_features);

                // Send similarity value
                match_result_sender.send(similarity).ok();

                // Store time-domain signals and features in Mutex Vecs sent to GUI
                {
                    let mut input_signal_hist = input_signal_history.lock().unwrap();
                    let mut expected_signal_hist = expected_signal_history.lock().unwrap();
                    let mut input_mfcc_hist = mfcc_history.lock().unwrap();
                    let mut expected_mfcc_hist = expected_mfcc_history.lock().unwrap();
                    let mut input_zcr_hist = zcr_history.lock().unwrap();
                    let mut expected_zcr_hist = expected_zcr_history.lock().unwrap();

                    input_signal_hist.push(normalized_input_signal.clone());
                    expected_signal_hist.push(normalized_expected_signal.clone());

                    // Store features
                    input_mfcc_hist.push(input_mfcc.clone());
                    expected_mfcc_hist.push(expected_mfcc.clone());

                    input_zcr_hist.push(input_zcr);
                    expected_zcr_hist.push(expected_zcr);

                    // Limit history size
                    const MAX_HISTORY_LENGTH: usize = 100;
                    if input_signal_hist.len() > MAX_HISTORY_LENGTH {
                        input_signal_hist.remove(0);
                        expected_signal_hist.remove(0);
                        input_mfcc_hist.remove(0);
                        expected_mfcc_hist.remove(0);
                        input_zcr_hist.remove(0);
                        expected_zcr_hist.remove(0);
                    }
                }
            } else {
                // No expected signal to compare
                match_result_sender.send(0.0).ok();
            }
        }
    }

    /// Computes MFCC features for a given audio frame.
    fn compute_mfcc(
        signal: &[f32],
        sample_rate: f32,
        num_filters: usize,
        num_coefficients: usize,
    ) -> Vec<f32> {
        // Step 1: Pre-Emphasis (optional)
        let pre_emphasized: Vec<f32> = signal
            .iter()
            .enumerate()
            .map(|(i, &x)| if i == 0 { x } else { x - 0.97 * signal[i - 1] })
            .collect();

        // Step 2: Windowing (Hamming window)
        let windowed: Vec<f32> = pre_emphasized
            .iter()
            .enumerate()
            .map(|(i, &x)| {
                x * (0.54
                    - 0.46
                        * (2.0 * std::f32::consts::PI * i as f32 / (signal.len() as f32 - 1.0))
                            .cos())
            })
            .collect();

        // Step 3: FFT
        let fft_size = signal.len();
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(fft_size);
        let mut buffer: Vec<Complex<f32>> = windowed
            .iter()
            .map(|&s| Complex { re: s, im: 0.0 })
            .collect();
        fft.process(&mut buffer);

        // Step 4: Power Spectrum
        let power_spectrum: Vec<f32> = buffer
            .iter()
            .take(fft_size / 2 + 1)
            .map(|c| c.norm_sqr())
            .collect();

        // Step 5: Mel Filterbank
        let filterbank = Self::create_mel_filterbank(
            num_filters,
            fft_size,
            sample_rate,
            300.0,             // Lower frequency bound
            sample_rate / 2.0, // Upper frequency bound (Nyquist)
        );

        // Step 6: Apply Mel filters to power spectrum
        let mut mel_energies = Vec::with_capacity(num_filters);
        for filter in &filterbank {
            let energy: f32 = filter.iter().zip(&power_spectrum).map(|(f, p)| f * p).sum();
            // Avoid log(0) by replacing zero energies with a small value
            mel_energies.push(energy.max(1e-10).ln());
        }

        // Step 7: DCT-II to get MFCCs
        let mfcc = Self::dct(&mel_energies);

        // Step 8: Keep the first 'num_coefficients' MFCCs
        mfcc.into_iter().take(num_coefficients).collect()
    }

    /// Computes Zero-Crossing Rate for a given audio frame.
    fn compute_zcr(signal: &[f32]) -> f32 {
        signal.windows(2).filter(|w| w[0] * w[1] < 0.0).count() as f32 / (signal.len() - 1) as f32
    }

    /// Converts frequency (Hz) to Mel scale.
    fn hz_to_mel(hz: f32) -> f32 {
        2595.0 * (1.0 + hz / 700.0).log10()
    }

    /// Converts Mel scale to frequency (Hz).
    fn mel_to_hz(mel: f32) -> f32 {
        700.0 * (10_f32.powf(mel / 2595.0) - 1.0)
    }

    /// Creates Mel filterbank.
    fn create_mel_filterbank(
        num_filters: usize,
        fft_size: usize,
        sample_rate: f32,
        min_freq: f32,
        max_freq: f32,
    ) -> Vec<Vec<f32>> {
        let min_mel = Self::hz_to_mel(min_freq);
        let max_mel = Self::hz_to_mel(max_freq);
        let mel_points: Vec<f32> = (0..=num_filters + 2)
            .map(|i| min_mel + (max_mel - min_mel) * (i as f32) / ((num_filters + 1) as f32))
            .collect();
        let hz_points: Vec<f32> = mel_points.iter().map(|&m| Self::mel_to_hz(m)).collect();
        let bin_points: Vec<usize> = hz_points
            .iter()
            .map(|&hz| ((fft_size + 1) as f32 * hz / sample_rate).round() as usize)
            .collect();

        let mut filterbank = Vec::with_capacity(num_filters);

        for i in 1..=num_filters {
            let mut filter = vec![0.0; fft_size / 2 + 1];
            let start = bin_points[i - 1];
            let center = bin_points[i];
            let end = bin_points[i + 1];

            // Rising slope
            for j in start..center {
                filter[j] = (j as f32 - bin_points[i - 1] as f32)
                    / (bin_points[i] - bin_points[i - 1]) as f32;
            }

            // Falling slope
            for j in center..end {
                filter[j] = (bin_points[i + 1] as f32 - j as f32)
                    / (bin_points[i + 1] - bin_points[i]) as f32;
            }

            filterbank.push(filter);
        }

        filterbank
    }

    /// Computes the Discrete Cosine Transform (DCT-II) of a vector.
    fn dct(vector: &[f32]) -> Vec<f32> {
        let n = vector.len();
        let mut result = Vec::with_capacity(n);
        for k in 0..n {
            let mut sum = 0.0;
            for i in 0..n {
                sum += vector[i]
                    * (std::f32::consts::PI * k as f32 * (2.0 * i as f32 + 1.0) / (2.0 * n as f32))
                        .cos();
            }
            // Scale factor for orthonormal DCT
            let scale = if k == 0 {
                (1.0 / n as f32).sqrt()
            } else {
                (2.0 / n as f32).sqrt()
            };
            result.push(sum * scale);
        }
        result
    }

    /// Computes cosine similarity between two vectors.
    fn compute_cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let magnitude_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let magnitude_b: f32 = b.iter().map(|y| y * y).sum::<f32>().sqrt();
        if magnitude_a > 0.0 && magnitude_b > 0.0 {
            dot_product / (magnitude_a * magnitude_b)
        } else {
            0.0
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

/// Normalizes the amplitude of a signal to the range [-1, 1].
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
