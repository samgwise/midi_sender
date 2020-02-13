use tokio::sync::mpsc;
use tokio::time::{delay_queue, DelayQueue, Duration, Error, Instant};

use std::collections::HashMap;
use std::task::{Context, Poll};

use futures::ready;

pub mod types;
pub use types::*;

pub struct Scheduler {
    entries: HashMap<String, (MidiEvent, delay_queue::Key)>,
    expirations: DelayQueue<String>,
    midi_tx: mpsc::Sender<MidiEvent>,
}

const TTL_SECS: u64 = 30;

impl Scheduler {
    pub fn new(tx: &mpsc::Sender<MidiEvent>) -> Scheduler {
        Scheduler {
            entries: HashMap::new(),
            expirations: DelayQueue::new(),
            midi_tx: tx.clone(),
        }
    }

    fn insert(&mut self, key: String, value: MidiEvent) {
        let delay = self
            .expirations
            .insert(key.clone(), Duration::from_secs(TTL_SECS));

        self.entries.insert(key, (value, delay));
    }

    fn get(&self, key: &String) -> Option<&MidiEvent> {
        self.entries.get(key).map(|&(ref v, _)| v)
    }

    fn remove(&mut self, key: &String) {
        if let Some((_, cache_key)) = self.entries.remove(key) {
            self.expirations.remove(&cache_key);
        }
    }

    async fn poll_purge(&mut self, context: &mut Context<'_>) -> Poll<Result<(), Error>> {
        while let Some(res) = ready!(self.expirations.poll_expired(context)) {
            let entry = res?;
            match self.entries.remove(entry.get_ref()) {
                Some((event, _)) => self.midi_tx.send(event).await.ok().unwrap(),
                None => (),
            }
        }

        Poll::Ready(Ok(()))
    }
}

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

    pub fn register_event_id(&mut self) -> u32 {
        self.event_counter += 1;
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

pub enum KeyStateChange {
    Play(StateChangeMessage),
    Stop(StateChangeMessage),
    Close,
}
