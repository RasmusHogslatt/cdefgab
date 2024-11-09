// karplus_strong.rs

use rand::random;
use std::f32::consts::PI;

use crate::guitar::guitar::GuitarConfig;

pub struct KarplusStrong {
    buffer: Vec<f32>,
    position: usize,
    remaining_samples: usize,
}

impl KarplusStrong {
    pub fn new(
        frequency: f32,
        duration_seconds: f32,
        sample_rate: f32,
        config: &GuitarConfig,
    ) -> Self {
        let buffer_length = (sample_rate / frequency).ceil() as usize;
        let mut buffer = Vec::with_capacity(buffer_length);

        let mut prev = 0.0;
        for _ in 0..buffer_length {
            let white = random::<f32>() * 2.0 - 1.0;

            let tension_effect = config.string_tension * white;
            let filtered =
                config.string_damping * prev + (1.0 - config.string_damping) * tension_effect;
            buffer.push(filtered);
            prev = filtered;
        }

        let remaining_samples = (duration_seconds * sample_rate) as usize;
        KarplusStrong {
            buffer,
            position: 0,
            remaining_samples,
        }
    }

    pub fn next_sample(&mut self, config: &GuitarConfig, sample_rate: f32) -> Option<f32> {
        if self.remaining_samples == 0 {
            return None;
        }

        let current_value = self.buffer[self.position];
        let next_index = (self.position + 1) % self.buffer.len();
        let next_value = self.buffer[next_index];

        let string_sample = config.decay
            * (config.string_damping * current_value + (1.0 - config.string_damping) * next_value);

        let body_freq = 2.0 * PI * config.body_resonance / sample_rate;

        let resonated = string_sample * body_freq.sin();
        let body_sample = resonated * (1.0 - config.body_damping);

        self.buffer[self.position] = string_sample;
        self.position = next_index;
        self.remaining_samples -= 1;

        Some(string_sample * 0.7 + body_sample * 0.3)
    }
}
