// audio/audio_listener.rs

use crate::music_representation::{calculate_frequency, Note};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream};
use rustfft::num_complex::Complex;
use rustfft::FftPlanner;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

const CHROMA_BINS: usize = 12;

pub struct AudioListener {
    pub stream: Option<Stream>,
    pub match_result_sender: Arc<Sender<bool>>,
    pub expected_notes: Arc<Mutex<Option<Vec<Note>>>>,
    pub sample_rate: f32,
    pub input_buffer: Arc<Mutex<Vec<f32>>>,
    pub input_chroma_history: Arc<Mutex<Vec<Vec<f32>>>>,
    pub expected_chroma_history: Arc<Mutex<Vec<Vec<f32>>>>,
    pub input_signal_history: Arc<Mutex<Vec<Vec<f32>>>>,
    pub matching_threshold: Arc<Mutex<f32>>,
    pub silence_threshold: Arc<Mutex<f32>>,
}

impl AudioListener {
    pub fn new(
        match_result_sender: Sender<bool>,
        expected_notes: Arc<Mutex<Option<Vec<Note>>>>,
        matching_threshold: Arc<Mutex<f32>>,
        silence_threshold: Arc<Mutex<f32>>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or("No input device available")?;
        println!("Using input device: {}", device.name()?);
        let config = device.default_input_config()?;

        let sample_rate = config.sample_rate().0 as f32;

        let input_buffer = Arc::new(Mutex::new(Vec::new()));
        let input_chroma_history = Arc::new(Mutex::new(Vec::new()));
        let expected_chroma_history = Arc::new(Mutex::new(Vec::new()));
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
            matching_threshold,
            silence_threshold,
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
        let matching_threshold = Arc::clone(&self.matching_threshold);
        let silence_threshold = Arc::clone(&self.silence_threshold);

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
                        &matching_threshold,
                        &silence_threshold,
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

fn normalize_signal(signal: &mut [f32]) {
    // Find the maximum absolute value in the signal
    if let Some(max_amplitude) = signal
        .iter()
        .map(|x| x.abs())
        .max_by(|a, b| a.partial_cmp(b).unwrap())
    {
        if max_amplitude > 0.0 {
            // Normalize the signal to be between -1 and 1
            for sample in signal.iter_mut() {
                *sample /= max_amplitude;
            }
        }
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
    matching_threshold: &Arc<Mutex<f32>>,
    silence_threshold: &Arc<Mutex<f32>>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Read thresholds
    let matching_threshold = *matching_threshold.lock().unwrap();
    let silence_threshold = *silence_threshold.lock().unwrap();

    // Append incoming data to the input buffer
    {
        let mut buffer = input_buffer.lock().unwrap();
        buffer.extend_from_slice(data);
    }

    // Frame size and hop size
    const FRAME_SIZE: usize = 4096;
    const HOP_SIZE: usize = 1024;

    loop {
        let mut buffer = input_buffer.lock().unwrap();

        if buffer.len() < FRAME_SIZE {
            break;
        }

        // Extract the current frame
        let mut input_signal = buffer[..FRAME_SIZE].to_vec();
        normalize_signal(&mut input_signal);

        // Remove the processed samples, keeping the overlap
        buffer.drain(..HOP_SIZE);
        drop(buffer); // Release the lock

        // Compute RMS energy to check for silence
        let rms_energy =
            (input_signal.iter().map(|&x| x * x).sum::<f32>() / input_signal.len() as f32).sqrt();

        if rms_energy < silence_threshold {
            // Skip processing this frame
            continue;
        }

        // Apply pre-emphasis filter to boost high frequencies
        let pre_emphasis = 0.97;
        let mut emphasized_signal = vec![0.0; input_signal.len()];
        emphasized_signal[0] = input_signal[0];
        for i in 1..input_signal.len() {
            emphasized_signal[i] = input_signal[i] - pre_emphasis * input_signal[i - 1];
        }

        // Compute chroma features from the input signal
        let input_chroma = compute_chroma_features(&emphasized_signal, sample_rate);

        // Retrieve expected notes and generate expected chroma features
        let expected_chroma = {
            let expected_notes = expected_notes.lock().unwrap();
            generate_expected_chroma(&expected_notes)
        };

        let mut input_chroma = input_chroma.clone();
        let mut expected_chroma = expected_chroma.clone();

        normalize_chroma(&mut input_chroma);
        normalize_chroma(&mut expected_chroma);

        // Store the input signal for plotting
        {
            let mut signal_history = input_signal_history.lock().unwrap();
            signal_history.push(input_signal.clone());
            if signal_history.len() > 100 {
                signal_history.remove(0);
            }
        }

        // Store the normalized input chroma for plotting
        {
            let mut input_chroma_hist = input_chroma_history.lock().unwrap();
            input_chroma_hist.push(input_chroma.clone());
            if input_chroma_hist.len() > 100 {
                input_chroma_hist.remove(0);
            }
        }

        // Store the normalized expected chroma for plotting
        {
            let mut expected_chroma_hist = expected_chroma_history.lock().unwrap();
            expected_chroma_hist.push(expected_chroma.clone());
            if expected_chroma_hist.len() > 100 {
                expected_chroma_hist.remove(0);
            }
        }

        // Exaggerate chroma values by raising to a power (optional)
        let exponent = 1.5;
        for c in input_chroma.iter_mut() {
            *c = c.powf(exponent);
        }
        for c in expected_chroma.iter_mut() {
            *c = c.powf(exponent);
        }

        // Normalize again after exaggeration if necessary
        normalize_chroma(&mut input_chroma);
        normalize_chroma(&mut expected_chroma);

        // Perform similarity comparison
        let match_result =
            compare_chroma_similarity(&input_chroma, &expected_chroma, matching_threshold);

        // Send match result
        match_result_sender.send(match_result).ok();
    }

    Ok(())
}

fn compute_chroma_features(signal: &[f32], sample_rate: f32) -> Vec<f32> {
    let fft_size = signal.len();
    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(fft_size);

    // Apply a Hann window to the signal
    let hann_window: Vec<f32> = (0..fft_size)
        .map(|i| {
            0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / (fft_size as f32 - 1.0)).cos())
        })
        .collect();

    let mut buffer: Vec<Complex<f32>> = signal
        .iter()
        .zip(hann_window.iter())
        .map(|(&s, &w)| Complex { re: s * w, im: 0.0 })
        .collect();

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
        for harmonic in 1..=5 {
            let harmonic_midi = midi + 12.0 * (harmonic - 1) as f32;
            let pitch_class = (harmonic_midi.round() as usize % 12) as usize;
            if pitch_class < CHROMA_BINS {
                // Decrease weight for higher harmonics
                chroma[pitch_class] += mag / harmonic as f32;
            }
        }
    }
    chroma
}

fn normalize_chroma(chroma: &mut [f32]) {
    if let Some(&max_value) = chroma.iter().max_by(|a, b| a.partial_cmp(b).unwrap()) {
        if max_value > 0.0 {
            for c in chroma.iter_mut() {
                *c /= max_value;
            }
        }
    }
}

fn freq_to_midi(freq: f32) -> f32 {
    69.0 + 12.0 * (freq / 440.0).log2()
}

fn generate_expected_chroma(expected_notes: &Option<Vec<Note>>) -> Vec<f32> {
    let mut chroma = vec![0.0; CHROMA_BINS];
    if let Some(notes) = expected_notes {
        for note in notes {
            if let (Some(_string), Some(_fret)) = (note.string, note.fret) {
                let frequency = calculate_frequency(note, 25.5, 0);
                for harmonic in 1..=5 {
                    let harmonic_freq = frequency * harmonic as f32;
                    let midi = freq_to_midi(harmonic_freq);
                    let pitch_class = (midi.round() as usize % 12) as usize;
                    if pitch_class < CHROMA_BINS {
                        // Decrease weight for higher harmonics
                        chroma[pitch_class] += 1.0 / harmonic as f32;
                    }
                }
            }
        }
    }
    chroma
}

fn compare_chroma_similarity(
    input_chroma: &[f32],
    expected_chroma: &[f32],
    threshold: f32,
) -> bool {
    let dot_product: f32 = input_chroma
        .iter()
        .zip(expected_chroma.iter())
        .map(|(a, b)| a * b)
        .sum();

    let input_norm: f32 = input_chroma.iter().map(|&x| x * x).sum::<f32>().sqrt();
    let expected_norm: f32 = expected_chroma.iter().map(|&x| x * x).sum::<f32>().sqrt();

    if input_norm == 0.0 || expected_norm == 0.0 {
        return false;
    }

    let similarity = dot_product / (input_norm * expected_norm);

    similarity >= threshold
}
