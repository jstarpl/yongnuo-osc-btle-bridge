use num_traits::ToPrimitive;
use rosc::decoder::decode as osc_decode;
use rosc::{OscBundle, OscMessage, OscPacket, OscType};
use std::cell::Cell;
use std::net::{SocketAddr, UdpSocket};
use std::str::FromStr;
use std::thread;
use std::time::Duration;

use btleplug::api::{BDAddr, Central, Characteristic, Peripheral as ApiPeripheral, UUID};
#[cfg(target_os = "linux")]
use btleplug::bluez::{adapter::ConnectedAdapter, manager::Manager, peripheral::Peripheral};
#[cfg(target_os = "macos")]
use btleplug::corebluetooth::{adapter::Adapter, manager::Manager, peripheral::Peripheral};
#[cfg(target_os = "windows")]
use btleplug::winrtble::{adapter::Adapter, manager::Manager, peripheral::Peripheral};

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

struct RGBState {
    red: Cell<u8>,
    green: Cell<u8>,
    blue: Cell<u8>,
}

struct WhiteState {
    warm: Cell<u8>,
    cool: Cell<u8>,
}

struct LightState {
    rgb: RGBState,
    white: WhiteState,
}

struct StateModification {
    rgb: bool,
    white: bool,
}

fn send_rgb_state(state: &LightState, light: &Peripheral, cmd_char: &Characteristic) {
    let red = state.rgb.red.get();
    let green = state.rgb.green.get();
    let blue = state.rgb.blue.get();
    println!("Sending RGB state: {0}, {1}, {2}", red, green, blue);
    let result = light.command(cmd_char, &[0xae, 0xa1, red, green, blue, 0x56]);
    if result.is_err() {
        println!("Could not send RGB state: {:#?}", result)
    }
}

fn send_white_state(state: &LightState, light: &Peripheral, cmd_char: &Characteristic) {
    let cool = state.white.cool.get();
    let warm = state.white.warm.get();
    println!("Sending White state: {0}, {1}", cool, warm);
    let result = light.command(cmd_char, &[0xae, 0xaa, 1, cool, warm, 0x56]);
    if result.is_err() {
        println!("Could not send RGB state: {:#?}", result)
    }
}

fn handle_message(message: OscMessage, state: &LightState) -> StateModification {
    println!(
        "{}: {}",
        message.addr,
        (&message.args)
            .into_iter()
            .map(|v| v.to_string())
            .collect::<Vec<String>>()
            .join(" ")
    );

    let value = (message.args).into_iter().nth(0);

    match message.addr.as_ref() {
        "/red" => {
            let basic_value = (value.unwrap().float().unwrap_or(0.0) * 255.0)
                .to_u8()
                .unwrap_or(0);
            state.rgb.red.set(basic_value);
            return StateModification {
                rgb: true,
                white: false,
            };
        }
        "/green" => {
            let basic_value = (value.unwrap().float().unwrap_or(0.0) * 255.0)
                .to_u8()
                .unwrap_or(0);
            state.rgb.green.set(basic_value);
            return StateModification {
                rgb: true,
                white: false,
            };
        }
        "/blue" => {
            let basic_value = (value.unwrap().float().unwrap_or(0.0) * 255.0)
                .to_u8()
                .unwrap_or(0);
            state.rgb.blue.set(basic_value);
            return StateModification {
                rgb: true,
                white: false,
            };
        }
        "/warm" => {
            let basic_value = (value.unwrap().float().unwrap_or(0.0) * 99.0)
                .to_u8()
                .unwrap_or(0);
            state.white.warm.set(basic_value);
            return StateModification {
                rgb: false,
                white: true,
            };
        }
        "/cool" => {
            let basic_value = (value.unwrap().float().unwrap_or(0.0) * 99.0)
                .to_u8()
                .unwrap_or(0);
            state.white.cool.set(basic_value);
            return StateModification {
                rgb: false,
                white: true,
            };
        }
        _ => {
            println!("Unsupported OSC address: {0}", message.addr);
            return StateModification {
                rgb: false,
                white: false,
            };
        }
    }
}

fn handle_bundle(bundle: OscBundle, state: &LightState) -> StateModification {
    bundle
        .content
        .into_iter()
        .map(|p| handle_packet(p, state))
        .into_iter()
        .fold(
            StateModification {
                rgb: false,
                white: false,
            },
            |acc: StateModification, item: StateModification| StateModification {
                rgb: acc.rgb | item.rgb,
                white: acc.white | item.white,
            },
        )
}

fn handle_packet(packet: OscPacket, state: &LightState) -> StateModification {
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

    let light_state: LightState = LightState {
        rgb: RGBState {
            red: Cell::new(0),
            green: Cell::new(0),
            blue: Cell::new(0),
        },
        white: WhiteState {
            warm: Cell::new(0),
            cool: Cell::new(0),
        },
    };

    let send_char_uuid = UUID::from_str("f0:00:aa:61:04:51:40:00:b0:00:00:00:00:00:00:00").unwrap();
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

    socket
        .set_read_timeout(Some(Duration::new(0, 1)))
        .expect("Could not set read timeout");

    loop {
        let mut buf = [0; 4098];
        let mut bundled_modification: StateModification = StateModification {
            rgb: false,
            white: false,
        };

        loop {
            let result = socket.recv_from(&mut buf);
            if result.is_err() {
                break;
            }
            let osc_packet = osc_decode(&buf).ok().unwrap();
            let this_modification = handle_packet(osc_packet, &light_state);
            bundled_modification.rgb = bundled_modification.rgb | this_modification.rgb;
            bundled_modification.white = bundled_modification.white | this_modification.white;
        }

        if bundled_modification.rgb {
            send_rgb_state(&light_state, &light, cmd_char)
        }
        if bundled_modification.white {
            send_white_state(&light_state, &light, cmd_char)
        }
    }
}
