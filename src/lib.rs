#[macro_use] extern crate vst2;
//extern crate libc;

use vst2::buffer::AudioBuffer;
use vst2::plugin::{Category, Plugin, Info, CanDo};
use vst2::editor::Editor;
use vst2::event::Event;
use vst2::api::{Events,Supported};

use std::f64::consts::PI;

use std::os::raw::c_void;

/// Convert the midi note into the equivalent frequency.
///
/// This function assumes A4 is 440hz.
fn midi_note_to_hz(note: u8) -> f64 {
    const A4: f64 = 440.0;

    (A4 / 32.0) * ((note as f64 - 9.0) / 12.0).exp2()
}

struct SineSynth {
    // Parameters
    attack: f64,
    release: f64,

    sample_rate: f64,
    time: f64,
    note_duration: f64,
    note: Option<u8>,
    is_pressed: bool,  // true if current pressed
    last_released: f64,  // time since the note was last released

    editor: SineSynthEditor,
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
        if self.note == None || !self.is_pressed{
            // if no note is already pressed
            // basically enforcing monophonic control
            self.note_duration = 0.0;
            self.note = Some(note);
            self.is_pressed = true;
        }
    }

    fn note_off(&mut self, note: u8) {
        if let Some(current_note) = self.note {
            if current_note == note {
                // only consider note-off events
                // for the current note
                self.last_released = 0.0;
                self.is_pressed = false;
            }
        }
    }
}

pub const TAU : f64 = PI * 2.0;

impl Default for SineSynth {
    fn default() -> SineSynth {
        SineSynth {
            attack: 0.0001,
            release: 0.0001,

            sample_rate: 44100.0,
            note_duration: 0.0,
            time: 0.0,
            note: None,
            is_pressed: false,
            last_released: 0.0,

            editor: Default::default()
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
            parameters: 2,
            initial_delay: 0,
            ..Info::default()
        }
    }

    fn get_parameter(&self, index: i32) -> f32 {
        match index {
            0 => self.attack as f32,
            1 => self.release as f32,
            _ => 0.0,
        }
    }

    fn set_parameter(&mut self, index: i32, value: f32) {
        match index {
            0 => self.attack = value.max(1.0) as f64,
            1 => self.release = value.max(1.0) as f64,
            _ => (),
        }
    }

    fn get_parameter_name(&self, index: i32) -> String {
        match index {
            0 => "Attack".to_string(),
            1 => "Release".to_string(),
            _ => "".to_string(),
        }
    }

    fn get_parameter_text(&self, index: i32) -> String {
        match index {
            0 => format!("{}", self.attack),
            1 => format!("{}", self.release),
            _ => "".to_string(),
        }
    }

    fn get_parameter_label(&self, index: i32) -> String {
        match index {
            0 => "s".to_string(),
            1 => "s".to_string(),
            _ => "".to_string(),
        }
    }

    #[allow(unused_variables)]
    fn process_events(&mut self, events: &Events) {
        for &e in events.events_raw() {
            let event: Event = Event::from(unsafe { *e });
            match event {
                Event::Midi(ev) => self.process_midi_event(ev.data),
                // More events can be handled here.
                _ => ()
            }
        }
    }

    fn set_sample_rate(&mut self, rate: f32) {
        self.sample_rate = rate as f64;
    }

    fn process(&mut self, buffer: &mut AudioBuffer<f32>) {
        let samples = buffer.samples();

        let per_sample = self.time_per_sample();

        for (input_buffer, output_buffer) in buffer.zip() {
            let mut t = self.time;
            let mut n = 0.0;

            for (_, output_sample) in input_buffer.iter().zip(output_buffer) {
                if let Some(note) = self.note {
                    let signal = (t * midi_note_to_hz(note) * TAU).sin();

                    let attack = 0.01;
                    let release = 0.01;

                    // Apply attack
                    let alpha = if (self.note_duration + n) < attack {
                        (self.note_duration + n) / (self.attack * attack)
                    } else {
                        1.0
                    };

                    // Apply release
                    let beta = if self.is_pressed {
                        1.0
                    } else if (self.last_released + n) < release {
                        1.0 - ((self.last_released + n) / (release))
                    } else {
                        0.0
                    };

                    let multiplier = alpha * beta;
                    *output_sample = (signal * multiplier) as f32;

                    n += per_sample;
                    t += per_sample;
                } else {
                    *output_sample = 0.0;
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

    fn get_editor(&mut self) -> Option<&mut Editor> {
        Some(&mut self.editor)
    }
}

struct SineSynthEditor {
    is_open: bool,
}

impl Default for SineSynthEditor {
    fn default() -> SineSynthEditor {
        SineSynthEditor {
            is_open: false,
        }
    }
}

impl Editor for SineSynthEditor {
    fn size(&self) -> (i32, i32) {
        (320, 240)
    }

    fn position(&self) -> (i32, i32) {
        (100, 100)
    }

    fn open(&mut self, _: *mut c_void) {

        self.is_open = true;
    }

    fn is_open(&mut self) -> bool {
        self.is_open
    }
}

plugin_main!(SineSynth);
