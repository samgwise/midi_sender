pub const NOTE_ON_MSG: u8 = 0x90;
pub const NOTE_OFF_MSG: u8 = 0x80;

// A type to interface internal messages to midi messages
pub enum MidiEvent {
    Raw([u8; 3]),
    Close,
}
