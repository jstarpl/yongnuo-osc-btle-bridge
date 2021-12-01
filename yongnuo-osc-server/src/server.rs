use num_traits::ToPrimitive;
use rosc::decoder::decode as osc_decode;
use rosc::{OscBundle, OscMessage, OscPacket, OscType};
use std::net::{SocketAddr, UdpSocket};
use std::str::FromStr;
use std::sync::Mutex;
use std::sync::{Arc, Condvar};
use std::thread;
use std::time::Duration;

use btleplug::api::{BDAddr, Central, Characteristic, Peripheral as ApiPeripheral, UUID};
#[cfg(target_os = "linux")]
use btleplug::bluez::{adapter::ConnectedAdapter, manager::Manager};
#[cfg(target_os = "macos")]
use btleplug::corebluetooth::{adapter::Adapter, manager::Manager};
#[cfg(target_os = "windows")]
use btleplug::winrtble::{adapter::Adapter, manager::Manager};

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn get_central(manager: &Manager) -> Adapter {
    let adapters = manager.adapters().unwrap();
    adapters.into_iter().nth(0).unwrap()
}

#[cfg(target_os = "linux")]
fn get_central(manager: &Manager) -> ConnectedAdapter {
    let adapters = manager.adapters().unwrap();
    let adapter = adapters.into_iter().nth(0).unwrap();
    adapter.connect().unwrap()
}

pub trait StringableOscType {
    fn to_string(&self) -> String;
}

impl StringableOscType for OscType {
    fn to_string(&self) -> String {
        match self {
            OscType::Float(num) => num.to_string(),
            OscType::Int(num) => num.to_string(),
            _ => String::from("?"),
        }
    }
}

#[derive(Default, Clone)]
struct RGBState {
    red: u8,
    green: u8,
    blue: u8,
}

#[derive(Default, Clone)]
struct WhiteState {
    warm: u8,
    cool: u8,
}

#[derive(Default, Clone)]
struct LightState {
    rgb: RGBState,
    white: WhiteState,
}

#[derive(Clone, PartialEq)]
enum StateModification {
    RGB,
    White,
    None,
}

fn send_rgb_state(state: &LightState, light: &impl ApiPeripheral, cmd_char: &Characteristic) {
    let red = state.rgb.red;
    let green = state.rgb.green;
    let blue = state.rgb.blue;
    println!("Sending RGB state: {0}, {1}, {2}", red, green, blue);
    let result = light.command(cmd_char, &[0xae, 0xa1, red, green, blue, 0x56]);
    if result.is_err() {
        println!("Could not send RGB state: {:#?}", result)
    }
}

fn send_white_state(state: &LightState, light: &impl ApiPeripheral, cmd_char: &Characteristic) {
    let cool = state.white.cool;
    let warm = state.white.warm;
    println!("Sending White state: {0}, {1}", cool, warm);
    let result = light.command(cmd_char, &[0xae, 0xaa, 1, cool, warm, 0x56]);
    if result.is_err() {
        println!("Could not send RGB state: {:#?}", result)
    }
}

fn handle_message(message: OscMessage, state: &mut LightState) -> StateModification {
    // println!(
    //     "{}: {}",
    //     message.addr,
    //     (&message.args)
    //         .into_iter()
    //         .map(|v| v.to_string())
    //         .collect::<Vec<String>>()
    //         .join(" ")
    // );

    let value = (message.args).into_iter().nth(0);

    match message.addr.as_ref() {
        "/red" => {
            let basic_value = (value.unwrap().float().unwrap_or(0.0) * 255.0)
                .to_u8()
                .unwrap_or(0);
            state.rgb.red = basic_value;
            return StateModification::RGB;
        }
        "/green" => {
            let basic_value = (value.unwrap().float().unwrap_or(0.0) * 255.0)
                .to_u8()
                .unwrap_or(0);
            state.rgb.green = basic_value;
            return StateModification::RGB;
        }
        "/blue" => {
            let basic_value = (value.unwrap().float().unwrap_or(0.0) * 255.0)
                .to_u8()
                .unwrap_or(0);
            state.rgb.blue = basic_value;
            return StateModification::RGB;
        }
        "/warm" => {
            let basic_value = (value.unwrap().float().unwrap_or(0.0) * 99.0)
                .to_u8()
                .unwrap_or(0);
            state.white.warm = basic_value;
            return StateModification::White;
        }
        "/cool" => {
            let basic_value = (value.unwrap().float().unwrap_or(0.0) * 99.0)
                .to_u8()
                .unwrap_or(0);
            state.white.cool = basic_value;
            return StateModification::White;
        }
        _ => {
            println!("Unsupported OSC address: {0}", message.addr);
            return StateModification::None;
        }
    }
}

fn handle_bundle(bundle: OscBundle, state: &mut LightState) -> StateModification {
    bundle
        .content
        .into_iter()
        .map(|p| handle_packet(p, state))
        .into_iter()
        .last()
        .unwrap_or(StateModification::None)
}

fn handle_packet(packet: OscPacket, state: &mut LightState) -> StateModification {
    match packet {
        OscPacket::Message(osc_message) => handle_message(osc_message, state),
        OscPacket::Bundle(osc_bundle) => handle_bundle(osc_bundle, state),
    }
}

pub fn serve(port: u16, mac: &str) {
    let socket = UdpSocket::bind(SocketAddr::from(([0, 0, 0, 0], port)))
        .ok()
        .expect("Can't open server socket");

    let target_address = BDAddr::from_str(mac).ok().expect("Target address invalid");

    print!("Connecting to device {0}... ", target_address);
    let light_state_channel_send = Arc::new((
        Mutex::new((LightState::default(), StateModification::None)),
        Condvar::new(),
    ));
    let light_state_channel_recv = Arc::clone(&light_state_channel_send);

    let threads = (
        thread::spawn(move || {
            let manager = Manager::new().unwrap();

            // get the first bluetooth adapter
            //
            // connect to the adapter
            let central = get_central(&manager);

            // start scanning for devices
            central
                .start_scan()
                .expect("Can't start scanning for the device");
            // instead of waiting, you can use central.on_event to be notified of
            // new devices
            thread::sleep(Duration::from_secs(5));

            // find the device we're interested in
            let light = central
                .peripherals()
                .into_iter()
                .find(|p| p.properties().address.eq(&target_address))
                .expect("Could not find devices with the specified address");

            // connect to the device
            light.connect().ok().expect("Could not connect to device");

            let send_char_uuid =
                UUID::from_str("f0:00:aa:61:04:51:40:00:b0:00:00:00:00:00:00:00").unwrap();
            // find the characteristic we want
            let chars = light
                .discover_characteristics()
                .ok()
                .expect("Could not discover characteristics");
            let cmd_char = chars
                .iter()
                .find(|c| c.uuid == send_char_uuid)
                .expect("Could not find matching command characteristic");

            light
                .command(cmd_char, &[0xae, 0x33, 0x00, 0x00, 0x00, 0x56])
                .expect("Couldn't send initialize message");

            println!("Connected.");

            loop {
                let msg = {
                    let (lock, recv) = &*light_state_channel_recv;
                    let mut msg = lock.lock().unwrap();
                    if msg.1 == StateModification::None {
                        msg = recv.wait(msg).unwrap();
                    }
                    let res = (*msg).clone();
                    msg.1 = StateModification::None;
                    res
                };

                let light_state = &msg.0;
                let bundled_modification = &msg.1;

                match bundled_modification {
                    StateModification::RGB => send_rgb_state(&light_state, &light, cmd_char),
                    StateModification::White => send_white_state(&light_state, &light, cmd_char),
                    StateModification::None => {}
                }
            }
        }),
        thread::spawn(move || {
            socket
                .set_read_timeout(Some(Duration::new(0, 1)))
                .expect("Could not set read timeout");

            let mut light_state = LightState::default();

            loop {
                let mut buf = [0; 4098];

                let result = socket.recv_from(&mut buf);
                if result.is_err() {
                    continue;
                }

                let osc_packet = osc_decode(&buf);
                if let Err(err) = osc_packet {
                    // log
                    println!("Broken OSC message received: {:#?}", err);
                    continue;
                }

                let osc_packet = osc_packet.unwrap();
                let this_modification = handle_packet(osc_packet, &mut light_state);

                let (lock, send) = &*light_state_channel_send;
                let mut light_state_send = lock.lock().unwrap();
                *light_state_send = (light_state.clone(), this_modification);
                send.notify_one();
            }
        }),
    );

    threads.0.join().unwrap();
    threads.1.join().unwrap();
}
