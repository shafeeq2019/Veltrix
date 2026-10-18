//! Host ↔ app round trip over the real local transport.
#![cfg(feature = "server")]

use odm_ipc::server::{serve, HandlerFuture};
use odm_ipc::transport::{read_token, Client};
use odm_ipc::*;
use std::sync::Arc;
use std::time::Duration;

fn handler() -> Arc<dyn odm_ipc::server::Handler> {
    Arc::new(|_origin: Option<String>, req: Envelope<Request>| -> HandlerFuture {
        Box::pin(async move {
            match req.body {
                Request::Ping => Response::Pong { app_version: "test".into() },
                Request::GetConfig => Response::Config { config: CaptureConfig::default() },
                Request::Offer(o) if o.url.contains("queue") => Response::Decision { decision: Decision::Queued },
                Request::Offer(_) => Response::Decision { decision: Decision::Accepted },
            }
        })
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn authenticated_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().to_path_buf();
    let d2 = data.clone();
    tokio::spawn(async move { serve(&d2, handler()).await.unwrap() });
    // wait for token + socket
    for _ in 0..50 {
        if read_token(&data).is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    tokio::time::sleep(Duration::from_millis(50)).await;

    let data2 = data.clone();
    let results = tokio::task::spawn_blocking(move || {
        let c = Client::new(data2);
        let pong = c.request(None, Envelope::new("p1", Request::Ping)).unwrap();
        let offer = c
            .request(
                Some("chrome-extension://abc/".into()),
                Envelope::new(
                    "o1",
                    Request::Offer(Offer { url: "https://x.org/queue.zip".into(), ..Default::default() }),
                ),
            )
            .unwrap();
        let bad = c
            .request(
                None,
                Envelope::new("o2", Request::Offer(Offer { url: "file:///etc/passwd".into(), ..Default::default() })),
            )
            .unwrap();
        (pong, offer, bad)
    })
    .await
    .unwrap();
    assert_eq!(results.0.id, "p1");
    assert!(matches!(results.0.body, Response::Pong { .. }));
    assert_eq!(results.1.body, Response::Decision { decision: Decision::Queued });
    assert!(matches!(results.2.body, Response::Error { code: IpcErrorCode::BadRequest, .. }));

    // wrong token is rejected
    let token_file = odm_ipc::transport::token_path(&data);
    let real = std::fs::read_to_string(&token_file).unwrap();
    std::fs::write(&token_file, "0".repeat(64)).unwrap();
    let data3 = data.clone();
    let denied = tokio::task::spawn_blocking(move || {
        Client::new(data3).request(None, Envelope::new("p2", Request::Ping)).unwrap()
    })
    .await
    .unwrap();
    assert!(matches!(denied.body, Response::Error { code: IpcErrorCode::Unauthorized, .. }));
    std::fs::write(&token_file, real).unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&token_file).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
}

#[test]
fn not_running_is_reported() {
    let dir = tempfile::tempdir().unwrap();
    let err = Client::new(dir.path().to_path_buf()).request(None, Envelope::new("p", Request::Ping)).unwrap_err();
    assert!(matches!(err, odm_ipc::transport::ClientError::NotRunning(_)));
}
