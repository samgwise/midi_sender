use tokio::sync::{mpsc, oneshot};
use tokio::time::{delay_until, Duration, Instant};

pub mod types;
pub use types::*;

// Interface play event for scheduler
pub struct KeyPlay {
    pub at: u64,
    pub duration: u64,
    pub channel: u8,
    pub note: u8,
    pub velocity: u8,
}

impl KeyPlay {
    pub fn new(at: u64, duration: u64, channel: u8, note: u8, velocity: u8) -> KeyPlay {
        KeyPlay {
            at: at,
            duration: duration,
            channel: channel,
            note: note,
            velocity: velocity,
        }
    }
}

// Interface cancel event for scheduler
pub struct KeyCancel {
    pub at: u64,
    pub channel: u8,
    pub note: u8,
}

impl KeyCancel {
    pub fn new(at: u64, channel: u8, note: u8) -> KeyCancel {
        KeyCancel {
            at: at,
            channel: channel,
            note: note,
        }
    }
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
    pub reply: oneshot::Sender<MutexMessage>,
    pub channel: u8,
    pub note: u8,
}

impl MutexQuery {
    pub fn new(reply: oneshot::Sender<MutexMessage>, channel: u8, note: u8) -> MutexQuery {
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
    SyncUpdate,
    Close,
}

pub async fn play_note(midi_tx: &mpsc::Sender<KeyStateChange>, action: KeyPlay) {
    // Set up outbound transmission
    let mut tx = midi_tx.clone();

    // Request mutex for subject note
    let (mutex_request, mutex_response) = oneshot::channel();
    tx.send(KeyStateChange::MutexRequest(MutexQuery::new(
        mutex_request,
        action.channel,
        action.note,
    )))
    .await
    .ok()
    .unwrap();
    let mutex = mutex_response.await.ok().unwrap();

    // define message for use in play and stop requests
    let message =
        StateChangeMessage::new(mutex.mutex, action.channel, action.note, action.velocity);

    // Delay until event start
    let at = mutex.sync + Duration::from_nanos(action.at);
    if at > Instant::now() {
        delay_until(at).await;
    }
    // Wake up and submit play action
    tx.send(KeyStateChange::Play(message)).await.ok().unwrap();
    // delay until end of duration
    delay_until(mutex.sync + Duration::from_nanos(action.duration)).await;
    // Wake up and submit stop action
    tx.send(KeyStateChange::Stop(message)).await.ok().unwrap();
}

pub async fn cancel_note(midi_tx: &mpsc::Sender<KeyStateChange>, action: KeyCancel) {
    // Set up outbound transmission
    let mut tx = midi_tx.clone();

    // Request mutex for subject command
    let (mutex_request, mutex_response) = oneshot::channel();
    tx.send(KeyStateChange::MutexRequest(MutexQuery::new(
        mutex_request,
        action.channel,
        action.note,
    )))
    .await
    .ok()
    .unwrap();
    let mutex = mutex_response.await.ok().unwrap();

    // Delay until event start
    let at = mutex.sync + Duration::from_nanos(action.at);
    if at > Instant::now() {
        delay_until(at).await;
    }

    // Request mutex for subject note
    let (mutex_request, mutex_response) = oneshot::channel();

    // Send and await response of update
    tx.send(KeyStateChange::MutexRequest(MutexQuery::new(
        mutex_request,
        action.channel,
        action.note,
    )))
    .await
    .ok()
    .unwrap();
    mutex_response.await.ok().unwrap();
}
