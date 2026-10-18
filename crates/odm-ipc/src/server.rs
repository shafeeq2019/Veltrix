//! Async IPC server run by the desktop app.

use crate::framing::asynchronous::{read_message, write_message};
use crate::protocol::{AppRequest, Envelope, IpcErrorCode, Request, Response};
use crate::transport::{endpoint, token_eq, write_new_token};
use std::future::Future;
use std::io;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite};

pub type HandlerFuture = Pin<Box<dyn Future<Output = Response> + Send>>;

/// Application callback for validated, authenticated requests.
pub trait Handler: Send + Sync + 'static {
    fn handle(&self, origin: Option<String>, request: Envelope<Request>) -> HandlerFuture;
}

impl<F> Handler for F
where
    F: Fn(Option<String>, Envelope<Request>) -> HandlerFuture + Send + Sync + 'static,
{
    fn handle(&self, origin: Option<String>, request: Envelope<Request>) -> HandlerFuture {
        self(origin, request)
    }
}

/// Bind the endpoint, write a fresh token and serve until the task is dropped.
pub async fn serve(data_dir: &Path, handler: Arc<dyn Handler>) -> io::Result<()> {
    let token: Arc<str> = write_new_token(data_dir)?.into();
    let name = endpoint(data_dir);
    platform::serve(&name, token, handler).await
}

async fn handle_connection<S: AsyncRead + AsyncWrite + Unpin>(
    mut stream: S,
    token: Arc<str>,
    handler: Arc<dyn Handler>,
) {
    // The first message must arrive quickly; slow-loris peers are dropped.
    let msg = match tokio::time::timeout(Duration::from_secs(10), read_message::<_, AppRequest>(&mut stream)).await {
        Ok(Ok(m)) => m,
        Ok(Err(e)) => {
            let resp = Envelope::new("invalid", Response::error(IpcErrorCode::BadRequest, format!("bad message: {e}")));
            let _ = write_message(&mut stream, &resp).await;
            return;
        }
        Err(_) => return,
    };
    let id = msg.request.id.clone();
    if !token_eq(&msg.token, &token) {
        let resp = Envelope::new(id, Response::error(IpcErrorCode::Unauthorized, "invalid token"));
        let _ = write_message(&mut stream, &resp).await;
        return;
    }
    let response = match msg.request.validate() {
        Ok(()) => handler.handle(msg.origin, msg.request).await,
        Err(e) => Response::error(IpcErrorCode::BadRequest, e.0),
    };
    let _ = write_message(&mut stream, &Envelope::new(id, response)).await;
}

#[cfg(unix)]
mod platform {
    use super::*;
    use tokio::net::UnixListener;

    pub async fn serve(name: &str, token: Arc<str>, handler: Arc<dyn Handler>) -> io::Result<()> {
        let path = Path::new(name);
        if path.exists() {
            // A live instance answers; a stale socket from a crash does not.
            if std::os::unix::net::UnixStream::connect(path).is_ok() {
                return Err(io::Error::new(io::ErrorKind::AddrInUse, "another instance is already listening"));
            }
            std::fs::remove_file(path)?;
        }
        let listener = UnixListener::bind(path)?;
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        }
        let my_uid = unsafe { libc::getuid() };
        loop {
            let (stream, _) = listener.accept().await?;
            match stream.peer_cred() {
                Ok(cred) if cred.uid() == my_uid => {}
                _ => continue,
            }
            tokio::spawn(handle_connection(stream, token.clone(), handler.clone()));
        }
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    use tokio::net::windows::named_pipe::ServerOptions;

    pub async fn serve(name: &str, token: Arc<str>, handler: Arc<dyn Handler>) -> io::Result<()> {
        let mut server = ServerOptions::new().first_pipe_instance(true).reject_remote_clients(true).create(name)?;
        loop {
            server.connect().await?;
            let connected = server;
            server = ServerOptions::new().reject_remote_clients(true).create(name)?;
            tokio::spawn(handle_connection(connected, token.clone(), handler.clone()));
        }
    }
}
