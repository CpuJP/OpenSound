// SPDX-License-Identifier: GPL-3.0-or-later
//! One RFCOMM control link: stop-and-wait sends with ACKs, and a reader thread for replies.

use std::fs::File;
use std::io::{self, Read, Write};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::Duration;

use opensound_core::frame::{self, FrameReader, DT_ACK};

use crate::bt;

const ACK_TIMEOUT: Duration = Duration::from_millis(1500);
const RETRIES: usize = 3;

/// Sender-side sequence tracking. The device ACKs with the next sequence number.
#[derive(Default)]
struct AckState {
    seq: u8,
    acked: bool,
}

pub struct Link {
    sock: File,
    ack: Mutex<AckState>,
    acked: Condvar,
    send_lock: Mutex<()>, // one outstanding frame at a time, as the device expects
}

impl Link {
    /// Finds the control channel through SDP and connects. `on_message` runs on the reader thread;
    /// `on_close` runs once when the link drops.
    pub fn connect(
        addr: &str,
        on_message: impl Fn(u8, Vec<u8>) + Send + 'static,
        on_close: impl FnOnce() + Send + 'static,
    ) -> io::Result<Arc<Link>> {
        let channel = match bt::sdp_rfcomm_channel(addr, &bt::UUID_V2)? {
            Some(c) => c,
            None => bt::sdp_rfcomm_channel(addr, &bt::UUID_V1)?
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "MDR service not found on device"))?,
        };
        Ok(Self::start(bt::rfcomm_connect(addr, channel)?, on_message, on_close))
    }

    /// Runs the protocol over an already connected stream socket.
    fn start(
        sock: File,
        on_message: impl Fn(u8, Vec<u8>) + Send + 'static,
        on_close: impl FnOnce() + Send + 'static,
    ) -> Arc<Link> {
        let link = Arc::new(Link { sock, ack: Mutex::default(), acked: Condvar::new(), send_lock: Mutex::default() });
        let reader = link.clone();
        thread::spawn(move || {
            reader.read_loop(on_message);
            on_close();
        });
        link
    }

    /// Sends one frame and waits for its ACK. Returns false when the device never acknowledged it.
    pub fn send(&self, data_type: u8, payload: &[u8]) -> io::Result<bool> {
        let _one_at_a_time = self.send_lock.lock().unwrap();
        for _ in 0..RETRIES {
            let mut st = self.ack.lock().unwrap();
            st.acked = false;
            (&self.sock).write_all(&frame::encode(data_type, st.seq, payload))?;
            let (st, _) = self.acked.wait_timeout_while(st, ACK_TIMEOUT, |s| !s.acked).unwrap();
            if st.acked {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub fn close(&self) {
        bt::shutdown(&self.sock);
    }

    fn read_loop(&self, on_message: impl Fn(u8, Vec<u8>)) {
        let (mut reader, mut buf, mut last_rx_seq) = (FrameReader::default(), [0u8; 4096], None);
        loop {
            let n = match (&self.sock).read(&mut buf) {
                Ok(0) | Err(_) => return,
                Ok(n) => n,
            };
            for f in reader.push(&buf[..n]) {
                if f.data_type == DT_ACK {
                    let mut st = self.ack.lock().unwrap();
                    if f.seq != st.seq {
                        st.seq = f.seq;
                        st.acked = true;
                        self.acked.notify_all();
                    }
                    continue;
                }
                if (&self.sock).write_all(&frame::ack(f.seq)).is_err() {
                    return;
                }
                if last_rx_seq == Some(f.seq) {
                    continue; // retransmission of a frame we already handled
                }
                last_rx_seq = Some(f.seq);
                on_message(f.data_type, f.payload);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opensound_core::frame::{Frame, DT_MDR, DT_MDR2};
    use std::os::fd::OwnedFd;
    use std::os::unix::net::UnixStream;
    use std::sync::mpsc;

    /// A fake headset on the other end of a socket pair.
    struct Peer {
        sock: UnixStream,
        reader: FrameReader,
        pending: std::collections::VecDeque<Frame>,
    }

    impl Peer {
        fn next(&mut self) -> Frame {
            let mut buf = [0u8; 256];
            while self.pending.is_empty() {
                let n = self.sock.read(&mut buf).unwrap();
                assert!(n > 0, "link closed");
                self.pending.extend(self.reader.push(&buf[..n]));
            }
            self.pending.pop_front().unwrap()
        }
        fn write(&mut self, data: &[u8]) {
            self.sock.write_all(data).unwrap();
        }
    }

    type Messages = mpsc::Receiver<(u8, Vec<u8>)>;

    fn setup() -> (Arc<Link>, Peer, Messages, mpsc::Receiver<()>) {
        let (a, b) = UnixStream::pair().unwrap();
        b.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let (msg_tx, msg_rx) = mpsc::channel();
        let (close_tx, close_rx) = mpsc::channel();
        let link = Link::start(
            File::from(OwnedFd::from(a)),
            move |dt, p| msg_tx.send((dt, p)).unwrap(),
            move || close_tx.send(()).unwrap(),
        );
        (link, Peer { sock: b, reader: FrameReader::default(), pending: Default::default() }, msg_rx, close_rx)
    }

    #[test]
    fn send_waits_for_ack_and_alternates_sequence() {
        let (link, mut peer, _, _) = setup();
        let sender = {
            let link = link.clone();
            thread::spawn(move || {
                (link.send(DT_MDR, &[0x22, 0x00]).unwrap(), link.send(DT_MDR, &[0x12, 0x02]).unwrap())
            })
        };
        let first = peer.next();
        assert_eq!((first.data_type, first.seq, first.payload.as_slice()), (DT_MDR, 0, &[0x22, 0x00][..]));
        peer.write(&frame::ack(first.seq));
        let second = peer.next();
        assert_eq!(second.seq, 1, "sequence advances after the ACK");
        peer.write(&frame::ack(second.seq));
        assert_eq!(sender.join().unwrap(), (true, true));
    }

    #[test]
    fn send_retransmits_without_ack_then_gives_up() {
        let (link, mut peer, _, _) = setup();
        let sender = thread::spawn(move || link.send(DT_MDR, &[0x22, 0x00]).unwrap());
        for _ in 0..RETRIES {
            assert_eq!(peer.next().payload, [0x22, 0x00]);
        }
        assert!(!sender.join().unwrap());
    }

    #[test]
    fn incoming_frames_are_acked_delivered_and_deduplicated() {
        let (_link, mut peer, messages, _) = setup();
        let notify = frame::encode(DT_MDR, 0, &[0x25, 0x00, 0x40, 0x00]);
        peer.write(&notify);
        assert_eq!(peer.next(), Frame { data_type: frame::DT_ACK, seq: 1, payload: vec![] });
        peer.write(&notify); // retransmission: ACK again, deliver once
        assert_eq!(peer.next().seq, 1);
        peer.write(&frame::encode(DT_MDR2, 1, &[0x49, 0x01, 0x00]));
        assert_eq!(peer.next().seq, 0);
        let got: Vec<_> = messages.iter().take(2).collect();
        assert_eq!(got, [(DT_MDR, vec![0x25, 0x00, 0x40, 0x00]), (DT_MDR2, vec![0x49, 0x01, 0x00])]);
        assert!(messages.recv_timeout(Duration::from_millis(100)).is_err(), "duplicate was delivered");
    }

    #[test]
    fn close_runs_callback_once() {
        let (link, _peer, _, closed) = setup();
        link.close();
        closed.recv_timeout(Duration::from_secs(2)).expect("on_close not called");
        assert!(closed.recv_timeout(Duration::from_millis(100)).is_err());
    }

    #[test]
    fn peer_disconnect_runs_callback() {
        let (_link, peer, _, closed) = setup();
        drop(peer);
        closed.recv_timeout(Duration::from_secs(2)).expect("on_close not called");
    }
}
