#[macro_use] extern crate vst2;

use vst2::buffer::AudioBuffer;
use vst2::plugin::{Category, Plugin, Info, CanDo};
use vst2::event::Event;
use vst2::api::Supported;

use std::f64::consts::PI;

/// Convert the midi note into the equivalent frequency.
///
/// This function assumes A4 is 440hz.
fn midi_note_to_hz(note: u8) -> f64 {
    const A4: f64 = 440.0;

    (A4 / 32.0) * ((note as f64 - 9.0) / 12.0).exp2()
}

struct SineSynth {
    sample_rate: f64,
    time: f64,
    note_duration: f64,
    note: Option<u8>,
    last_note: Option<u8>,
    last_released: f64,  // time since the note was last released
}

impl SineSynth {
    fn time_per_sample(&self) -> f64 {
        1.0 / self.sample_rate
    }

    /// Process an incoming midi event.
    ///
    /// The midi data is split up like so:
    ///
    /// `data[0]`: Contains the status and the channel. Source: [source]
    /// `data[1]`: Contains the supplemental data for the message - so, if this was a NoteOn then
    ///            this would contain the note.
    /// `data[2]`: Further supplemental data. Would be velocity in the case of a NoteOn message.
    ///
    /// [source]: http://www.midimountain.com/midi/midi_status.htm
    fn process_midi_event(&mut self, data: [u8; 3]) {
        match data[0] {
            128 => self.note_off(data[1]),
            144 => self.note_on(data[1]),
            _ => ()
        }
    }

    fn note_on(&mut self, note: u8) {
        self.note_duration = 0.0;
        self.note = Some(note)
    }

    fn note_off(&mut self, note: u8) {
        if self.note == Some(note) {
            self.note = None;
            self.last_note = Some(note);
            self.last_released = 0.0;
        }
    }
}

pub const TAU : f64 = PI * 2.0;

impl Default for SineSynth {
    fn default() -> SineSynth {
        SineSynth {
            sample_rate: 44100.0,
            note_duration: 0.0,
            time: 0.0,
            note: None,
            last_note: None,
            last_released: 0.0,
        }
    }
}

impl Plugin for SineSynth {
    fn get_info(&self) -> Info {
        Info {
            name: "SineSynth".to_string(),
            vendor: "DeathDisco".to_string(),
            unique_id: 6667,
            category: Category::Synth,
            inputs: 2,
            outputs: 2,
            parameters: 0,
            initial_delay: 0,
            ..Info::default()
        }
    }

    #[allow(unused_variables)]
    fn process_events(&mut self, events: Vec<Event>) {
        for event in events {
            match event {
                Event::Midi { data, ..  } => self.process_midi_event(data),
                // More events can be handled here.
                _ => {}
            }
        }
    }

    fn set_sample_rate(&mut self, rate: f32) {
        self.sample_rate = rate as f64;
    }

    fn process(&mut self, buffer: AudioBuffer<f32>) {
        let (inputs, outputs) = buffer.split();

        let samples = inputs
            .first()
            .map(|channel| channel.len())
            .unwrap_or(0);

        let per_sample = self.time_per_sample();

        for (input_buffer, output_buffer) in inputs.iter().zip(outputs) {
            let mut t = self.time;

            for (_, output_sample) in input_buffer.iter().zip(output_buffer) {
                if let Some(current_note) = self.note {
                    // note is pressed

                    let signal = (t * midi_note_to_hz(current_note) * TAU).sin();

                    // Apply a quick envelope to the attack of the signal to avoid popping.
                    let attack = 0.5;
                    let alpha = if self.note_duration < attack {
                        self.note_duration / attack
                    } else {
                        1.0
                    };

                    *output_sample = (signal * alpha) as f32;

                    t += per_sample;
                } else {
                    // if a note was pressed earlier
                    if let Some(last_note) = self.last_note {
                        let signal = (t * midi_note_to_hz(last_note) * TAU).sin();

                        let release = 0.1;
                        let beta = if self.last_released < release {
                            1.0 - (self.last_released / release)
                        } else {
                            0.0
                        };

                        *output_sample = (signal * beta) as f32;

                        t += per_sample;
                    } else {
                        *output_sample = 0.0;
                    }
                }
            }
        }

        self.time += samples as f64 * per_sample;
        self.note_duration += samples as f64 * per_sample;
        self.last_released += samples as f64 * per_sample;
    }

    fn can_do(&self, can_do: CanDo) -> Supported {
        match can_do {
            CanDo::ReceiveMidiEvent => Supported::Yes,
            _ => Supported::Maybe
        }
    }
}

plugin_main!(SineSynth);
