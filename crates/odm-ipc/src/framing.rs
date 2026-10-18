//! Length-prefixed JSON framing.
//!
//! Native messaging (browser ↔ host) uses a 32-bit length in native byte
//! order. The local socket (host ↔ app) uses the same layout so one
//! implementation serves both.

use crate::protocol::MAX_MESSAGE_SIZE;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::io::{self, Read, Write};

#[derive(Debug, thiserror::Error)]
pub enum FrameError {
    #[error("connection closed")]
    Closed,
    #[error("message too large ({0} bytes)")]
    TooLarge(usize),
    #[error("invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
}

pub fn encode<T: Serialize>(msg: &T) -> Result<Vec<u8>, FrameError> {
    let body = serde_json::to_vec(msg)?;
    if body.len() > MAX_MESSAGE_SIZE {
        return Err(FrameError::TooLarge(body.len()));
    }
    let mut out = Vec::with_capacity(body.len() + 4);
    out.extend_from_slice(&(body.len() as u32).to_ne_bytes());
    out.extend_from_slice(&body);
    Ok(out)
}

pub fn write_message<W: Write, T: Serialize>(w: &mut W, msg: &T) -> Result<(), FrameError> {
    w.write_all(&encode(msg)?)?;
    w.flush()?;
    Ok(())
}

/// Read the raw JSON bytes of one message. `Closed` on clean EOF before a header.
pub fn read_frame<R: Read>(r: &mut R) -> Result<Vec<u8>, FrameError> {
    let mut len = [0u8; 4];
    let mut filled = 0;
    while filled < 4 {
        match r.read(&mut len[filled..]) {
            Ok(0) if filled == 0 => return Err(FrameError::Closed),
            Ok(0) => return Err(FrameError::Io(io::ErrorKind::UnexpectedEof.into())),
            Ok(n) => filled += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e.into()),
        }
    }
    let len = u32::from_ne_bytes(len) as usize;
    if len > MAX_MESSAGE_SIZE {
        return Err(FrameError::TooLarge(len));
    }
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf)?;
    Ok(buf)
}

pub fn read_message<R: Read, T: DeserializeOwned>(r: &mut R) -> Result<T, FrameError> {
    let buf = read_frame(r)?;
    Ok(serde_json::from_slice(&buf)?)
}

#[cfg(feature = "server")]
pub mod asynchronous {
    use super::*;
    use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

    pub async fn read_message<R: AsyncRead + Unpin, T: DeserializeOwned>(r: &mut R) -> Result<T, FrameError> {
        let mut len = [0u8; 4];
        match r.read_exact(&mut len).await {
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Err(FrameError::Closed),
            Err(e) => return Err(e.into()),
        }
        let len = u32::from_ne_bytes(len) as usize;
        if len > MAX_MESSAGE_SIZE {
            return Err(FrameError::TooLarge(len));
        }
        let mut buf = vec![0u8; len];
        r.read_exact(&mut buf).await?;
        Ok(serde_json::from_slice(&buf)?)
    }

    pub async fn write_message<W: AsyncWrite + Unpin, T: Serialize>(w: &mut W, msg: &T) -> Result<(), FrameError> {
        w.write_all(&encode(msg)?).await?;
        w.flush().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn roundtrip() {
        let mut buf = Vec::new();
        write_message(&mut buf, &serde_json::json!({"a": 1})).unwrap();
        write_message(&mut buf, &"second").unwrap();
        let mut c = Cursor::new(buf);
        let v: serde_json::Value = read_message(&mut c).unwrap();
        assert_eq!(v["a"], 1);
        let s: String = read_message(&mut c).unwrap();
        assert_eq!(s, "second");
        assert!(matches!(read_message::<_, String>(&mut c), Err(FrameError::Closed)));
    }

    #[test]
    fn rejects_oversized_and_truncated() {
        let mut big = (MAX_MESSAGE_SIZE as u32 + 1).to_ne_bytes().to_vec();
        big.extend_from_slice(b"{}");
        assert!(matches!(read_frame(&mut Cursor::new(big)), Err(FrameError::TooLarge(_))));
        let mut short = 10u32.to_ne_bytes().to_vec();
        short.extend_from_slice(b"{}");
        assert!(matches!(read_frame(&mut Cursor::new(short)), Err(FrameError::Io(_))));
        let mut garbage = 3u32.to_ne_bytes().to_vec();
        garbage.extend_from_slice(b"{{{");
        assert!(matches!(read_message::<_, serde_json::Value>(&mut Cursor::new(garbage)), Err(FrameError::Json(_))));
    }
}
