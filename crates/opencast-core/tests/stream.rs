//! End-to-end streaming against a fake HTTP server, one per wire format.

use opencast_core::config::Config;
use opencast_core::providers::{complete, ChatEvent, ChatRequest, Message, Role, Router};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot};

/// Serve exactly one request; returns the base URL and the raw request.
async fn serve_once(status: &str, content_type: &str, body: String) -> (String, oneshot::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (tx, rx) = oneshot::channel();
    let head = format!(
        "HTTP/1.1 {status}\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        body.len()
    );
    tokio::spawn(async move {
        let (mut sock, _) = listener.accept().await.unwrap();
        let mut req = Vec::new();
        let mut buf = [0u8; 4096];
        loop {
            let n = sock.read(&mut buf).await.unwrap();
            req.extend_from_slice(&buf[..n]);
            let text = String::from_utf8_lossy(&req);
            if let Some(split) = text.find("\r\n\r\n") {
                let len = text[..split]
                    .lines()
                    .find_map(|l| {
                        l.to_lowercase()
                            .strip_prefix("content-length:")
                            .map(|v| v.trim().parse::<usize>().unwrap())
                    })
                    .unwrap_or(0);
                if req.len() >= split + 4 + len {
                    break;
                }
            }
            if n == 0 {
                break;
            }
        }
        sock.write_all(head.as_bytes()).await.unwrap();
        // Dribble the body out in small pieces to exercise chunk splitting.
        for piece in body.as_bytes().chunks(7) {
            sock.write_all(piece).await.unwrap();
            sock.flush().await.unwrap();
        }
        let _ = tx.send(String::from_utf8_lossy(&req).into_owned());
    });
    (format!("http://{addr}"), rx)
}

fn router_for(kind: &str, base: &str) -> Router {
    let cfg = Config::parse(&format!(
        "[[providers]]\nid = \"p\"\nkind = \"{kind}\"\nbase_url = \"{base}\"\napi_key_cmd = \"echo test-key\"\nmodel = \"m-1\"\n"
    ))
    .unwrap();
    let (r, w) = Router::from_config(&cfg);
    assert!(w.is_empty(), "{w:?}");
    r
}

fn req() -> ChatRequest {
    ChatRequest {
        messages: vec![Message::new(Role::System, "sys"), Message::new(Role::User, "hi")],
        ..Default::default()
    }
}

#[tokio::test]
async fn openai_compatible_stream() {
    let body = [
        r#"data: {"choices":[{"delta":{"role":"assistant"}}]}"#,
        r#"data: {"choices":[{"delta":{"content":"Hello"}}]}"#,
        r#"data: {"choices":[{"delta":{"content":", wörld"}}]}"#,
        "data: [DONE]",
    ]
    .join("\n\n")
        + "\n\n";
    let (base, seen) = serve_once("200 OK", "text/event-stream", body).await;
    let r = router_for("openai", &format!("{base}/v1"));
    let out = complete(r.get(None).unwrap().as_ref(), req()).await.unwrap();
    assert_eq!(out, "Hello, wörld");
    let raw = seen.await.unwrap();
    assert!(raw.starts_with("POST /v1/chat/completions"));
    assert!(raw.to_lowercase().contains("authorization: bearer test-key"));
    assert!(raw.contains(r#""model":"m-1""#));
}

#[tokio::test]
async fn anthropic_stream() {
    let body = concat!(
        "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{}}\n\n",
        "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hi \"}}\n\n",
        "event: ping\ndata: {\"type\":\"ping\"}\n\n",
        "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"there\"}}\n\n",
        "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
    )
    .to_string();
    let (base, seen) = serve_once("200 OK", "text/event-stream", body).await;
    let r = router_for("anthropic", &base);
    let out = complete(r.get(None).unwrap().as_ref(), req()).await.unwrap();
    assert_eq!(out, "Hi there");
    let raw = seen.await.unwrap().to_lowercase();
    assert!(raw.starts_with("post /v1/messages"));
    assert!(raw.contains("x-api-key: test-key"));
    assert!(raw.contains("anthropic-version: 2023-06-01"));
    assert!(raw.contains(r#""system":"sys""#));
}

#[tokio::test]
async fn gemini_stream_without_terminator() {
    let body = concat!(
        "data: {\"candidates\":[{\"content\":{\"role\":\"model\",\"parts\":[{\"text\":\"Bon\"}]}}]}\r\n\r\n",
        "data: {\"candidates\":[{\"content\":{\"role\":\"model\",\"parts\":[{\"text\":\"jour\"}]},\"finishReason\":\"STOP\"}]}\r\n\r\n",
    )
    .to_string();
    let (base, seen) = serve_once("200 OK", "text/event-stream", body).await;
    let r = router_for("gemini", &base);
    let out = complete(r.get(None).unwrap().as_ref(), req()).await.unwrap();
    assert_eq!(out, "Bonjour");
    let raw = seen.await.unwrap();
    assert!(raw.starts_with("POST /v1beta/models/m-1:streamGenerateContent?alt=sse"));
    assert!(raw.to_lowercase().contains("x-goog-api-key: test-key"));
}

#[tokio::test]
async fn http_errors_become_readable_events() {
    let (base, _seen) = serve_once(
        "401 Unauthorized",
        "application/json",
        r#"{"error":{"message":"Incorrect API key provided"}}"#.into(),
    )
    .await;
    let r = router_for("openai", &base);
    let (tx, mut rx) = mpsc::channel(8);
    r.get(None).unwrap().stream(req(), tx).await.unwrap();
    assert_eq!(
        rx.recv().await.unwrap(),
        ChatEvent::Error("HTTP 401: Incorrect API key provided — check the provider's API key".into())
    );
}
