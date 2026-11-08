# The ONE keyboard Bluetooth protocol

Notes on how to control a The ONE Smart Piano / ONE Light Keyboard over Bluetooth, as implemented in `src/protocol.rs`. Worked out for interoperability; not official documentation.

## Transport

The keyboard is a standard Bluetooth LE MIDI device.

| | UUID |
| --- | --- |
| MIDI service | `03b80e5a-ede8-4b33-a751-6ce34ec4c700` |
| MIDI I/O characteristic | `7772e5db-3868-4112-a1a9-f2669d106bf3` (write without response, notify) |

Every write is a BLE MIDI packet: a header byte and a timestamp byte (`80 80` works fine) followed by MIDI data. Notifications carry the keys you play (normal Note On/Off) and the keyboard's replies (SysEx).

It only allows one connection, so the phone app has to let go first.

## SysEx framing

All of the keyboard's own commands look like:

```
F0 00 20 2B 69 <command> <sub-command> <data...> F7
```

`00 20 2B` is a MIDI manufacturer ID and `69` identifies the product family.

## Commands

| Purpose | Send | Reply |
| --- | --- | --- |
| Start a session | `00 00 55` | `40 00 xx xx` |
| Ask for the current tone settings | `1D 01` | `5D 01 <settings>` |
| Turn a layer on or off | `06 00 <layer> <on>` | none |
| Set a layer's tone | `06 01 <layer> <bank MSB> <bank LSB> <program>` | none (the keyboard echoes Bank Select and Program Change) |

**Start a session first.** Until the keyboard has received `00 00 55` on the current connection, it ignores tone commands. Send it once right after connecting.

**Layers:** `0` is the main tone (what the keys normally play), `1` is a second tone stacked on the main one, and `2` is the left-hand tone when the keyboard is split.

**On/off:** `06 00 00 00` switches the main layer off, and the keys go silent. `06 00 00 01` turns it back on.

**Bank numbers are model-specific.** Each model has its own tone table, and the keyboard only accepts bank/program combinations from its own table. Anything else makes it fall back to its default piano. The tables are in `assets/tones.json`.

Example: on a ONE Piano COLOR, make sure the main layer is on and switch it to E.Piano / W (bank 16/0, program 4):

```
F0 00 20 2B 69 06 00 00 01 F7
F0 00 20 2B 69 06 01 00 10 00 04 F7
```

### Key lights

Lights use plain MIDI, not SysEx:

| Purpose | Send |
| --- | --- |
| Light one key | `A2 <light> <color>` (Polyphonic Key Pressure, MIDI channel 3) |
| Turn one key's light off | `A2 <light> 00` |
| Turn every light off | SysEx `16 02 00` |
| Turn every light on | SysEx `16 02 <color>` |

`<light>` is the key's MIDI note **minus 21**: lights are numbered from the bottom key of an 88-key piano (A0), whatever the keyboard's size. Middle C (MIDI 60) is light 39.

`<color>` is `1` for red and `2` for blue. Keyboards with single-color LEDs, like the ONE Piano COLOR, show both as red.

### Keys being played

The keyboard sends standard MIDI for what you play: Note On (`90 <note> <velocity>`, velocity 0 meaning released), Note Off (`80 <note> <velocity>`), and the sustain pedal as Control Change 64 (`B0 40 <value>`, 64 and up meaning down). Several messages often share one BLE packet using running status.

### Settings reply

The data after `5D 01` (at least 25 bytes). The fields this app uses:

| Byte | Meaning |
| --- | --- |
| 0, 1, 2 | Layer 0, 1, 2 on/off |
| 3, 4, 5 | Layer 0, 1, 2 volume |
| 6, 7, 8 | Layer 0 bank **LSB**, bank **MSB**, program |
| 9, 10, 11 | Layer 1 bank LSB, bank MSB, program |
| 12, 13, 14 | Layer 2 bank LSB, bank MSB, program |
| 15 | Split point (MIDI note) |

Note the reply lists LSB before MSB, the reverse of the `06 01` command.

### Which tone table?

The app doesn't yet know how to ask the keyboard for its model directly. Instead it reads the current tone and picks the table that contains it, since each model uses different bank numbers. A ONE Piano COLOR powers on playing bank 16/0 program 0, which only the `TOC` table has. Users can also pick the table by hand.

| Table | Seen on |
| --- | --- |
| TOC | The ONE Piano COLOR (confirmed) |
| TOK, TON, TOP, TOP3, TOPP | Other ONE models (not yet matched to specific products) |

Tones follow General MIDI program numbers, with the bank picking variations of the same instrument.
