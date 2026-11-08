//! Headless check: connect to the first keyboard found and set a tone.
//! Run with: OPEN_ONE_PIANO_DEBUG=1 cargo run --example probe -- <msb> <lsb> <program>
use open_one_piano::ble::{self, Command, Event};
use open_one_piano::protocol::Tone;
use std::time::Duration;

fn main() {
    let a: Vec<u8> = std::env::args()
        .skip(1)
        .filter_map(|x| x.parse().ok())
        .collect();
    let tone = if a.len() == 3 {
        Tone {
            msb: a[0],
            lsb: a[1],
            prog: a[2],
        }
    } else {
        Tone {
            msb: 16,
            lsb: 0,
            prog: 4,
        }
    };
    let (tx, rx) = std::sync::mpsc::channel();
    let cmd = ble::spawn(tx, || {});
    cmd.send(Command::Scan).unwrap();
    loop {
        match rx.recv_timeout(Duration::from_secs(20)) {
            Ok(Event::Found(d)) => {
                println!("found {}", d.name);
                cmd.send(Command::Connect(d)).unwrap();
            }
            Ok(Event::Connected(n)) => {
                println!("connected {n}");
                std::thread::sleep(Duration::from_millis(800));
                cmd.send(Command::SetTone(tone)).unwrap();
            }
            Ok(Event::Settings(s)) => {
                println!("settings {s:?}");
                if s.main == tone {
                    println!("OK tone applied: {tone:?}");
                    break;
                }
            }
            Ok(Event::Error(e)) => println!("error {e}"),
            Ok(Event::Status(s)) => println!("status {s}"),
            Ok(_) => {}
            Err(_) => {
                println!("timeout");
                break;
            }
        }
    }
    cmd.send(Command::Disconnect).unwrap();
    std::thread::sleep(Duration::from_millis(500));
}
