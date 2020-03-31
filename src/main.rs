use tokio::sync::mpsc;
use tokio::time::{Duration, Instant};

// use tokio::join;

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

// Configuration utilities
mod config;
use config::*;

// Float to instant suitable for use in an at scheduler
fn seconds_to_instant(seconds: f64) -> Instant {
    if seconds.is_normal() && seconds > 0.0 {
        Instant::now() + Duration::from_secs_f64(seconds)
    } else {
        Instant::now()
    }
}

// Routings for OSC packet
fn route_packet(packet: OscPacket, midi_tx: &mpsc::Sender<KeyStateChange>) {
    match packet {
        OscPacket::Message(mut msg) => {
            println!("OSC address: {}", msg.addr);
            println!("OSC arguments: {:?}", msg.args);
            route_message(&mut msg, midi_tx)
        }
        OscPacket::Bundle(bundle) => {
            println!("OSC Bundle: {:?}", bundle);
        }
    }
}

fn route_message(message: &mut rosc::OscMessage, midi_tx: &mpsc::Sender<KeyStateChange>) {
    if message.addr == "/midi_sender/play" {
        // Move arguments into the struct
        let mut args = message.args.drain(0..);
        // let at = seconds_to_instant(
        //     args.nth(0)
        //         .expect("Missing 1st argument.")
        //         .double()
        //         .unwrap(),
        // );
        let at = args.nth(0).expect("Missing 1st argument.").long().unwrap();
        let duration = args.nth(0).expect("Missing 2nd argument.").long().unwrap();
        let channel = args.nth(0).expect("Missing 3rd argument.").int().unwrap() as u8;
        let note = args.nth(0).expect("Missing 4th argument.").int().unwrap() as u8;
        let velocity = args.nth(0).expect("Missing 5th argument.").int().unwrap() as u8;

        let event = KeyPlay::new(at as u64, duration as u64, channel, note, velocity);
        let midi_tx_movable = midi_tx.clone();
        tokio::spawn(async move {
            play_note(&midi_tx_movable, event).await;
        });
    } else if message.addr == "/midi_sender/cancel" {
        // Move arguments into the struct
        let mut args = message.args.drain(0..);
        // let at = seconds_to_instant(
        //     args.nth(0)
        //         .expect("Missing 1st argument.")
        //         .double()
        //         .unwrap(),
        // );
        let at = args.nth(0).expect("Missing 1st argument.").long().unwrap();
        let channel = args.nth(1).expect("Missing 2nd argument.").int().unwrap() as u8;
        let note = args.nth(0).expect("Missing 3rd argument.").int().unwrap() as u8;

        let event = KeyCancel::new(at as u64, channel, note);
        let midi_tx_movable = midi_tx.clone();
        tokio::spawn(async move {
            cancel_note(&midi_tx_movable, event).await;
        });
    } else if message.addr == "/midi_sender/augment" {
        println!("Augment action is yet to be implemented");
    } else if message.addr == "/midi_sender/sync" {
        println!("Sync action triggered");
        let mut midi_tx_movable = midi_tx.clone();
        tokio::spawn(async move {
            midi_tx_movable
                .send(KeyStateChange::SyncUpdate)
                .await
                .ok()
                .unwrap();
        });
    } else {
        println!("No behaviour defined for OSC path '{}'", message.addr);
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
                play.channel,
                play.note,
                play.velocity,
            ),
            KeyStateChange::Stop(stop) => midi_state.key_state(stop.channel, stop.note).stop(
                stop.id,
                stop.channel,
                stop.note,
                stop.velocity,
            ),
            KeyStateChange::MutexRequest(request) => {
                request
                    .reply
                    .send(MutexMessage::new(
                        midi_state.key_state(request.channel, request.note).mutex(),
                        midi_state.sync,
                    ))
                    .unwrap();
                None
            }
            KeyStateChange::MutexUpdate(request) => {
                request
                    .reply
                    .send(MutexMessage::new(
                        midi_state
                            .key_state(request.channel, request.note)
                            .update_mutex(),
                        midi_state.sync,
                    ))
                    .unwrap();
                None
            }
            KeyStateChange::SyncUpdate => {
                midi_state.set_sync();
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
    // Read configuration
    let config_path = "./midi_sender.toml";
    let config = match slurp(config_path.to_string())
        .and_then(|config_toml| config_from_toml(config_toml))
    {
        Ok(config) => config,
        Err(error) => {
            println!("Unable to open configuration: '{}' with error: {}. Using default configuration:\n{}", config_path, error, default_config_toml());
            default_config()
        }
    };

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

            match config.midi_port {
                Some(port) => port,
                None => {
                    print!("No port defined. Please select output port: ");
                    stdout().flush().unwrap();
                    let mut input = String::new();
                    stdin().read_line(&mut input).unwrap();
                    input.trim().parse().unwrap()
                }
            }
        }
    };

    println!("\nOpening connection");
    let mut conn_out = midi_out.connect(out_port, "midir-test").unwrap();
    println!("Connection open.");

    // Handle multiple async inputs to midi output
    let (mut midi_tx, mut midi_rx) = mpsc::channel(100);
    let message_handler = tokio::spawn(async move {
        messages_to_midi_out(&mut conn_out, &mut midi_rx).await;
        conn_out.close();
    });

    // Handle midi key state changes
    let (mut midi_state_tx, mut midi_state_rx) = mpsc::channel(100);
    let mut state_change_out_tx = midi_tx.clone();
    let state_change_reactor = tokio::spawn(async move {
        manage_midi_state(&mut midi_state_rx, &mut state_change_out_tx).await
    });

    // // Play out tests

    // // Broken chord
    // play_note(&midi_state_tx, KeyPlay::new(Instant::now(), 600, 1, 60, 80)).await;
    // play_note(&midi_state_tx, KeyPlay::new(Instant::now(), 600, 1, 64, 80)).await;
    // play_note(&midi_state_tx, KeyPlay::new(Instant::now(), 600, 1, 67, 80)).await;

    // // Alternative Broken chord
    // join!(
    //     play_note(&midi_state_tx, KeyPlay::new(Instant::now(), 300, 1, 72, 80)),
    //     play_note(
    //         &midi_state_tx,
    //         KeyPlay::new(Instant::now() + Duration::from_millis(600), 300, 1, 67, 80)
    //     ),
    //     play_note(
    //         &midi_state_tx,
    //         KeyPlay::new(Instant::now() + Duration::from_millis(1200), 600, 1, 64, 80)
    //     )
    // );

    // // Chord
    // join!(
    //     play_note(&midi_state_tx, KeyPlay::new(Instant::now(), 600, 1, 60, 80)),
    //     play_note(&midi_state_tx, KeyPlay::new(Instant::now(), 600, 1, 64, 80)),
    //     play_note(&midi_state_tx, KeyPlay::new(Instant::now(), 600, 1, 67, 80))
    // );

    // // Chord
    // join!(
    //     play_note(&midi_state_tx, KeyPlay::new(Instant::now(), 600, 1, 59, 80)),
    //     play_note(&midi_state_tx, KeyPlay::new(Instant::now(), 600, 1, 65, 80)),
    //     play_note(&midi_state_tx, KeyPlay::new(Instant::now(), 600, 1, 67, 80))
    // );

    // // Chord
    // join!(
    //     play_note(&midi_state_tx, KeyPlay::new(Instant::now(), 600, 1, 60, 80)),
    //     play_note(&midi_state_tx, KeyPlay::new(Instant::now(), 600, 1, 64, 80)),
    //     play_note(&midi_state_tx, KeyPlay::new(Instant::now(), 600, 1, 67, 80))
    // );

    // // Async test chord
    // let midi_state_tx_moved = midi_state_tx.clone();
    // tokio::spawn(async move {
    //     play_note(
    //         &midi_state_tx_moved,
    //         KeyPlay::new(Instant::now(), 600, 1, 60, 80),
    //     )
    //     .await;
    //     play_note(
    //         &midi_state_tx_moved,
    //         KeyPlay::new(Instant::now(), 600, 1, 60, 80),
    //     )
    //     .await;
    // });

    // play_note(&midi_state_tx, KeyPlay::new(Instant::now(), 600, 1, 64, 80)).await;
    // play_note(&midi_state_tx, KeyPlay::new(Instant::now(), 600, 1, 67, 80)).await;

    //
    // Test OSC service
    //
    let address = SocketAddrV4::from_str(&config.listen_address).unwrap();
    let socket = UdpSocket::bind(address).unwrap();

    let mut in_buffer = [0u8; rosc::decoder::MTU];

    println!("Listening for OSC messages on: {:?}", address);
    loop {
        match socket.recv_from(&mut in_buffer) {
            Ok((size, _from_address)) => {
                let packet = rosc::decoder::decode(&in_buffer[..size]).unwrap();
                route_packet(packet, &midi_state_tx);
            }
            Err(e) => {
                println!("Error receiving OSC message from socket: {}", e);
                break;
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
    message_handler.await.ok().unwrap();

    println!("Connection closed");
}
