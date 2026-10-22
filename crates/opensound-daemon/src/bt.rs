// SPDX-License-Identifier: GPL-3.0-or-later
//! Minimal Bluetooth Classic sockets (L2CAP for SDP, RFCOMM for the control link) over libc.

use std::fs::File;
use std::io::{self, Read, Write};
use std::mem::size_of;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::time::Duration;

const AF_BLUETOOTH: libc::c_int = 31;
const BTPROTO_L2CAP: libc::c_int = 0;
const BTPROTO_RFCOMM: libc::c_int = 3;
const SDP_PSM: u16 = 1;

/// MDR v2 control service UUID (used through SDP).
pub const UUID_V2: [u8; 16] =
    [0x95, 0x6C, 0x7B, 0x26, 0xD4, 0x9A, 0x4B, 0xA8, 0xB0, 0x3F, 0xB1, 0x7D, 0x39, 0x3C, 0xB6, 0xE2];
/// Older MDR v1 control service UUID.
pub const UUID_V1: [u8; 16] =
    [0x96, 0xCC, 0x20, 0x3E, 0x50, 0x68, 0x46, 0xAD, 0xB3, 0x2D, 0xE3, 0x16, 0xF5, 0xE0, 0x69, 0xBA];
pub const UUID_V2_STR: &str = "956c7b26-d49a-4ba8-b03f-b17d393cb6e2";

#[repr(C)]
struct SockaddrRc {
    family: libc::sa_family_t,
    bdaddr: [u8; 6],
    channel: u8,
}

#[repr(C)]
struct SockaddrL2 {
    family: libc::sa_family_t,
    psm: u16,
    bdaddr: [u8; 6],
    cid: u16,
    bdaddr_type: u8,
}

/// "AA:BB:CC:DD:EE:FF" to the little-endian byte order the kernel expects.
pub fn parse_addr(s: &str) -> Option<[u8; 6]> {
    let parts: Vec<u8> = s.split(':').map(|p| u8::from_str_radix(p, 16)).collect::<Result<_, _>>().ok()?;
    let mut addr: [u8; 6] = parts.try_into().ok()?;
    addr.reverse();
    Some(addr)
}

fn invalid(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, msg)
}

fn cvt(ret: libc::c_int) -> io::Result<libc::c_int> {
    if ret < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(ret)
    }
}

fn socket(kind: libc::c_int, proto: libc::c_int) -> io::Result<OwnedFd> {
    // SAFETY: plain socket(2) call; the fd is owned by the returned OwnedFd.
    let fd = cvt(unsafe { libc::socket(AF_BLUETOOTH, kind | libc::SOCK_CLOEXEC, proto) })?;
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

fn connect<T>(fd: &OwnedFd, addr: &T) -> io::Result<()> {
    // SAFETY: `addr` is a valid repr(C) sockaddr of the given size.
    cvt(unsafe { libc::connect(fd.as_raw_fd(), addr as *const T as *const libc::sockaddr, size_of::<T>() as u32) })?;
    Ok(())
}

fn set_timeouts(fd: &OwnedFd, timeout: Option<Duration>) -> io::Result<()> {
    let d = timeout.unwrap_or_default();
    let tv = libc::timeval { tv_sec: d.as_secs() as libc::time_t, tv_usec: d.subsec_micros() as libc::suseconds_t };
    for opt in [libc::SO_RCVTIMEO, libc::SO_SNDTIMEO] {
        // SAFETY: setsockopt with a correctly sized timeval.
        cvt(unsafe {
            libc::setsockopt(
                fd.as_raw_fd(),
                libc::SOL_SOCKET,
                opt,
                &tv as *const _ as *const libc::c_void,
                size_of::<libc::timeval>() as u32,
            )
        })?;
    }
    Ok(())
}

/// Opens the RFCOMM control link. The returned file is blocking with no timeout.
pub fn rfcomm_connect(addr: &str, channel: u8) -> io::Result<File> {
    let bdaddr = parse_addr(addr).ok_or_else(|| invalid("bad Bluetooth address"))?;
    let fd = socket(libc::SOCK_STREAM, BTPROTO_RFCOMM)?;
    set_timeouts(&fd, Some(Duration::from_secs(10)))?;
    connect(&fd, &SockaddrRc { family: AF_BLUETOOTH as _, bdaddr, channel })?;
    set_timeouts(&fd, None)?;
    Ok(File::from(fd))
}

/// Unblocks any thread reading the socket.
pub fn shutdown(file: &File) {
    // SAFETY: shutdown(2) on a socket fd we own; errors are irrelevant here.
    unsafe { libc::shutdown(file.as_raw_fd(), libc::SHUT_RDWR) };
}

/// SDP ServiceSearchAttributeRequest for the ProtocolDescriptorList of `uuid`.
pub fn sdp_request(tid: u16, uuid: &[u8; 16], continuation: &[u8]) -> Vec<u8> {
    let mut params = vec![0x35, 17, 0x1C];
    params.extend(uuid);
    params.extend([0xFF, 0xFF]); // max attribute bytes
    params.extend([0x35, 3, 0x09, 0x00, 0x04]); // attribute 0x0004 ProtocolDescriptorList
    params.extend(continuation);
    let mut pdu = vec![0x06];
    pdu.extend(tid.to_be_bytes());
    pdu.extend((params.len() as u16).to_be_bytes());
    pdu.extend(params);
    pdu
}

/// Splits an SDP ServiceSearchAttributeResponse into (attribute bytes, continuation state).
pub fn sdp_response(resp: &[u8]) -> Option<(&[u8], &[u8])> {
    if resp.first() != Some(&0x07) || resp.len() < 7 {
        return None;
    }
    let count = u16::from_be_bytes([resp[5], resp[6]]) as usize;
    let data = resp.get(7..7 + count)?;
    Some((data, &resp[7 + count..]))
}

/// RFCOMM channel from a ProtocolDescriptorList: UUID16 0x0003 followed by a uint8.
pub fn rfcomm_channel(attrs: &[u8]) -> Option<u8> {
    attrs.windows(5).find(|w| w[..4] == [0x19, 0x00, 0x03, 0x08]).map(|w| w[4])
}

/// Looks up the RFCOMM channel of `uuid` on the device through SDP.
pub fn sdp_rfcomm_channel(addr: &str, uuid: &[u8; 16]) -> io::Result<Option<u8>> {
    let bdaddr = parse_addr(addr).ok_or_else(|| invalid("bad Bluetooth address"))?;
    let fd = socket(libc::SOCK_SEQPACKET, BTPROTO_L2CAP)?;
    set_timeouts(&fd, Some(Duration::from_secs(10)))?;
    connect(&fd, &SockaddrL2 { family: AF_BLUETOOTH as _, psm: SDP_PSM.to_le(), bdaddr, cid: 0, bdaddr_type: 0 })?;
    let mut sock = File::from(fd);
    let (mut attrs, mut cont, mut buf) = (Vec::new(), vec![0u8], vec![0u8; 4096]);
    for tid in 1..=16u16 {
        sock.write_all(&sdp_request(tid, uuid, &cont))?;
        let n = sock.read(&mut buf)?;
        let Some((data, next)) = sdp_response(&buf[..n]) else { return Ok(None) };
        attrs.extend(data);
        if next.first().is_none_or(|&c| c == 0) {
            break;
        }
        cont = next.to_vec();
    }
    Ok(rfcomm_channel(&attrs))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_are_reversed() {
        assert_eq!(parse_addr("12:34:56:78:9A:BC"), Some([0xBC, 0x9A, 0x78, 0x56, 0x34, 0x12]));
        assert_eq!(parse_addr("12:34:56:78:9A"), None);
        assert_eq!(parse_addr("zz:34:56:78:9A:BC"), None);
        assert_eq!(parse_addr(""), None);
    }

    #[test]
    fn sockaddr_layouts_match_the_kernel() {
        assert_eq!(size_of::<SockaddrRc>(), 10);
        assert_eq!(size_of::<SockaddrL2>(), 14);
    }

    #[test]
    fn uuid_constants_match_text_form() {
        let hex: String = UUID_V2.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(hex, UUID_V2_STR.replace('-', ""));
    }

    #[test]
    fn sdp_request_layout() {
        let pdu = sdp_request(1, &UUID_V2, &[0]);
        assert_eq!(&pdu[..5], &[0x06, 0x00, 0x01, 0x00, (pdu.len() - 5) as u8]);
        assert_eq!(&pdu[5..8], &[0x35, 17, 0x1C]);
        assert_eq!(&pdu[8..24], &UUID_V2);
        assert_eq!(*pdu.last().unwrap(), 0);
    }

    #[test]
    fn sdp_response_and_channel() {
        // ProtocolDescriptorList: L2CAP, RFCOMM channel 9
        let attrs = [
            0x35, 0x0E, 0x09, 0x00, 0x04, 0x35, 0x0C, 0x35, 0x03, 0x19, 0x01, 0x00, 0x35, 0x05, 0x19, 0x00, 0x03, 0x08,
            0x09,
        ];
        let mut resp = vec![0x07, 0x00, 0x01, 0x00, 0x00, 0x00, attrs.len() as u8];
        resp.extend(attrs);
        resp.push(0x00);
        let (data, cont) = sdp_response(&resp).unwrap();
        assert_eq!(cont, [0x00]);
        assert_eq!(rfcomm_channel(data), Some(9));
    }

    #[test]
    fn sdp_errors_and_missing_channel() {
        assert_eq!(sdp_response(&[0x01, 0, 1, 0, 2, 0, 3]), None); // error PDU
        assert_eq!(sdp_response(&[0x07, 0, 1, 0, 2, 0, 50, 1, 2]), None); // count beyond buffer
        assert_eq!(rfcomm_channel(&[0x35, 0x03, 0x19, 0x01, 0x00]), None);
    }
}
