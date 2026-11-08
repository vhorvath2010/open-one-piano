//! The ONE keyboard protocol: Bluetooth LE MIDI framing plus the keyboard's
//! own SysEx commands (manufacturer ID 00 20 2B, product byte 69).
//!
//! See PROTOCOL.md for the full write-up.

use uuid::{Uuid, uuid};

/// Standard Bluetooth LE MIDI service and characteristic.
pub const MIDI_SERVICE: Uuid = uuid!("03b80e5a-ede8-4b33-a751-6ce34ec4c700");
pub const MIDI_CHAR: Uuid = uuid!("7772e5db-3868-4112-a1a9-f2669d106bf3");

const SYSEX_HEADER: [u8; 5] = [0xF0, 0x00, 0x20, 0x2B, 0x69];

/// Which sound layer a command targets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // Layered/Left are documented here for future split and dual-tone features
pub enum Layer {
    /// The main tone: what the keys play normally.
    Main = 0,
    /// A second tone stacked on top of the main one.
    Layered = 1,
    /// The left-hand tone when the keyboard is split.
    Left = 2,
}

/// Bank select + program for one sound.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Hash)]
pub struct Tone {
    pub msb: u8,
    pub lsb: u8,
    pub prog: u8,
}

/// Wrap one SysEx body (everything between the header and F7) in a BLE MIDI packet.
pub fn sysex_packet(body: &[u8]) -> Vec<u8> {
    let mut p = Vec::with_capacity(body.len() + 8);
    p.extend_from_slice(&[0x80, 0x80]); // BLE MIDI header + timestamp
    p.extend_from_slice(&SYSEX_HEADER);
    p.extend_from_slice(body);
    p.push(0xF7);
    p
}

/// Sent once after connecting. Until the keyboard has seen this it ignores tone commands.
pub fn hello() -> Vec<u8> {
    sysex_packet(&[0x00, 0x00, 0x55])
}

/// Ask for the current tone settings (reply: 5D 01 <data>).
pub fn query_settings() -> Vec<u8> {
    sysex_packet(&[0x1D, 0x01])
}

/// Turn a layer on or off. With the main layer off, the keys make no sound.
pub fn layer_switch(layer: Layer, on: bool) -> Vec<u8> {
    sysex_packet(&[0x06, 0x00, layer as u8, on as u8])
}

/// Set a layer's tone. Bank numbers must be the keyboard's own (from its tone list).
pub fn select_tone(layer: Layer, tone: Tone) -> Vec<u8> {
    sysex_packet(&[
        0x06,
        0x01,
        layer as u8,
        tone.msb & 0x7F,
        tone.lsb & 0x7F,
        tone.prog & 0x7F,
    ])
}

/// The parts of the settings reply we use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    pub main_on: bool,
    pub main: Tone,
}

/// Something the keyboard told us.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reply {
    Hello,
    Settings(Settings),
}

/// Pull complete SysEx messages out of a BLE MIDI notification and recognise our replies.
/// Note and controller messages from the keys are ignored.
pub fn parse_notification(packet: &[u8]) -> Vec<Reply> {
    let mut out = Vec::new();
    let mut i = 1; // skip packet header byte
    let mut sysex: Option<Vec<u8>> = None;
    while i < packet.len() {
        let b = packet[i];
        if let Some(buf) = sysex.as_mut() {
            if b & 0x80 == 0 {
                buf.push(b);
            } else if b == 0xF7 || packet.get(i + 1) == Some(&0xF7) {
                // End of SysEx (possibly preceded by a timestamp byte)
                if b != 0xF7 {
                    i += 1;
                }
                if let Some(r) = recognise(buf) {
                    out.push(r);
                }
                sysex = None;
            }
            // other high-bit bytes inside SysEx are timestamps; skip
            i += 1;
            continue;
        }
        if b & 0x80 != 0 && packet.get(i + 1) == Some(&0xF0) {
            sysex = Some(Vec::new());
            i += 2;
            continue;
        }
        i += 1;
    }
    out
}

/// `body` is the SysEx contents after F0 (no F0, no F7).
fn recognise(body: &[u8]) -> Option<Reply> {
    let rest = body.strip_prefix(&SYSEX_HEADER[1..])?;
    match rest {
        [0x40, 0x00, ..] => Some(Reply::Hello),
        // The reply lists each tone as LSB, MSB, program (the reverse of the command).
        [0x5D, 0x01, data @ ..] if data.len() >= 9 => Some(Reply::Settings(Settings {
            main_on: data[0] != 0,
            main: Tone {
                lsb: data[6],
                msb: data[7],
                prog: data[8],
            },
        })),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_hello() {
        assert_eq!(
            hello(),
            [
                0x80, 0x80, 0xF0, 0x00, 0x20, 0x2B, 0x69, 0x00, 0x00, 0x55, 0xF7
            ]
        );
    }

    #[test]
    fn builds_tone_commands() {
        let t = Tone {
            msb: 16,
            lsb: 0,
            prog: 4,
        };
        assert_eq!(
            select_tone(Layer::Main, t),
            [
                0x80, 0x80, 0xF0, 0x00, 0x20, 0x2B, 0x69, 0x06, 0x01, 0x00, 16, 0, 4, 0xF7
            ]
        );
        assert_eq!(
            layer_switch(Layer::Main, true),
            [
                0x80, 0x80, 0xF0, 0x00, 0x20, 0x2B, 0x69, 0x06, 0x00, 0x00, 0x01, 0xF7
            ]
        );
    }

    #[test]
    fn parses_real_replies() {
        // Captured from a ONE Piano COLOR.
        let hello = [
            0xB6, 0xF2, 0xF0, 0x00, 0x20, 0x2B, 0x69, 0x40, 0x00, 0x61, 0x33, 0xF2, 0xF7,
        ];
        assert_eq!(parse_notification(&hello), vec![Reply::Hello]);

        // After selecting "E.Piano / W" (bank 16/0, program 4).
        let mut settings = vec![0x80, 0x81, 0xF0, 0x00, 0x20, 0x2B, 0x69, 0x5D, 0x01];
        settings.extend([
            1, 0, 0, 100, 80, 72, 0, 16, 4, 0, 0, 48, 0, 0, 48, 54, 1, 1, 100, 12, 50, 0, 0, 0, 0,
        ]);
        settings.extend([0x81, 0xF7]);
        assert_eq!(
            parse_notification(&settings),
            vec![Reply::Settings(Settings {
                main_on: true,
                main: Tone {
                    msb: 16,
                    lsb: 0,
                    prog: 4
                }
            })]
        );
    }

    #[test]
    fn ignores_key_presses() {
        assert!(parse_notification(&[0xA1, 0x80, 0x90, 0x3E, 0x40]).is_empty());
    }
}
