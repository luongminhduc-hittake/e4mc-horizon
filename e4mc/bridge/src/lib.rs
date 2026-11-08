//! e4mc's relay session, as Horizon's mod needs it: ask the broker for a
//! relay, speak its control protocol over QUIC (ALPN `quiclime`), and carry
//! each joining player's stream to a TCP socket on this computer.
//!
//! Written from the protocol `vgskye/e4mc-quiclime` implements (Apache-2.0)
//! and from e4mc's Java client (MIT, vgskye and contributors). No game types
//! live here, so the whole crate is tested with `cargo test` alone.

pub mod broker;
pub mod frame;
pub mod session;

pub use broker::{Broker, Relay};
pub use frame::{FromRelay, ToRelay};
pub use session::{Session, SessionState};
