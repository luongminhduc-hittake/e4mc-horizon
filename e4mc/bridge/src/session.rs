//! The relay session: one QUIC connection to the relay, whose control stream
//! carries `kind`-tagged JSON and whose bidirectional streams *are* the
//! joining players.
//!
//! The relay (`e4mc-quiclime`) takes the connection, reads the control stream,
//! and for every Minecraft client that dials the assigned domain it opens a
//! fresh bidirectional stream here. Bridging each of those to a local TCP
//! socket is all the tunnelling there is.

use crate::broker::{Broker, Relay};
use crate::frame::{write_varint, FromRelay, ToRelay, MAX_CONTROL_MESSAGE};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;
use tokio::io::AsyncWriteExt;

/// The ALPN the relay requires on the QUIC handshake.
pub const ALPN: &[u8] = b"quiclime";

/// What the session is doing, for the game thread to show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionState {
    Stopped,
    Starting,
    Started { domain: String },
}

/// How long each wait of the session may take.
#[derive(Debug, Clone, Copy)]
pub struct Timeouts {
    /// The relay may take this long to answer a control message.
    pub domain: Duration,
    /// e4mc's own QUIC connect timeout.
    pub connect: Duration,
}

impl Default for Timeouts {
    fn default() -> Self {
        Self {
            domain: Duration::from_secs(15),
            connect: Duration::from_secs(15),
        }
    }
}

/// A running relay session, seen from the game thread.
pub struct Session {
    state: Arc<std::sync::Mutex<SessionState>>,
    stop: Arc<AtomicBool>,
    /// The domain, or the error that ended the attempt, as soon as it is known.
    events: Receiver<Result<String, String>>,
    join: Option<std::thread::JoinHandle<()>>,
    relay: Relay,
}

impl Session {
    /// Dials the broker's relay on a thread of its own and asks for a domain.
    ///
    /// `local_port` is asked for every joining player, so a LAN port that
    /// changes is picked up without restarting the session.
    pub fn start(
        broker: Broker,
        local_port: Arc<dyn Fn() -> Option<u16> + Send + Sync>,
    ) -> Session {
        Session::launch(broker, local_port, Timeouts::default())
    }

    #[doc(hidden)]
    pub fn launch(
        broker: Broker,
        local_port: Arc<dyn Fn() -> Option<u16> + Send + Sync>,
        timeouts: Timeouts,
    ) -> Session {
        let relay = broker.relay();
        let state = Arc::new(std::sync::Mutex::new(SessionState::Starting));
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, events) = mpsc::channel();
        let thread_state = Arc::clone(&state);
        let thread_stop = Arc::clone(&stop);
        let thread_relay = relay.clone();
        let join = std::thread::Builder::new()
            .name("e4mc-relay".into())
            .spawn(move || {
                run(
                    thread_relay,
                    local_port,
                    thread_state,
                    thread_stop,
                    tx,
                    timeouts,
                )
            })
            .ok();
        Session {
            state,
            stop,
            events,
            join,
            relay,
        }
    }

    pub fn state(&self) -> SessionState {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn relay(&self) -> &Relay {
        &self.relay
    }

    /// The domain, once the relay has given one.
    pub fn domain(&self) -> Option<String> {
        match self.state() {
            SessionState::Started { domain } => Some(domain),
            _ => None,
        }
    }

    /// Blocks until the domain arrives, or the error that stopped the attempt.
    pub fn wait_for_domain(&self, timeout: Duration) -> Result<String, String> {
        match self.events.recv_timeout(timeout) {
            Ok(result) => result,
            Err(_) => Err("e4mc: no domain from the relay in time".into()),
        }
    }

    /// Ends the session. The relay drops the domain once the connection closes.
    pub fn stop(mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
        *self.state.lock().unwrap_or_else(|e| e.into_inner()) = SessionState::Stopped;
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

/// The session thread: a tokio runtime of its own, then the relay loop.
fn run(
    relay: Relay,
    local_port: Arc<dyn Fn() -> Option<u16> + Send + Sync>,
    state: Arc<std::sync::Mutex<SessionState>>,
    stop: Arc<AtomicBool>,
    events: Sender<Result<String, String>>,
    timeouts: Timeouts,
) {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(e) => {
            let _ = events.send(Err(format!("e4mc: no runtime: {e}")));
            *state.lock().unwrap_or_else(|x| x.into_inner()) = SessionState::Stopped;
            return;
        }
    };
    let outcome = runtime.block_on(serve(
        &relay,
        local_port,
        &state,
        &stop,
        &events,
        timeouts,
    ));
    // The state changes before the event: whoever reads the error must find
    // the session already stopped, or it would wait on a session that is gone.
    *state.lock().unwrap_or_else(|x| x.into_inner()) = SessionState::Stopped;
    if let Err(message) = outcome {
        let _ = events.send(Err(message));
    }
}

async fn serve(
    relay: &Relay,
    local_port: Arc<dyn Fn() -> Option<u16> + Send + Sync>,
    state: &Arc<std::sync::Mutex<SessionState>>,
    stop: &Arc<AtomicBool>,
    events: &Sender<Result<String, String>>,
    timeouts: Timeouts,
) -> Result<(), String> {
    let connection = connect(relay, timeouts.connect).await?;
    let (mut control_send, mut control_recv) = connection.open_bi().await.map_err(|e| {
        format!(
            "e4mc: the relay {}:{} opened no control stream: {e}",
            relay.host, relay.port
        )
    })?;

    // e4mc's handshake: capabilities, then a domain.
    write_control(&mut control_send, &ToRelay::ProbeCapabilities).await?;
    // The answer is the relay's capabilities. A relay too old to know the
    // probe still answers a domain, so its silence is not fatal.
    let _ = read_control(&mut control_recv, timeouts.domain).await;

    write_control(&mut control_send, &ToRelay::RequestDomainAssignment).await?;
    let domain = loop {
        match read_control(&mut control_recv, timeouts.domain)
            .await
            .map_err(|e| format!("e4mc: no domain: {e}"))?
        {
            Some(Some(FromRelay::DomainAssignmentComplete { domain })) => break domain,
            Some(Some(_)) => continue,
            Some(None) => return Err("e4mc: the relay closed the control stream".into()),
            None => return Err("e4mc: no answer from the relay in time".into()),
        }
    };

    *state.lock().unwrap_or_else(|x| x.into_inner()) = SessionState::Started {
        domain: domain.clone(),
    };
    let _ = events.send(Ok(domain));

    // From here the relay pushes one stream per joining player. The control
    // stream is drained on its own task so the loop below only waits for
    // players; losing it ends the whole session, because a relay that closed
    // it has forgotten the domain.
    let (control_closed_tx, mut control_closed_rx) = tokio::sync::mpsc::channel::<()>(1);
    tokio::spawn(async move {
        // A broadcast (the relay's `/broadcast`) or an unknown kind: the game
        // has nothing to do with either, so read past them.
        while let Ok(Some(_)) = read_control(&mut control_recv, Duration::ZERO).await {
        }
        let _ = control_closed_tx.send(()).await;
    });

    loop {
        if stop.load(Ordering::SeqCst) {
            break;
        }
        // `try_recv` first: a closed control stream must not be missed while
        // waiting for a player that never comes.
        if control_closed_rx.try_recv().is_ok() {
            break;
        }
        // A bounded wait, so `Session::stop` is noticed: a session with no
        // players would otherwise sit in `accept_bi` until the relay hung up.
        let Some(player) = wait_for_player(&connection).await else {
            continue;
        };
        let (send, recv) = player;
        // A joining player with the LAN closed under us gets a stream that
        // ends at once, which their client reads as a disconnect.
        let Some(port) = local_port() else {
            continue;
        };
        tokio::spawn(async move {
            if let Err(e) = bridge(send, recv, port).await {
                log::debug!("e4mc: a joining player's stream ended: {e}");
            }
        });
    }
    connection.close(0u32.into(), b"e4mc stopped");
    Ok(())
}

/// One control message, `None` when `limit` ran out first.
async fn read_control(
    recv: &mut quinn::RecvStream,
    limit: Duration,
) -> Result<Option<Option<FromRelay>>, String> {
    let read = read_message_async(recv);
    if limit.is_zero() {
        return read.await.map(Some);
    }
    match tokio::time::timeout(limit, read).await {
        Ok(result) => result.map(Some),
        Err(_) => Ok(None),
    }
}

/// A framed JSON message; `None` when the relay closed the stream cleanly.
async fn read_message_async(recv: &mut quinn::RecvStream) -> Result<Option<FromRelay>, String> {
    let mut length = 0usize;
    let mut shift = 0u32;
    loop {
        let mut byte = [0u8; 1];
        match recv.read(&mut byte).await {
            Ok(None) | Ok(Some(0)) => {
                return if shift == 0 {
                    Ok(None)
                } else {
                    Err("control message's length was cut off".into())
                };
            }
            Ok(Some(_)) => {}
            Err(e) => return Err(format!("control stream: {e}")),
        }
        length |= ((byte[0] & 0x7f) as usize) << shift;
        if byte[0] & 0x80 == 0 {
            break;
        }
        shift += 7;
        if shift > 28 {
            return Err("control message's length is over five bytes".into());
        }
    }
    if length > MAX_CONTROL_MESSAGE {
        return Err(format!("control message of {length} bytes is too long"));
    }
    let mut buf = vec![0u8; length];
    recv.read_exact(&mut buf)
        .await
        .map_err(|e| format!("control message body: {e}"))?;
    serde_json::from_slice(&buf)
        .map(Some)
        .map_err(|e| format!("control message: {e}"))
}

/// The next joining player, `None` when none arrived within a tick of the
/// stop flag's clock.
async fn wait_for_player(connection: &quinn::Connection) -> Option<(quinn::SendStream, quinn::RecvStream)> {
    match tokio::time::timeout(Duration::from_millis(250), connection.accept_bi()).await {
        Ok(Ok(pair)) => Some(pair),
        Ok(Err(_)) => None,
        Err(_) => None,
    }
}

async fn write_control(send: &mut quinn::SendStream, message: &ToRelay) -> Result<(), String> {
    let json = serde_json::to_vec(message).map_err(|e| format!("control message: {e}"))?;
    if json.len() > MAX_CONTROL_MESSAGE {
        return Err("control message too long".into());
    }
    let mut framed = Vec::with_capacity(json.len() + 2);
    write_varint(json.len(), &mut framed);
    framed.extend_from_slice(&json);
    send.write_all(&framed)
        .await
        .map_err(|e| format!("control stream: {e}"))
}

async fn connect(relay: &Relay, connect_timeout: Duration) -> Result<quinn::Connection, String> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let mut crypto = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_protocol_versions(&[&rustls::version::TLS13])
    .map_err(|e| format!("e4mc: TLS: {e}"))?
    .with_root_certificates(roots)
    .with_no_client_auth();
    crypto.alpn_protocols = vec![ALPN.to_vec()];
    let client = quinn::ClientConfig::new(Arc::new(
        quinn::crypto::rustls::QuicClientConfig::try_from(crypto)
            .map_err(|e| format!("e4mc: QUIC: {e}"))?,
    ));
    let mut endpoint = quinn::Endpoint::client("0.0.0.0:0".parse().map_err(|e| format!("{e}"))?)
        .map_err(|e| format!("e4mc: no UDP socket: {e}"))?;
    endpoint.set_default_client_config(client);

    let address = tokio::net::lookup_host((relay.host.as_str(), relay.port))
        .await
        .map_err(|e| format!("e4mc: {}: {e}", relay.host))?
        .find(|address| address.is_ipv4())
        .ok_or_else(|| format!("e4mc: {} has no IPv4 address", relay.host))?;
    let connecting = endpoint
        .connect(address, relay.host.as_str())
        .map_err(|e| format!("e4mc: {}: {e}", relay.host))?;
    match tokio::time::timeout(connect_timeout, connecting).await {
        Ok(Ok(connection)) => Ok(connection),
        Ok(Err(e)) => Err(format!(
            "e4mc: {}:{} didn't answer: {e}",
            relay.host, relay.port
        )),
        Err(_) => Err(format!(
            "e4mc: {}:{} timed out",
            relay.host, relay.port
        )),
    }
}

/// One joining player: the relay's stream on one side, a TCP connection to the
/// world's LAN port on the other.
async fn bridge(
    send: quinn::SendStream,
    recv: quinn::RecvStream,
    port: u16,
) -> std::io::Result<()> {
    let tcp = tokio::net::TcpStream::connect(("127.0.0.1", port)).await?;
    // `copy_bidirectional` ends both sides when either closes, and half-closes
    // the other, which is what a Minecraft connection needs on a kick.
    let (mut tcp_read, mut tcp_write) = tcp.into_split();
    let (mut quic_send, mut quic_recv) = (send, recv);
    // Each direction ends on its own; the first to finish closes the other's
    // side. `tokio::join!` is a macro, and `tokio-macros` is not in Horizon's
    // lock, so the two are driven by polling them in one `poll_fn` below.
    let up = async {
        let copied = tokio::io::copy(&mut quic_recv, &mut tcp_write).await;
        let _ = tcp_write.shutdown().await;
        copied.map(|_| ())
    };
    let down = async {
        let copied = tokio::io::copy(&mut tcp_read, &mut quic_send).await;
        let _ = quic_send.finish();
        copied.map(|_| ())
    };
    join2(up, down).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::broker::Broker;

    /// A port supplier that always answers 25565.
    fn fixed_port(port: u16) -> Arc<dyn Fn() -> Option<u16> + Send + Sync> {
        Arc::new(move || Some(port))
    }

    /// Waits a test can afford: a closed loopback port fails in a blink.
    fn quick() -> super::Timeouts {
        super::Timeouts {
            domain: Duration::from_millis(300),
            connect: Duration::from_millis(500),
        }
    }

    #[test]
    fn a_session_nobody_answers_reports_an_error_and_stops() {
        let broker = Broker {
            use_broker: false,
            url: String::new(),
            // A closed port on the loopback: the QUIC handshake never starts.
            fallback_host: "127.0.0.1".into(),
            fallback_port: 9,
        };
        let session = Session::launch(broker, fixed_port(25565), quick());
        let result = session.wait_for_domain(Duration::from_secs(5));
        assert!(result.is_err(), "{result:?}");
        assert_eq!(session.state(), SessionState::Stopped);
    }

    #[test]
    fn a_session_that_never_started_has_no_domain() {
        let broker = Broker {
            use_broker: false,
            url: String::new(),
            fallback_host: "127.0.0.1".into(),
            fallback_port: 9,
        };
        let session = Session::launch(broker, fixed_port(25565), quick());
        assert_eq!(session.domain(), None);
    }
}

/// Awaits two futures at once without `tokio::join!` (a macro, and
/// `tokio-macros` is not in Horizon's lock). Both are driven on the same
/// task; the first error wins, and both are always polled to completion so a
/// half-closed stream still flushes what it got.
async fn join2<A, B>(a: A, b: B) -> std::io::Result<()>
where
    A: std::future::Future<Output = std::io::Result<()>>,
    B: std::future::Future<Output = std::io::Result<()>>,
{
    use std::future::Future;
    use std::pin::Pin;
    use std::task::Poll;

    let mut a = std::pin::pin!(a);
    let mut b = std::pin::pin!(b);
    let (mut a_done, mut b_done) = (None, None);
    std::future::poll_fn(|cx| {
        if a_done.is_none() {
            if let Poll::Ready(result) = Pin::new(&mut a).poll(cx) {
                a_done = Some(result);
            }
        }
        if b_done.is_none() {
            if let Poll::Ready(result) = Pin::new(&mut b).poll(cx) {
                b_done = Some(result);
            }
        }
        if a_done.is_some() && b_done.is_some() {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    })
    .await;

    match a_done {
        Some(Err(e)) => Err(e),
        _ => match b_done {
            Some(Err(e)) => Err(e),
            _ => Ok(()),
        },
    }
}
