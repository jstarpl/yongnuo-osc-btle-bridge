// use rosc::decoder::decode as osc_decode;
// use std::net::UdpSocket;
use clap::{App, Arg, ArgMatches, SubCommand};
use std::process::exit;
mod discover;
mod server;

const DEFAULT_TIMEOUT: &str = "10";
const DEFAULT_PORT: &str = "8000";

fn discover(matches: &ArgMatches) {
    let timeout = matches
        .value_of("timeout")
        .unwrap_or_default()
        .parse()
        .ok()
        .unwrap_or_default();
    println!("Discovering available lights... {0}s", timeout);

    let devices: Vec<discover::DeviceInfo> = discover::discover_devices(timeout);
    let devices: Vec<String> = devices
        .into_iter()
        .map(|d| {
            format!(
                "{1} ({0})",
                d.name.unwrap_or("Unknown".to_string()),
                d.address
            )
        })
        .collect();
    println!("\nFound:");
    println!("{}", devices.join("\n"));
    exit(exitcode::OK);
}

fn connect(matches: &ArgMatches) {
    let mac_address = matches.value_of("mac").unwrap_or_default();
    let port: u16 = matches
        .value_of("port")
        .unwrap_or_default()
        .parse()
        .ok()
        .unwrap_or_default();

    println!("OSC server on port {0}.", port);

    server::serve(port, mac_address);

    exit(exitcode::OK)
}

fn help(matches: &ArgMatches) {
    println!("{}", matches.usage());
    exit(exitcode::USAGE);
}

fn main() {
    let app_m = App::new("Yongnuo BTLE OSC Server")
        .version("0.0.1")
        .author("Jan Starzak <jan.starzak@gmail.com>")
        .about("Connect to a Yongnuo LED light over Bluetooth LE and control it using OSC.")
        .long_about("Connect to a Yongnuo LED light over Bluetooth LE and control it using OSC.\nSupported OSC addresses are: \\red, \\green, \\blue, \\warm, \\cool.\nAccepting single float values in range 0..1")
        .subcommand(
            SubCommand::with_name("discover")
                .about("Discover available Bluetooth LE devices")
                .arg(
                    Arg::with_name("timeout")
                        .short("t")
                        .long("timeout")
                        .takes_value(true)
                        .default_value(DEFAULT_TIMEOUT),
                ),
        )
        .subcommand(
            SubCommand::with_name("connect")
                .about("Connect to a Yongnuo Bluetooth LE device")
                .arg(
                    Arg::with_name("mac")
                        .short("m")
                        .takes_value(true)
                        .required(true)
                        .help("MAC address of the device"),
                )
                .arg(
                    Arg::with_name("port")
                        .short("p")
                        .takes_value(true)
                        .help("UDP port where the OSC server should listen for messages")
                        .default_value(DEFAULT_PORT),
                ),
        )
        .get_matches();

    match app_m.subcommand() {
        ("discover", Some(sub_m)) => discover(&sub_m),
        ("connect", Some(sub_m)) => connect(&sub_m),
        _ => help(&app_m),
    }
}
