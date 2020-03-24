pub const NOTE_ON_MSG: u8 = 0x90;
pub const NOTE_OFF_MSG: u8 = 0x80;

// A type to interface internal messages to midi messages
pub enum MidiEvent {
    Raw([u8; 3]),
    Close,
}

// Manages if a key is on or off and what the mutex is up to
#[derive(Debug, Copy, Clone)]
pub struct KeyState {
    event_counter: u32,
    key_on: bool,
}

impl KeyState {
    pub fn new() -> KeyState {
        KeyState {
            event_counter: 0,
            key_on: false,
        }
    }

    pub fn update_mutex(&mut self) -> u32 {
        self.event_counter += 1;
        self.event_counter
    }

    pub fn mutex(&mut self) -> u32 {
        self.event_counter
    }

    pub fn compare_event_id(&mut self, id: u32) -> bool {
        self.event_counter == id
    }

    pub fn play(&mut self, id: u32, note: u8, velocity: u8) -> Option<MidiEvent> {
        if self.compare_event_id(id) && !self.key_on {
            self.key_on = true;
            Some(MidiEvent::Raw([NOTE_ON_MSG, note, velocity]))
        } else {
            None
        }
    }

    pub fn stop(&mut self, id: u32, note: u8, velocity: u8) -> Option<MidiEvent> {
        if self.compare_event_id(id) && self.key_on {
            self.key_on = false;
            Some(MidiEvent::Raw([NOTE_OFF_MSG, note, velocity]))
        } else {
            None
        }
    }
}

// Holds one midi universe of KeyState structs
pub struct KeyStateStore {
    pub channels: [[KeyState; 127]; 16],
}

impl KeyStateStore {
    pub fn new() -> KeyStateStore {
        KeyStateStore {
            channels: [[KeyState::new(); 127]; 16],
        }
    }

    pub fn key_state(&mut self, channel: u8, note: u8) -> &mut KeyState {
        &mut self.channels[channel as usize][note as usize]
    }
}
