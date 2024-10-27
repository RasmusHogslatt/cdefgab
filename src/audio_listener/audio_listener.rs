// audio_listener.rs

use crate::audio_player::audio_player::KarplusStrong;
use crate::music_representation::musical_structures::{calculate_frequency, Note};
use augurs_dtw::Dtw;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream};
use rustfft::{num_complex::Complex, FftPlanner};
use std::sync::{mpsc::Sender, Arc, Mutex};

// Number of chroma bins
const CHROMA_BINS: usize = 12;

// Enum representing the available similarity metrics.
#[derive(Clone, Copy)]
pub enum SimilarityMetric {
    DTW,
    // Future metrics can be added here
}

enum DistanceMetric {
    Euclidean,
    Manhattan,
}

impl SimilarityMetric {
    /// Computes the similarity between two feature sequences based on the selected metric.
    fn compute_similarity(
        &self,
        a: &[Vec<f32>],
        b: &[Vec<f32>],
        distance_metric: DistanceMetric,
        sample_rate: f32,
    ) -> f32 {
        match self {
            SimilarityMetric::DTW => compute_dtw_similarity(a, b, &distance_metric),
            // Add more metrics here as needed
        }
    }
}

/// Computes DTW-based similarity between two chroma feature sequences using the `augurs_dtw` crate.
/// This function assumes that both `a` and `b` are sequences of chroma vectors (Vec<f32>).
fn compute_dtw_similarity(a: &[Vec<f32>], b: &[Vec<f32>], distance_metric: &DistanceMetric) -> f32 {
    // Ensure both sequences are non-empty
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }

    let a_flat: Vec<f32> = a.iter().flat_map(|v| v.iter().cloned()).collect();
    let b_flat: Vec<f32> = b.iter().flat_map(|v| v.iter().cloned()).collect();

    let a_flat_f64: Vec<f64> = a_flat.iter().map(|&x| x as f64).collect();
    let b_flat_f64: Vec<f64> = b_flat.iter().map(|&x| x as f64).collect();
    let distance = match distance_metric {
        DistanceMetric::Euclidean => Dtw::euclidean().distance(&a_flat_f64, &b_flat_f64),
        DistanceMetric::Manhattan => Dtw::manhattan().distance(&a_flat_f64, &b_flat_f64),
    };
    // let distance = Dtw::euclidean().distance(&a_flat_f64, &b_flat_f64);

    // Convert distance to similarity score (higher is better)
    // You may adjust the scaling based on observed distance ranges
    if distance == 0.0 {
        1.0
    } else {
        1.0 / distance as f32
    }
}
//  ÄR DENNA SKALNING RÄTT?

/// Computes chroma features for a given audio frame.
fn compute_chroma_features(signal: &[f32], sample_rate: f32) -> Vec<f32> {
    let fft_size = signal.len();
    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(fft_size);
    let mut buffer: Vec<Complex<f32>> =
        signal.iter().map(|&s| Complex { re: s, im: 0.0 }).collect();
    fft.process(&mut buffer);

    // Compute magnitude spectrum
    let magnitude_spectrum: Vec<f32> = buffer
        .iter()
        .take(fft_size / 2 + 1)
        .map(|c| c.norm())
        .collect();

    // Initialize chroma vector
    let mut chroma = vec![0.0; CHROMA_BINS];

    // Frequency resolution
    let freq_res = sample_rate / fft_size as f32;

    for (i, &mag) in magnitude_spectrum.iter().enumerate() {
        let freq = i as f32 * freq_res;
        if freq < 20.0 || freq > 5000.0 {
            continue; // Ignore frequencies outside typical guitar range
        }
        let midi = freq_to_midi(freq);
        let pitch_class = (midi % 12) as usize;
        if pitch_class < CHROMA_BINS {
            chroma[pitch_class] += mag;
        }
    }

    // Normalize chroma vector
    let sum: f32 = chroma.iter().sum();
    if sum > 0.0 {
        chroma.iter().map(|&c| c / sum).collect()
    } else {
        chroma
    }
}

/// Converts frequency (Hz) to MIDI note number.
fn freq_to_midi(freq: f32) -> u8 {
    (69.0 + 12.0 * (freq / 440.0).log2()).round() as u8
}

pub struct AudioListener {
    stream: Option<Stream>,
    match_result_sender: Arc<Sender<f32>>,
    expected_notes: Arc<Mutex<Option<Vec<Note>>>>,
    pub sample_rate: f32,
    input_buffer: Arc<Mutex<Vec<f32>>>,
    // Fields for storing chroma feature histories
    pub input_chroma_history: Arc<Mutex<Vec<Vec<f32>>>>,
    pub expected_chroma_history: Arc<Mutex<Vec<Vec<f32>>>>,
    // Fields for storing raw signal histories
    pub input_signal_history: Arc<Mutex<Vec<Vec<f32>>>>,
    pub expected_signal_history: Arc<Mutex<Vec<Vec<f32>>>>,
    pub similarity_metric: Arc<Mutex<SimilarityMetric>>, // Current similarity metric
    // Flag to ensure similarity is computed only once per set of notes
    pub similarity_computed: Arc<Mutex<bool>>,
    pub expected_active_notes: Arc<Mutex<Vec<KarplusStrong>>>,
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

        // Initialize chroma feature histories
        let input_chroma_history = Arc::new(Mutex::new(Vec::new()));
        let expected_chroma_history = Arc::new(Mutex::new(Vec::new()));

        // Initialize raw signal histories
        let input_signal_history = Arc::new(Mutex::new(Vec::new()));
        let expected_signal_history = Arc::new(Mutex::new(Vec::new()));

        let similarity_metric = Arc::new(Mutex::new(initial_metric));

        let similarity_computed = Arc::new(Mutex::new(true)); // Initially true
        let expected_active_notes = Arc::new(Mutex::new(Vec::new()));

        Self {
            stream: None,
            match_result_sender: Arc::new(match_result_sender),
            expected_notes,
            sample_rate,
            input_buffer,
            input_chroma_history,
            expected_chroma_history,
            input_signal_history,
            expected_signal_history,
            similarity_metric,
            similarity_computed,
            expected_active_notes,
        }
    }

    /// Sets a new decay parameter for all active expected notes.
    pub fn set_decay(&self, new_decay: f32) {
        let mut active_notes = self.expected_active_notes.lock().unwrap();
        for ks in active_notes.iter_mut() {
            ks.decay = new_decay;
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
        let input_chroma_history = Arc::clone(&self.input_chroma_history);
        let expected_chroma_history = Arc::clone(&self.expected_chroma_history);
        let input_signal_history = Arc::clone(&self.input_signal_history);
        let expected_signal_history = Arc::clone(&self.expected_signal_history);
        let similarity_metric = Arc::clone(&self.similarity_metric);
        let similarity_computed = Arc::clone(&self.similarity_computed);
        let expected_active_notes = Arc::clone(&self.expected_active_notes);

        let stream = match config.sample_format() {
            SampleFormat::F32 => device
                .build_input_stream(
                    &config.into(),
                    move |data: &[f32], _| {
                        process_audio_input(
                            data,
                            sample_rate,
                            &match_result_sender,
                            &expected_notes,
                            &input_buffer,
                            &input_chroma_history,
                            &expected_chroma_history,
                            &input_signal_history,
                            &expected_signal_history,
                            &similarity_metric,
                            &similarity_computed,
                            &expected_active_notes,
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
}

fn process_audio_input(
    data: &[f32],
    sample_rate: f32,
    match_result_sender: &Arc<Sender<f32>>,
    expected_notes: &Arc<Mutex<Option<Vec<Note>>>>,
    input_buffer: &Arc<Mutex<Vec<f32>>>,
    input_chroma_history: &Arc<Mutex<Vec<Vec<f32>>>>,
    expected_chroma_history: &Arc<Mutex<Vec<Vec<f32>>>>,
    input_signal_history: &Arc<Mutex<Vec<Vec<f32>>>>,
    expected_signal_history: &Arc<Mutex<Vec<Vec<f32>>>>,
    similarity_metric: &Arc<Mutex<SimilarityMetric>>,
    similarity_computed: &Arc<Mutex<bool>>,
    expected_active_notes: &Arc<Mutex<Vec<KarplusStrong>>>,
) {
    // Append incoming data to the input buffer
    {
        let mut buffer = input_buffer.lock().unwrap();
        buffer.extend_from_slice(data);
    }

    // Define frame and hop sizes
    const FRAME_SIZE: usize = 2048 * 2; // Adjusted for better performance
    const HOP_SIZE: usize = 512 * 2; // Overlap of FRAME_SIZE - HOP_SIZE

    loop {
        let mut buffer = input_buffer.lock().unwrap();

        if buffer.len() < FRAME_SIZE {
            break;
        }

        // Extract the current frame
        let input_signal = buffer[..FRAME_SIZE].to_vec();
        // Normalize the input_signal
        let normalized_input_signal = normalize_signal(&input_signal);

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

        // Generate expected signal using Karplus-Strong
        let expected_signal = generate_expected_signal(
            &expected_notes_clone,
            sample_rate,
            FRAME_SIZE,
            &expected_active_notes,
        );
        if let Some(expected_signal) = expected_signal {
            // Normalize the expected_signal
            let normalized_expected_signal = normalize_signal(&expected_signal);

            println!(
                "Raw MIC: {}, Raw notes: {}",
                input_signal.len(),
                expected_signal.len()
            );

            // Extract chroma features
            let input_chroma = compute_chroma_features(&normalized_input_signal, sample_rate);
            let expected_chroma = compute_chroma_features(&normalized_expected_signal, sample_rate);

            // Store chroma features in Mutex Vecs sent to GUI
            {
                let mut input_chroma_hist = input_chroma_history.lock().unwrap();
                let mut expected_chroma_hist = expected_chroma_history.lock().unwrap();

                input_chroma_hist.push(input_chroma.clone());
                expected_chroma_hist.push(expected_chroma.clone());

                // Limit history size
                const MAX_CHROMA_HISTORY: usize = 100;
                if input_chroma_hist.len() > MAX_CHROMA_HISTORY {
                    input_chroma_hist.remove(0);
                    expected_chroma_hist.remove(0);
                }
            }

            // Store raw signals for time-domain plots
            {
                let mut input_signal_hist = input_signal_history.lock().unwrap();
                let mut expected_signal_hist = expected_signal_history.lock().unwrap();

                input_signal_hist.push(normalized_input_signal.clone());
                expected_signal_hist.push(normalized_expected_signal.clone());

                // Limit history size
                const MAX_SIGNAL_HISTORY: usize = 100;
                if input_signal_hist.len() > MAX_SIGNAL_HISTORY {
                    input_signal_hist.remove(0);
                    expected_signal_hist.remove(0);
                }
            }

            // Acquire the current similarity metric and check if similarity has been computed
            let (metric, mut computed) = {
                let metric_lock = similarity_metric.lock().unwrap();
                let computed_lock = similarity_computed.lock().unwrap();
                (*metric_lock, *computed_lock)
            };

            if computed {
                // Similarity already computed for the current set of notes
                continue;
            }

            // Collect chroma feature sequences
            let input_chroma_sequence = {
                let input_chroma_hist = input_chroma_history.lock().unwrap();
                input_chroma_hist.clone()
            };
            let expected_chroma_sequence = {
                let expected_chroma_hist = expected_chroma_history.lock().unwrap();
                expected_chroma_hist.clone()
            };

            if input_chroma_sequence.is_empty() || expected_chroma_sequence.is_empty() {
                // Not enough data to compare
                match_result_sender.send(0.0).ok();
                continue;
            }

            // Compute similarity using the selected metric
            let similarity = metric.compute_similarity(
                &input_chroma_sequence,
                &expected_chroma_sequence,
                DistanceMetric::Euclidean,
                sample_rate,
            );

            // Send similarity value
            match_result_sender.send(similarity).ok();

            // Set similarity_computed to true to prevent further computations until new notes are received
            {
                let mut computed_lock = similarity_computed.lock().unwrap();
                *computed_lock = true;
            }
        } else {
            // No expected signal to compare
            match_result_sender.send(0.0).ok();
        }
    }
}

/// Generates the expected signal using the Karplus-Strong algorithm to match the audio_player's signal.
fn generate_expected_signal(
    expected_notes: &Option<Vec<Note>>,
    sample_rate: f32,
    num_samples: usize,
    expected_active_notes: &Arc<Mutex<Vec<KarplusStrong>>>,
) -> Option<Vec<f32>> {
    if let Some(notes) = expected_notes {
        let mut signal = vec![0.0; num_samples];
        let mut active_notes = expected_active_notes.lock().unwrap();

        // Add new expected notes as KarplusStrong instances
        for note in notes {
            if let (Some(string), Some(fret)) = (note.string, note.fret) {
                let frequency = calculate_frequency(string, fret);
                let duration_seconds = 0.5; // Must match audio_player's duration
                let decay = 0.996; // Must match audio_player's decay

                // Create a new KarplusStrong instance
                let ks = KarplusStrong::new(frequency, duration_seconds, sample_rate, decay);
                active_notes.push(ks);
            }
        }

        // Generate samples by summing all active KarplusStrong instances
        for i in 0..num_samples {
            let mut sample = 0.0;

            // Retain only active notes and sum their samples
            active_notes.retain_mut(|ks| {
                if let Some(s) = ks.next_sample() {
                    sample += s;
                    true // Keep the note active
                } else {
                    false // Remove the note if it's done
                }
            });

            signal[i] = sample;
        }

        Some(signal)
    } else {
        None
    }
}

/// Normalizes a signal to the range [-1.0, 1.0]
fn normalize_signal(signal: &[f32]) -> Vec<f32> {
    let max_val = signal.iter().cloned().fold(f32::MIN, f32::max);
    let min_val = signal.iter().cloned().fold(f32::MAX, f32::min);
    let range = max_val - min_val;

    if range == 0.0 {
        vec![0.0; signal.len()]
    } else {
        signal
            .iter()
            .map(|&x| (x - min_val) / range * 2.0 - 1.0)
            .collect()
    }
}
