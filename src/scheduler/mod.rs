use tokio::sync::oneshot;
use tokio::time::Instant;

pub mod types;
pub use types::*;

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

pub struct KeyPlay {
    pub at: Instant,
    pub duration: u64,
    pub channel: u8,
    pub note: u8,
    pub velocity: u8,
}

impl KeyPlay {
    pub fn new(at: Instant, duration: u64, channel: u8, note: u8, velocity: u8) -> KeyPlay {
        KeyPlay {
            at: at,
            duration: duration,
            channel: channel,
            note: note,
            velocity: velocity,
        }
    }
}

pub struct KeyCancel {
    pub at: Instant,
    pub channel: u8,
    pub note: u8,
}

// Remote interface
pub enum KeyEvent {
    Play(KeyPlay),
    Cancel(KeyCancel),
}

//
// State change interface
//
#[derive(Debug, Copy, Clone)]
pub struct StateChangeMessage {
    pub id: u32,
    pub channel: u8,
    pub note: u8,
    pub velocity: u8,
}

impl StateChangeMessage {
    pub fn new(id: u32, channel: u8, note: u8, velocity: u8) -> StateChangeMessage {
        StateChangeMessage {
            id: id,
            note: note,
            channel: channel,
            velocity: velocity,
        }
    }
}

pub struct MutexQuery {
    pub reply: oneshot::Sender<u32>,
    pub channel: u8,
    pub note: u8,
}

impl MutexQuery {
    pub fn new(reply: oneshot::Sender<u32>, channel: u8, note: u8) -> MutexQuery {
        MutexQuery {
            reply: reply,
            channel: channel,
            note: note,
        }
    }
}

pub enum KeyStateChange {
    Play(StateChangeMessage),
    Stop(StateChangeMessage),
    MutexRequest(MutexQuery),
    MutexUpdate(MutexQuery),
    Close,
}
