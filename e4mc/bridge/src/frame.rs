//! The wire format between e4mc's relay and this mod: every control message is
//! a length followed by that many bytes of JSON, from `e4mc-quiclime`'s
//! `src/proto.rs` (`kind`-tagged enums) and `src/main.rs` (the control loop).
//!
//! Kept on `std::io` so the codec is tested without a runtime or a network.

use serde::{Deserialize, Serialize};
use std::io::{self, Read};

/// The longest control message either side sends: the relay refuses anything
/// over 8192 bytes (`main.rs`, the control loop's `0..=8192` check).
pub const MAX_CONTROL_MESSAGE: usize = 8192;

/// What this mod sends: `ServerboundControlMessage` in quiclime.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "kind")]
#[serde(rename_all = "snake_case")]
pub enum ToRelay {
    ProbeCapabilities,
    RequestDomainAssignment,
    DialtoneRegisterTicket { ticket: String },
}

/// What the relay answers: `ClientboundControlMessage` in quiclime.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "kind")]
#[serde(rename_all = "snake_case")]
pub enum FromRelay {
    UnknownMessage,
    HasCapabilities { caps: Vec<String> },
    DomainAssignmentComplete { domain: String },
    RequestMessageBroadcast { message: String },
    TicketRegistered,
}

/// Writes a `VarInt` (Big Endian base 128), as Minecraft and the relay do.
pub fn write_varint(mut value: usize, out: &mut Vec<u8>) {
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

/// Reads one `VarInt`. `Ok(None)` means the reader ran out before the varint
/// ended (a stream that closes mid-message).
///
/// The relay writes its lengths as one byte (`send_control.write_all(&[len as
/// u8])`), and every message it sends is under 128 bytes, where a varint and
/// that byte are the same thing. Reading a varint therefore handles both.
pub fn read_varint<R: Read>(reader: &mut R) -> io::Result<Option<usize>> {
    let mut value = 0usize;
    let mut shift = 0u32;
    loop {
        let mut byte = [0u8; 1];
        match reader.read(&mut byte) {
            Ok(0) => return Ok(None),
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
        value |= ((byte[0] & 0x7f) as usize) << shift;
        if byte[0] & 0x80 == 0 {
            return Ok(Some(value));
        }
        shift += 7;
        if shift > 28 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "a control message's length is over five bytes",
            ));
        }
    }
}

/// One framed JSON message, `None` when the stream ended cleanly.
pub fn read_message<R: Read, T: for<'de> Deserialize<'de>>(
    reader: &mut R,
) -> Result<Option<T>, String> {
    let Some(len) = read_varint(reader).map_err(|e| format!("control stream: {e}"))? else {
        return Ok(None);
    };
    if len > MAX_CONTROL_MESSAGE {
        return Err(format!("control message of {len} bytes is too long"));
    }
    let mut buf = vec![0u8; len];
    reader
        .read_exact(&mut buf)
        .map_err(|e| format!("control message body: {e}"))?;
    serde_json::from_slice(&buf)
        .map(Some)
        .map_err(|e| format!("control message {buf:?}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// The exact bytes the relay reads: a `VarInt` length, then the JSON.
    fn framed(json: &str) -> Vec<u8> {
        let mut out = Vec::new();
        write_varint(json.len(), &mut out);
        out.extend_from_slice(json.as_bytes());
        out
    }

    #[test]
    fn a_probe_is_one_length_byte_then_its_json() {
        let bytes = serde_json::to_vec(&ToRelay::ProbeCapabilities).unwrap();
        assert_eq!(bytes, br#"{"kind":"probe_capabilities"}"#);
        let mut expected = vec![bytes.len() as u8];
        expected.extend(bytes);
        assert_eq!(framed(r#"{"kind":"probe_capabilities"}"#), expected);
    }

    #[test]
    fn a_length_over_127_uses_two_bytes() {
        let mut written = Vec::new();
        write_varint(300, &mut written);
        assert_eq!(written, vec![0xac, 0x02]);
        let mut cursor = Cursor::new(written);
        assert_eq!(read_varint(&mut cursor).unwrap(), Some(300));
        assert_eq!(cursor.position(), 2);
    }

    #[test]
    fn two_messages_back_to_back_are_two_messages() {
        let mut stream = framed(r#"{"kind":"has_capabilities","caps":[]}"#);
        stream.extend(framed(r#"{"kind":"ticket_registered"}"#));
        let mut cursor = Cursor::new(stream);
        assert_eq!(
            read_message::<_, FromRelay>(&mut cursor).unwrap(),
            Some(FromRelay::HasCapabilities { caps: vec![] })
        );
        assert_eq!(
            read_message::<_, FromRelay>(&mut cursor).unwrap(),
            Some(FromRelay::TicketRegistered)
        );
        // The stream is at its end: no third message.
        assert_eq!(read_message::<_, FromRelay>(&mut cursor).unwrap(), None);
    }

    #[test]
    fn a_truncated_length_is_the_end_of_the_stream_not_a_panic() {
        let mut cursor = Cursor::new(vec![0x80]);
        assert_eq!(read_varint(&mut cursor).unwrap(), None);
    }

    #[test]
    fn a_truncated_body_is_an_error_naming_the_body() {
        // Length 10, three bytes present.
        let mut cursor = Cursor::new(vec![10u8, b'{', b'}', b' ']);
        let err = read_message::<_, FromRelay>(&mut cursor).unwrap_err();
        assert!(err.contains("body"), "{err}");
    }

    #[test]
    fn a_body_over_the_relay_limit_is_refused_before_reading_it() {
        let mut too_long = Vec::new();
        write_varint(MAX_CONTROL_MESSAGE + 1, &mut too_long);
        let err = read_message::<_, FromRelay>(&mut Cursor::new(too_long)).unwrap_err();
        assert!(err.contains("too long"), "{err}");
    }

    #[test]
    fn the_domain_assignment_reads_back_as_its_domain() {
        let bytes = framed(r#"{"kind":"domain_assignment_complete","domain":"book-visor.ap.e4mc.link"}"#);
        let message: FromRelay = read_message(&mut Cursor::new(bytes)).unwrap().unwrap();
        assert_eq!(
            message,
            FromRelay::DomainAssignmentComplete {
                domain: "book-visor.ap.e4mc.link".into()
            }
        );
    }

    #[test]
    fn the_relays_own_capabilities_message_is_read() {
        let bytes = framed(r#"{"kind":"has_capabilities","caps":["dialtone_sidecar"]}"#);
        let message: FromRelay = read_message(&mut Cursor::new(bytes)).unwrap().unwrap();
        assert_eq!(
            message,
            FromRelay::HasCapabilities {
                caps: vec!["dialtone_sidecar".into()]
            }
        );
    }

    #[test]
    fn an_unknown_kind_from_a_newer_relay_is_an_error_not_a_panic() {
        let bytes = framed(r#"{"kind":"something_new"}"#);
        let err = read_message::<_, FromRelay>(&mut Cursor::new(bytes)).unwrap_err();
        assert!(err.contains("something_new"), "{err}");
    }
}
