# Zelda

[![Crates.io][crates_icon]][crates_link] ![MIT][license]

[crates_icon]: https://img.shields.io/crates/v/zelda.svg
[crates_link]: https://crates.io/crates/zelda/
[license]: https://img.shields.io/crates/l/zelda.svg?maxAge=2592000

Zelda is message based server/client transport library for game networking, with support for reliable and unreliable messages.

## Features
* [x] Reliable messages with configurable maximum size
* [x] Unreliable messages
* [x] Encryption for reliable messages (TLS)
* [x] Message authentication for unreliable messages (not encrypted)
* [x] Thread-safe async send/receive
* [x] Thread-safe non-blocking send/receive

## Examples

To run the `echo`-example with logs:
```bash
RUST_LOG=info cargo run --example echo
```

## Simulating network conditions 

Zelda does not include a link conditioner, instead you should use a separate program such as [netem](https://wiki.linuxfoundation.org/networking/netem) to simulate link conditions.

### Using netem on linux

Setting 100ms latency with a 20ms normally distributed variation:

```bash
tc qdisc change dev <interface-name> root netem delay 100ms 20ms distribution normal
```

Check the [netem documentation](https://wiki.linuxfoundation.org/networking/netem#packet_loss) for more options.