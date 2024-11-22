// guitar.rs

use std::fmt;

#[derive(Clone, Debug)]
pub enum GuitarType {
    Custom,
    Acoustic,
    Classical,
    Electric,
    Bass,
    TwelveString,
}

impl fmt::Display for GuitarType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GuitarType::Custom => write!(f, "Custom"),
            GuitarType::Acoustic => write!(f, "Acoustic"),
            GuitarType::Classical => write!(f, "Classical"),
            GuitarType::Electric => write!(f, "Electric"),
            GuitarType::Bass => write!(f, "Bass"),
            GuitarType::TwelveString => write!(f, "Twelve string"),
        }
    }
}

#[derive(Clone)]
pub struct GuitarConfig {
    pub decay: f32,
    pub string_damping: f32,
    pub body_resonance: f32,
    pub body_damping: f32,
    pub string_tension: f32,
    pub scale_length: f32,
    pub capo_fret: u8,
    pub name: GuitarType,
    pub volume: f32,
}

impl GuitarConfig {
    pub fn acoustic() -> Self {
        Self {
            name: GuitarType::Acoustic,
            decay: 0.995,
            string_damping: 0.4,
            body_resonance: 150.0,
            body_damping: 0.2,
            string_tension: 0.8,
            scale_length: 25.5,
            capo_fret: 0,
            volume: 0.5,
        }
    }

    pub fn electric() -> Self {
        Self {
            name: GuitarType::Electric,
            decay: 0.999,
            string_damping: 0.1,
            body_resonance: 70.0,
            body_damping: 0.8,
            string_tension: 0.8,
            scale_length: 25.5,
            capo_fret: 0,
            volume: 0.5,
        }
    }

    pub fn classical() -> Self {
        Self {
            name: GuitarType::Classical,
            decay: 0.990,
            string_damping: 0.6,
            body_resonance: 120.0,
            body_damping: 0.3,
            string_tension: 0.5,
            scale_length: 25.6,
            capo_fret: 0,
            volume: 0.5,
        }
    }

    pub fn bass_guitar() -> Self {
        Self {
            name: GuitarType::Bass,
            decay: 0.997,
            string_damping: 0.3,
            body_resonance: 0.0,
            body_damping: 0.9,
            string_tension: 0.9,
            scale_length: 34.0,
            capo_fret: 0,
            volume: 0.5,
        }
    }

    pub fn twelve_string() -> Self {
        Self {
            name: GuitarType::TwelveString,
            decay: 0.994,
            string_damping: 0.5,
            body_resonance: 150.0,
            body_damping: 0.2,
            string_tension: 0.9,
            scale_length: 25.5,
            capo_fret: 0,
            volume: 0.5,
        }
    }

    pub fn custom(
        decay: f32,
        string_damping: f32,
        body_resonance: f32,
        body_damping: f32,
        string_tension: f32,
        scale_length: f32,
        capo_fret: u8,
        volume: f32,
    ) -> Self {
        let validated_capo_fret = capo_fret.min(24); // Assuming a maximum of 24 frets

        GuitarConfig {
            decay,
            string_damping,
            body_resonance,
            body_damping,
            string_tension,
            scale_length,
            capo_fret: validated_capo_fret,
            name: GuitarType::Custom,
            volume,
        }
    }
}
