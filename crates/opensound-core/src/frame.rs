// SPDX-License-Identifier: GPL-3.0-or-later
//! MDR frame codec: `3E type seq len32 payload checksum 3C`, with byte escaping.
//! See docs/PROTOCOL.md.

pub const START: u8 = 0x3E;
pub const END: u8 = 0x3C;
pub const ESC: u8 = 0x3D;

pub const DT_ACK: u8 = 0x01;
pub const DT_MDR: u8 = 0x0C;
pub const DT_MDR2: u8 = 0x0E;

/// A decoded frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub data_type: u8,
    pub seq: u8,
    pub payload: Vec<u8>,
}

pub fn escape(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    for &b in data {
        if matches!(b, START | END | ESC) {
            out.extend([ESC, b & 0xEF]);
        } else {
            out.push(b);
        }
    }
    out
}

/// Returns `None` when an escape byte is the last byte.
pub fn unescape(data: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(data.len());
    let mut it = data.iter();
    while let Some(&b) = it.next() {
        out.push(if b == ESC { it.next()? | 0x10 } else { b });
    }
    Some(out)
}

fn checksum(data: &[u8]) -> u8 {
    data.iter().fold(0u8, |acc, &b| acc.wrapping_add(b))
}

pub fn encode(data_type: u8, seq: u8, payload: &[u8]) -> Vec<u8> {
    let mut body = Vec::with_capacity(payload.len() + 7);
    body.extend([data_type, seq]);
    body.extend((payload.len() as u32).to_be_bytes());
    body.extend(payload);
    body.push(checksum(&body));
    let mut out = vec![START];
    out.extend(escape(&body));
    out.push(END);
    out
}

/// Decodes the bytes between START and END. Returns `None` on bad escaping, checksum or length.
pub fn decode(raw: &[u8]) -> Option<Frame> {
    let body = unescape(raw)?;
    if body.len() < 7 {
        return None;
    }
    let (content, sum) = body.split_at(body.len() - 1);
    if checksum(content) != sum[0] {
        return None;
    }
    let len = u32::from_be_bytes(content[2..6].try_into().ok()?) as usize;
    let payload = content.get(6..6 + len)?.to_vec();
    Some(Frame { data_type: content[0], seq: content[1], payload })
}

/// ACK frame acknowledging a received frame with sequence `seq`.
pub fn ack(seq: u8) -> Vec<u8> {
    encode(DT_ACK, 1 - (seq & 1), &[])
}

/// Streaming decoder: feed arbitrary chunks from the socket, get whole valid frames back.
#[derive(Default)]
pub struct FrameReader {
    buf: Vec<u8>,
    in_frame: bool,
}

impl FrameReader {
    pub fn push(&mut self, chunk: &[u8]) -> Vec<Frame> {
        let mut frames = Vec::new();
        for &b in chunk {
            if !self.in_frame {
                self.in_frame = b == START;
            } else if b == END {
                frames.extend(decode(&self.buf));
                self.buf.clear();
                self.in_frame = false;
            } else if b == START {
                self.buf.clear(); // a new frame started before the old one ended: drop the broken one
            } else {
                self.buf.push(b);
            }
        }
        frames
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_handshake_frame() {
        assert_eq!(encode(DT_MDR, 0, &[0, 0]), [0x3e, 0x0c, 0, 0, 0, 0, 0x02, 0, 0, 0x0e, 0x3c]);
    }

    #[test]
    fn escape_roundtrip_every_byte() {
        let all: Vec<u8> = (0..=255).collect();
        let esc = escape(&all);
        assert!(!esc.iter().any(|&b| b == START || b == END));
        assert_eq!(unescape(&esc).unwrap(), all);
    }

    #[test]
    fn encode_decode_roundtrip_with_special_bytes() {
        for seq in [0, 1] {
            let payload = [START, ESC, END, 0x00, 0xFF, 0x68];
            let raw = encode(DT_MDR2, seq, &payload);
            assert_eq!((raw[0], *raw.last().unwrap()), (START, END));
            assert!(!raw[1..raw.len() - 1].contains(&END));
            let f = decode(&raw[1..raw.len() - 1]).unwrap();
            assert_eq!(f, Frame { data_type: DT_MDR2, seq, payload: payload.to_vec() });
        }
    }

    #[test]
    fn checksum_overflow_wraps() {
        let payload = vec![0xFF; 300];
        let raw = encode(DT_MDR, 1, &payload);
        assert_eq!(decode(&raw[1..raw.len() - 1]).unwrap().payload, payload);
    }

    #[test]
    fn rejects_corruption() {
        let raw = encode(DT_MDR, 0, &[0x22, 0x00]);
        let inner = &raw[1..raw.len() - 1];
        for i in 0..inner.len() {
            let mut bad = inner.to_vec();
            bad[i] ^= 0x01;
            assert_eq!(decode(&bad), None, "flipped byte {i}");
        }
        assert_eq!(decode(&[]), None);
        assert_eq!(decode(&[0x0c, 0x00, 0x00]), None);
        assert_eq!(unescape(&[0x01, ESC]), None);
    }

    #[test]
    fn rejects_length_beyond_body() {
        let mut body = vec![DT_MDR, 0, 0, 0, 0, 9, 0x22];
        body.push(checksum(&body));
        assert_eq!(decode(&body), None);
    }

    #[test]
    fn ack_uses_inverted_sequence() {
        assert_eq!(decode(&ack(0)[1..ack(0).len() - 1]).unwrap(), Frame { data_type: DT_ACK, seq: 1, payload: vec![] });
        assert_eq!(decode(&ack(1)[1..ack(1).len() - 1]).unwrap().seq, 0);
    }

    #[test]
    fn reader_handles_split_junk_and_multiple_frames() {
        let a = encode(DT_MDR, 0, &[0x23, 0x00, 0x3a, 0x00]);
        let b = encode(DT_MDR2, 1, &[0x47, 0x01, 0x00, 0x04]);
        let mut stream = vec![0x00, 0x11]; // noise before the first frame
        stream.extend(&a);
        stream.extend(&b);
        let mut r = FrameReader::default();
        let mut got = Vec::new();
        for chunk in stream.chunks(3) {
            got.extend(r.push(chunk));
        }
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].payload, [0x23, 0x00, 0x3a, 0x00]);
        assert_eq!(got[1].data_type, DT_MDR2);
    }

    #[test]
    fn reader_drops_corrupt_frame_and_recovers() {
        let mut bad = encode(DT_MDR, 0, &[0x22, 0x00]);
        let n = bad.len();
        bad[n - 2] ^= 0xFF;
        let good = encode(DT_MDR, 1, &[0x13, 0x02, 0x10]);
        let mut r = FrameReader::default();
        assert!(r.push(&bad).is_empty());
        assert_eq!(r.push(&good).len(), 1);
    }

    #[test]
    fn reader_resyncs_on_unterminated_frame() {
        let mut stream = vec![START, 0x0c, 0x00]; // truncated frame
        stream.extend(encode(DT_MDR, 0, &[0x22, 0x00]));
        assert_eq!(FrameReader::default().push(&stream).len(), 1);
    }
}
