// audio.rs

pub mod audio {
    use crate::karplus_strong::KarplusStrong;
    use crate::music_representation::Note;
    use std::sync::{Arc, Mutex};

    #[cfg(target_arch = "wasm32")]
    use kira::manager::{backend::WebAudioBackend, AudioManager, AudioManagerSettings};
    #[cfg(not(target_arch = "wasm32"))]
    use kira::manager::{AudioManager, AudioManagerSettings};

    #[cfg(not(target_arch = "wasm32"))]
    pub struct AudioPlayer {
        manager: AudioManager,
    }

    #[cfg(target_arch = "wasm32")]
    pub struct AudioPlayer {
        manager: AudioManager<WebAudioBackend>,
    }

    impl AudioPlayer {
        pub fn new() -> Self {
            #[cfg(not(target_arch = "wasm32"))]
            let manager = AudioManager::new(AudioManagerSettings::default())
                .expect("Failed to create AudioManager");

            #[cfg(target_arch = "wasm32")]
            let manager = AudioManager::<WebAudioBackend>::new(AudioManagerSettings::default())
                .expect("Failed to create AudioManager");

            AudioPlayer { manager }
        }

        pub fn play_note_sequence(&mut self, notes: Vec<Note>) {
            for note in notes {
                self.play_note(note);
            }
        }

        pub fn play_note(&mut self, note: Note) {
            let frequency = note.frequency();
            let karplus_strong = KarplusStrong::new(frequency);

            // Convert KarplusStrong output to audio data
            let audio_data = karplus_strong.generate_audio_data();

            // Create a sound from the audio data
            let sound = kira::sound::Sound::from_frames(
                kira::Frame::from_mono_samples(audio_data),
                kira::sound::SoundSettings::default(),
            );

            // Play the sound
            self.manager.play(sound).expect("Failed to play sound");
        }
    }
}
