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

struct NotePress {
    note: u8,
    pressed_time: f64,  // time at which the note was pressed
    released_time: f64,  // time at which the note was released
    is_pressed: bool,  // true, if note is currently pressed
}

struct SineSynth {
    // Parameters
    attack: f64,
    release: f64,

    sample_rate: f64,
    time: f64,

    notes: Vec<NotePress>,  // all currently pressed/just pressed notes

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
        // sanity check, make sure note isn't already in list?
        self.notes.retain(|ref x| x.note != note);

        // make a new note
        let new_note_press = NotePress {
            note: note,
            pressed_time: self.time, // current time
            released_time: 0.0, // null
            is_pressed: true,
        };

        self.notes.push(new_note_press);
    }

    fn note_off(&mut self, note: u8) {

        match self.notes.iter().position(|ref x| x.note == note) {
            Some(i) => {
                let note_press = self.notes.get_mut(i).unwrap();
                note_press.is_pressed = false;
                note_press.released_time = self.time;
            },
            None => (),
        };
    }
}

pub const TAU : f64 = PI * 2.0;

impl Default for SineSynth {
    fn default() -> SineSynth {
        SineSynth {
            attack: 0.0001,
            release: 0.0001,

            sample_rate: 44100.0,
            time: 0.0,
            notes: Vec::new(),

            editor: Default::default()
        }
    }
}

impl Plugin for SineSynth {
    fn get_info(&self) -> Info {
        Info {
            name: "PolySineSynth".to_string(),
            vendor: "test vendor".to_string(),
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

            for (_, output_sample) in input_buffer.iter().zip(output_buffer) {
                let num_notes = self.notes.len();

                for note_press in self.notes.iter() {
                    let signal = (t * midi_note_to_hz(note_press.note) * TAU).sin();

                    let attack = 0.01;
                    let release = 0.01;

                    // Apply attack
                    let time_since_press = t - note_press.pressed_time;
                    let alpha = if time_since_press < attack {
                        time_since_press / self.attack
                    } else {
                        1.0
                    };

                    // Apply release
                    let beta = if note_press.is_pressed {
                        1.0
                    } else {
                        let time_since_release = t - note_press.released_time;
                        if time_since_release < release {
                            1.0 - (time_since_release / release)
                        } else {
                            0.0
                        }
                    };

                    let multiplier = alpha * beta / (num_notes as f64);
                    *output_sample += (signal * multiplier) as f32;
                }

                t += per_sample;
            }
        }

        self.time += samples as f64 * per_sample;
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
