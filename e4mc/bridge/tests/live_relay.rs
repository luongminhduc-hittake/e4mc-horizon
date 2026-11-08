//! Against e4mc's real relay: asks the broker for a relay, gets a domain, and
//! checks that a Minecraft client dialling that domain reaches a TCP socket
//! here — the whole tunnel, end to end.
//!
//! Network test: `cargo test --test live_relay -- --ignored --nocapture`.

use e4mc_bridge::broker::Broker;
use e4mc_bridge::session::{Session, SessionState};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::time::Duration;

/// The relay routes by the hostname in the Minecraft handshake, so a joining
/// client has to send one. This is the status-ping pair: handshake with
/// `next_state = 1`, then a status request.
fn handshake(host: &str, port: u16) -> Vec<u8> {
    let mut body = Vec::new();
    body.push(0x00); // handshake packet id
    write_varint(777, &mut body); // protocol
    write_varint(host.len(), &mut body);
    body.extend_from_slice(host.as_bytes());
    body.extend_from_slice(&port.to_be_bytes());
    body.push(0x01); // next state: status
    let mut packet = Vec::new();
    write_varint(body.len(), &mut packet);
    packet.extend(body);
    // Status request: length 1, id 0.
    packet.extend([0x01, 0x00]);
    packet
}

fn write_varint(mut value: usize, out: &mut Vec<u8>) {
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

fn read_varint(stream: &mut impl Read) -> std::io::Result<usize> {
    let mut value = 0usize;
    let mut shift = 0;
    loop {
        let mut byte = [0u8; 1];
        stream.read_exact(&mut byte)?;
        value |= ((byte[0] & 0x7f) as usize) << shift;
        if byte[0] & 0x80 == 0 {
            return Ok(value);
        }
        shift += 7;
    }
}

#[test]
#[ignore = "talks to e4mc's broker and relay over the internet"]
fn a_live_domain_carries_a_joining_client_to_the_local_port() {
    // A stand-in world: reads a status ping, answers the status JSON.
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            let json = r#"{"version":{"name":"stand-in","protocol":777},"players":{"max":8,"online":0},"description":{"text":"E4MC-BRIDGE-OK"}}"#;
            let mut body = vec![0x00];
            write_varint(json.len(), &mut body);
            body.extend_from_slice(json.as_bytes());
            let mut packet = Vec::new();
            write_varint(body.len(), &mut packet);
            packet.extend(body);
            let _ = stream.write_all(&packet);
        }
    });

    let session = Session::start(Broker::default(), Arc::new(move || Some(port)));
    let domain = session
        .wait_for_domain(Duration::from_secs(30))
        .expect("a domain from e4mc");
    println!("domain: {domain}");
    assert!(domain.contains("e4mc.link"), "{domain}");
    assert!(matches!(session.state(), SessionState::Started { .. }));

    // A host waits before anyone joins (that is what a real session does: the
    // domain shows, then a friend joins a minute later). A session that let its
    // endpoint go still answers here and the join times out instead.
    std::thread::sleep(Duration::from_secs(8));
    assert!(
        matches!(session.state(), SessionState::Started { .. }),
        "the session stopped while idle: {:?}",
        session.state()
    );

    // Join it the way Minecraft does: TCP 25565 with the domain in the
    // handshake. The relay reads that hostname and pairs us with the session.
    let mut client = std::net::TcpStream::connect((domain.as_str(), 25565)).expect("the relay");
    client
        .set_read_timeout(Some(Duration::from_secs(20)))
        .unwrap();
    client.write_all(&handshake(&domain, 25565)).expect("write");

    // The session must still be alive *after* the domain arrived: the endpoint
    // drives the connection, and a session that drops it hands out a domain
    // that routes nowhere (the relay answers "Unknown server").
    assert!(
        matches!(session.state(), SessionState::Started { .. }),
        "the session stopped: {:?}",
        session.state()
    );

    let length = read_varint(&mut client).expect("the status answer");
    let mut body = vec![0u8; length];
    client.read_exact(&mut body).expect("the status body");
    let text = String::from_utf8_lossy(&body);
    println!("status: {text}");
    assert!(text.contains("E4MC-BRIDGE-OK"), "{text}");

    session.stop();
}
