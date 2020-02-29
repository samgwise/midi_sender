use tokio::sync::{mpsc, oneshot};
use tokio::time::{delay_for, delay_until, Duration, Instant};

use tokio::join;

// Midi IO
extern crate midir;
use midir::MidiOutput;

// OSC Interface
use rosc::OscPacket;
use std::net::{SocketAddrV4, UdpSocket};
use std::str::FromStr;

// Used for midi config interactive prompts
use std::io::{stdin, stdout, Write};

mod scheduler;
use scheduler::*;

async fn play_note(midi_tx: &mpsc::Sender<KeyStateChange>, action: KeyPlay) {
    // Set up outbound transmition
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
    let message = StateChangeMessage::new(mutex, action.channel, action.note, action.velocity);

    // Delay until event start
    delay_until(action.at).await;
    // Wake up and submit play action
    tx.send(KeyStateChange::Play(message)).await.ok().unwrap();
    // delay until end of duration
    delay_until(action.at + Duration::from_millis(action.duration)).await;
    // Wake up and submit stop action
    tx.send(KeyStateChange::Stop(message)).await.ok().unwrap();
}

async fn cancel_note(midi_tx: &mpsc::Sender<KeyStateChange>, action: KeyCancel) {
    // Set up outbound transmition
    let mut tx = midi_tx.clone();

    // Request mutex for subject note
    let (mutex_request, mutex_response) = oneshot::channel();

    // Delay until event start
    delay_until(action.at).await;

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

// Routings for OSC packet
fn route_packet(packet: OscPacket) {
    match packet {
        OscPacket::Message(msg) => {
            println!("OSC address: {}", msg.addr);
            println!("OSC arguments: {:?}", msg.args);
        }
        OscPacket::Bundle(bundle) => {
            println!("OSC Bundle: {:?}", bundle);
        }
    }
}

async fn manage_midi_state(
    event_rx: &mut mpsc::Receiver<KeyStateChange>,
    midi_tx: &mpsc::Sender<MidiEvent>,
) {
    // Set up
    let mut midi_state = KeyStateStore::new();
    let mut tx = midi_tx.clone();

    // Reactor
    while let Some(change_request) = event_rx.recv().await {
        let try_state_change = match change_request {
            KeyStateChange::Play(play) => midi_state.key_state(play.channel, play.note).play(
                play.id,
                play.note,
                play.velocity,
            ),
            KeyStateChange::Stop(stop) => midi_state.key_state(stop.channel, stop.note).stop(
                stop.id,
                stop.note,
                stop.velocity,
            ),
            KeyStateChange::MutexRequest(request) => {
                request
                    .reply
                    .send(midi_state.key_state(request.channel, request.note).mutex())
                    .unwrap();
                None
            }
            KeyStateChange::MutexUpdate(request) => {
                request
                    .reply
                    .send(
                        midi_state
                            .key_state(request.channel, request.note)
                            .update_mutex(),
                    )
                    .unwrap();
                None
            }
            KeyStateChange::Close => break,
        };

        match try_state_change {
            Some(message) => tx.send(message).await.ok().unwrap(),
            None => (),
        };
    }
}

// manage midi connection in an asynchronous context
async fn messages_to_midi_out(
    midi_connection: &mut midir::MidiOutputConnection,
    midi_rx: &mut mpsc::Receiver<MidiEvent>,
) {
    while let Some(event) = midi_rx.recv().await {
        match event {
            MidiEvent::Raw(msg) => {
                println!("Sending raw message {}, {}, {}", msg[0], msg[1], msg[2]);
                let _ = midi_connection.send(&msg);
            }
            MidiEvent::Close => break,
        }
    }

    println!("Finished messages to midi out proxy.");
}

#[tokio::main]
async fn main() {
    // let cache_test: Cache<MidiEvent> = Cache::new();
    // Set up midi IO
    let midi_out = MidiOutput::new("My Test Output").unwrap();

    // Get an output port (read from console if multiple are available)
    let out_port = match midi_out.port_count() {
        0 => panic!("no output port found"),
        1 => {
            println!(
                "Choosing the only available output port: {}",
                midi_out.port_name(0).unwrap()
            );
            0
        }
        _ => {
            println!("\nAvailable output ports:");
            for i in 0..midi_out.port_count() {
                println!("{}: {}", i, midi_out.port_name(i).unwrap());
            }
            print!("Please select output port: ");
            stdout().flush().unwrap();
            let mut input = String::new();
            stdin().read_line(&mut input).unwrap();
            input.trim().parse().unwrap()
        }
    };

    println!("\nOpening connection");
    let mut conn_out = midi_out.connect(out_port, "midir-test").unwrap();
    println!("Connection open.");

    // Handle multiple async inputs to midi output
    let (mut midi_tx, mut midi_rx) = mpsc::channel(100);
    let message_hanlder = tokio::spawn(async move {
        messages_to_midi_out(&mut conn_out, &mut midi_rx).await;
        conn_out.close();
    });

    // Handle midi key state changes
    let (mut midi_state_tx, mut midi_state_rx) = mpsc::channel(100);
    let mut state_change_out_tx = midi_tx.clone();
    let state_change_reactor = tokio::spawn(async move {
        manage_midi_state(&mut midi_state_rx, &mut state_change_out_tx).await
    });

    // Play out tests

    // Broken chord
    play_note(&midi_state_tx, KeyPlay::new(Instant::now(), 600, 1, 60, 80)).await;
    play_note(&midi_state_tx, KeyPlay::new(Instant::now(), 600, 1, 64, 80)).await;
    play_note(&midi_state_tx, KeyPlay::new(Instant::now(), 600, 1, 67, 80)).await;

    // Alternative Broken chord
    join!(
        play_note(&midi_state_tx, KeyPlay::new(Instant::now(), 300, 1, 72, 80)),
        play_note(
            &midi_state_tx,
            KeyPlay::new(Instant::now() + Duration::from_millis(600), 300, 1, 67, 80)
        ),
        play_note(
            &midi_state_tx,
            KeyPlay::new(Instant::now() + Duration::from_millis(1200), 600, 1, 64, 80)
        )
    );

    // Chord
    join!(
        play_note(&midi_state_tx, KeyPlay::new(Instant::now(), 600, 1, 60, 80)),
        play_note(&midi_state_tx, KeyPlay::new(Instant::now(), 600, 1, 64, 80)),
        play_note(&midi_state_tx, KeyPlay::new(Instant::now(), 600, 1, 67, 80))
    );

    // Chord
    join!(
        play_note(&midi_state_tx, KeyPlay::new(Instant::now(), 600, 1, 59, 80)),
        play_note(&midi_state_tx, KeyPlay::new(Instant::now(), 600, 1, 65, 80)),
        play_note(&midi_state_tx, KeyPlay::new(Instant::now(), 600, 1, 67, 80))
    );

    // Chord
    join!(
        play_note(&midi_state_tx, KeyPlay::new(Instant::now(), 600, 1, 60, 80)),
        play_note(&midi_state_tx, KeyPlay::new(Instant::now(), 600, 1, 64, 80)),
        play_note(&midi_state_tx, KeyPlay::new(Instant::now(), 600, 1, 67, 80))
    );

    // Async test chord
    let midi_state_tx_moved = midi_state_tx.clone();
    tokio::spawn(async move {
        play_note(
            &midi_state_tx_moved,
            KeyPlay::new(Instant::now(), 600, 1, 60, 80),
        )
        .await;
        play_note(
            &midi_state_tx_moved,
            KeyPlay::new(Instant::now(), 600, 1, 60, 80),
        )
        .await;
    });

    play_note(&midi_state_tx, KeyPlay::new(Instant::now(), 600, 1, 64, 80)).await;
    play_note(&midi_state_tx, KeyPlay::new(Instant::now(), 600, 1, 67, 80)).await;

    //
    // Test OSC service
    //
    let address = SocketAddrV4::from_str("127.0.0.1:10009").unwrap();
    let socket = UdpSocket::bind(address).unwrap();

    let mut in_buffer = [0u8; rosc::decoder::MTU];

    println!("Listening for OSC messages on: {:?}", address);
    loop {
        match socket.recv_from(&mut in_buffer) {
            Ok((size, from_address)) => {
                let packet = rosc::decoder::decode(&in_buffer[..size]).unwrap();
                route_packet(packet);
            }
            Err(e) => {
                println!("Error receiving OSC message from socket: {}", e);
            }
        }
    }

    // End midi state management
    midi_state_tx
        .send(KeyStateChange::Close)
        .await
        .ok()
        .unwrap();
    state_change_reactor.await.ok().unwrap();

    // Release midi port
    midi_tx.send(MidiEvent::Close).await.ok().unwrap();
    message_hanlder.await.ok().unwrap();

    println!("Connection closed");
}
