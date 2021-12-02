Yongnuo BTLE OSC Server
===

A tiny utility that connects to a Yongnuo Studio LED Light over Bluetooth LE and exposes an OSC server allowing you
to control it over the network. Written in Rust.

Run
---
```
yongnuo-osc-server discover [-t 10]
```

Look for available Bluetooth devices and list their addresses and names, if available. By default, searches for 10s.

```
yongnuo-osc-server connect -m XX:XX:XX:XX:XX:XX [-p 8000]
```

Connect to the specified device and start the OSC server on port 8000.

Build
---
After installing [Rust tools](https://www.rust-lang.org/) do:

```
cargo build
```

Acknowledgements
---
https://github.com/kenkeiter/lantern