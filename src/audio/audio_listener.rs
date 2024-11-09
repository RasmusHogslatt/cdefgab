// audio/audio_listener.rs

use crate::music_representation::{calculate_frequency, Note};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream};
use rustfft::num_complex::Complex;
use rustfft::FftPlanner;

use std::collections::HashSet;
use std::sync::{mpsc::Sender, Arc, Mutex};

// Number of chroma bins
const CHROMA_BINS: usize = 12;

// Threshold for silence detection
const SILENCE_THRESHOLD: f32 = 0.01; // Adjust as needed

pub struct AudioListener {
    pub stream: Option<Stream>,
    pub match_result_sender: Arc<Sender<bool>>,
    pub expected_notes: Arc<Mutex<Option<Vec<Note>>>>,
    pub sample_rate: f32,
    pub input_buffer: Arc<Mutex<Vec<f32>>>,
    pub input_chroma_history: Arc<Mutex<Vec<Vec<f32>>>>,
    pub expected_chroma_history: Arc<Mutex<Vec<Vec<f32>>>>,
    pub input_signal_history: Arc<Mutex<Vec<Vec<f32>>>>,
}

impl AudioListener {
    pub fn new(
        match_result_sender: Sender<bool>,
        expected_notes: Arc<Mutex<Option<Vec<Note>>>>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // Initialize the sample rate
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or("No input device available")?;
        let config = device.default_input_config()?;
        let sample_rate = config.sample_rate().0 as f32;

        // Initialize the input buffer
        let input_buffer = Arc::new(Mutex::new(Vec::new()));

        // Initialize chroma feature histories
        let input_chroma_history = Arc::new(Mutex::new(Vec::new()));
        let expected_chroma_history = Arc::new(Mutex::new(Vec::new()));

        // Initialize raw signal histories
        let input_signal_history = Arc::new(Mutex::new(Vec::new()));

        Ok(Self {
            stream: None,
            match_result_sender: Arc::new(match_result_sender),
            expected_notes,
            sample_rate,
            input_buffer,
            input_chroma_history,
            expected_chroma_history,
            input_signal_history,
        })
    }

    pub fn start(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or("No input device available")?;
        let config = device.default_input_config()?;

        // Clone fields to move into the closure
        let sample_rate = self.sample_rate;
        let match_result_sender = Arc::clone(&self.match_result_sender);
        let expected_notes = Arc::clone(&self.expected_notes);
        let input_buffer = Arc::clone(&self.input_buffer);
        let input_chroma_history = Arc::clone(&self.input_chroma_history);
        let expected_chroma_history = Arc::clone(&self.expected_chroma_history);
        let input_signal_history = Arc::clone(&self.input_signal_history);

        let stream = match config.sample_format() {
            SampleFormat::F32 => device.build_input_stream(
                &config.into(),
                move |data: &[f32], _| {
                    if let Err(e) = process_audio_input(
                        data,
                        sample_rate,
                        &match_result_sender,
                        &expected_notes,
                        &input_buffer,
                        &input_chroma_history,
                        &expected_chroma_history,
                        &input_signal_history,
                    ) {
                        eprintln!("Error processing audio input: {}", e);
                    }
                },
                |err| eprintln!("Stream error: {}", err),
                None,
            )?,
            _ => return Err("Unsupported sample format".into()),
        };

        self.stream = Some(stream);

        if let Some(ref stream) = self.stream {
            stream.play()?;
        }

        Ok(())
    }
}

fn process_audio_input(
    data: &[f32],
    sample_rate: f32,
    match_result_sender: &Arc<Sender<bool>>,
    expected_notes: &Arc<Mutex<Option<Vec<Note>>>>,
    input_buffer: &Arc<Mutex<Vec<f32>>>,
    input_chroma_history: &Arc<Mutex<Vec<Vec<f32>>>>,
    expected_chroma_history: &Arc<Mutex<Vec<Vec<f32>>>>,
    input_signal_history: &Arc<Mutex<Vec<Vec<f32>>>>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Append incoming data to the input buffer
    {
        let mut buffer = input_buffer.lock().unwrap();
        buffer.extend_from_slice(data);
    }

    // Define frame and hop sizes
    const FRAME_SIZE: usize = 4096;
    const HOP_SIZE: usize = 1024;

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

        // Generate expected chroma features directly from the notes
        let expected_chroma = generate_expected_chroma(&expected_notes_clone);

        // Normalize input signal per frame
        let normalized_input_signal = normalize_signal_per_frame(&input_signal);

        // Compute RMS energy of the normalized input signal
        let rms_energy = normalized_input_signal.iter().map(|x| x * x).sum::<f32>()
            / normalized_input_signal.len() as f32;

        if rms_energy < SILENCE_THRESHOLD {
            // Skip processing this frame
            continue;
        }

        // Extract chroma features from the input signal
        let input_chroma = compute_chroma_features(&normalized_input_signal, sample_rate);

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

            input_signal_hist.push(normalized_input_signal.clone());

            // Limit history size
            const MAX_SIGNAL_HISTORY: usize = 100;
            if input_signal_hist.len() > MAX_SIGNAL_HISTORY {
                input_signal_hist.remove(0);
            }
        }

        // Perform peak detection and comparison
        let match_result = compare_chroma_peaks(&input_chroma, &expected_chroma);

        // Send match result
        match_result_sender.send(match_result).ok();
    }

    Ok(())
}

/// Generates expected chroma features directly from the expected notes.
fn generate_expected_chroma(expected_notes: &Option<Vec<Note>>) -> Vec<f32> {
    let mut chroma = vec![0.0; CHROMA_BINS];
    if let Some(notes) = expected_notes {
        for note in notes {
            if let (Some(_string), Some(_fret)) = (note.string, note.fret) {
                // Don't hardcode scale length
                let frequency = calculate_frequency(note, 25.5, 0);
                let midi = freq_to_midi(frequency);
                let pitch_class = (midi % 12) as usize;
                chroma[pitch_class] += 1.0;
            }
        }
    }
    // Normalize the chroma vector
    let sum: f32 = chroma.iter().sum();
    if sum > 0.0 {
        chroma.iter().map(|&c| c / sum).collect()
    } else {
        chroma
    }
}
/// Compares the peaks in the input chroma to the expected chroma peaks.
/// Returns true if all expected peaks are found in the input chroma.
fn compare_chroma_peaks(input_chroma: &[f32], expected_chroma: &[f32]) -> bool {
    // Identify peaks in the expected chroma
    let expected_peaks = identify_peaks_expected(expected_chroma);
    let num_expected_peaks = expected_peaks.len();

    // Identify peaks in the input chroma
    let input_peaks = identify_peaks_input(input_chroma, num_expected_peaks);

    // Check if all expected peaks are present in the input peaks
    expected_peaks.is_subset(&input_peaks)
}

/// Identifies peaks in the expected chroma vector.
/// Returns a set of pitch class indices corresponding to the peaks.
fn identify_peaks_expected(chroma: &[f32]) -> HashSet<usize> {
    chroma
        .iter()
        .enumerate()
        .filter_map(|(i, &value)| if value > 0.0 { Some(i) } else { None })
        .collect()
}
/// Identifies the top N peaks in the input chroma vector.
/// Returns a set of pitch class indices corresponding to the peaks.
fn identify_peaks_input(chroma: &[f32], num_peaks: usize) -> HashSet<usize> {
    // Collect indices and values, dereferencing the &f32 to f32
    let mut indices_and_values: Vec<(usize, f32)> = chroma
        .iter()
        .enumerate()
        .map(|(i, &value)| (i, value)) // Dereference &f32 to f32
        .collect();

    // Sort by value descending
    indices_and_values.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    // Take the top num_peaks indices
    indices_and_values
        .iter()
        .take(num_peaks)
        .map(|&(i, _)| i)
        .collect()
}
/// Computes chroma features for a given audio frame.
fn compute_chroma_features(signal: &[f32], sample_rate: f32) -> Vec<f32> {
    let fft_size = signal.len();
    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(fft_size);
    let mut buffer: Vec<Complex<f32>> =
        signal.iter().map(|&s| Complex { re: s, im: 0.0 }).collect();

    // Apply pre-emphasis filter to boost high frequencies
    let pre_emphasis = 0.97;
    for i in (1..buffer.len()).rev() {
        buffer[i].re = buffer[i].re - pre_emphasis * buffer[i - 1].re;
    }

    fft.process(&mut buffer);

    // Compute magnitude spectrum
    let magnitude_spectrum: Vec<f32> = buffer
        .iter()
        .take(fft_size / 2 + 1)
        .map(|c| c.norm())
        .collect();

    // Apply logarithmic scaling to compress dynamic range
    let magnitude_spectrum: Vec<f32> = magnitude_spectrum
        .iter()
        .map(|&mag| (1.0 + mag).ln())
        .collect();

    // Initialize chroma vector
    let mut chroma = vec![0.0; CHROMA_BINS];

    // Frequency resolution
    let freq_res = sample_rate / fft_size as f32;

    for (i, &mag) in magnitude_spectrum.iter().enumerate() {
        let freq = i as f32 * freq_res;
        if freq < 82.0 || freq > 1000.0 {
            continue; // Adjusted frequency range for guitar
        }
        let midi = freq_to_midi(freq);
        let pitch_class = (midi % 12) as usize;
        if pitch_class < CHROMA_BINS {
            chroma[pitch_class] += mag;
        }
    }

    // Normalize chroma vector
    let sum: f32 = chroma.iter().sum();
    let chroma_normalized = if sum > 0.0 {
        chroma.iter().map(|&c| c / sum).collect()
    } else {
        chroma.clone()
    };

    // Apply smoothing to chroma vector
    let chroma_smoothed = smooth_chroma(&chroma_normalized);

    chroma_smoothed
}
/// Smooths a chroma vector by averaging each bin with its neighbors.
fn smooth_chroma(chroma: &[f32]) -> Vec<f32> {
    let mut smoothed = vec![0.0; chroma.len()];
    let len = chroma.len();
    for i in 0..len {
        let prev = chroma[(i + len - 1) % len];
        let curr = chroma[i];
        let next = chroma[(i + 1) % len];
        smoothed[i] = (prev + curr + next) / 3.0;
    }
    smoothed
}

/// Converts frequency (Hz) to MIDI note number.
fn freq_to_midi(freq: f32) -> u8 {
    (69.0 + 12.0 * (freq / 440.0).log2()).round() as u8
}

/// Normalizes a signal per frame based on its own maximum amplitude.
fn normalize_signal_per_frame(signal: &[f32]) -> Vec<f32> {
    let max_amplitude = signal.iter().map(|x| x.abs()).fold(0.0, f32::max);
    if max_amplitude == 0.0 {
        vec![0.0; signal.len()]
    } else {
        signal.iter().map(|&x| x / max_amplitude).collect()
    }
}
