# MDR v2 protocol notes

These are interoperability notes for the control protocol used by the WH-1000XM5. They were written for a non-profit project, because no official option exists for Linux. They were verified against a WH-1000XM5 running firmware 2.5.1. All values are hexadecimal.

## Transport

- Bluetooth Classic, RFCOMM (Serial Port Profile).
- The RFCOMM channel is found through SDP using the service UUID `956C7B26-D49A-4BA8-B03F-B17D393CB6E2` (MDR v2). Older devices use `96CC203E-5068-46ad-B32D-E316F5E069BA` (MDR v1).
- The device accepts one control connection at a time.

## Framing

```
3E | type | seq | length (4 bytes, big endian) | payload | checksum | 3C
```

| Field | Meaning |
|---|---|
| `type` | `0C` = command table 1, `0E` = command table 2, `01` = ACK |
| `seq` | Alternates between `0` and `1` |
| `checksum` | Sum of all bytes from `type` to the end of the payload, modulo 256 |

Inside a frame, the bytes `3C`, `3D` and `3E` are escaped as `3D` followed by `byte & EF`.

Each data frame is acknowledged with an empty `ACK` frame whose `seq` is `1 − received seq`. The sender waits for the ACK before sending the next frame, and retransmits if no ACK arrives.

## Commands

The payload starts with a command byte, followed by a type byte that selects the function. Each function group has `GET`, `RET` (reply), `SET` and `NTFY` (unsolicited notification) variants. `NTFY` is also sent after a successful `SET`.

| Function | Request | Reply / notification | Write |
|---|---|---|---|
| Handshake | `00 00` | `01 00 …` | — |
| Model / firmware | `04 01` / `04 02` | `05 01\|02 len ascii` | — |
| Battery | `22 00` | `23\|25 00 level charging` | — |
| Codec | `12 02` | `13\|15 02 codec` (`01` SBC, `02` AAC, `10` LDAC, `20` aptX, `21` aptX HD, `30` LC3) | — |
| NC / ambient sound | `66 17` | `67\|69 17 01 on mode voice level` | `68 17 01 on mode voice level` (mode `0` NC, `1` ambient; level `01`–`14`) |
| [NC/AMB] button | `66 30` | `67\|69 30 function` | `68 30 function` (`1` NC/AMB/Off, `2` NC/AMB, `3` NC/Off, `4` AMB/Off) |
| Equalizer + Clear Bass | `56 00` | `57\|59 00 preset 06 cb b400 b1k b2k5 b6k3 b16k` (value + 10) | `58 00 preset 06 …` |
| Equalizer band info | `5A 00` | `5B 00 06 …` (band frequencies) | — |
| Connection quality | `E6 00` | `E7\|E9 00 mode` | `E8 00 mode` (`0` sound quality, `1` stable connection) |
| Touch panel | `D6 D1` | `D7\|D9 D1 00 v` | `D8 D1 00 v` |
| Multipoint | `D6 D2` | `D7\|D9 D2 00 v` | `D8 D2 00 v` |
| Pause when taken off | `F6 01` | `F7\|F9 01 v` | `F8 01 v` |
| Speak-to-chat | `F6 0C` (capability: `F0 0C`) | `F7\|F9 0C v 01` | `F8 0C v` |
| Speak-to-chat detail | `FA 0C` | `FB\|FD 0C sensitivity timeout` | `FC 0C sensitivity timeout` (sensitivity `00` auto, `01` high, `02` low; timeout `00` short ≈5 s, `01` standard ≈15 s, `02` long ≈30 s, `03` never) |
| Automatic power off | `26 05` (options: `20 05`) | `27\|29 05 active timer` | `28 05 active 00` (`10` when taken off, `11` never) |
| Voice guidance (table 2) | `46 01` | `47\|49 01 v language` | `48 01 v` |
| Wearing (notification) | — | `A5 01 00 v 00` (`01` put on, `02` taken off) | — |
| Equalizer presets | `50 00` | `51 00 bands levels count (id namelen name…)` | `58 00 preset 00` (`00` off, `10`–`17` presets, `A0` manual, `A1`–`A2` custom). Band writes are not notified; read them back with `56 00`. |
| [NC/AMB] double / triple tap | `F6 0D` (capability: `F0 0D`) | `F7\|F9 0D 02 double triple` | `F8 0D 02 double triple` (`00` no function, `01` Spotify) |
| Power off | — | — (the link drops) | `24 03 01` (type `03` power off, `01` user request) |

Boolean values `v` use **`00` = on, `01` = off**.

`D6 D1` and `D6 D2` are generic "general settings". The device reports what each one means through `D0 D1` / `D0 D2`, which return `TOUCH_PANEL_SETTING` and `MULTIPOINT_SETTING`.

Equalizer presets `A0`–`A5` are user-editable (custom and user settings 1–5). Writing bands while another preset is active switches to `A0`.

## Confirmation alerts

Some changes disconnect or restart the headphones, such as multipoint, the touch panel or connection quality. The device asks the controlling host for confirmation before applying them:

1. After connecting, the host enables alerts with `94 00 00`.
2. When a change needs confirmation, the device sends `99 00 <message> <kind>`. Kind `00` is informational only; kind `01` expects yes or no.
3. The host replies `98 00 <message> <01 yes | 00 no>`.

If alerts are not enabled, the device silently rejects these changes and notifies the previous value.

Message IDs seen on the WH-1000XM5:

| ID | Meaning |
|---|---|
| `07` | Changing multipoint reconnects the headphones |
| `0A` | Disabling the touch panel |
