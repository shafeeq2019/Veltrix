//! Runs the real host binary the way a browser does: framed JSON on stdin,
//! framed JSON on stdout, app reachable over the local IPC endpoint.

use odm_ipc::framing;
use odm_ipc::server::{serve, HandlerFuture};
use odm_ipc::*;
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

fn run_host(data_dir: &std::path::Path, messages: &[serde_json::Value]) -> Vec<serde_json::Value> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_odm-native-host"))
        .arg("chrome-extension://testextension/")
        .env("ODM_DATA_DIR", data_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    {
        let mut stdin = child.stdin.take().unwrap();
        for m in messages {
            let body = serde_json::to_vec(m).unwrap();
            stdin.write_all(&(body.len() as u32).to_ne_bytes()).unwrap();
            stdin.write_all(&body).unwrap();
        }
    }
    let mut stdout = child.stdout.take().unwrap();
    let mut out = Vec::new();
    while let Ok(v) = framing::read_message::<_, serde_json::Value>(&mut stdout) {
        out.push(v);
    }
    child.wait().unwrap();
    out
}

#[test]
fn reports_app_not_running() {
    let dir = tempfile::tempdir().unwrap();
    let out = run_host(dir.path(), &[serde_json::json!({"v":1,"id":"p1","type":"ping"})]);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0]["type"], "error");
    assert_eq!(out[0]["code"], "app_not_running");
    assert_eq!(out[0]["id"], "p1");
}

#[tokio::test(flavor = "multi_thread")]
async fn relays_to_app_and_validates() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().to_path_buf();
    let seen_origin = Arc::new(std::sync::Mutex::new(None));
    let seen = seen_origin.clone();
    let handler: Arc<dyn odm_ipc::server::Handler> = Arc::new(move |origin: Option<String>, req: Envelope<Request>| -> HandlerFuture {
        *seen.lock().unwrap() = origin;
        Box::pin(async move {
            match req.body {
                Request::Offer(_) => Response::Decision { decision: Decision::Accepted },
                _ => Response::Pong { app_version: "t".into() },
            }
        })
    });
    let d2 = data.clone();
    tokio::spawn(async move { serve(&d2, handler).await.unwrap() });
    tokio::time::sleep(Duration::from_millis(200)).await;

    let out = tokio::task::spawn_blocking(move || {
        run_host(
            &data,
            &[
                serde_json::json!({"v":1,"id":"a","type":"ping"}),
                serde_json::json!({"v":1,"id":"b","type":"offer","url":"https://example.org/f.iso","size":5}),
                serde_json::json!({"v":1,"id":"c","type":"offer","url":"file:///etc/shadow"}),
                serde_json::json!({"v":1,"id":"d","type":"shell","cmd":"x"}),
            ],
        )
    })
    .await
    .unwrap();
    assert_eq!(out.len(), 4);
    assert_eq!(out[0]["type"], "pong");
    assert_eq!(out[1]["decision"], "accepted");
    assert_eq!(out[2]["code"], "bad_request");
    assert_eq!(out[3]["code"], "bad_request");
    assert_eq!(out[3]["id"], "d");
    assert_eq!(seen_origin.lock().unwrap().as_deref(), Some("chrome-extension://testextension/"));
}
