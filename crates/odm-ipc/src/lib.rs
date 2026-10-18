//! Protocol and transport shared by the browser extension's native
//! messaging host and the desktop app.

pub mod framing;
pub mod protocol;
#[cfg(feature = "server")]
pub mod server;
pub mod transport;

pub use protocol::*;
