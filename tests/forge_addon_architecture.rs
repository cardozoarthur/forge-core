use assert_cmd::Command;
use chrono::{Duration, Utc};
use ed25519_dalek::{Signer, SigningKey};
use forge_core::addon::load_addon_manifest_from_path;
use forge_core::artifact::hex_sha256;
use forge_core::storage::ForgeStore;
use rusqlite::Connection;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command as StdCommand, Stdio};
use std::thread;
use std::time::{Duration as StdDuration, Instant};
use tempfile::tempdir;

fn forge() -> Command {
    Command::cargo_bin("forge").expect("forge binary should build")
}

fn write_fake_event_egress_credential_vault(bin_dir: &Path) -> std::path::PathBuf {
    let script_path = bin_dir.join("credential-vault");
    fs::write(
        &script_path,
        r#"#!/bin/sh
printf '%s\n' "$@" > "$FORGE_FAKE_CREDENTIAL_VAULT_ARGS"
case "$1" in
  resolve)
    shift
    while [ "$#" -gt 0 ]; do
      case "$1" in
        --contract) contract="$2"; shift 2 ;;
        --data) data="$2"; shift 2 ;;
        --record) record="$2"; shift 2 ;;
        --field) field="$2"; shift 2 ;;
        --allow-secret-stdout) allow_secret_stdout=1; shift ;;
        --no-newline) no_newline=1; shift ;;
        *) shift ;;
      esac
    done
    [ -n "$contract" ] || exit 3
    [ -n "$data" ] || exit 4
    [ "$record" = "partner_api" ] || exit 5
    [ "$field" = "auth.token" ] || exit 6
    [ "$allow_secret_stdout" = "1" ] || exit 7
    [ "$no_newline" = "1" ] || exit 8
    printf '%s' "$FORGE_FAKE_VAULT_SECRET"
    ;;
  *)
    printf 'unexpected action: %s\n' "$1" >&2
    exit 2
    ;;
esac
"#,
    )
    .unwrap();
    let mut permissions = fs::metadata(&script_path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&script_path, permissions).unwrap();
    script_path
}

fn test_hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn test_hmac_sha256_header(secret: &str, body: &str) -> String {
    let mut key = secret.as_bytes().to_vec();
    if key.len() > 64 {
        key = Sha256::digest(&key).to_vec();
    }
    key.resize(64, 0);
    let mut outer_key_pad = vec![0x5c; 64];
    let mut inner_key_pad = vec![0x36; 64];
    for (index, byte) in key.iter().enumerate() {
        outer_key_pad[index] ^= byte;
        inner_key_pad[index] ^= byte;
    }
    let mut inner = Sha256::new();
    inner.update(&inner_key_pad);
    inner.update(body.as_bytes());
    let inner_hash = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(&outer_key_pad);
    outer.update(inner_hash);
    format!("sha256={}", test_hex_encode(&outer.finalize()))
}

fn insert_expired_event_service(store: &Path, service_id: &str, service_kind: &str) {
    let acquired_at = (Utc::now() - Duration::minutes(20)).to_rfc3339();
    let expires_at = (Utc::now() - Duration::minutes(10)).to_rfc3339();
    let heartbeat_at = (Utc::now() - Duration::minutes(11)).to_rfc3339();
    let data = serde_json::json!({
        "schema_version": "test.event_service_state.v1",
        "health": {
            "status": "running",
            "heartbeat_count": 1
        }
    });
    let connection = Connection::open(store).unwrap();
    connection
        .execute(
            r#"
            INSERT INTO event_services (
                id,
                service_kind,
                status,
                lease_owner,
                lease_id,
                lease_acquired_at,
                lease_expires_at,
                last_heartbeat_at,
                heartbeat_ttl_seconds,
                data_json
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            "#,
            rusqlite::params![
                service_id,
                service_kind,
                "running",
                format!("{service_id}-owner"),
                format!("{service_id}-lease"),
                acquired_at,
                expires_at,
                heartbeat_at,
                60_i64,
                serde_json::to_string(&data).unwrap()
            ],
        )
        .unwrap();
}

fn start_external_api_worker_server(expected_requests: usize) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!(
        "http://127.0.0.1:{}/runtime/execute",
        listener.local_addr().unwrap().port()
    );
    let handle = thread::spawn(move || {
        for _ in 0..expected_requests {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut buffer = [0u8; 1024];
            let header_end = loop {
                let read = stream.read(&mut buffer).unwrap();
                assert!(read > 0, "HTTP worker request closed before headers");
                request.extend_from_slice(&buffer[..read]);
                if let Some(index) = request.windows(4).position(|window| window == b"\r\n\r\n") {
                    break index + 4;
                }
            };
            let headers = String::from_utf8_lossy(&request[..header_end]).to_string();
            let content_length = headers
                .lines()
                .find_map(|line| {
                    line.strip_prefix("Content-Length:")
                        .or_else(|| line.strip_prefix("content-length:"))
                        .and_then(|value| value.trim().parse::<usize>().ok())
                })
                .unwrap();
            while request.len() < header_end + content_length {
                let read = stream.read(&mut buffer).unwrap();
                assert!(read > 0, "HTTP worker request closed before body");
                request.extend_from_slice(&buffer[..read]);
            }
            let body = &request[header_end..header_end + content_length];
            let worker_request: Value = serde_json::from_slice(body).unwrap();
            assert_eq!(
                worker_request["schema_version"],
                "forge.addon_runtime_worker_request.v1"
            );
            assert_eq!(worker_request["runtime"], "external_api");
            assert_eq!(worker_request["entrypoint"], "gateway.payment.charge");
            let response_body = serde_json::json!({
                "status": "completed",
                "result": {
                    "gateway_status": "authorized",
                    "payment_id": worker_request["input"]["payment_id"],
                    "entrypoint": worker_request["entrypoint"],
                },
                "attestation": {
                    "schema_version": "forge.addon_runtime_worker_attestation.v1",
                    "execution_mode": "external_api",
                    "worker_id": worker_request["worker_id"],
                    "dispatch_id": worker_request["dispatch_id"],
                    "request_schema": worker_request["schema_version"],
                }
            })
            .to_string();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            stream.write_all(response.as_bytes()).unwrap();
        }
    });
    (endpoint, handle)
}

fn start_external_api_planning_strategy_worker_server(
    expected_requests: usize,
) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!(
        "http://127.0.0.1:{}/runtime/planning-strategy",
        listener.local_addr().unwrap().port()
    );
    let handle = thread::spawn(move || {
        for _ in 0..expected_requests {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut buffer = [0u8; 1024];
            let header_end = loop {
                let read = stream.read(&mut buffer).unwrap();
                assert!(read > 0, "planner worker request closed before headers");
                request.extend_from_slice(&buffer[..read]);
                if let Some(index) = request.windows(4).position(|window| window == b"\r\n\r\n") {
                    break index + 4;
                }
            };
            let headers = String::from_utf8_lossy(&request[..header_end]).to_string();
            let content_length = headers
                .lines()
                .find_map(|line| {
                    line.strip_prefix("Content-Length:")
                        .or_else(|| line.strip_prefix("content-length:"))
                        .and_then(|value| value.trim().parse::<usize>().ok())
                })
                .unwrap();
            while request.len() < header_end + content_length {
                let read = stream.read(&mut buffer).unwrap();
                assert!(read > 0, "planner worker request closed before body");
                request.extend_from_slice(&buffer[..read]);
            }
            let body = &request[header_end..header_end + content_length];
            let worker_request: Value = serde_json::from_slice(body).unwrap();
            assert_eq!(
                worker_request["schema_version"],
                "forge.addon_runtime_worker_request.v1"
            );
            assert_eq!(worker_request["runtime"], "external_api");
            assert_eq!(worker_request["contract_type"], "planning_strategy");
            assert_eq!(worker_request["entrypoint"], "route_strategy.plan");
            let tasks =
                worker_request["input"]["context"]["core_reference"]["atomic_tasks"].clone();
            assert!(tasks.as_array().unwrap().len() >= 8);
            let response_body = serde_json::json!({
                "status": "completed",
                "result": {
                    "schema_version": "forge.addon_planning_strategy_result.v1",
                    "strategy_status": "ready_for_equivalence_audit",
                    "tasks": tasks,
                },
                "attestation": {
                    "schema_version": "forge.addon_runtime_worker_attestation.v1",
                    "execution_mode": "external_api",
                    "worker_id": worker_request["worker_id"],
                    "dispatch_id": worker_request["dispatch_id"],
                    "request_schema": worker_request["schema_version"],
                }
            })
            .to_string();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            stream.write_all(response.as_bytes()).unwrap();
        }
    });
    (endpoint, handle)
}

fn start_authenticated_external_api_worker_server(
    expected_requests: usize,
    secret: &str,
) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!(
        "http://127.0.0.1:{}/runtime/authenticated-execute",
        listener.local_addr().unwrap().port()
    );
    let expected_authorization = format!("Bearer {secret}");
    let handle = thread::spawn(move || {
        for _ in 0..expected_requests {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut buffer = [0u8; 1024];
            let header_end = loop {
                let read = stream.read(&mut buffer).unwrap();
                assert!(
                    read > 0,
                    "authenticated worker request closed before headers"
                );
                request.extend_from_slice(&buffer[..read]);
                if let Some(index) = request.windows(4).position(|window| window == b"\r\n\r\n") {
                    break index + 4;
                }
            };
            let headers = String::from_utf8_lossy(&request[..header_end]).to_string();
            assert!(headers.starts_with("POST /runtime/authenticated-execute HTTP/1.1"));
            let observed_authorization = headers
                .lines()
                .filter_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.trim()
                        .eq_ignore_ascii_case("authorization")
                        .then(|| value.trim().to_string())
                })
                .next()
                .expect("authenticated external_api worker request should include Authorization");
            assert_eq!(observed_authorization, expected_authorization);
            let content_length = headers
                .lines()
                .find_map(|line| {
                    line.strip_prefix("Content-Length:")
                        .or_else(|| line.strip_prefix("content-length:"))
                        .and_then(|value| value.trim().parse::<usize>().ok())
                })
                .unwrap();
            while request.len() < header_end + content_length {
                let read = stream.read(&mut buffer).unwrap();
                assert!(read > 0, "authenticated worker request closed before body");
                request.extend_from_slice(&buffer[..read]);
            }
            let body = &request[header_end..header_end + content_length];
            let worker_request: Value = serde_json::from_slice(body).unwrap();
            assert_eq!(
                worker_request["schema_version"],
                "forge.addon_runtime_worker_request.v1"
            );
            assert_eq!(worker_request["runtime"], "external_api");
            assert_eq!(worker_request["entrypoint"], "gateway.payment.charge");
            let response_body = serde_json::json!({
                "status": "completed",
                "result": {
                    "gateway_status": "authorized",
                    "payment_id": worker_request["input"]["payment_id"],
                    "authenticated": true
                }
            })
            .to_string();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            stream.write_all(response.as_bytes()).unwrap();
        }
    });
    (endpoint, handle)
}

fn start_event_egress_server(expected_requests: usize) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!(
        "http://127.0.0.1:{}/events/partner",
        listener.local_addr().unwrap().port()
    );
    let handle = thread::spawn(move || {
        for _ in 0..expected_requests {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut buffer = [0u8; 1024];
            let header_end = loop {
                let read = stream.read(&mut buffer).unwrap();
                assert!(read > 0, "event egress request closed before headers");
                request.extend_from_slice(&buffer[..read]);
                if let Some(index) = request.windows(4).position(|window| window == b"\r\n\r\n") {
                    break index + 4;
                }
            };
            let headers = String::from_utf8_lossy(&request[..header_end]).to_string();
            assert!(headers.starts_with("POST /events/partner HTTP/1.1"));
            let content_length = headers
                .lines()
                .find_map(|line| {
                    line.strip_prefix("Content-Length:")
                        .or_else(|| line.strip_prefix("content-length:"))
                        .and_then(|value| value.trim().parse::<usize>().ok())
                })
                .unwrap();
            while request.len() < header_end + content_length {
                let read = stream.read(&mut buffer).unwrap();
                assert!(read > 0, "event egress request closed before body");
                request.extend_from_slice(&buffer[..read]);
            }
            let body = &request[header_end..header_end + content_length];
            let event_request: Value = serde_json::from_slice(body).unwrap();
            assert_eq!(
                event_request["schema_version"],
                "forge.event_egress_request.v1"
            );
            assert_eq!(event_request["addon_id"], "forge.addon.partner");
            assert_eq!(event_request["adapter_id"], "partner.webhook_egress");
            assert_eq!(event_request["event_type"], "partner.notification");
            assert_eq!(event_request["action"], "notify_partner");
            assert_eq!(event_request["origin"], "codex");
            let response_body = serde_json::json!({
                "status": "accepted",
                "request_id": event_request["request_id"],
                "payload_id": event_request["payload"]["id"],
            })
            .to_string();
            let response = format!(
                "HTTP/1.1 202 Accepted\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            stream.write_all(response.as_bytes()).unwrap();
        }
    });
    (endpoint, handle)
}

fn start_signed_event_egress_server(
    expected_requests: usize,
    secret: &str,
    signature_header: &str,
) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!(
        "http://127.0.0.1:{}/events/signed-partner",
        listener.local_addr().unwrap().port()
    );
    let secret = secret.to_string();
    let signature_header = signature_header.to_ascii_lowercase();
    let handle = thread::spawn(move || {
        for _ in 0..expected_requests {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut buffer = [0u8; 1024];
            let header_end = loop {
                let read = stream.read(&mut buffer).unwrap();
                assert!(
                    read > 0,
                    "signed event egress request closed before headers"
                );
                request.extend_from_slice(&buffer[..read]);
                if let Some(index) = request.windows(4).position(|window| window == b"\r\n\r\n") {
                    break index + 4;
                }
            };
            let headers = String::from_utf8_lossy(&request[..header_end]).to_string();
            assert!(headers.starts_with("POST /events/signed-partner HTTP/1.1"));
            let content_length = headers
                .lines()
                .find_map(|line| {
                    line.strip_prefix("Content-Length:")
                        .or_else(|| line.strip_prefix("content-length:"))
                        .and_then(|value| value.trim().parse::<usize>().ok())
                })
                .unwrap();
            while request.len() < header_end + content_length {
                let read = stream.read(&mut buffer).unwrap();
                assert!(read > 0, "signed event egress request closed before body");
                request.extend_from_slice(&buffer[..read]);
            }
            let body = &request[header_end..header_end + content_length];
            let body_text = String::from_utf8(body.to_vec()).unwrap();
            let expected_signature = test_hmac_sha256_header(&secret, &body_text);
            let observed_signature = headers
                .lines()
                .filter_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    (name.trim().eq_ignore_ascii_case(&signature_header))
                        .then(|| value.trim().to_string())
                })
                .next()
                .expect("signed event egress request should include signature header");
            assert_eq!(observed_signature, expected_signature);
            let event_request: Value = serde_json::from_slice(body).unwrap();
            assert_eq!(
                event_request["schema_version"],
                "forge.event_egress_request.v1"
            );
            assert_eq!(event_request["auth"], "hmac");
            assert_eq!(event_request["secret_env"], "FORGE_TEST_EGRESS_SECRET");
            assert_eq!(event_request["signature_header"], "X-Partner-Signature");
            let response_body = serde_json::json!({
                "status": "accepted",
                "request_id": event_request["request_id"],
                "signed": true,
            })
            .to_string();
            let response = format!(
                "HTTP/1.1 202 Accepted\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            stream.write_all(response.as_bytes()).unwrap();
        }
    });
    (endpoint, handle)
}

fn start_bearer_event_egress_server(
    expected_requests: usize,
    secret: &str,
    expected_secret_env: Option<&str>,
) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!(
        "http://127.0.0.1:{}/events/bearer-partner",
        listener.local_addr().unwrap().port()
    );
    let expected_authorization = format!("Bearer {secret}");
    let expected_secret_env = expected_secret_env.map(str::to_string);
    let handle = thread::spawn(move || {
        for _ in 0..expected_requests {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut buffer = [0u8; 1024];
            let header_end = loop {
                let read = stream.read(&mut buffer).unwrap();
                assert!(
                    read > 0,
                    "bearer event egress request closed before headers"
                );
                request.extend_from_slice(&buffer[..read]);
                if let Some(index) = request.windows(4).position(|window| window == b"\r\n\r\n") {
                    break index + 4;
                }
            };
            let headers = String::from_utf8_lossy(&request[..header_end]).to_string();
            assert!(headers.starts_with("POST /events/bearer-partner HTTP/1.1"));
            let observed_authorization = headers
                .lines()
                .filter_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.trim()
                        .eq_ignore_ascii_case("authorization")
                        .then(|| value.trim().to_string())
                })
                .next()
                .expect("bearer event egress request should include Authorization header");
            assert_eq!(observed_authorization, expected_authorization);
            let content_length = headers
                .lines()
                .find_map(|line| {
                    line.strip_prefix("Content-Length:")
                        .or_else(|| line.strip_prefix("content-length:"))
                        .and_then(|value| value.trim().parse::<usize>().ok())
                })
                .unwrap();
            while request.len() < header_end + content_length {
                let read = stream.read(&mut buffer).unwrap();
                assert!(read > 0, "bearer event egress request closed before body");
                request.extend_from_slice(&buffer[..read]);
            }
            let body = &request[header_end..header_end + content_length];
            let event_request: Value = serde_json::from_slice(body).unwrap();
            assert_eq!(
                event_request["schema_version"],
                "forge.event_egress_request.v1"
            );
            assert_eq!(event_request["auth"], "bearer");
            match expected_secret_env.as_deref() {
                Some(secret_env) => assert_eq!(event_request["secret_env"], secret_env),
                None => assert!(event_request.get("secret_env").is_none()),
            }
            let response_body = serde_json::json!({
                "status": "accepted",
                "request_id": event_request["request_id"],
                "bearer": true,
            })
            .to_string();
            let response = format!(
                "HTTP/1.1 202 Accepted\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            stream.write_all(response.as_bytes()).unwrap();
        }
    });
    (endpoint, handle)
}

fn reserve_local_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    port
}

fn post_json_with_retry(port: u16, path: &str, body: &str) -> String {
    post_json_with_retry_headers(port, path, body, &[])
}

fn post_json_with_retry_headers(
    port: u16,
    path: &str,
    body: &str,
    headers: &[(&str, &str)],
) -> String {
    let deadline = Instant::now() + StdDuration::from_secs(5);
    loop {
        match TcpStream::connect(("127.0.0.1", port)) {
            Ok(mut stream) => {
                let extra_headers = headers
                    .iter()
                    .map(|(name, value)| format!("{name}: {value}\r\n"))
                    .collect::<String>();
                let request = format!(
                    "POST {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\n{extra_headers}Content-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.as_bytes().len(),
                    body
                );
                if let Err(error) = stream.write_all(request.as_bytes()) {
                    if Instant::now() < deadline {
                        let _ = error;
                        thread::sleep(StdDuration::from_millis(50));
                        continue;
                    }
                    panic!("webhook ingress server closed during request write: {error}");
                }
                let mut response = String::new();
                match stream.read_to_string(&mut response) {
                    Ok(_) => return response,
                    Err(error) if Instant::now() < deadline => {
                        let _ = error;
                        thread::sleep(StdDuration::from_millis(50));
                    }
                    Err(error) => {
                        panic!("webhook ingress server closed before response body: {error}")
                    }
                }
            }
            Err(error) if Instant::now() < deadline => {
                let _ = error;
                thread::sleep(StdDuration::from_millis(50));
            }
            Err(error) => panic!("webhook ingress server did not accept connection: {error}"),
        }
    }
}

fn addon_package_payload_bytes(
    manifest_path: &Path,
    package_id: &str,
    addon_id: &str,
    addon_version: &str,
    repository: &str,
    channel: &str,
) -> Vec<u8> {
    let manifest_sha256 = hex_sha256(&fs::read(manifest_path).unwrap());
    let mut manifest = load_addon_manifest_from_path(manifest_path).unwrap();
    manifest.source = format!("file:{}", manifest_path.display());
    let manifest_canonical_sha256 = hex_sha256(&serde_json::to_vec(&manifest).unwrap());
    serde_json::to_vec(&serde_json::json!({
        "schema_version": "forge.addon_package.v1",
        "package_id": package_id,
        "addon_id": addon_id,
        "addon_version": addon_version,
        "manifest_sha256": manifest_sha256,
        "manifest_canonical_sha256": manifest_canonical_sha256,
        "repository": repository,
        "channel": channel,
    }))
    .unwrap()
}

#[test]
fn addons_catalog_exposes_core_kernel_and_first_party_addons() {
    let output = forge()
        .args(["addons", "catalog", "--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["schema_version"], "forge.addon_catalog.v1");
    assert_eq!(json["status"], "loaded");
    assert!(json["capability_count"].as_u64().unwrap() >= 10);
    assert!(json["addons"]
        .as_array()
        .unwrap()
        .iter()
        .any(|addon| addon["id"] == "forge.core.kernel"));
    let core = json["addons"]
        .as_array()
        .unwrap()
        .iter()
        .find(|addon| addon["id"] == "forge.core.kernel")
        .unwrap();
    assert!(core["context_providers"]
        .as_array()
        .unwrap()
        .iter()
        .any(|provider| provider["id"] == "forge.core.operating_context"
            && provider["scopes"]
                .as_array()
                .unwrap()
                .iter()
                .any(|scope| scope == "organization")
            && provider["provides_sections"]
                .as_array()
                .unwrap()
                .iter()
                .any(|section| section == "operating_context")));
    assert!(core["memory_providers"]
        .as_array()
        .unwrap()
        .iter()
        .any(|provider| provider["id"] == "forge.core.file_memory"
            && provider["memory_levels"]
                .as_array()
                .unwrap()
                .iter()
                .any(|level| level == "MEMORY_STANDARD")));
    assert!(core["event_adapters"]
        .as_array()
        .unwrap()
        .iter()
        .any(|adapter| adapter["id"] == "forge.core.event_inbox"
            && adapter["transport"] == "forge_inbox"
            && adapter["direction"] == "ingress"));
    assert!(json["addons"]
        .as_array()
        .unwrap()
        .iter()
        .any(|addon| addon["id"] == "forge.addon.visual_workspace"));
    let workflow_automation = json["addons"]
        .as_array()
        .unwrap()
        .iter()
        .find(|addon| addon["id"] == "forge.addon.workflow_automation")
        .unwrap();
    assert!(workflow_automation["runtime_contracts"]
        .as_array()
        .unwrap()
        .iter()
        .any(
            |contract| contract["id"] == "n8n_primitive_research.planning_strategy"
                && contract["contract_type"] == "planning_strategy"
                && contract["runtime"] == "forge_core_builtin"
        ));
    let visual_workspace = json["addons"]
        .as_array()
        .unwrap()
        .iter()
        .find(|addon| addon["id"] == "forge.addon.visual_workspace")
        .unwrap();
    assert!(visual_workspace["views"]
        .as_array()
        .unwrap()
        .iter()
        .any(|view| view["id"] == "visual.workspace" && view["surface"] == "ops_console"));
}

#[test]
fn mcp_manifest_exposes_addon_capability_tools() {
    let output = forge()
        .args(["mcp", "tools", "--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_slice(&output).unwrap();
    let tools = json["tools"].as_array().unwrap();
    let events = tools
        .iter()
        .find(|tool| tool["name"] == "forge.events.list")
        .expect("event stream MCP tool should be listed");
    assert_eq!(events["output_schema"], "forge.event_stream.v1");
    assert_eq!(events["mutates_workflow"], false);
    for tool_name in [
        "forge.events.ingest",
        "forge.events.inbox",
        "forge.events.scan",
        "forge.events.worker",
        "forge.events.service_plan",
        "forge.events.service_run",
        "forge.events.service_supervise",
        "forge.events.runtime_reconcile",
        "forge.events.runtime_daemon",
        "forge.events.services",
        "forge.events.route",
    ] {
        let tool = tools
            .iter()
            .find(|tool| tool["name"] == tool_name)
            .expect("inbound event MCP tool should be listed");
        assert!(tool["output_schema"]
            .as_str()
            .unwrap()
            .starts_with("forge.event_"));
    }
    let adapters = tools
        .iter()
        .find(|tool| tool["name"] == "forge.events.adapters")
        .expect("event adapter MCP tool should be listed");
    assert_eq!(adapters["output_schema"], "forge.addon_event_adapters.v1");
    assert_eq!(adapters["mutates_workflow"], false);
    let contracts = tools
        .iter()
        .find(|tool| tool["name"] == "forge.addons.contracts")
        .expect("addon runtime contract MCP tool should be listed");
    assert_eq!(
        contracts["output_schema"],
        "forge.addon_runtime_contracts.v1"
    );
    assert_eq!(contracts["mutates_workflow"], false);
    let planners = tools
        .iter()
        .find(|tool| tool["name"] == "forge.addons.planners")
        .expect("addon planner registry MCP tool should be listed");
    assert_eq!(planners["output_schema"], "forge.addon_planner_registry.v1");
    assert_eq!(planners["mutates_workflow"], false);
    let observability = tools
        .iter()
        .find(|tool| tool["name"] == "forge.addons.observability")
        .expect("addon observability MCP tool should be listed");
    assert_eq!(
        observability["output_schema"],
        "forge.addon_observability.v1"
    );
    assert_eq!(observability["mutates_workflow"], false);
    let contract_policy = tools
        .iter()
        .find(|tool| tool["name"] == "forge.addons.contract_policy")
        .expect("addon runtime contract policy MCP tool should be listed");
    assert_eq!(
        contract_policy["output_schema"],
        "forge.addon_runtime_contract_policy.v1"
    );
    assert_eq!(contract_policy["mutates_workflow"], false);
    let dispatch_contract = tools
        .iter()
        .find(|tool| tool["name"] == "forge.addons.dispatch_contract")
        .expect("addon runtime dispatch MCP tool should be listed");
    assert_eq!(
        dispatch_contract["output_schema"],
        "forge.addon_runtime_contract_dispatch.v1"
    );
    assert_eq!(dispatch_contract["mutates_workflow"], true);
    let dispatch_planner = tools
        .iter()
        .find(|tool| tool["name"] == "forge.addons.dispatch_planner")
        .expect("addon planner dispatch MCP tool should be listed");
    assert_eq!(
        dispatch_planner["output_schema"],
        "forge.addon_runtime_contract_dispatch.v1"
    );
    assert_eq!(dispatch_planner["mutates_workflow"], true);
    let dispatches = tools
        .iter()
        .find(|tool| tool["name"] == "forge.addons.dispatches")
        .expect("addon runtime dispatch list MCP tool should be listed");
    assert_eq!(
        dispatches["output_schema"],
        "forge.addon_runtime_contract_dispatch.v1"
    );
    assert_eq!(dispatches["mutates_workflow"], false);
    let run_dispatch = tools
        .iter()
        .find(|tool| tool["name"] == "forge.addons.run_dispatch")
        .expect("addon runtime dispatch run MCP tool should be listed");
    assert_eq!(
        run_dispatch["output_schema"],
        "forge.addon_runtime_contract_dispatch.v1"
    );
    assert_eq!(run_dispatch["mutates_workflow"], true);
    let execute_dispatch = tools
        .iter()
        .find(|tool| tool["name"] == "forge.addons.execute_dispatch")
        .expect("addon runtime dispatch local execute MCP tool should be listed");
    assert_eq!(
        execute_dispatch["output_schema"],
        "forge.addon_runtime_contract_dispatch.v1"
    );
    assert_eq!(execute_dispatch["mutates_workflow"], true);
    let claim_dispatch = tools
        .iter()
        .find(|tool| tool["name"] == "forge.addons.claim_dispatch")
        .expect("addon runtime dispatch claim MCP tool should be listed");
    assert_eq!(
        claim_dispatch["output_schema"],
        "forge.addon_runtime_contract_dispatch.v1"
    );
    assert_eq!(claim_dispatch["mutates_workflow"], true);
    let complete_dispatch = tools
        .iter()
        .find(|tool| tool["name"] == "forge.addons.complete_dispatch")
        .expect("addon runtime dispatch complete MCP tool should be listed");
    assert_eq!(
        complete_dispatch["output_schema"],
        "forge.addon_runtime_contract_dispatch.v1"
    );
    assert_eq!(complete_dispatch["mutates_workflow"], true);
    let register_worker = tools
        .iter()
        .find(|tool| tool["name"] == "forge.addons.register_worker")
        .expect("addon runtime worker register MCP tool should be listed");
    assert_eq!(
        register_worker["output_schema"],
        "forge.addon_runtime_workers.v1"
    );
    assert_eq!(register_worker["mutates_workflow"], true);
    let workers = tools
        .iter()
        .find(|tool| tool["name"] == "forge.addons.workers")
        .expect("addon runtime worker list MCP tool should be listed");
    assert_eq!(workers["output_schema"], "forge.addon_runtime_workers.v1");
    assert_eq!(workers["mutates_workflow"], false);
    let views = tools
        .iter()
        .find(|tool| tool["name"] == "forge.addons.views")
        .expect("addon views MCP tool should be listed");
    assert_eq!(views["output_schema"], "forge.addon_views.v1");
    assert_eq!(views["mutates_workflow"], false);

    let identity = tools
        .iter()
        .find(|tool| tool["name"] == "forge.identity.context")
        .expect("identity context MCP tool should be listed");
    assert_eq!(identity["output_schema"], "forge.operating_context_load.v1");
    assert_eq!(identity["mutates_workflow"], false);
    let identity_registry = tools
        .iter()
        .find(|tool| tool["name"] == "forge.identity.registry")
        .expect("identity registry MCP tool should be listed");
    assert_eq!(
        identity_registry["output_schema"],
        "forge.identity_registry.v1"
    );
    let tenant_index = tools
        .iter()
        .find(|tool| tool["name"] == "forge.identity.tenant_index")
        .expect("tenant index MCP tool should be listed");
    assert_eq!(tenant_index["output_schema"], "forge.tenant_index.v1");
    let tenant_audit = tools
        .iter()
        .find(|tool| tool["name"] == "forge.identity.tenant_audit")
        .expect("tenant audit MCP tool should be listed");
    assert_eq!(tenant_audit["output_schema"], "forge.tenant_audit.v1");
    let identity_memberships = tools
        .iter()
        .find(|tool| tool["name"] == "forge.identity.memberships")
        .expect("identity memberships MCP tool should be listed");
    assert_eq!(
        identity_memberships["output_schema"],
        "forge.identity_memberships.v1"
    );
    let membership_update = tools
        .iter()
        .find(|tool| tool["name"] == "forge.identity.membership_update")
        .expect("identity membership update MCP tool should be listed");
    assert_eq!(
        membership_update["output_schema"],
        "forge.identity_membership_update.v1"
    );
    assert_eq!(membership_update["mutates_workflow"], true);
    let identity_link = tools
        .iter()
        .find(|tool| tool["name"] == "forge.identity.link")
        .expect("identity link MCP tool should be listed");
    assert_eq!(identity_link["output_schema"], "forge.identity_link.v1");
    assert_eq!(identity_link["mutates_workflow"], true);
    let identity_unlink = tools
        .iter()
        .find(|tool| tool["name"] == "forge.identity.unlink")
        .expect("identity unlink MCP tool should be listed");
    assert_eq!(identity_unlink["output_schema"], "forge.identity_link.v1");
    assert_eq!(identity_unlink["mutates_workflow"], true);
    let identity_links = tools
        .iter()
        .find(|tool| tool["name"] == "forge.identity.links")
        .expect("identity links MCP tool should be listed");
    assert_eq!(identity_links["output_schema"], "forge.identity_links.v1");
    assert_eq!(identity_links["mutates_workflow"], false);
    let identity_resolve = tools
        .iter()
        .find(|tool| tool["name"] == "forge.identity.resolve")
        .expect("identity resolve MCP tool should be listed");
    assert_eq!(
        identity_resolve["output_schema"],
        "forge.identity_resolve.v1"
    );
    assert_eq!(identity_resolve["mutates_workflow"], false);
    let tenant_policy = tools
        .iter()
        .find(|tool| tool["name"] == "forge.identity.tenant_policy")
        .expect("tenant policy MCP tool should be listed");
    assert_eq!(tenant_policy["output_schema"], "forge.tenant_policy.v1");
    let identity_sync = tools
        .iter()
        .find(|tool| tool["name"] == "forge.identity.sync")
        .expect("identity sync MCP tool should be listed");
    assert_eq!(identity_sync["output_schema"], "forge.identity_sync.v1");

    let catalog = tools
        .iter()
        .find(|tool| tool["name"] == "forge.addons.catalog")
        .expect("addon catalog MCP tool should be listed");
    assert_eq!(catalog["output_schema"], "forge.addon_catalog.v1");
    assert_eq!(catalog["mutates_workflow"], false);

    let resolve = tools
        .iter()
        .find(|tool| tool["name"] == "forge.addons.resolve")
        .expect("addon resolver MCP tool should be listed");
    assert_eq!(resolve["output_schema"], "forge.capability_resolution.v1");
    assert_eq!(
        resolve["input_schema"]["properties"]["goal"]["type"],
        "string"
    );

    let validate = tools
        .iter()
        .find(|tool| tool["name"] == "forge.addons.validate")
        .expect("addon validation MCP tool should be listed");
    assert_eq!(validate["output_schema"], "forge.addon_validation.v1");
    assert_eq!(validate["mutates_workflow"], false);

    for tool_name in [
        "forge.addons.installed",
        "forge.addons.capabilities",
        "forge.addons.permissions",
        "forge.addons.authorize_permission",
        "forge.addons.revoke_permission",
        "forge.addons.install",
        "forge.addons.package",
        "forge.addons.trust_key",
        "forge.addons.trust_store",
        "forge.addons.publish_package",
        "forge.addons.fetch_package",
        "forge.addons.sync_registry",
        "forge.addons.package_lock",
        "forge.addons.marketplace",
        "forge.addons.install_package",
        "forge.addons.migration_workflow",
        "forge.addons.upgrade",
        "forge.addons.downgrade",
        "forge.addons.enable",
        "forge.addons.disable",
        "forge.addons.uninstall",
    ] {
        let tool = tools
            .iter()
            .find(|tool| tool["name"] == tool_name)
            .expect("addon lifecycle MCP tool should be listed");
        assert!(tool["output_schema"]
            .as_str()
            .unwrap()
            .starts_with("forge."));
    }
}

#[test]
fn mcp_call_resolves_goal_capabilities_through_addon_registry() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.addons.resolve",
            "--input",
            r#"{"goal":"Criar whiteboard colaborativo com sistema de design"}"#,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["schema_version"], "forge.mcp.call.v1");
    assert_eq!(json["status"], "ok");
    assert_eq!(json["tool_name"], "forge.addons.resolve");
    let result = &json["result"];
    assert_eq!(result["schema_version"], "forge.capability_resolution.v1");
    assert!(result["required_capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .any(|capability| capability["id"] == "visual_workspace"
            && capability["source_addon"] == "forge.addon.visual_workspace"));
}

#[test]
fn capability_resolution_suggests_actions_for_missing_dependencies() {
    let temp = tempdir().unwrap();
    let addon_dir = temp.path().join("addons");
    fs::create_dir_all(&addon_dir).unwrap();
    fs::write(
        addon_dir.join("dispatch.yaml"),
        r#"
id: forge.addon.dispatch
name: Dispatch Addon
version: 0.1.0
capabilities:
  - id: fleet_dispatch
    title: Fleet dispatch
    domains: [logistics]
    keywords: [dispatch]
    requires_capabilities: [route_optimization]
"#,
    )
    .unwrap();
    fs::write(
        addon_dir.join("route.yaml"),
        r#"
id: forge.addon.route
name: Route Addon
version: 0.1.0
lifecycle: disabled
capabilities:
  - id: route_optimization
    title: Route optimization
    domains: [logistics]
    keywords: [route]
"#,
    )
    .unwrap();

    let output = forge()
        .args([
            "addons",
            "resolve",
            "--goal",
            "Criar dispatch logístico com roteirização",
            "--addon-dir",
            addon_dir.to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["schema_version"], "forge.capability_resolution.v1");
    assert_eq!(json["status"], "missing_capabilities");
    assert!(json["missing_capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .any(|capability| capability["id"] == "route_optimization"
            && capability["required_by"] == "fleet_dispatch"));
    let suggestion = json["capability_suggestions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|suggestion| {
            suggestion["capability_id"] == "route_optimization"
                && suggestion["addon_id"] == "forge.addon.route"
        })
        .expect("missing dependency should expose an Addon activation suggestion");
    assert_eq!(suggestion["action"], "enable_addon");
    assert_eq!(suggestion["status"], "available_disabled_addon");
    assert_eq!(suggestion["mcp_tools"][0], "forge.addons.enable");
    assert_eq!(suggestion["commands"][0][0], "forge");
    assert_eq!(suggestion["commands"][0][2], "enable");
    assert_eq!(suggestion["commands"][0][3], "forge.addon.route");
}

#[test]
fn capability_resolution_suggests_installable_marketplace_package_for_missing_dependencies() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let addon_dir = temp.path().join("addons");
    fs::create_dir_all(&addon_dir).unwrap();
    fs::write(
        addon_dir.join("dispatch.yaml"),
        r#"
id: forge.addon.dispatch
name: Dispatch Addon
version: 0.1.0
capabilities:
  - id: fleet_dispatch
    title: Fleet dispatch
    domains: [logistics]
    keywords: [dispatch]
    requires_capabilities: [route_optimization]
"#,
    )
    .unwrap();

    let route_manifest = temp.path().join("route.yaml");
    fs::write(
        &route_manifest,
        r#"
id: forge.addon.route
name: Route Addon
version: 0.1.0
capabilities:
  - id: route_optimization
    title: Route optimization
    domains: [logistics]
    keywords: [route]
"#,
    )
    .unwrap();

    let repository = "registry://forge/routes";
    let channel = "stable";
    let package_id = "forge.addon.route@0.1.0";
    let signing_key = SigningKey::from_bytes(&[17u8; 32]);
    let public_key = test_hex_encode(signing_key.verifying_key().as_bytes());
    let payload = addon_package_payload_bytes(
        &route_manifest,
        package_id,
        "forge.addon.route",
        "0.1.0",
        repository,
        channel,
    );
    let signature = test_hex_encode(&signing_key.sign(&payload).to_bytes());
    let package_path = temp.path().join("packages").join("route.package.json");

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "package",
            "--manifest",
            route_manifest.to_str().unwrap(),
            "--repository",
            repository,
            "--channel",
            channel,
            "--signature",
            &signature,
            "--public-key",
            &public_key,
            "--package-path",
            package_path.to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .success();
    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "trust-key",
            "--repository",
            repository,
            "--channel",
            channel,
            "--public-key",
            &public_key,
            "--approved-by",
            "operator",
            "--output",
            "json",
        ])
        .assert()
        .success();
    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "publish-package",
            "--package",
            package_path.to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .success();

    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "resolve",
            "--goal",
            "Criar dispatch logístico",
            "--addon-dir",
            addon_dir.to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["schema_version"], "forge.capability_resolution.v1");
    assert_eq!(json["status"], "missing_capabilities");
    let suggestion = json["capability_suggestions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|suggestion| {
            suggestion["capability_id"] == "route_optimization"
                && suggestion["package_id"] == package_id
        })
        .expect("missing dependency should expose a trusted marketplace package suggestion");
    assert_eq!(suggestion["action"], "install_package");
    assert_eq!(suggestion["status"], "available_in_marketplace_package");
    assert_eq!(suggestion["addon_id"], "forge.addon.route");
    assert_eq!(suggestion["addon_lifecycle"], "marketplace");
    assert_eq!(suggestion["repository"], repository);
    assert_eq!(suggestion["channel"], channel);
    assert_eq!(suggestion["mcp_tools"][0], "forge.addons.marketplace");
    assert_eq!(suggestion["mcp_tools"][1], "forge.addons.install_package");
    assert_eq!(suggestion["commands"][0][2], "install-package");
    assert_eq!(suggestion["commands"][0][4], package_path.to_str().unwrap());
    assert_eq!(suggestion["commands"][1][2], "marketplace");
}

#[test]
fn capability_resolution_syncs_authorized_registry_sources_before_suggesting_packages() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let addon_dir = temp.path().join("addons");
    fs::create_dir_all(&addon_dir).unwrap();
    fs::write(
        addon_dir.join("dispatch.yaml"),
        r#"
id: forge.addon.dispatch
name: Dispatch Addon
version: 0.1.0
capabilities:
  - id: fleet_dispatch
    title: Fleet dispatch
    domains: [logistics]
    keywords: [dispatch]
    requires_capabilities: [route_optimization]
"#,
    )
    .unwrap();

    let route_manifest = temp.path().join("route.yaml");
    fs::write(
        &route_manifest,
        r#"
id: forge.addon.route
name: Route Addon
version: 0.4.0
capabilities:
  - id: route_optimization
    title: Route optimization
    domains: [logistics]
    keywords: [route]
"#,
    )
    .unwrap();

    let repository = "registry://forge/routes";
    let channel = "stable";
    let package_id = "forge.addon.route@0.4.0";
    let signing_key = SigningKey::from_bytes(&[37u8; 32]);
    let public_key = test_hex_encode(signing_key.verifying_key().as_bytes());
    let payload = addon_package_payload_bytes(
        &route_manifest,
        package_id,
        "forge.addon.route",
        "0.4.0",
        repository,
        channel,
    );
    let signature = test_hex_encode(&signing_key.sign(&payload).to_bytes());
    let package_path = temp
        .path()
        .join("packages")
        .join("route-registry.package.json");

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "package",
            "--manifest",
            route_manifest.to_str().unwrap(),
            "--repository",
            repository,
            "--channel",
            channel,
            "--signature",
            &signature,
            "--public-key",
            &public_key,
            "--package-path",
            package_path.to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .success();
    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "trust-key",
            "--repository",
            repository,
            "--channel",
            channel,
            "--public-key",
            &public_key,
            "--approved-by",
            "operator",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let expected_sha256 = hex_sha256(&fs::read(&package_path).unwrap());
    let registry_index = temp.path().join("resolve-registry-index.json");
    fs::write(
        &registry_index,
        serde_json::to_vec_pretty(&serde_json::json!({
            "schema_version": "forge.addon_registry_index.v1",
            "packages": [
                {
                    "source": format!("file://{}", package_path.display()),
                    "expected_sha256": expected_sha256
                }
            ]
        }))
        .unwrap(),
    )
    .unwrap();
    let cache_dir = temp.path().join("resolve-registry-cache");
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "resolve",
            "--goal",
            "Criar dispatch logístico",
            "--addon-dir",
            addon_dir.to_str().unwrap(),
            "--registry-source",
            registry_index.to_str().unwrap(),
            "--registry-cache-dir",
            cache_dir.to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["schema_version"], "forge.capability_resolution.v1");
    assert_eq!(
        json["registry_syncs"][0]["schema_version"],
        "forge.addon_registry_sync.v1"
    );
    assert_eq!(json["registry_syncs"][0]["status"], "synced");
    let suggestion = json["capability_suggestions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|suggestion| suggestion["package_id"] == package_id)
        .expect("registry sync should make package suggestions available");
    assert_eq!(suggestion["action"], "install_package");
    assert!(suggestion["commands"][0][4]
        .as_str()
        .unwrap()
        .contains("resolve-registry-cache"));

    let mcp_input = serde_json::json!({
        "goal": "Criar dispatch logístico",
        "addon_dirs": [addon_dir],
        "registry_sources": [registry_index],
        "registry_cache_dir": temp.path().join("resolve-registry-mcp-cache")
    });
    let mcp_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.addons.resolve",
            "--input",
            &mcp_input.to_string(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let mcp_json: Value = serde_json::from_slice(&mcp_output).unwrap();
    assert_eq!(
        mcp_json["result"]["registry_syncs"][0]["schema_version"],
        "forge.addon_registry_sync.v1"
    );
    assert!(mcp_json["result"]["capability_suggestions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|suggestion| suggestion["package_id"] == package_id));
}

#[test]
fn capability_resolution_suggests_fetch_for_remote_marketplace_package_sources() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let addon_dir = temp.path().join("addons");
    fs::create_dir_all(&addon_dir).unwrap();
    fs::write(
        addon_dir.join("dispatch.yaml"),
        r#"
id: forge.addon.dispatch
name: Dispatch Addon
version: 0.1.0
capabilities:
  - id: fleet_dispatch
    title: Fleet dispatch
    domains: [logistics]
    keywords: [dispatch]
    requires_capabilities: [route_optimization]
"#,
    )
    .unwrap();

    let route_manifest = temp.path().join("route.yaml");
    fs::write(
        &route_manifest,
        r#"
id: forge.addon.route
name: Route Addon
version: 0.2.0
capabilities:
  - id: route_optimization
    title: Route optimization
    domains: [logistics]
    keywords: [route]
"#,
    )
    .unwrap();

    let repository = "registry://forge/routes";
    let channel = "stable";
    let package_id = "forge.addon.route@0.2.0";
    let signing_key = SigningKey::from_bytes(&[19u8; 32]);
    let public_key = test_hex_encode(signing_key.verifying_key().as_bytes());
    let payload = addon_package_payload_bytes(
        &route_manifest,
        package_id,
        "forge.addon.route",
        "0.2.0",
        repository,
        channel,
    );
    let signature = test_hex_encode(&signing_key.sign(&payload).to_bytes());
    let package_path = temp
        .path()
        .join("packages")
        .join("route-remote.package.json");
    let remote_source = "https://example.com/forge/routes/route-0.2.0.package.json";

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "package",
            "--manifest",
            route_manifest.to_str().unwrap(),
            "--repository",
            repository,
            "--channel",
            channel,
            "--signature",
            &signature,
            "--public-key",
            &public_key,
            "--package-path",
            package_path.to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .success();
    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "trust-key",
            "--repository",
            repository,
            "--channel",
            channel,
            "--public-key",
            &public_key,
            "--approved-by",
            "operator",
            "--output",
            "json",
        ])
        .assert()
        .success();
    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "publish-package",
            "--package",
            package_path.to_str().unwrap(),
            "--source",
            remote_source,
            "--output",
            "json",
        ])
        .assert()
        .success();

    let package_sha256 = hex_sha256(&fs::read(&package_path).unwrap());
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "resolve",
            "--goal",
            "Criar dispatch logístico",
            "--addon-dir",
            addon_dir.to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let suggestion = json["capability_suggestions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|suggestion| suggestion["package_id"] == package_id)
        .expect("remote marketplace package should produce a fetch suggestion");
    assert_eq!(suggestion["action"], "fetch_package");
    assert_eq!(suggestion["status"], "available_in_marketplace_package");
    assert_eq!(suggestion["mcp_tools"][0], "forge.addons.marketplace");
    assert_eq!(suggestion["mcp_tools"][1], "forge.addons.fetch_package");
    let command = suggestion["commands"][0].as_array().unwrap();
    assert_eq!(command[2], "fetch-package");
    assert!(command.iter().any(|arg| arg == "--allow-remote"));
    assert!(command.iter().any(|arg| arg == remote_source));
    assert!(command.iter().any(|arg| arg == &package_sha256));
}

#[test]
fn first_party_extension_planners_prefer_capability_resolution_over_text_triggers() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let addon_dir = temp.path().join("addons");
    fs::create_dir_all(&addon_dir).unwrap();
    fs::write(
        addon_dir.join("blueprint.yaml"),
        r#"
id: forge.addon.workflow_blueprint
name: Workflow Blueprint Addon
version: 0.1.0
capabilities:
  - id: workflow_blueprint_catalog
    title: Workflow blueprint catalog
    domains: [operations]
    keywords: [blueprints-internos]
    workflow_extensions: [n8n_primitive_research]
workflows:
  - id: n8n_primitive_research
    title: Workflow primitive research
    kind: deterministic
    description: capability-driven planner extension without textual guard
"#,
    )
    .unwrap();

    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Catalogar blueprints-internos para operações",
            "--addon-dir",
            addon_dir.to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    assert!(
        json["intent"]["capability_resolution"]["workflow_extensions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|extension| extension["id"] == "n8n_primitive_research"
                && extension["source_addon"] == "forge.addon.workflow_blueprint")
    );
    let task_titles = json["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|task| task["title"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(task_titles.contains(&"Catalog n8n workflow primitives"));
    assert!(!task_titles
        .iter()
        .any(|title| *title == "Apply Addon workflow extension: n8n primitive research"));
}

#[test]
fn addon_planner_registry_exposes_core_and_external_planning_strategies() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let addon_dir = temp.path().join("addons");
    fs::create_dir_all(&addon_dir).unwrap();
    fs::write(
        addon_dir.join("route-planner.yaml"),
        r#"
id: forge.addon.route_planner
name: Route Planner Addon
version: 0.1.0
capabilities:
  - id: route_strategy
    title: Route strategy
    domains: [logistics]
    keywords: [route-strategy]
    workflow_extensions: [route_strategy_workflow]
workflows:
  - id: route_strategy_workflow
    title: Route strategy workflow
    kind: planning
    description: External route planning strategy
runtime_contracts:
  - id: route.strategy.planning
    title: Route strategy planner
    contract_type: planning_strategy
    capability_id: route_strategy
    workflow_extension_id: route_strategy_workflow
    runtime: wasm
    entrypoint: route_strategy.plan
    inputs: [goal, constraints]
    outputs: [route_strategy_plan]
"#,
    )
    .unwrap();

    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "planners",
            "--addon-dir",
            addon_dir.to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["schema_version"], "forge.addon_planner_registry.v1");
    assert_eq!(json["status"], "addon_planner_registry_loaded");
    assert!(json["planner_count"].as_u64().unwrap() >= 2);

    let planners = json["planners"].as_array().unwrap();
    let builtin = planners
        .iter()
        .find(|planner| planner["contract_id"] == "n8n_primitive_research.planning_strategy")
        .expect("first-party planner should be projected");
    assert_eq!(builtin["status"], "core_builder_registered");
    assert_eq!(builtin["source"], "internal_first_party_builder");
    assert_eq!(builtin["mcp_tools"][0], "forge.addons.resolve");

    let external = planners
        .iter()
        .find(|planner| planner["contract_id"] == "route.strategy.planning")
        .expect("external planner strategy should be projected");
    assert_eq!(external["status"], "external_planner_registered");
    assert_eq!(external["source"], "addon_runtime_contract");
    assert_eq!(external["runtime"], "wasm");
    assert_eq!(external["workflow_extension_id"], "route_strategy_workflow");
    assert!(external["mcp_tools"]
        .as_array()
        .unwrap()
        .contains(&Value::String("forge.addons.dispatch_planner".to_string())));
    assert!(external["commands"]
        .as_array()
        .unwrap()
        .iter()
        .any(|command| {
            command
                .as_array()
                .unwrap()
                .contains(&Value::String("dispatch-planner".to_string()))
        }));

    let dispatch_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "dispatch-planner",
            "--addon",
            "forge.addon.route_planner",
            "--contract",
            "route.strategy.planning",
            "--goal",
            "Create a route-strategy plan for a logistics workflow",
            "--constraint",
            "preserve operator approval gates",
            "--context",
            r#"{"tenant":"demo"}"#,
            "--addon-dir",
            addon_dir.to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let dispatch_json: Value = serde_json::from_slice(&dispatch_output).unwrap();
    assert_eq!(
        dispatch_json["schema_version"],
        "forge.addon_runtime_contract_dispatch.v1"
    );
    assert_eq!(dispatch_json["status"], "runtime_contract_dispatch_queued");
    assert_eq!(dispatch_json["queued_count"], 1);
    assert_eq!(
        dispatch_json["dispatches"][0]["input"]["schema_version"],
        "forge.addon_planner_dispatch_input.v1"
    );
    assert_eq!(
        dispatch_json["dispatches"][0]["input"]["planner"]["workflow_extension_id"],
        "route_strategy_workflow"
    );

    let input = format!(
        r#"{{"addon_dirs":["{}"],"workflow_extension_id":"route_strategy_workflow"}}"#,
        addon_dir.display()
    );
    let mcp_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.addons.planners",
            "--input",
            &input,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let mcp_json: Value = serde_json::from_slice(&mcp_output).unwrap();
    assert_eq!(mcp_json["schema_version"], "forge.mcp.call.v1");
    assert_eq!(mcp_json["status"], "ok");
    assert_eq!(
        mcp_json["result"]["schema_version"],
        "forge.addon_planner_registry.v1"
    );
    assert_eq!(mcp_json["result"]["planner_count"], 1);
    assert_eq!(
        mcp_json["result"]["planners"][0]["contract_id"],
        "route.strategy.planning"
    );

    let mcp_dispatch_input = serde_json::json!({
        "addon_id": "forge.addon.route_planner",
        "contract_id": "route.strategy.planning",
        "goal": "Replan route strategy",
        "constraints": ["use external planner dispatch"],
        "workflow_id": "wf_demo",
        "context": {"reason": "test"},
        "addon_dirs": [addon_dir]
    });
    let mcp_dispatch_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.addons.dispatch_planner",
            "--input",
            &mcp_dispatch_input.to_string(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let mcp_dispatch_json: Value = serde_json::from_slice(&mcp_dispatch_output).unwrap();
    assert_eq!(
        mcp_dispatch_json["result"]["dispatches"][0]["input"]["schema_version"],
        "forge.addon_planner_dispatch_input.v1"
    );
    assert_eq!(
        mcp_dispatch_json["result"]["dispatches"][0]["input"]["workflow_id"],
        "wf_demo"
    );
}

#[test]
fn external_planning_strategy_executes_through_worker_with_equivalence_audit() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let addon_dir = temp.path().join("addons");
    fs::create_dir_all(&addon_dir).unwrap();
    fs::write(
        addon_dir.join("route-planner.yaml"),
        r#"
id: forge.addon.route_planner_runtime
name: Route Planner Runtime Addon
version: 0.1.0
capabilities:
  - id: route_strategy
    title: Route strategy
    domains: [logistics]
    keywords: [route-strategy]
    workflow_extensions: [route_strategy_workflow]
workflows:
  - id: route_strategy_workflow
    title: Route strategy workflow
    kind: planning
    description: External route planning strategy
runtime_contracts:
  - id: route.strategy.planning
    title: Route strategy planner
    contract_type: planning_strategy
    capability_id: route_strategy
    workflow_extension_id: route_strategy_workflow
    runtime: external_api
    entrypoint: route_strategy.plan
    inputs: [goal, constraints, context]
    outputs: [tasks]
"#,
    )
    .unwrap();
    let (endpoint, handle) = start_external_api_planning_strategy_worker_server(2);
    let worker_data = serde_json::json!({
        "execution_mode": "external_api",
        "endpoint": endpoint,
        "allowed_entrypoints": ["route_strategy.plan"],
        "allowed_contracts": ["route.strategy.planning"],
        "timeout_seconds": 5,
        "max_response_bytes": 262144,
    })
    .to_string();
    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "register-worker",
            "--worker",
            "route-planner-api-worker",
            "--runtime",
            "external_api",
            "--trust-level",
            "local",
            "--source",
            "test",
            "--data",
            worker_data.as_str(),
            "--output",
            "json",
        ])
        .assert()
        .success();

    let cli_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "execute-planner",
            "--addon-dir",
            addon_dir.to_str().unwrap(),
            "--addon",
            "forge.addon.route_planner_runtime",
            "--contract",
            "route.strategy.planning",
            "--worker",
            "route-planner-api-worker",
            "--goal",
            "Create a route-strategy plan for a logistics workflow",
            "--constraint",
            "preserve operator approval gates",
            "--context",
            r#"{"tenant":"demo"}"#,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let cli_json: Value = serde_json::from_slice(&cli_output).unwrap();
    assert_eq!(
        cli_json["schema_version"],
        "forge.addon_planning_strategy_execution.v1"
    );
    assert_eq!(
        cli_json["status"],
        "planning_strategy_equivalence_validated"
    );
    assert_eq!(
        cli_json["dispatch_report"]["status"],
        "runtime_contract_dispatch_external_completed"
    );
    assert_eq!(cli_json["validation"]["status"], "valid");
    assert_eq!(cli_json["equivalence"]["replacement_ready"], true);
    assert_eq!(cli_json["equivalence"]["status"], "equivalent");
    assert_eq!(
        cli_json["strategy_result"]["schema_version"],
        "forge.addon_planning_strategy_result.v1"
    );

    let mcp_input = serde_json::json!({
        "addon_dirs": [addon_dir.display().to_string()],
        "addon_id": "forge.addon.route_planner_runtime",
        "contract_id": "route.strategy.planning",
        "worker_id": "route-planner-api-worker",
        "goal": "Create a route-strategy plan for a logistics workflow",
        "constraints": ["preserve operator approval gates"],
        "context": {"tenant": "demo-mcp"},
        "lease_seconds": 120,
    });
    let mcp_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.addons.execute_planner",
            "--input",
            &mcp_input.to_string(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let mcp_json: Value = serde_json::from_slice(&mcp_output).unwrap();
    assert_eq!(mcp_json["schema_version"], "forge.mcp.call.v1");
    assert_eq!(mcp_json["status"], "ok");
    assert_eq!(
        mcp_json["result"]["schema_version"],
        "forge.addon_planning_strategy_execution.v1"
    );
    assert_eq!(
        mcp_json["result"]["status"],
        "planning_strategy_equivalence_validated"
    );
    assert_eq!(mcp_json["result"]["equivalence"]["replacement_ready"], true);
    assert_eq!(
        mcp_json["result"]["dispatch_report"]["dispatches"][0]["input"]["context"]
            ["provided_context"]["tenant"],
        "demo-mcp"
    );
    handle.join().unwrap();
}

#[test]
fn addon_observability_consolidates_capabilities_resources_events_and_dispatch_usage() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let addon_dir = temp.path().join("addons");
    fs::create_dir_all(&addon_dir).unwrap();
    fs::write(
        addon_dir.join("telemetry.yaml"),
        r#"
id: forge.addon.telemetry
name: Telemetry Addon
version: 0.1.0
permissions:
  - id: telemetry.read
    risk: high
    tools: [mqtt-client]
    resources: [machine.telemetry]
    integrations: [mqtt.broker]
    actions: [event.consume]
    tenant_scopes: [organization]
capabilities:
  - id: machine_monitoring
    title: Machine monitoring
    domains: [operations]
    keywords: [telemetry]
runtime_contracts:
  - id: telemetry.monitor.executor
    title: Telemetry monitor executor
    contract_type: executor
    capability_id: machine_monitoring
    runtime: wasm
    entrypoint: telemetry.monitor
    permissions: [telemetry.read]
views:
  - id: telemetry.dashboard
    title: Telemetry Dashboard
    surface: ops_console
    type: dashboard
    component: telemetry-dashboard
artifact_types:
  - id: telemetry.snapshot
    title: Telemetry snapshot
    generic_kind: report
event_types:
  - id: telemetry.sample
    title: Telemetry sample
    transport: mqtt
  - id: telemetry.alert
    title: Telemetry alert
    transport: mqtt
event_adapters:
  - id: telemetry.mqtt.ingress
    title: MQTT telemetry ingress
    transport: mqtt
    direction: ingress
    event_types: [telemetry.sample]
    actions: [continue_workflow]
    permissions: [telemetry.read]
  - id: telemetry.mqtt.egress
    title: MQTT telemetry egress
    transport: mqtt
    direction: egress
    event_types: [telemetry.alert]
    actions: [modify_workflow]
    permissions: [telemetry.read]
integrations:
  - id: mqtt.broker
    title: MQTT Broker
    integration_type: messaging
"#,
    )
    .unwrap();

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "dispatch-contract",
            "--addon",
            "forge.addon.telemetry",
            "--contract",
            "telemetry.monitor.executor",
            "--input",
            r#"{"machine":"demo"}"#,
            "--addon-dir",
            addon_dir.to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .success();

    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "observability",
            "--addon",
            "forge.addon.telemetry",
            "--addon-dir",
            addon_dir.to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["schema_version"], "forge.addon_observability.v1");
    assert_eq!(json["addon_count"], 1);
    assert_eq!(json["totals"]["capability_count"], 1);
    assert_eq!(json["totals"]["dispatch_count"], 1);
    let addon = &json["addons"][0];
    assert_eq!(addon["addon_id"], "forge.addon.telemetry");
    assert_eq!(addon["capabilities"][0], "machine_monitoring");
    assert_eq!(addon["dependencies"].as_array().unwrap().len(), 0);
    assert!(addon["permission_gate"]["tools"]
        .as_array()
        .unwrap()
        .contains(&Value::String("mqtt-client".to_string())));
    assert!(addon["permission_gate"]["resources"]
        .as_array()
        .unwrap()
        .contains(&Value::String("machine.telemetry".to_string())));
    assert!(addon["event_flow"]["consumed_event_types"]
        .as_array()
        .unwrap()
        .contains(&Value::String("telemetry.sample".to_string())));
    assert!(addon["event_flow"]["emitted_event_types"]
        .as_array()
        .unwrap()
        .contains(&Value::String("telemetry.alert".to_string())));
    assert_eq!(addon["dispatches"]["queued_count"], 1);

    let mcp_input = serde_json::json!({
        "addon_id": "forge.addon.telemetry",
        "addon_dirs": [addon_dir],
        "dispatch_limit": 50
    });
    let mcp_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.addons.observability",
            "--input",
            &mcp_input.to_string(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let mcp_json: Value = serde_json::from_slice(&mcp_output).unwrap();
    assert_eq!(
        mcp_json["result"]["schema_version"],
        "forge.addon_observability.v1"
    );
    assert_eq!(mcp_json["result"]["addons"][0]["event_adapter_count"], 2);
}

#[test]
fn event_stream_projects_legacy_events_into_tenant_aware_envelopes() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let plan_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Operar fluxo persistente para atendimento humano e IA",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let plan_json: Value = serde_json::from_slice(&plan_output).unwrap();
    let workflow_id = plan_json["workflow_id"].as_str().unwrap();
    let events_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "list",
            "--workflow",
            workflow_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let events_json: Value = serde_json::from_slice(&events_output).unwrap();
    assert_eq!(events_json["schema_version"], "forge.event_stream.v1");
    assert_eq!(events_json["status"], "event_stream_loaded");
    assert_eq!(
        events_json["tenant_context"]["organization"]["id"],
        "default-org"
    );
    assert_eq!(
        events_json["events"][0]["schema_version"],
        "forge.event_envelope.v1"
    );
    assert_eq!(events_json["events"][0]["kind"], "workflow_planned");
    assert_eq!(events_json["events"][0]["category"], "workflow");
    assert_eq!(events_json["events"][0]["severity"], "info");

    let mcp_input = format!(r#"{{"workflow_id":"{workflow_id}","limit":1}}"#);
    let mcp_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.events.list",
            "--input",
            &mcp_input,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let mcp_json: Value = serde_json::from_slice(&mcp_output).unwrap();
    assert_eq!(mcp_json["schema_version"], "forge.mcp.call.v1");
    assert_eq!(
        mcp_json["result"]["schema_version"],
        "forge.event_stream.v1"
    );
    assert_eq!(mcp_json["result"]["event_count"], 1);
}

#[test]
fn event_envelopes_project_normalized_observability_metrics() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let plan_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Executar workflow com telemetria operacional normalizada",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let plan_json: Value = serde_json::from_slice(&plan_output).unwrap();
    let workflow_id = plan_json["workflow_id"].as_str().unwrap();
    let store_handle = ForgeStore::open(&store).unwrap();
    store_handle
        .record_event(
            workflow_id,
            "task_wait_retry_observed",
            &serde_json::json!({
                "origin": "ops_test",
                "source": "test_observer",
                "node_id": "node-payment",
                "task_id": "task-002",
                "addon_id": "forge.addon.payment",
                "duration_ms": 245,
                "retry_count": "2",
                "wait_seconds": 30,
                "state": "waiting_for_partner",
                "context": {
                    "effective_budget": 1000,
                    "context_bytes": 875,
                    "routing_summary": {
                        "remaining_budget": 125,
                        "budget_utilization_bps": 8750
                    },
                    "memory_policy": {
                        "memory_level": "standard",
                        "memory_scope": "project"
                    }
                }
            }),
        )
        .unwrap();

    let events_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "list",
            "--workflow",
            workflow_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let events_json: Value = serde_json::from_slice(&events_output).unwrap();
    let observed = events_json["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|event| event["kind"] == "task_wait_retry_observed")
        .unwrap();
    assert_eq!(
        observed["observability"]["schema_version"],
        "forge.event_observability.v1"
    );
    assert_eq!(observed["observability"]["node_ref"], "node-payment");
    assert_eq!(observed["observability"]["addon_id"], "forge.addon.payment");
    assert_eq!(observed["observability"]["duration_ms"], 245);
    assert_eq!(observed["observability"]["retry_count"], 2);
    assert_eq!(
        observed["observability"]["wait_state"],
        "waiting_for_partner"
    );
    assert_eq!(observed["observability"]["wait_seconds"], 30);
    assert_eq!(observed["observability"]["context_budget_bytes"], 1000);
    assert_eq!(observed["observability"]["selected_context_bytes"], 875);
    assert_eq!(observed["observability"]["context_remaining_bytes"], 125);
    assert_eq!(observed["observability"]["context_pressure_bps"], 8750);
    assert_eq!(observed["observability"]["context_pressure_state"], "high");
    assert_eq!(observed["observability"]["memory_level"], "standard");
    assert_eq!(observed["observability"]["memory_scope"], "project");

    let timeline_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "timeline",
            "--workflow",
            workflow_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let timeline_json: Value = serde_json::from_slice(&timeline_output).unwrap();
    let timeline_observed = timeline_json["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|event| event["kind"] == "task_wait_retry_observed")
        .unwrap();
    assert_eq!(timeline_observed["source"], "workflow_event");
    assert_eq!(
        timeline_observed["observability"]["schema_version"],
        "forge.event_observability.v1"
    );
    assert_eq!(timeline_observed["observability"]["duration_ms"], 245);
    assert_eq!(timeline_observed["observability"]["retry_count"], 2);
    assert_eq!(
        timeline_observed["observability"]["wait_state"],
        "waiting_for_partner"
    );
    assert_eq!(
        timeline_observed["observability"]["context_pressure_bps"],
        8750
    );
    assert_eq!(
        timeline_observed["observability"]["memory_level"],
        "standard"
    );

    let observability_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "observability",
            "--workflow",
            workflow_id,
            "--node",
            "node-payment",
            "--addon",
            "forge.addon.payment",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let observability_json: Value = serde_json::from_slice(&observability_output).unwrap();
    assert_eq!(
        observability_json["schema_version"],
        "forge.event_observability_index.v1"
    );
    assert_eq!(
        observability_json["status"],
        "event_observability_index_loaded"
    );
    assert_eq!(observability_json["index_source"], "sqlite_materialized");
    assert_eq!(observability_json["summary"]["total_event_count"], 1);
    assert_eq!(observability_json["summary"]["node_event_count"], 1);
    assert_eq!(observability_json["summary"]["addon_event_count"], 1);
    assert_eq!(observability_json["summary"]["total_duration_ms"], 245);
    assert_eq!(observability_json["summary"]["total_retry_count"], 2);
    assert_eq!(observability_json["summary"]["total_wait_seconds"], 30);
    assert_eq!(observability_json["summary"]["context_event_count"], 1);
    assert_eq!(
        observability_json["summary"]["context_pressure_event_count"],
        1
    );
    assert_eq!(
        observability_json["summary"]["total_context_budget_bytes"],
        1000
    );
    assert_eq!(
        observability_json["summary"]["total_selected_context_bytes"],
        875
    );
    assert_eq!(
        observability_json["summary"]["total_context_remaining_bytes"],
        125
    );
    assert_eq!(
        observability_json["summary"]["max_context_pressure_bps"],
        8750
    );
    assert_eq!(observability_json["summary"]["memory_event_count"], 1);
    assert_eq!(observability_json["nodes"][0]["node_ref"], "node-payment");
    assert_eq!(
        observability_json["nodes"][0]["addon_id"],
        "forge.addon.payment"
    );
    assert_eq!(
        observability_json["nodes"][0]["max_context_pressure_bps"],
        8750
    );
    assert_eq!(observability_json["nodes"][0]["memory_event_count"], 1);
    assert_eq!(
        observability_json["addons"][0]["addon_id"],
        "forge.addon.payment"
    );
    assert_eq!(
        observability_json["addons"][0]["max_context_pressure_bps"],
        8750
    );
    assert_eq!(
        observability_json["events"][0]["wait_state"],
        "waiting_for_partner"
    );
    assert_eq!(
        observability_json["events"][0]["context_pressure_bps"],
        8750
    );
    assert_eq!(observability_json["events"][0]["memory_scope"], "project");
    let materialized_records = store_handle
        .load_event_observability_index(
            Some(workflow_id),
            None,
            None,
            None,
            Some("node-payment"),
            Some("forge.addon.payment"),
        )
        .unwrap();
    assert_eq!(materialized_records.len(), 1);
    assert_eq!(materialized_records[0].duration_ms, Some(245));
    assert_eq!(materialized_records[0].retry_count, Some(2));
    assert_eq!(
        materialized_records[0].wait_state.as_deref(),
        Some("waiting_for_partner")
    );
    assert_eq!(materialized_records[0].context_budget_bytes, Some(1000));
    assert_eq!(materialized_records[0].selected_context_bytes, Some(875));
    assert_eq!(materialized_records[0].context_remaining_bytes, Some(125));
    assert_eq!(materialized_records[0].context_pressure_bps, Some(8750));
    assert_eq!(
        materialized_records[0].context_pressure_state.as_deref(),
        Some("high")
    );
    assert_eq!(
        materialized_records[0].memory_level.as_deref(),
        Some("standard")
    );
    assert_eq!(
        materialized_records[0].memory_scope.as_deref(),
        Some("project")
    );

    let mcp_input = serde_json::json!({
        "workflow_id": workflow_id,
        "node_ref": "node-payment",
        "addon_id": "forge.addon.payment",
        "limit": 1
    });
    let mcp_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.events.observability",
            "--input",
            &mcp_input.to_string(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let mcp_json: Value = serde_json::from_slice(&mcp_output).unwrap();
    assert_eq!(
        mcp_json["result"]["schema_version"],
        "forge.event_observability_index.v1"
    );
    assert_eq!(mcp_json["result"]["event_count"], 1);
    assert_eq!(
        mcp_json["result"]["events"][0]["store_sequence"],
        observability_json["events"][0]["store_sequence"]
    );
}

#[test]
fn event_timeline_enforces_project_tenant_policy_for_global_event_reads() {
    let temp = tempdir().unwrap();
    let forge_dir = temp.path().join(".forge");
    fs::create_dir_all(&forge_dir).unwrap();
    fs::write(
        forge_dir.join("operating-context.yaml"),
        r#"
organization:
  scope: organization
  id: timeline-org
  label: Timeline Org
brand:
  scope: brand
  id: timeline-brand
  label: Timeline Brand
product:
  scope: product
  id: timeline-product
  label: Timeline Product
user:
  scope: user
  id: timeline-user
  label: Timeline User
channel:
  scope: channel
  id: local_cli
  label: Local CLI
tenant_policy_mode: enforce
"#,
    )
    .unwrap();

    let store = temp.path().join("forge.sqlite");
    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "identity",
            "sync",
            "--project-root",
            temp.path().to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .success();

    let tenant_context = serde_json::json!({
        "organization": {"scope": "organization", "id": "timeline-org", "label": "Timeline Org"},
        "brand": {"scope": "brand", "id": "timeline-brand", "label": "Timeline Brand"},
        "product": {"scope": "product", "id": "timeline-product", "label": "Timeline Product"},
        "user": {"scope": "user", "id": "timeline-user", "label": "Timeline User"},
        "channel": {"scope": "channel", "id": "local_cli", "label": "Local CLI"},
        "tenant_policy_mode": "enforce"
    });
    let other_tenant_context = serde_json::json!({
        "organization": {"scope": "organization", "id": "other-org", "label": "Other Org"},
        "brand": {"scope": "brand", "id": "other-brand", "label": "Other Brand"},
        "product": {"scope": "product", "id": "other-product", "label": "Other Product"},
        "user": {"scope": "user", "id": "other-user", "label": "Other User"},
        "channel": {"scope": "channel", "id": "api", "label": "API"},
        "tenant_policy_mode": "enforce"
    });
    let connection = Connection::open(&store).unwrap();
    for (source_id, organization, brand, product, user, channel, context, data) in [
        (
            "tenant-visible",
            "timeline-org",
            "timeline-brand",
            "timeline-product",
            "timeline-user",
            "local_cli",
            tenant_context,
            serde_json::json!({"message": "visible tenant event"}),
        ),
        (
            "tenant-hidden",
            "other-org",
            "other-brand",
            "other-product",
            "other-user",
            "api",
            other_tenant_context,
            serde_json::json!({"message": "must not leak"}),
        ),
    ] {
        connection
            .execute(
                r#"
                INSERT INTO global_events (
                    source, source_id, workflow_id, kind, origin, status,
                    organization_id, brand_id, product_id, user_id, channel_id,
                    tenant_context_json, data_json, created_at
                )
                VALUES (
                    'tenant_timeline_seed', ?1, NULL, 'tenant_timeline_event', 'codex', 'recorded',
                    ?2, ?3, ?4, ?5, ?6, ?7, ?8, CURRENT_TIMESTAMP
                )
                "#,
                rusqlite::params![
                    source_id,
                    organization,
                    brand,
                    product,
                    user,
                    channel,
                    context.to_string(),
                    data.to_string()
                ],
            )
            .unwrap();
    }

    let timeline_output = forge()
        .current_dir(temp.path())
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "timeline",
            "--limit",
            "10",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let timeline_json: Value = serde_json::from_slice(&timeline_output).unwrap();
    assert_eq!(timeline_json["filters"]["organization_id"], "timeline-org");
    assert_eq!(timeline_json["event_count"], 1);
    let event = &timeline_json["events"][0];
    assert_eq!(
        event["tenant_context"]["organization"]["id"],
        "timeline-org"
    );
    assert_eq!(event["data"]["message"], "visible tenant event");
    assert!(!timeline_json["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|event| event["data"]["message"] == "must not leak"));

    let mcp_timeline_input = serde_json::json!({
        "project_root": temp.path().display().to_string(),
        "limit": 10
    });
    let mcp_timeline_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.events.timeline",
            "--input",
            &mcp_timeline_input.to_string(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let mcp_timeline_json: Value = serde_json::from_slice(&mcp_timeline_output).unwrap();
    assert_eq!(
        mcp_timeline_json["result"]["filters"]["organization_id"],
        "timeline-org"
    );
    assert_eq!(mcp_timeline_json["result"]["event_count"], 1);

    forge()
        .current_dir(temp.path())
        .args([
            "--store",
            store.to_str().unwrap(),
            "identity",
            "membership-update",
            "--subject",
            "timeline-user",
            "--organization",
            "timeline-org",
            "--brand",
            "timeline-brand",
            "--product",
            "timeline-product",
            "--deny",
            "context:read",
            "--source",
            "test-cli",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let denied_output = forge()
        .current_dir(temp.path())
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "timeline",
            "--limit",
            "10",
            "--output",
            "json",
        ])
        .assert()
        .failure()
        .get_output()
        .stderr
        .clone();
    let denied_stderr = String::from_utf8(denied_output).unwrap();
    assert!(denied_stderr.contains("multi-tenant enforcement blocked events timeline list"));
    assert!(denied_stderr.contains("context:read"));
}

#[test]
fn event_observability_enforces_project_tenant_policy_for_global_index_reads() {
    let temp = tempdir().unwrap();
    let forge_dir = temp.path().join(".forge");
    fs::create_dir_all(&forge_dir).unwrap();
    fs::write(
        forge_dir.join("operating-context.yaml"),
        r#"
organization:
  scope: organization
  id: obs-org
  label: Observability Org
brand:
  scope: brand
  id: obs-brand
  label: Observability Brand
product:
  scope: product
  id: obs-product
  label: Observability Product
user:
  scope: user
  id: obs-user
  label: Observability User
channel:
  scope: channel
  id: local_cli
  label: Local CLI
tenant_policy_mode: enforce
"#,
    )
    .unwrap();

    let store = temp.path().join("forge.sqlite");
    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "identity",
            "sync",
            "--project-root",
            temp.path().to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .success();

    let store_handle = ForgeStore::open(&store).unwrap();
    let tenant_context = serde_json::json!({
        "organization": {"scope": "organization", "id": "obs-org", "label": "Observability Org"},
        "brand": {"scope": "brand", "id": "obs-brand", "label": "Observability Brand"},
        "product": {"scope": "product", "id": "obs-product", "label": "Observability Product"},
        "user": {"scope": "user", "id": "obs-user", "label": "Observability User"},
        "channel": {"scope": "channel", "id": "local_cli", "label": "Local CLI"},
        "tenant_policy_mode": "enforce"
    });
    let other_tenant_context = serde_json::json!({
        "organization": {"scope": "organization", "id": "other-obs-org", "label": "Other Observability Org"},
        "brand": {"scope": "brand", "id": "other-obs-brand", "label": "Other Observability Brand"},
        "product": {"scope": "product", "id": "other-obs-product", "label": "Other Observability Product"},
        "user": {"scope": "user", "id": "other-obs-user", "label": "Other Observability User"},
        "channel": {"scope": "channel", "id": "api", "label": "API"},
        "tenant_policy_mode": "enforce"
    });
    store_handle
        .record_global_event(
            "obs_seed",
            "visible",
            None,
            "ai_executor_completed",
            "codex",
            "recorded",
            &serde_json::json!({
                "node_id": "node-visible",
                "addon_id": "forge.addon.visible",
                "duration_ms": 222,
                "message": "visible observability"
            }),
            &tenant_context,
        )
        .unwrap();
    store_handle
        .record_global_event(
            "obs_seed",
            "hidden",
            None,
            "ai_executor_completed",
            "codex",
            "recorded",
            &serde_json::json!({
                "node_id": "node-hidden",
                "addon_id": "forge.addon.hidden",
                "duration_ms": 999,
                "message": "must not leak"
            }),
            &other_tenant_context,
        )
        .unwrap();

    let observability_output = forge()
        .current_dir(temp.path())
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "observability",
            "--limit",
            "10",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let observability_json: Value = serde_json::from_slice(&observability_output).unwrap();
    assert_eq!(observability_json["filters"]["organization_id"], "obs-org");
    assert_eq!(observability_json["event_count"], 1);
    assert_eq!(observability_json["summary"]["total_duration_ms"], 222);
    assert_eq!(
        observability_json["events"][0]["organization_id"],
        "obs-org"
    );
    assert_eq!(observability_json["events"][0]["node_ref"], "node-visible");
    assert!(!observability_json["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|event| event["node_ref"] == "node-hidden"));

    let mcp_observability_input = serde_json::json!({
        "project_root": temp.path().display().to_string(),
        "limit": 10
    });
    let mcp_observability_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.events.observability",
            "--input",
            &mcp_observability_input.to_string(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let mcp_observability_json: Value = serde_json::from_slice(&mcp_observability_output).unwrap();
    assert_eq!(
        mcp_observability_json["result"]["filters"]["organization_id"],
        "obs-org"
    );
    assert_eq!(mcp_observability_json["result"]["event_count"], 1);

    forge()
        .current_dir(temp.path())
        .args([
            "--store",
            store.to_str().unwrap(),
            "identity",
            "membership-update",
            "--subject",
            "obs-user",
            "--organization",
            "obs-org",
            "--brand",
            "obs-brand",
            "--product",
            "obs-product",
            "--deny",
            "context:read",
            "--source",
            "test-cli",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let denied_output = forge()
        .current_dir(temp.path())
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "observability",
            "--limit",
            "10",
            "--output",
            "json",
        ])
        .assert()
        .failure()
        .get_output()
        .stderr
        .clone();
    let denied_stderr = String::from_utf8(denied_output).unwrap();
    assert!(denied_stderr.contains("multi-tenant enforcement blocked events observability list"));
    assert!(denied_stderr.contains("context:read"));
}

#[test]
fn event_observability_history_enforces_project_tenant_policy_for_global_rollups() {
    let temp = tempdir().unwrap();
    let forge_dir = temp.path().join(".forge");
    fs::create_dir_all(&forge_dir).unwrap();
    fs::write(
        forge_dir.join("operating-context.yaml"),
        r#"
organization:
  scope: organization
  id: history-org
  label: History Org
brand:
  scope: brand
  id: history-brand
  label: History Brand
product:
  scope: product
  id: history-product
  label: History Product
user:
  scope: user
  id: history-user
  label: History User
channel:
  scope: channel
  id: local_cli
  label: Local CLI
tenant_policy_mode: enforce
"#,
    )
    .unwrap();

    let store = temp.path().join("forge.sqlite");
    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "identity",
            "sync",
            "--project-root",
            temp.path().to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .success();

    let store_handle = ForgeStore::open(&store).unwrap();
    let tenant_context = serde_json::json!({
        "organization": {"scope": "organization", "id": "history-org", "label": "History Org"},
        "brand": {"scope": "brand", "id": "history-brand", "label": "History Brand"},
        "product": {"scope": "product", "id": "history-product", "label": "History Product"},
        "user": {"scope": "user", "id": "history-user", "label": "History User"},
        "channel": {"scope": "channel", "id": "local_cli", "label": "Local CLI"},
        "tenant_policy_mode": "enforce"
    });
    let other_tenant_context = serde_json::json!({
        "organization": {"scope": "organization", "id": "other-history-org", "label": "Other History Org"},
        "brand": {"scope": "brand", "id": "other-history-brand", "label": "Other History Brand"},
        "product": {"scope": "product", "id": "other-history-product", "label": "Other History Product"},
        "user": {"scope": "user", "id": "other-history-user", "label": "Other History User"},
        "channel": {"scope": "channel", "id": "api", "label": "API"},
        "tenant_policy_mode": "enforce"
    });
    store_handle
        .record_global_event(
            "history_seed",
            "visible",
            None,
            "ai_executor_completed",
            "codex",
            "recorded",
            &serde_json::json!({
                "node_id": "node-history-visible",
                "addon_id": "forge.addon.history_visible",
                "duration_ms": 333,
                "retry_count": 1
            }),
            &tenant_context,
        )
        .unwrap();
    store_handle
        .record_global_event(
            "history_seed",
            "hidden",
            None,
            "ai_executor_completed",
            "codex",
            "recorded",
            &serde_json::json!({
                "node_id": "node-history-hidden",
                "addon_id": "forge.addon.history_hidden",
                "duration_ms": 999,
                "retry_count": 9
            }),
            &other_tenant_context,
        )
        .unwrap();

    let history_output = forge()
        .current_dir(temp.path())
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "observability-history",
            "--group-by",
            "tenant",
            "--bucket",
            "day",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let history_json: Value = serde_json::from_slice(&history_output).unwrap();
    assert_eq!(history_json["filters"]["organization_id"], "history-org");
    assert_eq!(history_json["summary"]["total_event_count"], 1);
    assert_eq!(history_json["summary"]["total_duration_ms"], 333);
    assert_eq!(history_json["bucket_count"], 1);
    assert_eq!(
        history_json["buckets"][0]["group"]["organization_id"],
        "history-org"
    );
    assert_eq!(
        history_json["buckets"][0]["summary"]["total_retry_count"],
        1
    );

    let mcp_history_input = serde_json::json!({
        "project_root": temp.path().display().to_string(),
        "group_by": "tenant",
        "bucket": "day"
    });
    let mcp_history_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.events.observability_history",
            "--input",
            &mcp_history_input.to_string(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let mcp_history_json: Value = serde_json::from_slice(&mcp_history_output).unwrap();
    assert_eq!(
        mcp_history_json["result"]["filters"]["organization_id"],
        "history-org"
    );
    assert_eq!(
        mcp_history_json["result"]["summary"]["total_event_count"],
        1
    );

    forge()
        .current_dir(temp.path())
        .args([
            "--store",
            store.to_str().unwrap(),
            "identity",
            "membership-update",
            "--subject",
            "history-user",
            "--organization",
            "history-org",
            "--brand",
            "history-brand",
            "--product",
            "history-product",
            "--deny",
            "context:read",
            "--source",
            "test-cli",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let denied_output = forge()
        .current_dir(temp.path())
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "observability-history",
            "--group-by",
            "tenant",
            "--bucket",
            "day",
            "--output",
            "json",
        ])
        .assert()
        .failure()
        .get_output()
        .stderr
        .clone();
    let denied_stderr = String::from_utf8(denied_output).unwrap();
    assert!(denied_stderr
        .contains("multi-tenant enforcement blocked events observability history list"));
    assert!(denied_stderr.contains("context:read"));
}

#[test]
fn event_observability_index_backfills_existing_global_events_on_migration() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    {
        let connection = Connection::open(&store).unwrap();
        connection
            .execute_batch(
                r#"
                CREATE TABLE global_events (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    source TEXT NOT NULL,
                    source_id TEXT NOT NULL,
                    workflow_id TEXT,
                    kind TEXT NOT NULL,
                    origin TEXT NOT NULL,
                    status TEXT NOT NULL,
                    organization_id TEXT NOT NULL,
                    brand_id TEXT NOT NULL,
                    product_id TEXT NOT NULL,
                    user_id TEXT NOT NULL,
                    channel_id TEXT NOT NULL,
                    tenant_context_json TEXT NOT NULL,
                    data_json TEXT NOT NULL,
                    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                );
                CREATE TABLE event_observability_index (
                    global_event_id INTEGER PRIMARY KEY,
                    workflow_id TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    category TEXT NOT NULL,
                    severity TEXT NOT NULL,
                    origin TEXT NOT NULL,
                    source TEXT NOT NULL,
                    organization_id TEXT NOT NULL,
                    brand_id TEXT NOT NULL,
                    product_id TEXT NOT NULL,
                    node_ref TEXT,
                    addon_id TEXT,
                    duration_ms INTEGER,
                    retry_count INTEGER,
                    wait_state TEXT,
                    wait_seconds INTEGER,
                    data_json TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                );
                INSERT INTO global_events (
                    source, source_id, workflow_id, kind, origin, status,
                    organization_id, brand_id, product_id, user_id, channel_id,
                    tenant_context_json, data_json, created_at
                )
                VALUES (
                    'legacy_import', 'legacy-001', 'wf_legacy',
                    'legacy_wait_retry_observed', 'legacy_runner', 'recorded',
                    'org-demo', 'brand-demo', 'product-demo', 'user-demo', 'web',
                    '{}',
                    '{"node_id":"legacy-node","addon_id":"forge.addon.legacy","duration_ms":77,"retry_count":1,"wait_seconds":9,"state":"waiting_for_backfill","context":{"effective_budget":900,"routing_summary":{"selected_bytes":810,"remaining_budget":90},"memory_policy":{"memory_level":"short_term","memory_scope":"organization"}}}',
                    '2026-06-10T12:00:00Z'
                );
                INSERT INTO event_observability_index (
                    global_event_id, workflow_id, kind, category, severity, origin, source,
                    organization_id, brand_id, product_id, node_ref, addon_id,
                    duration_ms, retry_count, wait_state, wait_seconds, data_json, created_at
                )
                VALUES (
                    1, 'wf_legacy', 'legacy_wait_retry_observed', 'operational', 'info',
                    'legacy_runner', 'legacy_import', 'org-demo', 'brand-demo', 'product-demo',
                    'legacy-node', 'forge.addon.legacy', 77, 1, 'waiting_for_backfill', 9,
                    '{"legacy":"already_materialized_without_context_columns"}',
                    '2026-06-10T12:00:00Z'
                );
                "#,
            )
            .unwrap();
    }

    let store_handle = ForgeStore::open(&store).unwrap();
    let records = store_handle
        .load_event_observability_index(
            Some("wf_legacy"),
            Some("org-demo"),
            Some("brand-demo"),
            Some("product-demo"),
            Some("legacy-node"),
            Some("forge.addon.legacy"),
        )
        .unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].duration_ms, Some(77));
    assert_eq!(records[0].retry_count, Some(1));
    assert_eq!(records[0].wait_seconds, Some(9));
    assert_eq!(records[0].context_budget_bytes, Some(900));
    assert_eq!(records[0].selected_context_bytes, Some(810));
    assert_eq!(records[0].context_remaining_bytes, Some(90));
    assert_eq!(records[0].context_pressure_bps, Some(9000));
    assert_eq!(
        records[0].context_pressure_state.as_deref(),
        Some("critical")
    );
    assert_eq!(records[0].memory_level.as_deref(), Some("short_term"));
    assert_eq!(records[0].memory_scope.as_deref(), Some("organization"));

    let observability_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "observability",
            "--workflow",
            "wf_legacy",
            "--organization",
            "org-demo",
            "--brand",
            "brand-demo",
            "--product",
            "product-demo",
            "--node",
            "legacy-node",
            "--addon",
            "forge.addon.legacy",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let observability_json: Value = serde_json::from_slice(&observability_output).unwrap();
    assert_eq!(observability_json["index_source"], "sqlite_materialized");
    assert_eq!(observability_json["summary"]["total_event_count"], 1);
    assert_eq!(observability_json["summary"]["context_event_count"], 1);
    assert_eq!(
        observability_json["summary"]["max_context_pressure_bps"],
        9000
    );
    assert_eq!(observability_json["summary"]["memory_event_count"], 1);
    assert_eq!(
        observability_json["events"][0]["wait_state"],
        "waiting_for_backfill"
    );
    assert_eq!(
        observability_json["events"][0]["context_pressure_state"],
        "critical"
    );
}

#[test]
fn event_observability_history_rolls_up_time_buckets_for_cli_and_mcp() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    {
        let _store_handle = ForgeStore::open(&store).unwrap();
    }
    {
        let connection = Connection::open(&store).unwrap();
        connection
            .execute_batch(
                r#"
                INSERT INTO global_events (
                    source, source_id, workflow_id, kind, origin, status,
                    organization_id, brand_id, product_id, user_id, channel_id,
                    tenant_context_json, data_json, created_at
                )
                VALUES
                (
                    'history_seed', 'hist-001', 'wf_history',
                    'task_observability_sampled', 'history_test', 'recorded',
                    'org-demo', 'brand-demo', 'product-demo', 'user-demo', 'web',
                    '{}',
                    '{"node_id":"node-stock","addon_id":"forge.addon.inventory","duration_ms":120,"retry_count":0,"wait_seconds":5,"context":{"effective_budget":1000,"routing_summary":{"selected_bytes":500,"remaining_budget":500},"memory_policy":{"memory_level":"standard","memory_scope":"project"}}}',
                    '2026-06-09T10:15:00Z'
                ),
                (
                    'history_seed', 'hist-002', 'wf_history',
                    'task_observability_sampled', 'history_test', 'recorded',
                    'org-demo', 'brand-demo', 'product-demo', 'user-demo', 'web',
                    '{}',
                    '{"node_id":"node-stock","addon_id":"forge.addon.inventory","duration_ms":180,"retry_count":1,"wait_seconds":7,"context":{"effective_budget":1000,"routing_summary":{"selected_bytes":850,"remaining_budget":150},"memory_policy":{"memory_level":"standard","memory_scope":"project"}}}',
                    '2026-06-10T11:20:00Z'
                ),
                (
                    'history_seed', 'hist-003', 'wf_history',
                    'task_observability_sampled', 'history_test', 'recorded',
                    'org-demo', 'brand-demo', 'product-demo', 'user-demo', 'web',
                    '{}',
                    '{"node_id":"node-payment","addon_id":"forge.addon.payment","duration_ms":75,"retry_count":2,"wait_seconds":3,"context":{"effective_budget":800,"routing_summary":{"selected_bytes":760,"remaining_budget":40},"memory_policy":{"memory_level":"short_term","memory_scope":"organization"}}}',
                    '2026-06-10T12:25:00Z'
                );
                "#,
            )
            .unwrap();
    }
    let _store_handle = ForgeStore::open(&store).unwrap();

    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "observability-history",
            "--organization",
            "org-demo",
            "--bucket",
            "day",
            "--group-by",
            "addon",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(
        json["schema_version"],
        "forge.event_observability_history.v1"
    );
    assert_eq!(json["status"], "event_observability_history_loaded");
    assert_eq!(json["index_source"], "sqlite_materialized");
    assert_eq!(json["filters"]["bucket"], "day");
    assert_eq!(json["filters"]["group_by"], "addon");
    assert_eq!(json["summary"]["total_event_count"], 3);
    assert_eq!(json["summary"]["total_duration_ms"], 375);
    assert_eq!(json["summary"]["total_wait_seconds"], 15);
    assert_eq!(json["summary"]["max_context_pressure_bps"], 9500);
    assert_eq!(json["bucket_count"], 3);

    let buckets = json["buckets"].as_array().unwrap();
    let inventory_first_day = buckets
        .iter()
        .find(|bucket| {
            bucket["group_id"] == "forge.addon.inventory"
                && bucket["bucket_start"]
                    .as_str()
                    .unwrap()
                    .starts_with("2026-06-09T00:00:00")
        })
        .unwrap();
    assert_eq!(inventory_first_day["summary"]["total_event_count"], 1);
    assert_eq!(
        inventory_first_day["summary"]["total_selected_context_bytes"],
        500
    );

    let payment_second_day = buckets
        .iter()
        .find(|bucket| {
            bucket["group_id"] == "forge.addon.payment"
                && bucket["bucket_start"]
                    .as_str()
                    .unwrap()
                    .starts_with("2026-06-10T00:00:00")
        })
        .unwrap();
    assert_eq!(payment_second_day["summary"]["total_retry_count"], 2);
    assert_eq!(
        payment_second_day["summary"]["max_context_pressure_bps"],
        9500
    );

    let mcp_input = serde_json::json!({
        "workflow_id": "wf_history",
        "addon_id": "forge.addon.inventory",
        "bucket": "day",
        "group_by": "addon"
    });
    let mcp_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.events.observability_history",
            "--input",
            &mcp_input.to_string(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let mcp_json: Value = serde_json::from_slice(&mcp_output).unwrap();
    assert_eq!(
        mcp_json["result"]["schema_version"],
        "forge.event_observability_history.v1"
    );
    assert_eq!(mcp_json["result"]["bucket_count"], 2);
    assert_eq!(mcp_json["result"]["summary"]["total_event_count"], 2);
    assert_eq!(
        mcp_json["result"]["summary"]["total_selected_context_bytes"],
        1350
    );
}

#[test]
fn event_improvement_policy_recommends_deterministic_and_context_repairs_for_cli_and_mcp() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    {
        let _store_handle = ForgeStore::open(&store).unwrap();
    }
    {
        let connection = Connection::open(&store).unwrap();
        connection
            .execute_batch(
                r#"
                INSERT INTO global_events (
                    source, source_id, workflow_id, kind, origin, status,
                    organization_id, brand_id, product_id, user_id, channel_id,
                    tenant_context_json, data_json, created_at
                )
                VALUES
                (
                    'policy_seed', 'policy-001', 'wf_policy',
                    'ai_executor_completed', 'codex', 'recorded',
                    'org-demo', 'brand-demo', 'product-demo', 'user-demo', 'web',
                    '{}',
                    '{"node_id":"node-render","addon_id":"forge.addon.content","duration_ms":500,"retry_count":0,"wait_seconds":5,"context":{"effective_budget":1000,"routing_summary":{"selected_bytes":950,"remaining_budget":50},"memory_policy":{"memory_level":"standard","memory_scope":"project"}}}',
                    '2026-06-10T10:00:00Z'
                ),
                (
                    'policy_seed', 'policy-002', 'wf_policy',
                    'ai_executor_completed', 'codex', 'recorded',
                    'org-demo', 'brand-demo', 'product-demo', 'user-demo', 'web',
                    '{}',
                    '{"node_id":"node-render","addon_id":"forge.addon.content","duration_ms":600,"retry_count":0,"wait_seconds":5,"context":{"effective_budget":1000,"routing_summary":{"selected_bytes":960,"remaining_budget":40},"memory_policy":{"memory_level":"standard","memory_scope":"project"}}}',
                    '2026-06-10T10:05:00Z'
                ),
                (
                    'policy_seed', 'policy-003', 'wf_policy',
                    'ai_executor_completed', 'codex', 'recorded',
                    'org-demo', 'brand-demo', 'product-demo', 'user-demo', 'web',
                    '{}',
                    '{"node_id":"node-render","addon_id":"forge.addon.content","duration_ms":700,"retry_count":0,"wait_seconds":5,"context":{"effective_budget":1000,"routing_summary":{"selected_bytes":940,"remaining_budget":60},"memory_policy":{"memory_level":"standard","memory_scope":"project"}}}',
                    '2026-06-10T10:10:00Z'
                ),
                (
                    'policy_seed', 'policy-004', 'wf_policy',
                    'executor_retry_observed', 'opencode', 'recorded',
                    'org-demo', 'brand-demo', 'product-demo', 'user-demo', 'web',
                    '{}',
                    '{"node_id":"node-payment","addon_id":"forge.addon.billing","duration_ms":100,"retry_count":2,"wait_seconds":40,"context":{"effective_budget":1000,"routing_summary":{"selected_bytes":700,"remaining_budget":300},"memory_policy":{"memory_level":"short_term","memory_scope":"organization"}}}',
                    '2026-06-10T10:15:00Z'
                ),
                (
                    'policy_seed', 'policy-005', 'wf_policy',
                    'executor_retry_observed', 'opencode', 'recorded',
                    'org-demo', 'brand-demo', 'product-demo', 'user-demo', 'web',
                    '{}',
                    '{"node_id":"node-payment","addon_id":"forge.addon.billing","duration_ms":100,"retry_count":2,"wait_seconds":40,"context":{"effective_budget":1000,"routing_summary":{"selected_bytes":720,"remaining_budget":280},"memory_policy":{"memory_level":"short_term","memory_scope":"organization"}}}',
                    '2026-06-10T10:20:00Z'
                );
                "#,
            )
            .unwrap();
    }
    let _store_handle = ForgeStore::open(&store).unwrap();

    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "improvement-policy",
            "--workflow",
            "wf_policy",
            "--min-events",
            "2",
            "--min-duration-ms",
            "1000",
            "--min-retries",
            "3",
            "--min-context-pressure-bps",
            "9000",
            "--min-wait-seconds",
            "60",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["schema_version"], "forge.event_improvement_policy.v1");
    assert_eq!(json["status"], "event_improvement_policy_recommended");
    assert_eq!(json["index_source"], "sqlite_materialized");
    assert_eq!(json["thresholds"]["min_event_count"], 2);
    assert_eq!(json["summary"]["total_event_count"], 5);
    assert_eq!(json["summary"]["max_context_pressure_bps"], 9600);
    assert!(json["recommendation_count"].as_u64().unwrap() >= 4);

    let recommendations = json["recommendations"].as_array().unwrap();
    let deterministic = recommendations
        .iter()
        .find(|recommendation| {
            recommendation["kind"] == "deterministic_node_candidate"
                && recommendation["scope"] == "node"
                && recommendation["node_ref"] == "node-render"
        })
        .unwrap();
    assert_eq!(
        deterministic["recommended_policy"],
        "prefer_deterministic_node"
    );
    assert_eq!(deterministic["ai_signal_count"], 3);
    assert_eq!(deterministic["total_duration_ms"], 1800);
    assert!(deterministic["suggested_commands"]
        .as_array()
        .unwrap()
        .iter()
        .any(|command| command.as_array().unwrap()[1] == "workflow"));

    let context_hotspot = recommendations
        .iter()
        .find(|recommendation| {
            recommendation["kind"] == "context_pressure_hotspot"
                && recommendation["scope"] == "node"
                && recommendation["node_ref"] == "node-render"
        })
        .unwrap();
    assert_eq!(
        context_hotspot["recommended_policy"],
        "tighten_context_routing"
    );
    assert_eq!(context_hotspot["max_context_pressure_bps"], 9600);

    let retry_hotspot = recommendations
        .iter()
        .find(|recommendation| {
            recommendation["kind"] == "retry_hotspot"
                && recommendation["scope"] == "node"
                && recommendation["node_ref"] == "node-payment"
        })
        .unwrap();
    assert_eq!(retry_hotspot["total_retry_count"], 4);

    let mcp_input = serde_json::json!({
        "workflow_id": "wf_policy",
        "min_events": 2,
        "min_duration_ms": 1000,
        "min_retries": 3,
        "min_context_pressure_bps": 9000,
        "min_wait_seconds": 60,
        "limit": 2
    });
    let mcp_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.events.improvement_policy",
            "--input",
            &mcp_input.to_string(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let mcp_json: Value = serde_json::from_slice(&mcp_output).unwrap();
    assert_eq!(
        mcp_json["result"]["schema_version"],
        "forge.event_improvement_policy.v1"
    );
    assert_eq!(mcp_json["result"]["recommendation_count"], 2);
    assert_eq!(
        mcp_json["result"]["recommendations"][0]["workflow_id"],
        "wf_policy"
    );
}

#[test]
fn tenant_index_tracks_async_run_resources() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let request_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "request",
            "start",
            "--goal",
            "Executar fluxo assíncrono com índice tenant físico",
            "--origin",
            "test",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let request_json: Value = serde_json::from_slice(&request_output).unwrap();
    let workflow_id = request_json["workflow_id"].as_str().unwrap();
    let run_id = request_json["run_id"].as_str().unwrap();

    let tenant_index_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "identity",
            "tenant-index",
            "--resource-type",
            "run",
            "--workflow",
            workflow_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let tenant_index_json: Value = serde_json::from_slice(&tenant_index_output).unwrap();
    assert_eq!(tenant_index_json["schema_version"], "forge.tenant_index.v1");
    assert_eq!(tenant_index_json["resource_count"], 1);
    assert_eq!(tenant_index_json["resources"][0]["resource_id"], run_id);
    assert_eq!(
        tenant_index_json["resources"][0]["organization_id"],
        "default-org"
    );

    let audit_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "identity",
            "tenant-audit",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let audit_json: Value = serde_json::from_slice(&audit_output).unwrap();
    assert_eq!(audit_json["schema_version"], "forge.tenant_audit.v1");
    assert_eq!(audit_json["status"], "tenant_index_complete");
    assert_eq!(audit_json["missing_count"], 0);

    let mcp_audit_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.identity.tenant_audit",
            "--input",
            "{}",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let mcp_audit_json: Value = serde_json::from_slice(&mcp_audit_output).unwrap();
    assert_eq!(
        mcp_audit_json["result"]["schema_version"],
        "forge.tenant_audit.v1"
    );
    assert_eq!(mcp_audit_json["result"]["status"], "tenant_index_complete");
}

#[test]
fn tenant_policy_enforce_blocks_workflow_without_active_membership() {
    let temp = tempdir().unwrap();
    let forge_dir = temp.path().join(".forge");
    fs::create_dir_all(&forge_dir).unwrap();
    fs::write(
        forge_dir.join("operating-context.yaml"),
        r#"
organization:
  scope: organization
  id: policy-org
  label: Policy Org
brand:
  scope: brand
  id: policy-brand
  label: Policy Brand
product:
  scope: product
  id: policy-product
  label: Policy Product
user:
  scope: user
  id: policy-user
  label: Policy User
channel:
  scope: channel
  id: local_cli
  label: Local CLI
"#,
    )
    .unwrap();
    let store = temp.path().join("forge.sqlite");
    let plan_output = forge()
        .current_dir(temp.path())
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Operar workflow multi-tenant sem membership sincronizado",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let plan_json: Value = serde_json::from_slice(&plan_output).unwrap();
    let workflow_id = plan_json["workflow_id"].as_str().unwrap();

    let policy_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "identity",
            "tenant-policy",
            "--workflow",
            workflow_id,
            "--mode",
            "enforce",
            "--output",
            "json",
        ])
        .assert()
        .failure()
        .get_output()
        .stdout
        .clone();
    let policy_json: Value = serde_json::from_slice(&policy_output).unwrap();
    assert_eq!(policy_json["schema_version"], "forge.tenant_policy.v1");
    assert_eq!(policy_json["status"], "tenant_policy_denied");
    assert_eq!(policy_json["allowed"], false);
    assert!(policy_json["decisions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|decision| decision["gate"] == "active_membership" && decision["status"] == "denied"));
}

#[test]
fn plan_enforces_operating_context_membership_when_project_enables_tenant_policy() {
    let temp = tempdir().unwrap();
    let forge_dir = temp.path().join(".forge");
    fs::create_dir_all(&forge_dir).unwrap();
    fs::write(
        forge_dir.join("operating-context.yaml"),
        r#"
organization:
  scope: organization
  id: enforced-org
  label: Enforced Org
brand:
  scope: brand
  id: enforced-brand
  label: Enforced Brand
product:
  scope: product
  id: enforced-product
  label: Enforced Product
user:
  scope: user
  id: enforced-user
  label: Enforced User
channel:
  scope: channel
  id: local_cli
  label: Local CLI
tenant_policy_mode: enforce
"#,
    )
    .unwrap();
    let store = temp.path().join("forge.sqlite");

    let denied = forge()
        .current_dir(temp.path())
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Criar workflow com enforcement multi-tenant",
            "--output",
            "json",
        ])
        .assert()
        .failure()
        .get_output()
        .stderr
        .clone();
    let denied_stderr = String::from_utf8(denied).unwrap();
    assert!(denied_stderr.contains("multi-tenant enforcement blocked plan"));

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "identity",
            "sync",
            "--project-root",
            temp.path().to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .success();

    let planned = forge()
        .current_dir(temp.path())
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Criar workflow com enforcement multi-tenant",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let planned_json: Value = serde_json::from_slice(&planned).unwrap();
    assert_eq!(
        planned_json["intent"]["operating_context"]["tenant_policy_mode"],
        "enforce"
    );
    assert_eq!(
        planned_json["intent"]["operating_context"]["organization"]["id"],
        "enforced-org"
    );
}

#[test]
fn tenant_policy_resolves_cross_channel_identity_links() {
    let temp = tempdir().unwrap();
    let forge_dir = temp.path().join(".forge");
    fs::create_dir_all(&forge_dir).unwrap();
    let store = temp.path().join("forge.sqlite");
    fs::write(
        forge_dir.join("operating-context.yaml"),
        r#"
organization:
  scope: organization
  id: linked-org
  label: Linked Org
brand:
  scope: brand
  id: linked-brand
  label: Linked Brand
product:
  scope: product
  id: linked-product
  label: Linked Product
user:
  scope: user
  id: web-arthur
  label: Arthur Web
channel:
  scope: channel
  id: web
  label: Web
tenant_policy_mode: enforce
"#,
    )
    .unwrap();

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "identity",
            "sync",
            "--project-root",
            temp.path().to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .success();

    let link_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "identity",
            "link",
            "--left-scope",
            "telegram",
            "--left-id",
            "tg-123",
            "--right-scope",
            "user",
            "--right-id",
            "web-arthur",
            "--reason",
            "same operator across channels",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let link_json: Value = serde_json::from_slice(&link_output).unwrap();
    assert_eq!(link_json["schema_version"], "forge.identity_link.v1");
    assert_eq!(link_json["status"], "identity_linked");
    assert_eq!(link_json["resolved"]["identity_count"], 2);

    let resolve_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.identity.resolve",
            "--input",
            r#"{"scope":"telegram","id":"tg-123"}"#,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let resolve_json: Value = serde_json::from_slice(&resolve_output).unwrap();
    assert_eq!(
        resolve_json["result"]["schema_version"],
        "forge.identity_resolve.v1"
    );
    assert_eq!(
        resolve_json["result"]["canonical_identity"]["scope"],
        "user"
    );
    assert_eq!(
        resolve_json["result"]["canonical_identity"]["id"],
        "web-arthur"
    );

    fs::write(
        forge_dir.join("operating-context.yaml"),
        r#"
organization:
  scope: organization
  id: linked-org
  label: Linked Org
brand:
  scope: brand
  id: linked-brand
  label: Linked Brand
product:
  scope: product
  id: linked-product
  label: Linked Product
user:
  scope: telegram
  id: tg-123
  label: Arthur Telegram
channel:
  scope: channel
  id: telegram
  label: Telegram
tenant_policy_mode: enforce
"#,
    )
    .unwrap();

    let planned = forge()
        .current_dir(temp.path())
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Operar usando identidade do Telegram vinculada",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let planned_json: Value = serde_json::from_slice(&planned).unwrap();
    let workflow_id = planned_json["workflow_id"].as_str().unwrap();
    assert_eq!(
        planned_json["intent"]["operating_context"]["user"]["scope"],
        "telegram"
    );

    let policy_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "identity",
            "tenant-policy",
            "--workflow",
            workflow_id,
            "--mode",
            "enforce",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let policy_json: Value = serde_json::from_slice(&policy_output).unwrap();
    assert_eq!(policy_json["schema_version"], "forge.tenant_policy.v1");
    assert_eq!(policy_json["allowed"], true);
    assert_eq!(policy_json["active_membership_count"], 1);
    assert!(policy_json["membership_roles"]
        .as_array()
        .unwrap()
        .iter()
        .any(|role| role == "operator"));

    let unlink_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "identity",
            "unlink",
            "--left-scope",
            "telegram",
            "--left-id",
            "tg-123",
            "--right-scope",
            "user",
            "--right-id",
            "web-arthur",
            "--reason",
            "separate profiles",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let unlink_json: Value = serde_json::from_slice(&unlink_output).unwrap();
    assert_eq!(unlink_json["status"], "identity_unlinked");
    assert_eq!(unlink_json["resolved"]["identity_count"], 1);

    let blocked_policy = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "identity",
            "tenant-policy",
            "--workflow",
            workflow_id,
            "--mode",
            "enforce",
            "--output",
            "json",
        ])
        .assert()
        .failure()
        .get_output()
        .stdout
        .clone();
    let blocked_policy_json: Value = serde_json::from_slice(&blocked_policy).unwrap();
    assert_eq!(blocked_policy_json["allowed"], false);
    assert!(blocked_policy_json["decisions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|decision| decision["gate"] == "active_membership" && decision["status"] == "denied"));
}

#[test]
fn tenant_policy_enforcement_blocks_runtime_handoff_and_mutation_after_membership_loss() {
    let temp = tempdir().unwrap();
    let forge_dir = temp.path().join(".forge");
    fs::create_dir_all(&forge_dir).unwrap();
    fs::write(
        forge_dir.join("operating-context.yaml"),
        r#"
organization:
  scope: organization
  id: runtime-org
  label: Runtime Org
brand:
  scope: brand
  id: runtime-brand
  label: Runtime Brand
product:
  scope: product
  id: runtime-product
  label: Runtime Product
user:
  scope: user
  id: runtime-user
  label: Runtime User
channel:
  scope: channel
  id: local_cli
  label: Local CLI
tenant_policy_mode: enforce
"#,
    )
    .unwrap();
    let store = temp.path().join("forge.sqlite");

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "identity",
            "sync",
            "--project-root",
            temp.path().to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .success();

    let request_output = forge()
        .current_dir(temp.path())
        .args([
            "--store",
            store.to_str().unwrap(),
            "request",
            "start",
            "--goal",
            "Executar fluxo assíncrono com enforcement multi-tenant",
            "--origin",
            "test",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let request_json: Value = serde_json::from_slice(&request_output).unwrap();
    let workflow_id = request_json["workflow_id"].as_str().unwrap();
    let run_id = request_json["run_id"].as_str().unwrap();
    assert_eq!(
        request_json["handoff_contract"]["workflow_id"],
        Value::String(workflow_id.to_string())
    );

    let connection = Connection::open(&store).unwrap();
    connection
        .execute(
            "DELETE FROM identity_memberships WHERE subject_id = ?1",
            ["runtime-user"],
        )
        .unwrap();

    let drive_stderr = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "request",
            "drive",
            "--run",
            run_id,
            "--executor",
            "codex",
            "--output",
            "json",
        ])
        .assert()
        .failure()
        .get_output()
        .stderr
        .clone();
    assert!(String::from_utf8(drive_stderr)
        .unwrap()
        .contains("multi-tenant enforcement blocked request drive"));

    let handoff_stderr = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "task",
            "handoff",
            "--workflow",
            workflow_id,
            "--task",
            "task-001",
            "--executor",
            "codex",
            "--output",
            "json",
        ])
        .assert()
        .failure()
        .get_output()
        .stderr
        .clone();
    assert!(String::from_utf8(handoff_stderr)
        .unwrap()
        .contains("multi-tenant enforcement blocked task handoff"));

    let update_stderr = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "workflow",
            "update-goal",
            "--workflow",
            workflow_id,
            "--goal",
            "Novo objetivo bloqueado por policy",
            "--origin",
            "test",
            "--output",
            "json",
        ])
        .assert()
        .failure()
        .get_output()
        .stderr
        .clone();
    assert!(String::from_utf8(update_stderr)
        .unwrap()
        .contains("multi-tenant enforcement blocked workflow goal update"));

    let mcp_context_input = format!(r#"{{"workflow_id":"{workflow_id}","task_id":"task-001"}}"#);
    let mcp_context_stderr = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.context.request",
            "--input",
            &mcp_context_input,
            "--output",
            "json",
        ])
        .assert()
        .failure()
        .get_output()
        .stderr
        .clone();
    assert!(String::from_utf8(mcp_context_stderr)
        .unwrap()
        .contains("multi-tenant enforcement blocked context request"));
}

#[test]
fn inbound_event_inbox_routes_start_workflow_without_existing_workflow() {
    let temp = tempdir().unwrap();
    let forge_dir = temp.path().join(".forge");
    fs::create_dir_all(&forge_dir).unwrap();
    fs::write(
        forge_dir.join("operating-context.yaml"),
        r#"
organization:
  scope: organization
  id: event-org
  label: Event Org
brand:
  scope: brand
  id: event-brand
  label: Event Brand
product:
  scope: product
  id: event-product
  label: Event Product
"#,
    )
    .unwrap();

    let store = temp.path().join("forge.sqlite");
    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "authorize-permission",
            "--addon",
            "forge.addon.notification",
            "--permission",
            "telegram.send_message",
            "--risk",
            "medium",
            "--approved-by",
            "test",
            "--source",
            "test",
            "--output",
            "json",
        ])
        .assert()
        .success();
    let ingest_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "ingest",
            "--origin",
            "telegram",
            "--action",
            "start_workflow",
            "--input",
            r#"{"goal":"Criar workflow persistente acionado por evento","auth_verified":true,"schema":"telegram.update.v1"}"#,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let ingest_json: Value = serde_json::from_slice(&ingest_output).unwrap();
    assert_eq!(ingest_json["schema_version"], "forge.event_ingest.v1");
    assert_eq!(ingest_json["event"]["status"], "pending");
    let event_id = ingest_json["event"]["id"].as_str().unwrap();

    let inbox_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "inbox",
            "--status",
            "pending",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let inbox_json: Value = serde_json::from_slice(&inbox_output).unwrap();
    assert_eq!(inbox_json["schema_version"], "forge.event_inbox.v1");
    assert_eq!(inbox_json["event_count"], 1);

    let global_timeline_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "timeline",
            "--limit",
            "10",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let global_timeline_json: Value = serde_json::from_slice(&global_timeline_output).unwrap();
    assert!(global_timeline_json["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|event| event["kind"] == "inbound_event_ingested"
            && event["workflow_id"] == "_global"
            && event["data"]["global_event"]["source"] == "event_inbox"
            && event["origin"] == "telegram"));

    let route_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "route",
            "--event",
            event_id,
            "--project-root",
            temp.path().to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let route_json: Value = serde_json::from_slice(&route_output).unwrap();
    assert_eq!(route_json["schema_version"], "forge.event_route.v1");
    assert_eq!(route_json["status"], "event_routed");
    assert_eq!(
        route_json["adapter_policy"]["schema_version"],
        "forge.event_adapter_policy.v1"
    );
    assert_eq!(route_json["adapter_policy"]["status"], "matched");
    assert_eq!(
        route_json["adapter_policy"]["matched_adapter"]["adapter"]["id"],
        "telegram.bot_updates"
    );
    assert_eq!(
        route_json["adapter_policy"]["matched_adapter"]["permission_gate"]["allowed"],
        true
    );
    assert_eq!(
        route_json["created_workflow"]["intent"]["operating_context"]["organization"]["id"],
        "event-org"
    );
    assert_eq!(route_json["event"]["status"], "routed");
    let workflow_id = route_json["workflow_id"].as_str().unwrap();
    let task_id = route_json["created_workflow"]["tasks"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let events_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "list",
            "--workflow",
            workflow_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let events_json: Value = serde_json::from_slice(&events_output).unwrap();
    assert_eq!(events_json["events"][0]["kind"], "inbound_event_routed");
    assert_eq!(
        events_json["tenant_context"]["organization"]["id"],
        "event-org"
    );
    let workflow_timeline_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "timeline",
            "--workflow",
            workflow_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let workflow_timeline_json: Value = serde_json::from_slice(&workflow_timeline_output).unwrap();
    assert!(workflow_timeline_json["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|event| event["kind"] == "inbound_event_routed"
            && event["data"]["global_event"]["source"] == "workflow_event"
            && event["tenant_context"]["organization"]["id"] == "event-org"));

    let scan_ingest_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "ingest",
            "--origin",
            "telegram",
            "--action",
            "start_workflow",
            "--input",
            r#"{"goal":"Criar segundo workflow pelo worker de inbox","auth_verified":true,"schema":"telegram.update.v1"}"#,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let scan_ingest_json: Value = serde_json::from_slice(&scan_ingest_output).unwrap();
    let scan_event_id = scan_ingest_json["event"]["id"].as_str().unwrap();
    let scan_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "scan",
            "--project-root",
            temp.path().to_str().unwrap(),
            "--limit",
            "5",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let scan_json: Value = serde_json::from_slice(&scan_output).unwrap();
    assert_eq!(scan_json["schema_version"], "forge.event_worker.v1");
    assert_eq!(scan_json["status"], "event_worker_scanned");
    assert_eq!(scan_json["scanned_count"], 1);
    assert_eq!(scan_json["routed_count"], 1);
    assert_eq!(scan_json["failed_count"], 0);
    assert_eq!(scan_json["events"][0]["event_id"], scan_event_id);
    assert_eq!(scan_json["events"][0]["route_decision"], "start_workflow");

    let scan_mcp_input = serde_json::json!({
        "project_root": temp.path().display().to_string(),
        "limit": 5,
    })
    .to_string();
    let scan_mcp_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.events.scan",
            "--input",
            scan_mcp_input.as_str(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let scan_mcp_json: Value = serde_json::from_slice(&scan_mcp_output).unwrap();
    assert_eq!(
        scan_mcp_json["result"]["schema_version"],
        "forge.event_worker.v1"
    );
    assert_eq!(scan_mcp_json["result"]["scanned_count"], 0);

    for goal in [
        "Criar terceiro workflow pelo loop de inbox",
        "Criar quarto workflow pelo loop de inbox",
    ] {
        forge()
            .args([
                "--store",
                store.to_str().unwrap(),
                "events",
                "ingest",
                "--origin",
                "telegram",
                "--action",
                "start_workflow",
                "--input",
                &format!(
                    r#"{{"goal":"{goal}","auth_verified":true,"schema":"telegram.update.v1"}}"#
                ),
                "--output",
                "json",
            ])
            .assert()
            .success();
    }
    let worker_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "worker",
            "--project-root",
            temp.path().to_str().unwrap(),
            "--limit",
            "1",
            "--max-cycles",
            "2",
            "--interval-seconds",
            "0",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let worker_json: Value = serde_json::from_slice(&worker_output).unwrap();
    assert_eq!(worker_json["schema_version"], "forge.event_worker_loop.v1");
    assert_eq!(worker_json["status"], "event_worker_loop_completed");
    assert_eq!(worker_json["cycle_count"], 2);
    assert_eq!(worker_json["scanned_count"], 2);
    assert_eq!(worker_json["routed_count"], 2);
    assert_eq!(worker_json["failed_count"], 0);
    assert_eq!(worker_json["cycles"][0]["report"]["scanned_count"], 1);
    assert_eq!(worker_json["cycles"][1]["report"]["scanned_count"], 1);

    let worker_mcp_input = serde_json::json!({
        "project_root": temp.path().display().to_string(),
        "limit": 1,
        "max_cycles": 3,
        "interval_seconds": 0,
        "idle_exit": true,
    })
    .to_string();
    let worker_mcp_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.events.worker",
            "--input",
            worker_mcp_input.as_str(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let worker_mcp_json: Value = serde_json::from_slice(&worker_mcp_output).unwrap();
    assert_eq!(
        worker_mcp_json["result"]["schema_version"],
        "forge.event_worker_loop.v1"
    );
    assert_eq!(
        worker_mcp_json["result"]["status"],
        "event_worker_loop_idle"
    );
    assert_eq!(worker_mcp_json["result"]["cycle_count"], 1);
    assert_eq!(worker_mcp_json["result"]["stopped_reason"], "idle_exit");

    for (action, expected_prefix, expected_status) in [
        ("pause_workflow", "pause_workflow revision", "paused"),
        ("resume_workflow", "resume_workflow revision", "running"),
    ] {
        let payload = format!(r#"{{"workflow_id":"{workflow_id}"}}"#);
        let ingest_output = forge()
            .args([
                "--store",
                store.to_str().unwrap(),
                "events",
                "ingest",
                "--origin",
                "api",
                "--action",
                action,
                "--input",
                &payload,
                "--output",
                "json",
            ])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let ingest_json: Value = serde_json::from_slice(&ingest_output).unwrap();
        let event_id = ingest_json["event"]["id"].as_str().unwrap();
        let route_output = forge()
            .args([
                "--store",
                store.to_str().unwrap(),
                "events",
                "route",
                "--event",
                event_id,
                "--project-root",
                temp.path().to_str().unwrap(),
                "--output",
                "json",
            ])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let route_json: Value = serde_json::from_slice(&route_output).unwrap();
        assert!(route_json["route_decision"]
            .as_str()
            .unwrap()
            .starts_with(expected_prefix));
        assert_eq!(route_json["created_workflow"]["status"], expected_status);
    }

    let modify_payload = format!(
        r#"{{"workflow_id":"{workflow_id}","new_goal":"Criar workflow persistente atualizado por evento"}}"#
    );
    let modify_ingest_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "ingest",
            "--origin",
            "api",
            "--action",
            "modify_workflow",
            "--input",
            &modify_payload,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let modify_ingest_json: Value = serde_json::from_slice(&modify_ingest_output).unwrap();
    let modify_event_id = modify_ingest_json["event"]["id"].as_str().unwrap();

    let modify_route_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "route",
            "--event",
            modify_event_id,
            "--project-root",
            temp.path().to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let modify_route_json: Value = serde_json::from_slice(&modify_route_output).unwrap();
    assert_eq!(modify_route_json["status"], "event_routed");
    assert!(modify_route_json["route_decision"]
        .as_str()
        .unwrap()
        .starts_with("modify_workflow revision"));
    assert_eq!(
        modify_route_json["workflow_goal"],
        "Criar workflow persistente atualizado por evento"
    );

    let modified_events_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "list",
            "--workflow",
            workflow_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let modified_events_json: Value = serde_json::from_slice(&modified_events_output).unwrap();
    assert!(modified_events_json["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|event| event["kind"] == "workflow_goal_updated"));

    let evidence_path = temp.path().join("event-evidence.md");
    fs::write(&evidence_path, "evidência operacional anexada pelo inbox").unwrap();
    let attach_payload = serde_json::json!({
        "workflow_id": workflow_id,
        "continue_action": "attach_artifact",
        "path": evidence_path,
        "kind": "evidence"
    })
    .to_string();
    let attach_ingest_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "ingest",
            "--origin",
            "api",
            "--action",
            "continue_workflow",
            "--input",
            &attach_payload,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let attach_ingest_json: Value = serde_json::from_slice(&attach_ingest_output).unwrap();
    let attach_event_id = attach_ingest_json["event"]["id"].as_str().unwrap();
    let attach_route_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "route",
            "--event",
            attach_event_id,
            "--project-root",
            temp.path().to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let attach_route_json: Value = serde_json::from_slice(&attach_route_output).unwrap();
    assert_eq!(
        attach_route_json["route_decision"],
        "continue_workflow:attach_artifact"
    );
    assert_eq!(
        attach_route_json["route_result"]["status"],
        "artifact_attached"
    );
    assert_eq!(attach_route_json["event"]["status"], "routed");

    let checkpoint_payload = serde_json::json!({
        "workflow_id": workflow_id,
        "continue_action": "checkpoint",
        "task_id": task_id,
        "state": "waiting_external_partner",
        "summary": "checkpoint registrado por evento externo",
        "context_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    })
    .to_string();
    let checkpoint_ingest_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "ingest",
            "--origin",
            "api",
            "--action",
            "continue",
            "--input",
            &checkpoint_payload,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let checkpoint_ingest_json: Value = serde_json::from_slice(&checkpoint_ingest_output).unwrap();
    let checkpoint_event_id = checkpoint_ingest_json["event"]["id"].as_str().unwrap();
    let checkpoint_route_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "route",
            "--event",
            checkpoint_event_id,
            "--project-root",
            temp.path().to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let checkpoint_route_json: Value = serde_json::from_slice(&checkpoint_route_output).unwrap();
    assert_eq!(
        checkpoint_route_json["route_decision"],
        "continue_workflow:checkpoint"
    );
    assert_eq!(
        checkpoint_route_json["route_result"]["status"],
        "checkpoint_recorded"
    );

    let continued_events_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "list",
            "--workflow",
            workflow_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let continued_events_json: Value = serde_json::from_slice(&continued_events_output).unwrap();
    let event_kinds = continued_events_json["events"].as_array().unwrap();
    assert!(event_kinds
        .iter()
        .any(|event| event["kind"] == "artifact_attached"));
    assert!(event_kinds
        .iter()
        .any(|event| event["kind"] == "task_checkpoint_recorded"));

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "run",
            "--workflow",
            workflow_id,
            "--simulate",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let complete_payload = format!(r#"{{"workflow_id":"{workflow_id}"}}"#);
    let complete_ingest_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "ingest",
            "--origin",
            "api",
            "--action",
            "complete_workflow",
            "--input",
            &complete_payload,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let complete_ingest_json: Value = serde_json::from_slice(&complete_ingest_output).unwrap();
    let complete_event_id = complete_ingest_json["event"]["id"].as_str().unwrap();
    let complete_route_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "route",
            "--event",
            complete_event_id,
            "--project-root",
            temp.path().to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let complete_route_json: Value = serde_json::from_slice(&complete_route_output).unwrap();
    assert!(complete_route_json["route_decision"]
        .as_str()
        .unwrap()
        .starts_with("complete_workflow revision"));
    assert_eq!(
        complete_route_json["created_workflow"]["status"],
        "completed"
    );
}

#[test]
fn event_route_enforces_declared_addon_adapter_permission_policy() {
    let temp = tempdir().unwrap();
    let forge_dir = temp.path().join(".forge");
    let addon_dir = forge_dir.join("addons");
    fs::create_dir_all(&addon_dir).unwrap();
    fs::write(
        forge_dir.join("operating-context.yaml"),
        r#"
organization:
  scope: organization
  id: adapter-org
  label: Adapter Org
brand:
  scope: brand
  id: adapter-brand
  label: Adapter Brand
product:
  scope: product
  id: adapter-product
  label: Adapter Product
"#,
    )
    .unwrap();
    fs::write(
        addon_dir.join("partner.yaml"),
        r#"
id: forge.addon.partner_events
name: Partner Events
version: 0.1.0
permissions:
  - id: partner.ingest_webhook
    description: Ingest signed partner webhook events.
    risk: high
    requires_human_approval: true
    tools:
      - webhook_receiver
    resources:
      - partner_payload
    integrations:
      - partner.webhook
    actions:
      - start_workflow
    tenant_scopes:
      - organization
capabilities:
  - id: partner_event_workflow
    title: Partner event workflow
    keywords:
      - partner event
event_adapters:
  - id: partner.webhook_ingress
    title: Partner Webhook Ingress
    transport: webhook
    direction: ingress
    origins:
      - partner_gateway
    actions:
      - start_workflow
    event_types:
      - partner.workflow_requested
    schema: partner.event.v1
    auth: hmac
    permissions:
      - partner.ingest_webhook
"#,
    )
    .unwrap();

    let store = temp.path().join("forge.sqlite");
    let ingest_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "ingest",
            "--origin",
            "partner_gateway",
            "--action",
            "start_workflow",
            "--input",
            r#"{"goal":"Criar workflow para partner event","transport":"webhook","schema":"partner.event.v1","auth_verified":true}"#,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let ingest_json: Value = serde_json::from_slice(&ingest_output).unwrap();
    let event_id = ingest_json["event"]["id"].as_str().unwrap();

    let blocked_route = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "route",
            "--event",
            event_id,
            "--project-root",
            temp.path().to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .failure()
        .get_output()
        .stderr
        .clone();
    let blocked_route = String::from_utf8(blocked_route).unwrap();
    assert!(blocked_route.contains("inbound event blocked by adapter policy"));
    assert!(blocked_route.contains("missing_human_approval"));

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "authorize-permission",
            "--addon",
            "forge.addon.partner_events",
            "--permission",
            "partner.ingest_webhook",
            "--risk",
            "high",
            "--approved-by",
            "test",
            "--source",
            "test",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let route_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "route",
            "--event",
            event_id,
            "--project-root",
            temp.path().to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let route_json: Value = serde_json::from_slice(&route_output).unwrap();
    assert_eq!(route_json["status"], "event_routed");
    assert_eq!(route_json["adapter_policy"]["status"], "matched");
    assert_eq!(
        route_json["adapter_policy"]["matched_adapter"]["addon_id"],
        "forge.addon.partner_events"
    );
    assert_eq!(
        route_json["adapter_policy"]["matched_adapter"]["permission_gate"]["resources"],
        serde_json::json!(["partner_payload"])
    );
    assert_eq!(
        route_json["created_workflow"]["intent"]["operating_context"]["organization"]["id"],
        "adapter-org"
    );
}

#[test]
fn plan_loads_project_operating_context_and_project_addons() {
    let temp = tempdir().unwrap();
    let forge_dir = temp.path().join(".forge");
    let addon_dir = forge_dir.join("addons");
    fs::create_dir_all(&addon_dir).unwrap();
    fs::write(
        forge_dir.join("operating-context.yaml"),
        r#"
operating_context:
  organization:
    scope: organization
    id: digital-directive
    label: Digital Directive
  brand:
    scope: brand
    id: forge
    label: Forge
  product:
    scope: product
    id: forge-core
    label: Forge Core
  user:
    scope: user
    id: arthur
    label: Arthur
  channel:
    scope: channel
    id: local_cli
    label: Local CLI
  memory_scope: organization_project_session
  personality_scope: organization_workflow_node
  brand_identity:
    voice: consultivo
    tone: direto
    audience:
      - operadores
      - gestores
    values:
      - clareza
      - rigor
    terminology:
      - workflow
      - operação assistida
  design_system:
    token_source: .forge/design/tokens.json
    component_source: .forge/design/components
    guidelines:
      - usar componentes densos e operacionais
  operating_policy:
    data_classification: confidential
    memory_visibility: organization_project
    sharing_policy: private_by_default
    approval_policy: risk_based
"#,
    )
    .unwrap();
    fs::write(
        addon_dir.join("logistics.yaml"),
        r#"
id: forge.addon.logistics
name: Logistics Addon
version: 0.1.0
description: External logistics capability pack.
capabilities:
  - id: route_optimization
    title: Route optimization
    description: Optimize routes and loads for logistics.
    domains:
      - logistics
    keywords:
      - rota agrícola
    workflow_extensions:
      - route_optimization_workflow
"#,
    )
    .unwrap();
    let store = temp.path().join("forge.sqlite");
    let plan_output = forge()
        .current_dir(temp.path())
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Criar rota agrícola com restrições de carga",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let plan_json: Value = serde_json::from_slice(&plan_output).unwrap();
    assert_eq!(
        plan_json["intent"]["operating_context"]["organization"]["id"],
        "digital-directive"
    );
    assert_eq!(
        plan_json["intent"]["operating_context"]["brand_identity"]["voice"],
        "consultivo"
    );
    assert_eq!(
        plan_json["intent"]["operating_context"]["design_system"]["token_source"],
        ".forge/design/tokens.json"
    );
    assert_eq!(
        plan_json["intent"]["operating_context"]["operating_policy"]["data_classification"],
        "confidential"
    );
    assert!(plan_json["intent"]["required_capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .any(|capability| capability["id"] == "route_optimization"
            && capability["source_addon"] == "forge.addon.logistics"));
    assert!(plan_json["tasks"].as_array().unwrap().iter().any(|task| {
        task["title"] == "Apply Addon workflow extension: route optimization workflow"
            && task["context_requirements"]
                .as_array()
                .unwrap()
                .iter()
                .any(|requirement| requirement == "source Addon forge.addon.logistics")
            && task["execution_policy"]["reuse_hint"]
                == "addon_extension:route_optimization_workflow"
    }));

    let identity_output = forge()
        .args([
            "identity",
            "context",
            "--project-root",
            temp.path().to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let identity_json: Value = serde_json::from_slice(&identity_output).unwrap();
    assert_eq!(
        identity_json["schema_version"],
        "forge.operating_context_load.v1"
    );
    assert_eq!(identity_json["status"], "loaded");
    assert_eq!(identity_json["context"]["brand"]["id"], "forge");

    let sync_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "identity",
            "sync",
            "--project-root",
            temp.path().to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let sync_json: Value = serde_json::from_slice(&sync_output).unwrap();
    assert_eq!(sync_json["schema_version"], "forge.identity_sync.v1");
    assert_eq!(sync_json["status"], "identity_registry_synced");
    assert_eq!(sync_json["synced_count"], 5);
    assert_eq!(sync_json["membership_count"], 1);
    assert_eq!(sync_json["memberships"][0]["subject_id"], "arthur");
    assert_eq!(
        sync_json["memberships"][0]["organization_id"],
        "digital-directive"
    );
    assert!(sync_json["memberships"][0]["permissions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|permission| permission == "workflow:create"));
    assert!(sync_json["memberships"][0]["environments"]
        .as_array()
        .unwrap()
        .iter()
        .any(|environment| environment == "local"));
    assert_eq!(
        sync_json["memberships"][0]["data"]["brand_identity"]["tone"],
        "direto"
    );

    let registry_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "identity",
            "registry",
            "--scope",
            "organization",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let registry_json: Value = serde_json::from_slice(&registry_output).unwrap();
    assert_eq!(
        registry_json["schema_version"],
        "forge.identity_registry.v1"
    );
    assert_eq!(registry_json["identity_count"], 1);
    assert_eq!(registry_json["identities"][0]["id"], "digital-directive");

    let membership_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "identity",
            "memberships",
            "--subject-scope",
            "user",
            "--subject",
            "arthur",
            "--organization",
            "digital-directive",
            "--status",
            "active",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let membership_json: Value = serde_json::from_slice(&membership_output).unwrap();
    assert_eq!(
        membership_json["schema_version"],
        "forge.identity_memberships.v1"
    );
    assert_eq!(membership_json["membership_count"], 1);
    assert_eq!(membership_json["memberships"][0]["brand_id"], "forge");
    assert_eq!(membership_json["memberships"][0]["role"], "operator");
    assert!(membership_json["memberships"][0]["permissions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|permission| permission == "workflow:execute"));

    let workflow_id = plan_json["workflow_id"].as_str().unwrap();
    let tenant_evidence = temp.path().join("tenant-evidence.md");
    fs::write(&tenant_evidence, "tenant-index artifact evidence").unwrap();
    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "workflow",
            "attach-artifact",
            "--workflow",
            workflow_id,
            "--path",
            tenant_evidence.to_str().unwrap(),
            "--kind",
            "tenant_evidence",
            "--origin",
            "test",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let tenant_index_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "identity",
            "tenant-index",
            "--workflow",
            workflow_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let tenant_index_json: Value = serde_json::from_slice(&tenant_index_output).unwrap();
    assert_eq!(tenant_index_json["schema_version"], "forge.tenant_index.v1");
    let tenant_resources = tenant_index_json["resources"].as_array().unwrap();
    for resource_type in ["workflow", "event", "artifact"] {
        assert!(tenant_resources
            .iter()
            .any(|resource| resource["resource_type"] == resource_type
                && resource["organization_id"] == "digital-directive"
                && resource["brand_id"] == "forge"
                && resource["product_id"] == "forge-core"));
    }

    let tenant_policy_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "identity",
            "tenant-policy",
            "--workflow",
            workflow_id,
            "--mode",
            "enforce",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let tenant_policy_json: Value = serde_json::from_slice(&tenant_policy_output).unwrap();
    assert_eq!(
        tenant_policy_json["schema_version"],
        "forge.tenant_policy.v1"
    );
    assert_eq!(tenant_policy_json["status"], "tenant_policy_allowed");
    assert_eq!(tenant_policy_json["allowed"], true);
    assert_eq!(tenant_policy_json["action"], "tenant policy");
    assert_eq!(tenant_policy_json["required_permission"], "context:read");
    assert_eq!(tenant_policy_json["membership_count"], 1);
    assert_eq!(tenant_policy_json["missing_tenant_index_count"], 0);

    let documentation_task = plan_json["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|task| task["title"] == "Generate documentation")
        .unwrap();
    let context_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "context",
            "--workflow",
            workflow_id,
            "--task",
            documentation_task["id"].as_str().unwrap(),
            "--budget",
            "3200",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let context_json: Value = serde_json::from_slice(&context_output).unwrap();
    assert_eq!(
        context_json["operating_context"]["brand_identity"]["voice"],
        "consultivo"
    );
    assert_eq!(context_json["persona_profile"]["brand_voice"], "consultivo");
    assert_eq!(
        context_json["persona_contract"]["design_token_source"],
        ".forge/design/tokens.json"
    );
    let memory_policy = &context_json["memory_policy"];
    assert_eq!(
        memory_policy["schema_version"],
        "forge.context.memory_policy.v1"
    );
    assert_eq!(memory_policy["memory_source"], "forge_memory_router");
    assert_eq!(
        memory_policy["memory_scope"],
        "organization_project_session"
    );
    assert_eq!(memory_policy["memory_level"], "standard");
    assert!(memory_policy["allowed_scopes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|scope| scope == "organization"));
    assert!(memory_policy["allowed_scopes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|scope| scope == "project"));
    assert!(memory_policy["allowed_scopes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|scope| scope == "processing"));
    assert_eq!(memory_policy["default_audience"], "private");
    assert_eq!(
        memory_policy["tenant_boundary"]["organization_id"],
        "digital-directive"
    );
    assert_eq!(memory_policy["requires_explicit_search"], true);
    assert_eq!(memory_policy["inline_memory_allowed"], false);
    assert!(memory_policy["default_search_command"]
        .as_array()
        .unwrap()
        .iter()
        .any(|part| part == "--organization"));
    assert!(memory_policy["default_search_command"]
        .as_array()
        .unwrap()
        .iter()
        .any(|part| part == "digital-directive"));
    assert!(context_json["included_sections"]
        .as_array()
        .unwrap()
        .iter()
        .any(|section| section == "operating_context"));
    assert!(context_json["content"]
        .as_str()
        .unwrap()
        .contains("Brand voice: consultivo"));
    assert!(context_json["prompt_packet"]["instruction_sources"]
        .as_array()
        .unwrap()
        .iter()
        .any(|source| source == "forge_operating_context"));
    assert_eq!(
        context_json["lineage"]["operating_context_sha256"]
            .as_str()
            .unwrap()
            .len(),
        64
    );

    let events_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "list",
            "--workflow",
            workflow_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let events_json: Value = serde_json::from_slice(&events_output).unwrap();
    assert_eq!(
        events_json["tenant_context"]["organization"]["id"],
        "digital-directive"
    );
    assert_eq!(
        events_json["tenant_context"]["brand_identity"]["voice"],
        "consultivo"
    );
    assert_eq!(
        events_json["tenant_context"]["operating_policy"]["memory_visibility"],
        "organization_project"
    );

    let mcp_input = format!(r#"{{"project_root":"{}"}}"#, temp.path().display());
    let mcp_output = forge()
        .args([
            "mcp",
            "call",
            "forge.identity.context",
            "--input",
            &mcp_input,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let mcp_json: Value = serde_json::from_slice(&mcp_output).unwrap();
    assert_eq!(
        mcp_json["result"]["schema_version"],
        "forge.operating_context_load.v1"
    );
    assert_eq!(mcp_json["result"]["context"]["product"]["id"], "forge-core");

    let mcp_registry_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.identity.registry",
            "--input",
            r#"{"scope":"brand","id":"forge"}"#,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let mcp_registry_json: Value = serde_json::from_slice(&mcp_registry_output).unwrap();
    assert_eq!(
        mcp_registry_json["result"]["schema_version"],
        "forge.identity_registry.v1"
    );
    assert_eq!(mcp_registry_json["result"]["identity_count"], 1);

    let mcp_tenant_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.identity.tenant_index",
            "--input",
            r#"{"organization_id":"digital-directive","resource_type":"artifact"}"#,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let mcp_tenant_json: Value = serde_json::from_slice(&mcp_tenant_output).unwrap();
    assert_eq!(
        mcp_tenant_json["result"]["schema_version"],
        "forge.tenant_index.v1"
    );
    assert_eq!(mcp_tenant_json["result"]["resource_count"], 1);

    let mcp_membership_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.identity.memberships",
            "--input",
            r#"{"subject_scope":"user","subject_id":"arthur","organization_id":"digital-directive"}"#,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let mcp_membership_json: Value = serde_json::from_slice(&mcp_membership_output).unwrap();
    assert_eq!(
        mcp_membership_json["result"]["schema_version"],
        "forge.identity_memberships.v1"
    );
    assert_eq!(mcp_membership_json["result"]["membership_count"], 1);
    assert!(
        mcp_membership_json["result"]["memberships"][0]["permissions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|permission| permission == "workflow:mutate")
    );

    let mcp_policy_input = format!(
        r#"{{"workflow_id":"{workflow_id}","mode":"audit","action":"workflow goal update"}}"#
    );
    let mcp_policy_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.identity.tenant_policy",
            "--input",
            &mcp_policy_input,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let mcp_policy_json: Value = serde_json::from_slice(&mcp_policy_output).unwrap();
    assert_eq!(
        mcp_policy_json["result"]["schema_version"],
        "forge.tenant_policy.v1"
    );
    assert_eq!(mcp_policy_json["result"]["allowed"], true);
    assert_eq!(
        mcp_policy_json["result"]["required_permission"],
        "workflow:mutate"
    );
}

#[test]
fn tenant_policy_enforces_membership_role_permissions_for_actions() {
    let temp = tempdir().unwrap();
    let forge_dir = temp.path().join(".forge");
    fs::create_dir_all(&forge_dir).unwrap();
    fs::write(
        forge_dir.join("operating-context.yaml"),
        r#"
organization:
  scope: organization
  id: permission-org
  label: Permission Org
brand:
  scope: brand
  id: permission-brand
  label: Permission Brand
product:
  scope: product
  id: permission-product
  label: Permission Product
user:
  scope: user
  id: permission-user
  label: Permission User
channel:
  scope: channel
  id: local_cli
  label: Local CLI
tenant_policy_mode: enforce
"#,
    )
    .unwrap();
    let store = temp.path().join("forge.sqlite");

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "identity",
            "sync",
            "--project-root",
            temp.path().to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .success();

    let request_output = forge()
        .current_dir(temp.path())
        .args([
            "--store",
            store.to_str().unwrap(),
            "request",
            "start",
            "--goal",
            "Executar fluxo com permissões por papel",
            "--origin",
            "test",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let request_json: Value = serde_json::from_slice(&request_output).unwrap();
    let workflow_id = request_json["workflow_id"].as_str().unwrap();

    let connection = Connection::open(&store).unwrap();
    connection
        .execute(
            "UPDATE identity_memberships SET role = 'viewer' WHERE subject_id = ?1",
            ["permission-user"],
        )
        .unwrap();

    let read_policy_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "identity",
            "tenant-policy",
            "--workflow",
            workflow_id,
            "--mode",
            "enforce",
            "--action",
            "context request",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let read_policy_json: Value = serde_json::from_slice(&read_policy_output).unwrap();
    assert_eq!(read_policy_json["allowed"], true);
    assert_eq!(read_policy_json["required_permission"], "context:read");
    assert!(read_policy_json["decisions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|decision| decision["gate"] == "membership_permission"
            && decision["status"] == "allowed"));

    let mutate_policy_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "identity",
            "tenant-policy",
            "--workflow",
            workflow_id,
            "--mode",
            "enforce",
            "--action",
            "workflow goal update",
            "--output",
            "json",
        ])
        .assert()
        .failure()
        .get_output()
        .stdout
        .clone();
    let mutate_policy_json: Value = serde_json::from_slice(&mutate_policy_output).unwrap();
    assert_eq!(mutate_policy_json["allowed"], false);
    assert_eq!(mutate_policy_json["required_permission"], "workflow:mutate");
    assert!(mutate_policy_json["decisions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|decision| decision["gate"] == "membership_permission"
            && decision["status"] == "denied"));

    let update_stderr = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "workflow",
            "update-goal",
            "--workflow",
            workflow_id,
            "--goal",
            "Este update deve ser bloqueado por papel viewer",
            "--origin",
            "test",
            "--output",
            "json",
        ])
        .assert()
        .failure()
        .get_output()
        .stderr
        .clone();
    let update_stderr = String::from_utf8(update_stderr).unwrap();
    assert!(update_stderr.contains("multi-tenant enforcement blocked workflow goal update"));
    assert!(update_stderr.contains("membership_permission"));

    let membership_update_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "identity",
            "membership-update",
            "--subject",
            "permission-user",
            "--organization",
            "permission-org",
            "--brand",
            "permission-brand",
            "--product",
            "permission-product",
            "--grant",
            "workflow:mutate",
            "--source",
            "test-cli",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let membership_update_json: Value = serde_json::from_slice(&membership_update_output).unwrap();
    assert_eq!(
        membership_update_json["schema_version"],
        "forge.identity_membership_update.v1"
    );
    assert_eq!(
        membership_update_json["after"]["permission_grants"][0],
        "workflow:mutate"
    );

    let grant_policy_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "identity",
            "tenant-policy",
            "--workflow",
            workflow_id,
            "--mode",
            "enforce",
            "--action",
            "workflow goal update",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let grant_policy_json: Value = serde_json::from_slice(&grant_policy_output).unwrap();
    assert_eq!(grant_policy_json["allowed"], true);
    assert!(grant_policy_json["granted_permissions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|permission| permission == "workflow:mutate"));

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "identity",
            "membership-update",
            "--subject",
            "permission-user",
            "--organization",
            "permission-org",
            "--brand",
            "permission-brand",
            "--product",
            "permission-product",
            "--deny",
            "workflow:mutate",
            "--source",
            "test-cli",
            "--output",
            "json",
        ])
        .assert()
        .success();
    let deny_policy_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "identity",
            "tenant-policy",
            "--workflow",
            workflow_id,
            "--mode",
            "enforce",
            "--action",
            "workflow goal update",
            "--output",
            "json",
        ])
        .assert()
        .failure()
        .get_output()
        .stdout
        .clone();
    let deny_policy_json: Value = serde_json::from_slice(&deny_policy_output).unwrap();
    assert_eq!(deny_policy_json["allowed"], false);
    assert!(deny_policy_json["denied_permissions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|permission| permission == "workflow:mutate"));
    assert!(deny_policy_json["decisions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|decision| decision["gate"] == "membership_permission"
            && decision["status"] == "denied"));

    let mcp_update_input = serde_json::json!({
        "subject_id": "permission-user",
        "organization_id": "permission-org",
        "brand_id": "permission-brand",
        "product_id": "permission-product",
        "remove_denies": ["workflow:mutate"],
        "expires_at": (Utc::now() - Duration::minutes(5)).to_rfc3339(),
        "source": "test-mcp"
    })
    .to_string();
    let mcp_update_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.identity.membership_update",
        ])
        .arg("--input")
        .arg(mcp_update_input)
        .args(["--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let mcp_update_json: Value = serde_json::from_slice(&mcp_update_output).unwrap();
    assert_eq!(
        mcp_update_json["result"]["schema_version"],
        "forge.identity_membership_update.v1"
    );
    assert_eq!(mcp_update_json["result"]["after"]["expired"], true);
    let expired_policy_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "identity",
            "tenant-policy",
            "--workflow",
            workflow_id,
            "--mode",
            "enforce",
            "--action",
            "context request",
            "--output",
            "json",
        ])
        .assert()
        .failure()
        .get_output()
        .stdout
        .clone();
    let expired_policy_json: Value = serde_json::from_slice(&expired_policy_output).unwrap();
    assert_eq!(expired_policy_json["expired_membership_count"], 1);
    assert!(expired_policy_json["decisions"]
        .as_array()
        .unwrap()
        .iter()
        .any(
            |decision| decision["gate"] == "membership_validity" && decision["status"] == "denied"
        ));
}

#[test]
fn external_addon_manifest_can_register_new_domain_capability() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let addon_dir = temp.path().join("addons");
    fs::create_dir_all(&addon_dir).unwrap();
    fs::write(
        addon_dir.join("logistics.yaml"),
        r#"
id: forge.addon.logistics
name: Logistics Addon
version: 0.1.0
description: External logistics and agriculture capability pack.
permissions:
  - id: logistics.read_route_data
    description: Read route constraints and fleet capacity.
    risk: medium
    requires_human_approval: false
    tools:
      - route_optimizer
    resources:
      - route_constraints
      - fleet_capacity
    integrations:
      - logistics_partner_api
    actions:
      - validate_route
    tenant_scopes:
      - organization
      - product
capabilities:
  - id: route_optimization
    title: Route optimization
    description: Optimize routes and loads for logistics and agriculture.
    domains:
      - logistics
      - agriculture
    keywords:
      - roteirização
      - rota agrícola
      - route optimization
    workflow_extensions:
      - route_optimization_workflow
    deliverables:
      - route optimization plan
views:
  - id: logistics.route_dashboard
    title: Logistics Route Dashboard
    surface: ops_console
    type: dashboard
    component: forge.logistics.route_dashboard
    route: /ops/addons/logistics/routes
    layout:
      zone: main
      order: 30
      width: full
      height: auto
      density: compact
    data_bindings:
      - id: route_events
        source: forge.events.timeline
        query: capability:route_optimization
        scope: organization
        refresh_seconds: 10
        required_capability: route_optimization
    actions:
      - id: logistics.validate_route
        label: Validate route
        type: command
        target: forge.addons.dispatch_contract
        method: MCP
        permission: logistics.read_route_data
        requires_confirmation: true
        payload_schema:
          - route_id
    permissions:
      - logistics.read_route_data
context_providers:
  - id: logistics.route_context
    title: Logistics route context
    source: logistics_context_files
    scopes:
      - project
    provides_sections:
      - route_constraints
      - fleet_capacity
memory_providers:
  - id: logistics.route_memory
    title: Logistics route memory
    provider_type: file_first_markdown
    scopes:
      - project
      - processing
    memory_levels:
      - MEMORY_SHORT_TERM
      - MEMORY_STANDARD
event_adapters:
  - id: logistics.webhook_ingress
    title: Logistics Webhook Ingress
    transport: webhook
    direction: ingress
    origins:
      - logistics_partner_api
    actions:
      - start_workflow
      - continue_workflow
    event_types:
      - logistics.route_requested
    schema: logistics.route_event.v1
    auth: hmac
    permissions:
      - logistics.read_route_data
runtime_contracts:
  - id: logistics.route_validator
    title: Logistics Route Validator
    contract_type: validator
    capability_id: route_optimization
    workflow_extension_id: route_optimization_workflow
    runtime: wasm
    entrypoint: logistics_route_validator.validate
    inputs:
      - route_constraints
      - fleet_capacity
    outputs:
      - validation_report
    permissions:
      - logistics.read_route_data
"#,
    )
    .unwrap();

    let catalog_output = forge()
        .args([
            "addons",
            "catalog",
            "--addon-dir",
            addon_dir.to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let catalog_json: Value = serde_json::from_slice(&catalog_output).unwrap();
    let logistics = catalog_json["addons"]
        .as_array()
        .unwrap()
        .iter()
        .find(|addon| addon["id"] == "forge.addon.logistics")
        .unwrap();
    assert_eq!(
        logistics["context_providers"][0]["id"],
        "logistics.route_context"
    );
    assert_eq!(
        logistics["memory_providers"][0]["memory_levels"],
        serde_json::json!(["MEMORY_SHORT_TERM", "MEMORY_STANDARD"])
    );
    assert_eq!(
        logistics["event_adapters"][0]["id"],
        "logistics.webhook_ingress"
    );
    assert_eq!(
        logistics["runtime_contracts"][0]["id"],
        "logistics.route_validator"
    );
    assert_eq!(
        logistics["permissions"][0]["resources"],
        serde_json::json!(["route_constraints", "fleet_capacity"])
    );
    assert_eq!(logistics["views"][0]["id"], "logistics.route_dashboard");

    let adapters_output = forge()
        .args([
            "events",
            "adapters",
            "--addon-dir",
            addon_dir.to_str().unwrap(),
            "--addon",
            "forge.addon.logistics",
            "--transport",
            "webhook",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let adapters_json: Value = serde_json::from_slice(&adapters_output).unwrap();
    assert_eq!(
        adapters_json["schema_version"],
        "forge.addon_event_adapters.v1"
    );
    assert_eq!(adapters_json["adapter_count"], 1);
    assert_eq!(
        adapters_json["adapters"][0]["adapter"]["schema"],
        "logistics.route_event.v1"
    );
    assert_eq!(
        adapters_json["adapters"][0]["permission_gate"]["resources"],
        serde_json::json!(["route_constraints", "fleet_capacity"])
    );

    let contracts_output = forge()
        .args([
            "addons",
            "contracts",
            "--addon-dir",
            addon_dir.to_str().unwrap(),
            "--addon",
            "forge.addon.logistics",
            "--type",
            "validator",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let contracts_json: Value = serde_json::from_slice(&contracts_output).unwrap();
    assert_eq!(
        contracts_json["schema_version"],
        "forge.addon_runtime_contracts.v1"
    );
    assert_eq!(contracts_json["contract_count"], 1);
    assert_eq!(
        contracts_json["contracts"][0]["contract"]["entrypoint"],
        "logistics_route_validator.validate"
    );
    assert_eq!(
        contracts_json["contracts"][0]["permission_gate"]["status"],
        "allowed"
    );
    assert_eq!(
        contracts_json["contracts"][0]["permission_gate"]["tools"],
        serde_json::json!(["route_optimizer"])
    );

    let contract_policy_output = forge()
        .args([
            "addons",
            "contract-policy",
            "--addon-dir",
            addon_dir.to_str().unwrap(),
            "--addon",
            "forge.addon.logistics",
            "--contract",
            "logistics.route_validator",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let contract_policy_json: Value = serde_json::from_slice(&contract_policy_output).unwrap();
    assert_eq!(
        contract_policy_json["schema_version"],
        "forge.addon_runtime_contract_policy.v1"
    );
    assert_eq!(
        contract_policy_json["status"],
        "runtime_contract_policy_ready"
    );
    assert_eq!(contract_policy_json["dispatch_allowed_count"], 1);
    assert_eq!(
        contract_policy_json["contracts"][0]["status"],
        "dispatch_ready"
    );
    assert_eq!(
        contract_policy_json["contracts"][0]["permission_gate"]["tools"],
        serde_json::json!(["route_optimizer"])
    );

    let mcp_contract_policy_input = format!(
        r#"{{"addon_dirs":["{}"],"addon_id":"forge.addon.logistics","contract_id":"logistics.route_validator"}}"#,
        addon_dir.display()
    );
    let mcp_contract_policy_output = forge()
        .args([
            "mcp",
            "call",
            "forge.addons.contract_policy",
            "--input",
            &mcp_contract_policy_input,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let mcp_contract_policy_json: Value =
        serde_json::from_slice(&mcp_contract_policy_output).unwrap();
    assert_eq!(
        mcp_contract_policy_json["result"]["schema_version"],
        "forge.addon_runtime_contract_policy.v1"
    );
    assert_eq!(
        mcp_contract_policy_json["result"]["contracts"][0]["dispatch_allowed"],
        true
    );

    let dispatch_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "dispatch-contract",
            "--addon-dir",
            addon_dir.to_str().unwrap(),
            "--addon",
            "forge.addon.logistics",
            "--contract",
            "logistics.route_validator",
            "--input",
            r#"{"route_id":"route-001","fleet_capacity":12}"#,
            "--source",
            "test",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let dispatch_json: Value = serde_json::from_slice(&dispatch_output).unwrap();
    assert_eq!(
        dispatch_json["schema_version"],
        "forge.addon_runtime_contract_dispatch.v1"
    );
    assert_eq!(dispatch_json["status"], "runtime_contract_dispatch_queued");
    assert_eq!(dispatch_json["queued_count"], 1);
    assert_eq!(dispatch_json["dispatches"][0]["status"], "queued");
    assert_eq!(
        dispatch_json["dispatches"][0]["input"]["route_id"],
        "route-001"
    );
    assert_eq!(
        dispatch_json["dispatches"][0]["policy"]["dispatch_allowed"],
        true
    );
    let dispatch_id = dispatch_json["dispatches"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let signing_key = SigningKey::from_bytes(&[7u8; 32]);
    let public_key_hex = test_hex_encode(signing_key.verifying_key().as_bytes());
    let worker_data = serde_json::json!({
        "endpoint": "local://wasm-worker-cli",
        "signer": "test-signer",
        "signature_scheme": "ed25519",
        "public_key_hex": public_key_hex,
    })
    .to_string();

    let worker_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "register-worker",
            "--worker",
            "wasm-worker-cli",
            "--runtime",
            "wasm",
            "--trust-level",
            "signed",
            "--source",
            "test",
            "--data",
            worker_data.as_str(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let worker_json: Value = serde_json::from_slice(&worker_output).unwrap();
    assert_eq!(
        worker_json["schema_version"],
        "forge.addon_runtime_workers.v1"
    );
    assert_eq!(worker_json["status"], "runtime_worker_registered");
    assert_eq!(worker_json["available_count"], 1);

    let mcp_worker_input = r#"{"worker_id":"wasm-worker-mcp","runtime":"wasm","status":"available","trust_level":"signed","source":"mcp-test","data":{"endpoint":"local://wasm-worker-mcp"}}"#;
    let mcp_worker_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.addons.register_worker",
            "--input",
            mcp_worker_input,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let mcp_worker_json: Value = serde_json::from_slice(&mcp_worker_output).unwrap();
    assert_eq!(
        mcp_worker_json["result"]["status"],
        "runtime_worker_registered"
    );

    let workers_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "workers",
            "--runtime",
            "wasm",
            "--status",
            "available",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let workers_json: Value = serde_json::from_slice(&workers_output).unwrap();
    assert_eq!(workers_json["worker_count"], 2);
    assert_eq!(workers_json["available_count"], 2);

    let dispatches_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "dispatches",
            "--addon",
            "forge.addon.logistics",
            "--contract",
            "logistics.route_validator",
            "--status",
            "queued",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let dispatches_json: Value = serde_json::from_slice(&dispatches_output).unwrap();
    assert_eq!(dispatches_json["dispatch_count"], 1);
    assert_eq!(
        dispatches_json["dispatches"][0]["contract_id"],
        "logistics.route_validator"
    );

    let external_run_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "run-dispatch",
            "--addon-dir",
            addon_dir.to_str().unwrap(),
            "--dispatch",
            &dispatch_id,
            "--worker",
            "test-worker",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let external_run_json: Value = serde_json::from_slice(&external_run_output).unwrap();
    assert_eq!(
        external_run_json["status"],
        "runtime_contract_dispatch_needs_external_worker"
    );
    assert_eq!(external_run_json["needs_external_worker_count"], 1);
    assert_eq!(
        external_run_json["dispatches"][0]["status"],
        "needs_external_worker"
    );
    assert_eq!(
        external_run_json["dispatches"][0]["data"]["runtime_processing"]["outcome"]["runtime"],
        "wasm"
    );
    assert_eq!(
        external_run_json["dispatches"][0]["data"]["runtime_processing"]["outcome"]
            ["eligible_worker_count"],
        2
    );
    assert!(
        external_run_json["dispatches"][0]["data"]["runtime_processing"]["outcome"]
            ["eligible_workers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|worker| worker["id"] == "wasm-worker-cli")
    );

    let claim_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "claim-dispatch",
            "--addon-dir",
            addon_dir.to_str().unwrap(),
            "--dispatch",
            &dispatch_id,
            "--worker",
            "wasm-worker-cli",
            "--lease-seconds",
            "600",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let claim_json: Value = serde_json::from_slice(&claim_output).unwrap();
    assert_eq!(claim_json["status"], "runtime_contract_dispatch_claimed");
    assert_eq!(claim_json["claimed_count"], 1);
    assert_eq!(
        claim_json["dispatches"][0]["status"],
        "claimed_external_worker"
    );
    assert_eq!(
        claim_json["dispatches"][0]["data"]["runtime_processing"]["outcome"]["claim"]["worker_id"],
        "wasm-worker-cli"
    );
    assert_eq!(
        claim_json["dispatches"][0]["data"]["runtime_processing_history"][0]["outcome"]["runtime"],
        "wasm"
    );

    let rotated_signing_key = SigningKey::from_bytes(&[9u8; 32]);
    let rotated_worker_data = serde_json::json!({
        "endpoint": "local://wasm-worker-cli-v2",
        "signer": "test-signer-v2",
        "signature_scheme": "ed25519",
        "public_key_hex": test_hex_encode(rotated_signing_key.verifying_key().as_bytes()),
    })
    .to_string();
    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "register-worker",
            "--worker",
            "wasm-worker-cli",
            "--runtime",
            "wasm",
            "--trust-level",
            "signed",
            "--source",
            "key-rotation-test",
            "--data",
            rotated_worker_data.as_str(),
            "--output",
            "json",
        ])
        .assert()
        .success();

    let unsigned_completion_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "complete-dispatch",
            "--addon-dir",
            addon_dir.to_str().unwrap(),
            "--dispatch",
            &dispatch_id,
            "--worker",
            "wasm-worker-cli",
            "--status",
            "completed",
            "--result",
            r#"{"validation_report":"ok"}"#,
            "--output",
            "json",
        ])
        .assert()
        .failure()
        .get_output()
        .stdout
        .clone();
    let unsigned_completion_json: Value =
        serde_json::from_slice(&unsigned_completion_output).unwrap();
    assert_eq!(
        unsigned_completion_json["status"],
        "runtime_contract_dispatch_completion_rejected"
    );
    assert_eq!(
        unsigned_completion_json["dispatches"][0]["data"]["runtime_processing"]["outcome"]
            ["outcome"],
        "missing_worker_signature"
    );

    let completion_result = serde_json::json!({
        "validation_report": "ok",
        "route_id": "route-001",
    });
    let completion_attestation = serde_json::json!({
        "signer": "test-signer",
        "algorithm": "ed25519-fixture",
    });
    let completion_result_sha256 = hex_sha256(&serde_json::to_vec(&completion_result).unwrap());
    let completion_attestation_sha256 =
        hex_sha256(&serde_json::to_vec(&completion_attestation).unwrap());
    let completion_payload = format!(
        "forge.addon_runtime_contract_completion.v1\ndispatch_id={}\nworker_id=wasm-worker-cli\nstatus=completed\nresult_sha256={}\nattestation_sha256={}",
        dispatch_id, completion_result_sha256, completion_attestation_sha256
    );
    let invalid_complete_input = serde_json::json!({
        "addon_dirs": [addon_dir.display().to_string()],
        "dispatch_id": dispatch_id.clone(),
        "worker_id": "wasm-worker-cli",
        "status": "completed",
        "result": completion_result.clone(),
        "signature": "00".repeat(64),
        "attestation": completion_attestation.clone(),
    })
    .to_string();
    let invalid_complete_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.addons.complete_dispatch",
            "--input",
            invalid_complete_input.as_str(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let invalid_complete_json: Value = serde_json::from_slice(&invalid_complete_output).unwrap();
    assert_eq!(
        invalid_complete_json["result"]["status"],
        "runtime_contract_dispatch_completion_rejected"
    );
    assert_eq!(
        invalid_complete_json["result"]["dispatches"][0]["data"]["runtime_processing"]["outcome"]
            ["signature_verification"]["status"],
        "invalid"
    );

    let completion_signature = signing_key.sign(completion_payload.as_bytes());
    let completion_signature_hex = test_hex_encode(&completion_signature.to_bytes());
    let complete_input = serde_json::json!({
        "addon_dirs": [addon_dir.display().to_string()],
        "dispatch_id": dispatch_id,
        "worker_id": "wasm-worker-cli",
        "status": "completed",
        "result": completion_result,
        "signature": completion_signature_hex,
        "attestation": completion_attestation,
    })
    .to_string();
    let complete_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.addons.complete_dispatch",
            "--input",
            complete_input.as_str(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let complete_json: Value = serde_json::from_slice(&complete_output).unwrap();
    assert_eq!(
        complete_json["result"]["status"],
        "runtime_contract_dispatch_external_completed"
    );
    assert_eq!(complete_json["result"]["completed_count"], 1);
    assert_eq!(
        complete_json["result"]["dispatches"][0]["status"],
        "completed"
    );
    assert_eq!(
        complete_json["result"]["dispatches"][0]["data"]["runtime_processing"]["outcome"]
            ["signature_status"],
        "verified"
    );
    assert_eq!(
        complete_json["result"]["dispatches"][0]["data"]["runtime_processing"]["outcome"]
            ["signature_verification"]["scheme"],
        "ed25519"
    );
    assert_eq!(
        complete_json["result"]["dispatches"][0]["data"]["runtime_processing"]["outcome"]
            ["signature_verification_source"],
        "claim_snapshot"
    );
    assert_eq!(
        complete_json["result"]["dispatches"][0]["data"]["runtime_processing"]["outcome"]
            ["current_worker_status"],
        "available"
    );
    assert_eq!(
        complete_json["result"]["dispatches"][0]["data"]["runtime_processing"]["outcome"]["result"]
            ["route_id"],
        "route-001"
    );
    assert_eq!(
        complete_json["result"]["dispatches"][0]["data"]["runtime_processing"]["outcome"]
            ["result_sha256"]
            .as_str()
            .unwrap()
            .len(),
        64
    );

    let local_worker_script = temp.path().join("local-logistics-worker.py");
    fs::write(
        &local_worker_script,
        r#"#!/usr/bin/env python3
import json
import sys

request = json.load(sys.stdin)
print(json.dumps({
    "status": "completed",
    "result": {
        "validation_report": "ok from local process",
        "route_id": request["input"].get("route_id"),
        "entrypoint": request["entrypoint"],
    },
    "attestation": {
        "schema_version": "forge.addon_runtime_worker_attestation.v1",
        "execution_mode": "local_process",
        "worker_id": request["worker_id"],
        "dispatch_id": request["dispatch_id"],
        "request_schema": request["schema_version"],
    },
}))
"#,
    )
    .unwrap();
    let mut worker_permissions = fs::metadata(&local_worker_script).unwrap().permissions();
    worker_permissions.set_mode(0o755);
    fs::set_permissions(&local_worker_script, worker_permissions).unwrap();

    let local_dispatch_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "dispatch-contract",
            "--addon-dir",
            addon_dir.to_str().unwrap(),
            "--addon",
            "forge.addon.logistics",
            "--contract",
            "logistics.route_validator",
            "--input",
            r#"{"route_id":"route-003","fleet_capacity":18}"#,
            "--source",
            "local-worker-test",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let local_dispatch_json: Value = serde_json::from_slice(&local_dispatch_output).unwrap();
    let local_dispatch_id = local_dispatch_json["dispatches"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let local_worker_data = serde_json::json!({
        "execution_mode": "local_process",
        "command": local_worker_script.display().to_string(),
        "allowed_entrypoints": ["logistics_route_validator.validate"],
        "allowed_contracts": ["logistics.route_validator"],
        "timeout_seconds": 5,
    })
    .to_string();
    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "register-worker",
            "--worker",
            "local-logistics-worker",
            "--runtime",
            "wasm",
            "--trust-level",
            "local",
            "--source",
            "local-worker-test",
            "--data",
            local_worker_data.as_str(),
            "--output",
            "json",
        ])
        .assert()
        .success();

    let local_execute_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "execute-dispatch",
            "--addon-dir",
            addon_dir.to_str().unwrap(),
            "--dispatch",
            &local_dispatch_id,
            "--worker",
            "local-logistics-worker",
            "--lease-seconds",
            "120",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let local_execute_json: Value = serde_json::from_slice(&local_execute_output).unwrap();
    assert_eq!(
        local_execute_json["status"],
        "runtime_contract_dispatch_external_completed"
    );
    assert_eq!(local_execute_json["completed_count"], 1);
    assert_eq!(local_execute_json["dispatches"][0]["status"], "completed");
    assert_eq!(
        local_execute_json["dispatches"][0]["data"]["runtime_processing"]["outcome"]["result"]
            ["route_id"],
        "route-003"
    );
    assert_eq!(
        local_execute_json["dispatches"][0]["data"]["runtime_processing"]["outcome"]["attestation"]
            ["execution_mode"],
        "local_process"
    );
    assert!(
        local_execute_json["dispatches"][0]["data"]["runtime_processing_history"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["outcome"]["outcome"] == "claimed_external_worker")
    );

    let local_mcp_dispatch_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "dispatch-contract",
            "--addon-dir",
            addon_dir.to_str().unwrap(),
            "--addon",
            "forge.addon.logistics",
            "--contract",
            "logistics.route_validator",
            "--input",
            r#"{"route_id":"route-004","fleet_capacity":22}"#,
            "--source",
            "local-worker-mcp-test",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let local_mcp_dispatch_json: Value =
        serde_json::from_slice(&local_mcp_dispatch_output).unwrap();
    let local_mcp_dispatch_id = local_mcp_dispatch_json["dispatches"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let local_mcp_execute_input = serde_json::json!({
        "addon_dirs": [addon_dir.display().to_string()],
        "dispatch_id": local_mcp_dispatch_id,
        "worker_id": "local-logistics-worker",
        "lease_seconds": 120,
    })
    .to_string();
    let local_mcp_execute_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.addons.execute_dispatch",
            "--input",
            local_mcp_execute_input.as_str(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let local_mcp_execute_json: Value = serde_json::from_slice(&local_mcp_execute_output).unwrap();
    assert_eq!(
        local_mcp_execute_json["result"]["status"],
        "runtime_contract_dispatch_external_completed"
    );
    assert_eq!(
        local_mcp_execute_json["result"]["dispatches"][0]["data"]["runtime_processing"]["outcome"]
            ["result"]["route_id"],
        "route-004"
    );

    let mcp_dispatch_input = format!(
        r#"{{"addon_dirs":["{}"],"addon_id":"forge.addon.logistics","contract_id":"logistics.route_validator","input":{{"route_id":"route-002"}},"source":"mcp-test","dry_run":true}}"#,
        addon_dir.display()
    );
    let mcp_dispatch_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.addons.dispatch_contract",
            "--input",
            &mcp_dispatch_input,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let mcp_dispatch_json: Value = serde_json::from_slice(&mcp_dispatch_output).unwrap();
    assert_eq!(
        mcp_dispatch_json["result"]["status"],
        "runtime_contract_dispatch_dry_run"
    );
    assert_eq!(mcp_dispatch_json["result"]["dry_run"], true);

    let views_output = forge()
        .args([
            "addons",
            "views",
            "--addon-dir",
            addon_dir.to_str().unwrap(),
            "--addon",
            "forge.addon.logistics",
            "--surface",
            "ops_console",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let views_json: Value = serde_json::from_slice(&views_output).unwrap();
    assert_eq!(views_json["schema_version"], "forge.addon_views.v1");
    assert_eq!(views_json["view_count"], 1);
    assert_eq!(
        views_json["views"][0]["view"]["id"],
        "logistics.route_dashboard"
    );
    assert_eq!(views_json["views"][0]["view"]["type"], "dashboard");
    assert_eq!(
        views_json["views"][0]["view"]["component"],
        "forge.logistics.route_dashboard"
    );
    assert_eq!(views_json["views"][0]["view"]["layout"]["zone"], "main");
    assert_eq!(
        views_json["views"][0]["view"]["data_bindings"][0]["source"],
        "forge.events.timeline"
    );
    assert_eq!(
        views_json["views"][0]["view"]["actions"][0]["target"],
        "forge.addons.dispatch_contract"
    );
    assert_eq!(
        views_json["views"][0]["permission_gate"]["tenant_scopes"],
        serde_json::json!(["organization", "product"])
    );

    let ops_snapshot_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "ops",
            "snapshot",
            "--addon-dir",
            addon_dir.to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let ops_snapshot_json: Value = serde_json::from_slice(&ops_snapshot_output).unwrap();
    assert_eq!(
        ops_snapshot_json["addon_views"]["schema_version"],
        "forge.addon_views.v1"
    );
    assert!(ops_snapshot_json["addon_views"]["views"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["addon_id"] == "forge.addon.logistics"
            && entry["view"]["id"] == "logistics.route_dashboard"
            && entry["view"]["surface"] == "ops_console"
            && entry["view"]["type"] == "dashboard"
            && entry["view"]["data_bindings"][0]["id"] == "route_events"
            && entry["view"]["actions"][0]["id"] == "logistics.validate_route"
            && entry["permission_gate"]["allowed"] == true));

    let output = forge()
        .args([
            "addons",
            "resolve",
            "--goal",
            "Criar roteirização para rota agrícola com restrições de carga",
            "--addon-dir",
            addon_dir.to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["schema_version"], "forge.capability_resolution.v1");
    assert_eq!(json["status"], "resolved");
    let required = json["required_capabilities"].as_array().unwrap();
    let route = required
        .iter()
        .find(|capability| capability["id"] == "route_optimization")
        .expect("external capability should be required");
    assert_eq!(route["source_addon"], "forge.addon.logistics");
    assert!(route["workflow_extensions"]
        .as_array()
        .unwrap()
        .contains(&Value::String("route_optimization_workflow".to_string())));
    assert!(json["workflow_extensions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|extension| extension["id"] == "route_optimization_workflow"
            && extension["source_addon"] == "forge.addon.logistics"
            && extension["source_capability"] == "route_optimization"));
    assert!(json["runtime_contracts"]
        .as_array()
        .unwrap()
        .iter()
        .any(|contract| contract["id"] == "logistics.route_validator"
            && contract["contract_type"] == "validator"
            && contract["runtime"] == "wasm"
            && contract["source_addon"] == "forge.addon.logistics"
            && contract["source_capability"] == "route_optimization"
            && contract["workflow_extension_id"] == "route_optimization_workflow"
            && contract["permission_gate"]["status"] == "allowed"));
    assert!(json["active_addons"]
        .as_array()
        .unwrap()
        .contains(&Value::String("forge.addon.logistics".to_string())));

    let mut plan_command = forge();
    let plan_output = plan_command
        .current_dir(temp.path())
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Criar roteirização para rota agrícola com restrições de carga",
            "--addon-dir",
            addon_dir.to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let plan_json: Value = serde_json::from_slice(&plan_output).unwrap();
    assert!(
        plan_json["intent"]["capability_resolution"]["runtime_contracts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|contract| contract["id"] == "logistics.route_validator")
    );
    assert!(plan_json["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|task| task["context_requirements"]
            .as_array()
            .unwrap()
            .contains(&Value::String(
                "runtime contract logistics.route_validator (validator) via wasm".to_string()
            ))));
}

#[test]
fn external_api_runtime_worker_executes_dispatch_through_cli_and_mcp() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let addon_dir = temp.path().join("addons");
    fs::create_dir_all(&addon_dir).unwrap();
    fs::write(
        addon_dir.join("gateway.yaml"),
        r#"
id: forge.addon.gateway
name: Gateway Addon
version: 0.1.0
permissions:
  - id: gateway.charge
    risk: high
    tools: [payment-gateway]
    resources: [payment.intent]
    integrations: [gateway.external_api]
    actions: [payment.charge]
capabilities:
  - id: payment_execution
    title: Payment execution
    domains: [payments]
    keywords: [payment, gateway]
runtime_contracts:
  - id: gateway.payment_charge
    title: Gateway payment charge
    contract_type: executor
    capability_id: payment_execution
    runtime: external_api
    entrypoint: gateway.payment.charge
    permissions: [gateway.charge]
"#,
    )
    .unwrap();

    let (endpoint, handle) = start_external_api_worker_server(2);
    let worker_data = serde_json::json!({
        "execution_mode": "external_api",
        "endpoint": endpoint,
        "allowed_entrypoints": ["gateway.payment.charge"],
        "allowed_contracts": ["gateway.payment_charge"],
        "timeout_seconds": 5,
        "max_response_bytes": 65536,
    })
    .to_string();
    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "register-worker",
            "--worker",
            "gateway-api-worker",
            "--runtime",
            "external_api",
            "--trust-level",
            "local",
            "--source",
            "test",
            "--data",
            worker_data.as_str(),
            "--output",
            "json",
        ])
        .assert()
        .success();

    let dispatch_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "dispatch-contract",
            "--addon-dir",
            addon_dir.to_str().unwrap(),
            "--addon",
            "forge.addon.gateway",
            "--contract",
            "gateway.payment_charge",
            "--input",
            r#"{"payment_id":"pay-cli-001","amount":1234}"#,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let dispatch_json: Value = serde_json::from_slice(&dispatch_output).unwrap();
    let dispatch_id = dispatch_json["dispatches"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let execute_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "execute-dispatch",
            "--addon-dir",
            addon_dir.to_str().unwrap(),
            "--dispatch",
            &dispatch_id,
            "--worker",
            "gateway-api-worker",
            "--lease-seconds",
            "120",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let execute_json: Value = serde_json::from_slice(&execute_output).unwrap();
    assert_eq!(
        execute_json["status"],
        "runtime_contract_dispatch_external_completed"
    );
    assert_eq!(execute_json["completed_count"], 1);
    assert_eq!(execute_json["dispatches"][0]["status"], "completed");
    assert_eq!(
        execute_json["dispatches"][0]["data"]["runtime_processing"]["outcome"]["result"]
            ["payment_id"],
        "pay-cli-001"
    );
    assert_eq!(
        execute_json["dispatches"][0]["data"]["runtime_processing"]["outcome"]["attestation"]
            ["execution_mode"],
        "external_api"
    );
    assert!(
        execute_json["dispatches"][0]["data"]["runtime_processing_history"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["outcome"]["outcome"] == "claimed_external_worker")
    );

    let mcp_dispatch_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "dispatch-contract",
            "--addon-dir",
            addon_dir.to_str().unwrap(),
            "--addon",
            "forge.addon.gateway",
            "--contract",
            "gateway.payment_charge",
            "--input",
            r#"{"payment_id":"pay-mcp-001","amount":4321}"#,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let mcp_dispatch_json: Value = serde_json::from_slice(&mcp_dispatch_output).unwrap();
    let mcp_dispatch_id = mcp_dispatch_json["dispatches"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let mcp_execute_input = serde_json::json!({
        "addon_dirs": [addon_dir.display().to_string()],
        "dispatch_id": mcp_dispatch_id,
        "worker_id": "gateway-api-worker",
        "lease_seconds": 120,
    })
    .to_string();
    let mcp_execute_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.addons.execute_dispatch",
            "--input",
            mcp_execute_input.as_str(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let mcp_execute_json: Value = serde_json::from_slice(&mcp_execute_output).unwrap();
    assert_eq!(
        mcp_execute_json["result"]["status"],
        "runtime_contract_dispatch_external_completed"
    );
    assert_eq!(
        mcp_execute_json["result"]["dispatches"][0]["data"]["runtime_processing"]["outcome"]
            ["result"]["payment_id"],
        "pay-mcp-001"
    );
    handle.join().unwrap();
}

#[test]
fn external_api_runtime_worker_supports_auth_and_https_without_secret_leakage() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let addon_dir = temp.path().join("addons");
    let bin_dir = temp.path().join("bin");
    fs::create_dir_all(&addon_dir).unwrap();
    fs::create_dir_all(&bin_dir).unwrap();
    fs::write(
        addon_dir.join("gateway.yaml"),
        r#"
id: forge.addon.gateway_auth
name: Gateway Auth Addon
version: 0.1.0
permissions:
  - id: gateway.charge
    risk: high
    tools: [payment-gateway]
    resources: [payment.intent]
    integrations: [gateway.external_api]
    actions: [payment.charge]
capabilities:
  - id: payment_execution
    title: Payment execution
    domains: [payments]
    keywords: [payment, gateway]
runtime_contracts:
  - id: gateway.payment_charge
    title: Gateway payment charge
    contract_type: executor
    capability_id: payment_execution
    runtime: external_api
    entrypoint: gateway.payment.charge
    permissions: [gateway.charge]
"#,
    )
    .unwrap();

    let vault_bin = write_fake_event_egress_credential_vault(&bin_dir);
    let vault_args = temp.path().join("credential-vault.args");
    let contract = temp.path().join("worker.contract.yaml");
    let data = temp.path().join("worker.data.yaml");
    fs::write(
        &contract,
        r#"version: 1
vault:
  id: worker-api
records:
  partner_api:
    title: Partner API
    fields:
      - id: token
        path: auth.token
        kind: password
        secret: true
"#,
    )
    .unwrap();
    fs::write(&data, "records: {}\n").unwrap();
    let vault_secret = "forge-worker-vault-secret";
    let env_secret = "forge-worker-env-secret";
    let (endpoint, handle) = start_authenticated_external_api_worker_server(1, vault_secret);

    let vault_worker_data = serde_json::json!({
        "execution_mode": "external_api",
        "endpoint": endpoint,
        "auth": "bearer",
        "credential_vault": {
            "vault_bin": vault_bin.display().to_string(),
            "contract": contract.display().to_string(),
            "data": data.display().to_string(),
            "record": "partner_api",
            "field": "auth.token"
        },
        "allowed_entrypoints": ["gateway.payment.charge"],
        "allowed_contracts": ["gateway.payment_charge"],
        "timeout_seconds": 5,
        "max_response_bytes": 65536,
    })
    .to_string();
    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "register-worker",
            "--worker",
            "gateway-api-worker-vault",
            "--runtime",
            "external_api",
            "--trust-level",
            "local",
            "--source",
            "test",
            "--data",
            vault_worker_data.as_str(),
            "--output",
            "json",
        ])
        .assert()
        .success();

    let dispatch_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "dispatch-contract",
            "--addon-dir",
            addon_dir.to_str().unwrap(),
            "--addon",
            "forge.addon.gateway_auth",
            "--contract",
            "gateway.payment_charge",
            "--input",
            r#"{"payment_id":"pay-vault-001","amount":1111}"#,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let dispatch_json: Value = serde_json::from_slice(&dispatch_output).unwrap();
    let dispatch_id = dispatch_json["dispatches"][0]["id"].as_str().unwrap();
    let execute_output = forge()
        .env("FORGE_FAKE_VAULT_SECRET", vault_secret)
        .env("FORGE_FAKE_CREDENTIAL_VAULT_ARGS", &vault_args)
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "execute-dispatch",
            "--addon-dir",
            addon_dir.to_str().unwrap(),
            "--dispatch",
            dispatch_id,
            "--worker",
            "gateway-api-worker-vault",
            "--lease-seconds",
            "120",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let execute_text = String::from_utf8_lossy(&execute_output);
    let execute_json: Value = serde_json::from_slice(&execute_output).unwrap();
    let vault_outcome = &execute_json["dispatches"][0]["data"]["runtime_processing"]["outcome"];
    assert_eq!(
        execute_json["status"],
        "runtime_contract_dispatch_external_completed"
    );
    assert_eq!(vault_outcome["result"]["payment_id"], "pay-vault-001");
    assert_eq!(vault_outcome["result"]["authenticated"], true);
    assert_eq!(vault_outcome["attestation"]["auth_scheme"], "bearer");
    assert_eq!(
        vault_outcome["attestation"]["secret_source"],
        "credential_vault"
    );
    assert_eq!(
        vault_outcome["attestation"]["credential_vault"]["record"],
        "partner_api"
    );
    assert!(!execute_text.contains(vault_secret));
    let vault_args_text = fs::read_to_string(&vault_args).unwrap();
    assert!(vault_args_text.contains("--allow-secret-stdout"));
    assert!(vault_args_text.contains("--no-newline"));
    assert!(!vault_args_text.contains(vault_secret));
    handle.join().unwrap();

    let https_worker_data = serde_json::json!({
        "execution_mode": "external_api",
        "endpoint": "https://gateway.example.test/runtime/execute",
        "allowed_hosts": ["gateway.example.test"],
        "auth": "bearer",
        "secret_env": "FORGE_WORKER_HTTPS_TOKEN",
        "allowed_entrypoints": ["gateway.payment.charge"],
        "allowed_contracts": ["gateway.payment_charge"],
        "timeout_seconds": 5,
        "max_response_bytes": 65536,
    })
    .to_string();
    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "register-worker",
            "--worker",
            "gateway-api-worker-https",
            "--runtime",
            "external_api",
            "--trust-level",
            "local",
            "--source",
            "test",
            "--data",
            https_worker_data.as_str(),
            "--output",
            "json",
        ])
        .assert()
        .success();
    let https_dispatch_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "dispatch-contract",
            "--addon-dir",
            addon_dir.to_str().unwrap(),
            "--addon",
            "forge.addon.gateway_auth",
            "--contract",
            "gateway.payment_charge",
            "--input",
            r#"{"payment_id":"pay-https-001","amount":2222}"#,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let https_dispatch_json: Value = serde_json::from_slice(&https_dispatch_output).unwrap();
    let https_dispatch_id = https_dispatch_json["dispatches"][0]["id"].as_str().unwrap();
    let https_mcp_input = serde_json::json!({
        "addon_dirs": [addon_dir.display().to_string()],
        "dispatch_id": https_dispatch_id,
        "worker_id": "gateway-api-worker-https",
        "lease_seconds": 120,
    })
    .to_string();
    let https_output = forge()
        .env("FORGE_WORKER_HTTPS_TOKEN", env_secret)
        .env("FORGE_EXTERNAL_API_WORKER_HTTPS_MODE", "simulate")
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.addons.execute_dispatch",
            "--input",
            https_mcp_input.as_str(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let https_text = String::from_utf8_lossy(&https_output);
    let https_json: Value = serde_json::from_slice(&https_output).unwrap();
    let https_outcome =
        &https_json["result"]["dispatches"][0]["data"]["runtime_processing"]["outcome"];
    assert_eq!(
        https_json["result"]["status"],
        "runtime_contract_dispatch_external_completed"
    );
    assert_eq!(
        https_outcome["result"]["outcome"],
        "external_api_https_simulated"
    );
    assert_eq!(https_outcome["result"]["auth_scheme"], "bearer");
    assert_eq!(https_outcome["result"]["secret_source"], "env");
    assert_eq!(https_outcome["attestation"]["endpoint_scheme"], "https");
    assert_eq!(https_outcome["attestation"]["simulated"], true);
    assert!(!https_text.contains(env_secret));
}

#[test]
fn event_egress_adapter_emits_webhook_through_cli_and_mcp() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let addon_dir = temp.path().join("addons");
    fs::create_dir_all(&addon_dir).unwrap();
    let (endpoint, handle) = start_event_egress_server(2);
    fs::write(
        addon_dir.join("partner.yaml"),
        format!(
            r#"
id: forge.addon.partner
name: Partner Addon
version: 0.1.0
permissions:
  - id: partner.notify
    risk: medium
    tools: [http-client]
    resources: [partner.notification]
    integrations: [partner.webhook]
    actions: [notify_partner]
capabilities:
  - id: partner_notifications
    title: Partner notifications
    domains: [operations]
    keywords: [partner, notification]
event_types:
  - id: partner.notification
    title: Partner notification
    transport: webhook
event_adapters:
  - id: partner.webhook_egress
    title: Partner Webhook Egress
    transport: webhook
    direction: egress
    origins: [codex]
    actions: [notify_partner]
    event_types: [partner.notification]
    schema: partner.notification.v1
    auth: none
    permissions: [partner.notify]
    endpoint: "{endpoint}"
    allowed_hosts: [127.0.0.1]
    timeout_seconds: 5
    max_response_bytes: 65536
"#
        ),
    )
    .unwrap();

    let cli_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "emit",
            "--addon-dir",
            addon_dir.to_str().unwrap(),
            "--addon",
            "forge.addon.partner",
            "--adapter",
            "partner.webhook_egress",
            "--event-type",
            "partner.notification",
            "--action",
            "notify_partner",
            "--origin",
            "codex",
            "--payload",
            r#"{"id":"cli-001","message":"ready"}"#,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let cli_json: Value = serde_json::from_slice(&cli_output).unwrap();
    assert_eq!(cli_json["schema_version"], "forge.event_egress_emit.v1");
    assert_eq!(cli_json["status"], "event_egress_delivered");
    assert_eq!(cli_json["adapter_policy"]["status"], "matched");
    assert_eq!(cli_json["delivery"]["status_code"], 202);
    assert_eq!(cli_json["delivery"]["success"], true);
    assert_eq!(cli_json["request"]["payload"]["id"], "cli-001");
    assert!(cli_json["global_event_id"].as_i64().unwrap() > 0);

    let mcp_input = serde_json::json!({
        "addon_dirs": [addon_dir.display().to_string()],
        "addon_id": "forge.addon.partner",
        "adapter_id": "partner.webhook_egress",
        "event_type": "partner.notification",
        "action": "notify_partner",
        "origin": "codex",
        "payload": {"id": "mcp-001", "message": "ready"},
    })
    .to_string();
    let mcp_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.events.emit",
            "--input",
            mcp_input.as_str(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let mcp_json: Value = serde_json::from_slice(&mcp_output).unwrap();
    assert_eq!(mcp_json["result"]["status"], "event_egress_delivered");
    assert_eq!(mcp_json["result"]["delivery"]["status_code"], 202);
    assert_eq!(mcp_json["result"]["request"]["payload"]["id"], "mcp-001");
    assert!(mcp_json["result"]["global_event_id"].as_i64().unwrap() > 0);

    let timeline_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "timeline",
            "--limit",
            "20",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let timeline_json: Value = serde_json::from_slice(&timeline_output).unwrap();
    let egress_events = timeline_json["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|event| {
            event["kind"] == "event_egress_delivered"
                && event["source"] == "event_egress"
                && event["data"]["request"]["adapter_id"] == "partner.webhook_egress"
        })
        .collect::<Vec<_>>();
    assert_eq!(egress_events.len(), 2);

    handle.join().unwrap();
}

#[test]
fn event_egress_enforces_project_tenant_policy_without_workflow_target() {
    let temp = tempdir().unwrap();
    let forge_dir = temp.path().join(".forge");
    fs::create_dir_all(&forge_dir).unwrap();
    fs::write(
        forge_dir.join("operating-context.yaml"),
        r#"
organization:
  scope: organization
  id: global-egress-org
  label: Global Egress Org
brand:
  scope: brand
  id: global-egress-brand
  label: Global Egress Brand
product:
  scope: product
  id: global-egress-product
  label: Global Egress Product
user:
  scope: user
  id: global-egress-user
  label: Global Egress User
channel:
  scope: channel
  id: local_cli
  label: Local CLI
tenant_policy_mode: enforce
"#,
    )
    .unwrap();

    let store = temp.path().join("forge.sqlite");
    let addon_dir = temp.path().join("addons");
    fs::create_dir_all(&addon_dir).unwrap();
    let (endpoint, handle) = start_event_egress_server(1);
    fs::write(
        addon_dir.join("global-tenant-egress.yaml"),
        format!(
            r#"
id: forge.addon.partner
name: Global Tenant Egress Addon
version: 0.1.0
permissions:
  - id: partner.notify
    risk: medium
    tools: [http-client]
    resources: [partner.notification]
    integrations: [partner.webhook]
    actions: [notify_partner]
capabilities:
  - id: partner_notifications
    title: Global tenant notifications
    domains: [operations]
    keywords: [tenant, notification]
event_types:
  - id: partner.notification
    title: Global tenant notification
    transport: webhook
event_adapters:
  - id: partner.webhook_egress
    title: Global Tenant Webhook Egress
    transport: webhook
    direction: egress
    origins: [codex]
    actions: [notify_partner]
    event_types: [partner.notification]
    schema: partner.notification.v1
    auth: none
    permissions: [partner.notify]
    endpoint: "{endpoint}"
    allowed_hosts: [127.0.0.1]
    timeout_seconds: 5
    max_response_bytes: 65536
"#
        ),
    )
    .unwrap();

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "identity",
            "sync",
            "--project-root",
            temp.path().to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .success();

    let connection = Connection::open(&store).unwrap();
    connection
        .execute(
            "UPDATE identity_memberships SET role = 'viewer' WHERE subject_id = ?1",
            ["global-egress-user"],
        )
        .unwrap();

    let blocked_output = forge()
        .current_dir(temp.path())
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "emit",
            "--addon-dir",
            addon_dir.to_str().unwrap(),
            "--addon",
            "forge.addon.partner",
            "--adapter",
            "partner.webhook_egress",
            "--event-type",
            "partner.notification",
            "--action",
            "notify_partner",
            "--origin",
            "codex",
            "--payload",
            r#"{"message":"must be governed even without workflow_id"}"#,
            "--output",
            "json",
        ])
        .assert()
        .failure()
        .get_output()
        .stderr
        .clone();
    let blocked_stderr = String::from_utf8(blocked_output).unwrap();
    assert!(blocked_stderr.contains("multi-tenant enforcement blocked event egress delivery"));
    assert!(blocked_stderr.contains("workflow:deliver"));
    assert!(!blocked_stderr.contains("failed to connect to event egress endpoint"));

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "identity",
            "membership-update",
            "--subject",
            "global-egress-user",
            "--organization",
            "global-egress-org",
            "--brand",
            "global-egress-brand",
            "--product",
            "global-egress-product",
            "--grant",
            "workflow:deliver",
            "--source",
            "test-cli",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let delivered_output = forge()
        .current_dir(temp.path())
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "emit",
            "--addon-dir",
            addon_dir.to_str().unwrap(),
            "--addon",
            "forge.addon.partner",
            "--adapter",
            "partner.webhook_egress",
            "--event-type",
            "partner.notification",
            "--action",
            "notify_partner",
            "--origin",
            "codex",
            "--payload",
            r#"{"message":"allowed after workflow deliver grant"}"#,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let delivered_json: Value = serde_json::from_slice(&delivered_output).unwrap();
    assert_eq!(delivered_json["status"], "event_egress_delivered");
    assert_eq!(delivered_json["delivery"]["status_code"], 202);
    assert!(delivered_json["request"]["payload"]["workflow_id"].is_null());

    handle.join().unwrap();
}

#[test]
fn event_egress_respects_workflow_tenant_policy_before_external_delivery() {
    let temp = tempdir().unwrap();
    let forge_dir = temp.path().join(".forge");
    fs::create_dir_all(&forge_dir).unwrap();
    fs::write(
        forge_dir.join("operating-context.yaml"),
        r#"
organization:
  scope: organization
  id: egress-org
  label: Egress Org
brand:
  scope: brand
  id: egress-brand
  label: Egress Brand
product:
  scope: product
  id: egress-product
  label: Egress Product
user:
  scope: user
  id: egress-user
  label: Egress User
channel:
  scope: channel
  id: local_cli
  label: Local CLI
tenant_policy_mode: enforce
"#,
    )
    .unwrap();

    let store = temp.path().join("forge.sqlite");
    let addon_dir = temp.path().join("addons");
    fs::create_dir_all(&addon_dir).unwrap();
    fs::write(
        addon_dir.join("tenant-egress.yaml"),
        r#"
id: forge.addon.tenant_egress
name: Tenant Egress Addon
version: 0.1.0
permissions:
  - id: tenant.notify
    risk: medium
    tools: [http-client]
    resources: [tenant.notification]
    integrations: [tenant.webhook]
    actions: [notify_partner]
capabilities:
  - id: tenant_notifications
    title: Tenant notifications
    domains: [operations]
    keywords: [tenant, notification]
event_types:
  - id: tenant.notification
    title: Tenant notification
    transport: webhook
event_adapters:
  - id: tenant.webhook_egress
    title: Tenant Webhook Egress
    transport: webhook
    direction: egress
    origins: [codex]
    actions: [notify_partner]
    event_types: [tenant.notification]
    schema: tenant.notification.v1
    auth: none
    permissions: [tenant.notify]
    endpoint: "http://127.0.0.1:9/tenant"
    allowed_hosts: [127.0.0.1]
    timeout_seconds: 1
    max_response_bytes: 1024
"#,
    )
    .unwrap();

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "identity",
            "sync",
            "--project-root",
            temp.path().to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .success();

    let request_output = forge()
        .current_dir(temp.path())
        .args([
            "--store",
            store.to_str().unwrap(),
            "request",
            "start",
            "--goal",
            "Enviar notificação externa governada por tenant",
            "--origin",
            "codex",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let request_json: Value = serde_json::from_slice(&request_output).unwrap();
    let workflow_id = request_json["workflow_id"].as_str().unwrap();

    let connection = Connection::open(&store).unwrap();
    connection
        .execute(
            "UPDATE identity_memberships SET role = 'viewer' WHERE subject_id = ?1",
            ["egress-user"],
        )
        .unwrap();

    let blocked_output = forge()
        .current_dir(temp.path())
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "emit",
            "--addon-dir",
            addon_dir.to_str().unwrap(),
            "--addon",
            "forge.addon.tenant_egress",
            "--adapter",
            "tenant.webhook_egress",
            "--event-type",
            "tenant.notification",
            "--action",
            "notify_partner",
            "--origin",
            "codex",
            "--payload",
            &serde_json::json!({
                "workflow_id": workflow_id,
                "message": "must not leave tenant boundary"
            })
            .to_string(),
            "--output",
            "json",
        ])
        .assert()
        .failure()
        .get_output()
        .stderr
        .clone();
    let blocked_stderr = String::from_utf8(blocked_output).unwrap();
    assert!(blocked_stderr.contains("multi-tenant enforcement blocked event egress delivery"));
    assert!(blocked_stderr.contains("workflow:deliver"));
    assert!(blocked_stderr.contains("membership_permission"));
    assert!(!blocked_stderr.contains("failed to connect to event egress endpoint"));

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "identity",
            "membership-update",
            "--subject",
            "egress-user",
            "--organization",
            "egress-org",
            "--brand",
            "egress-brand",
            "--product",
            "egress-product",
            "--grant",
            "workflow:deliver",
            "--source",
            "test-cli",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let allowed_policy_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "identity",
            "tenant-policy",
            "--workflow",
            workflow_id,
            "--mode",
            "enforce",
            "--action",
            "event egress delivery",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let allowed_policy_json: Value = serde_json::from_slice(&allowed_policy_output).unwrap();
    assert_eq!(allowed_policy_json["allowed"], true);
    assert_eq!(
        allowed_policy_json["required_permission"],
        "workflow:deliver"
    );
}

#[test]
fn builtin_telegram_addon_declares_report_document_egress_for_dry_run_delivery() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "authorize-permission",
            "--addon",
            "forge.addon.notification",
            "--permission",
            "telegram.send_message",
            "--risk",
            "medium",
            "--approved-by",
            "test",
            "--source",
            "test",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let adapters_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "adapters",
            "--addon",
            "forge.addon.notification",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let adapters_json: Value = serde_json::from_slice(&adapters_output).unwrap();
    let adapters = adapters_json["adapters"].as_array().unwrap();
    assert!(adapters.iter().any(|adapter| {
        adapter["adapter"]["id"] == "telegram.bot_updates"
            && adapter["adapter"]["direction"] == "ingress"
            && adapter["permission_gate"]["allowed"] == true
    }));
    assert!(adapters.iter().any(|adapter| {
        adapter["adapter"]["id"] == "telegram.bot_send_message"
            && adapter["adapter"]["direction"] == "egress"
            && adapter["adapter"]["transport"] == "telegram"
            && adapter["adapter"]["auth"] == "bot_token"
            && adapter["adapter"]["secret_env"] == "TELEGRAM_BOT_TOKEN"
            && adapter["permission_gate"]["allowed"] == true
    }));
    assert!(adapters.iter().any(|adapter| {
        adapter["adapter"]["id"] == "telegram.bot_send_document"
            && adapter["adapter"]["event_types"]
                .as_array()
                .unwrap()
                .contains(&Value::String("telegram.report".to_string()))
            && adapter["adapter"]["transport"] == "telegram"
            && adapter["adapter"]["auth"] == "bot_token"
            && adapter["permission_gate"]["allowed"] == true
    }));

    let cli_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "emit",
            "--addon",
            "forge.addon.notification",
            "--adapter",
            "telegram.bot_send_document",
            "--event-type",
            "telegram.report",
            "--action",
            "send_final_report",
            "--origin",
            "codex",
            "--payload",
            r#"{"workflow_id":"wf_demo","document_ref":"artifacts/final-report.md","caption":"Forge final report"}"#,
            "--dry-run",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let cli_json: Value = serde_json::from_slice(&cli_output).unwrap();
    assert_eq!(cli_json["schema_version"], "forge.event_egress_emit.v1");
    assert_eq!(cli_json["status"], "event_egress_dry_run");
    assert_eq!(cli_json["dry_run"], true);
    assert_eq!(cli_json["adapter_policy"]["status"], "matched");
    assert_eq!(
        cli_json["request"]["adapter_id"],
        "telegram.bot_send_document"
    );
    assert_eq!(cli_json["request"]["transport"], "telegram");
    assert_eq!(cli_json["request"]["auth"], "bot_token");
    assert_eq!(cli_json["request"]["secret_env"], "TELEGRAM_BOT_TOKEN");
    assert!(cli_json["global_event_id"].as_i64().unwrap() > 0);
    assert_eq!(cli_json["delivery"], Value::Null);
    assert!(!String::from_utf8_lossy(&cli_output).contains("telegram-token"));

    let mcp_input = serde_json::json!({
        "addon_id": "forge.addon.notification",
        "adapter_id": "telegram.bot_send_message",
        "event_type": "telegram.message",
        "action": "send_report",
        "origin": "codex",
        "payload": {
            "workflow_id": "wf_demo",
            "message": "Forge report ready"
        },
        "dry_run": true
    })
    .to_string();
    let mcp_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.events.emit",
            "--input",
            mcp_input.as_str(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let mcp_json: Value = serde_json::from_slice(&mcp_output).unwrap();
    assert_eq!(mcp_json["result"]["status"], "event_egress_dry_run");
    assert_eq!(
        mcp_json["result"]["request"]["adapter_id"],
        "telegram.bot_send_message"
    );
    assert_eq!(
        mcp_json["result"]["request"]["secret_env"],
        "TELEGRAM_BOT_TOKEN"
    );
    assert!(mcp_json["result"]["global_event_id"].as_i64().unwrap() > 0);

    let start_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "request",
            "start",
            "--goal",
            "Entregar relatório final em Markdown e Telegram",
            "--origin",
            "codex",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let start_json: Value = serde_json::from_slice(&start_output).unwrap();
    let workflow_id = start_json["workflow_id"].as_str().unwrap().to_string();

    let report_path = temp.path().join("final-report.md");
    fs::write(&report_path, "# Forge final report\n\nvalidated\n").unwrap();
    let delivery_payload = serde_json::json!({
        "workflow_id": workflow_id.clone(),
        "document_path": report_path.display().to_string(),
        "caption": "Forge final report"
    })
    .to_string();
    let delivery_output = forge()
        .env("TELEGRAM_BOT_TOKEN", "telegram-token-from-test")
        .env("TELEGRAM_CHAT_ID", "123456")
        .env("FORGE_TELEGRAM_EGRESS_MODE", "simulate")
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "emit",
            "--addon",
            "forge.addon.notification",
            "--adapter",
            "telegram.bot_send_document",
            "--event-type",
            "telegram.report",
            "--action",
            "send_final_report",
            "--origin",
            "codex",
            "--payload",
            delivery_payload.as_str(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let delivery_json: Value = serde_json::from_slice(&delivery_output).unwrap();
    assert_eq!(delivery_json["status"], "event_egress_delivered");
    assert_eq!(
        delivery_json["delivery"]["endpoint"],
        "telegram://bot_api/sendDocument"
    );
    assert_eq!(delivery_json["delivery"]["auth_scheme"], "bot_token");
    assert_eq!(
        delivery_json["delivery"]["secret_env"],
        "TELEGRAM_BOT_TOKEN"
    );
    assert_eq!(delivery_json["delivery"]["success"], true);
    assert_eq!(delivery_json["delivery"]["status_code"], 200);
    assert_eq!(
        delivery_json["workflow_artifact"]["artifact"]["kind"],
        "telegram_delivery_record"
    );
    assert!(delivery_json["workflow_artifact"]["artifact"]["path"]
        .as_str()
        .unwrap()
        .contains("telegram_delivery_record"));
    assert!(!String::from_utf8_lossy(&delivery_output).contains("telegram-token-from-test"));

    let message_input = serde_json::json!({
        "addon_id": "forge.addon.notification",
        "adapter_id": "telegram.bot_send_message",
        "event_type": "telegram.message",
        "action": "send_message",
        "origin": "codex",
        "payload": {
            "workflow_id": workflow_id,
            "chat_id": "123456",
            "message": "Forge report ready"
        }
    })
    .to_string();
    let message_output = forge()
        .env("TELEGRAM_BOT_TOKEN", "telegram-token-from-test")
        .env("FORGE_TELEGRAM_EGRESS_MODE", "simulate")
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.events.emit",
            "--input",
            message_input.as_str(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let message_json: Value = serde_json::from_slice(&message_output).unwrap();
    assert_eq!(message_json["result"]["status"], "event_egress_delivered");
    assert_eq!(
        message_json["result"]["delivery"]["endpoint"],
        "telegram://bot_api/sendMessage"
    );
    assert_eq!(
        message_json["result"]["delivery"]["auth_scheme"],
        "bot_token"
    );
    assert_eq!(
        message_json["result"]["workflow_artifact"]["artifact"]["kind"],
        "telegram_delivery_record"
    );
    assert!(!String::from_utf8_lossy(&message_output).contains("telegram-token-from-test"));
}

#[test]
fn event_egress_adapter_signs_hmac_webhook_delivery_through_cli_and_mcp() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let addon_dir = temp.path().join("addons");
    fs::create_dir_all(&addon_dir).unwrap();
    let secret = "forge-egress-secret";
    let (endpoint, handle) = start_signed_event_egress_server(2, secret, "X-Partner-Signature");
    fs::write(
        addon_dir.join("signed-partner.yaml"),
        format!(
            r#"
id: forge.addon.signed_partner
name: Signed Partner Addon
version: 0.1.0
permissions:
  - id: partner.signed_notify
    risk: medium
    tools: [http-client]
    resources: [partner.signed_notification]
    integrations: [partner.signed_webhook]
    actions: [notify_partner]
capabilities:
  - id: signed_partner_notifications
    title: Signed partner notifications
    domains: [operations]
    keywords: [partner, notification, hmac]
event_types:
  - id: partner.signed_notification
    title: Signed partner notification
    transport: webhook
event_adapters:
  - id: partner.signed_webhook_egress
    title: Partner Signed Webhook Egress
    transport: webhook
    direction: egress
    origins: [codex]
    actions: [notify_partner]
    event_types: [partner.signed_notification]
    schema: partner.signed_notification.v1
    auth: hmac
    secret_env: FORGE_TEST_EGRESS_SECRET
    signature_header: X-Partner-Signature
    permissions: [partner.signed_notify]
    endpoint: "{endpoint}"
    allowed_hosts: [127.0.0.1]
    timeout_seconds: 5
    max_response_bytes: 65536
"#
        ),
    )
    .unwrap();

    let cli_output = forge()
        .env("FORGE_TEST_EGRESS_SECRET", secret)
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "emit",
            "--addon-dir",
            addon_dir.to_str().unwrap(),
            "--addon",
            "forge.addon.signed_partner",
            "--adapter",
            "partner.signed_webhook_egress",
            "--event-type",
            "partner.signed_notification",
            "--action",
            "notify_partner",
            "--origin",
            "codex",
            "--payload",
            r#"{"id":"signed-cli-001","message":"ready"}"#,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let cli_json: Value = serde_json::from_slice(&cli_output).unwrap();
    assert_eq!(cli_json["status"], "event_egress_delivered");
    assert_eq!(cli_json["adapter_policy"]["status"], "matched");
    assert_eq!(cli_json["request"]["auth"], "hmac");
    assert_eq!(
        cli_json["request"]["secret_env"],
        "FORGE_TEST_EGRESS_SECRET"
    );
    assert_eq!(
        cli_json["request"]["signature_header"],
        "X-Partner-Signature"
    );
    assert_eq!(cli_json["delivery"]["signed"], true);
    assert_eq!(
        cli_json["delivery"]["signature_header"],
        "X-Partner-Signature"
    );
    assert_eq!(
        cli_json["delivery"]["secret_env"],
        "FORGE_TEST_EGRESS_SECRET"
    );
    assert_eq!(cli_json["delivery"]["status_code"], 202);

    let mcp_input = serde_json::json!({
        "addon_dirs": [addon_dir.display().to_string()],
        "addon_id": "forge.addon.signed_partner",
        "adapter_id": "partner.signed_webhook_egress",
        "event_type": "partner.signed_notification",
        "action": "notify_partner",
        "origin": "codex",
        "payload": {"id": "signed-mcp-001", "message": "ready"},
    })
    .to_string();
    let mcp_output = forge()
        .env("FORGE_TEST_EGRESS_SECRET", secret)
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.events.emit",
            "--input",
            mcp_input.as_str(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let mcp_json: Value = serde_json::from_slice(&mcp_output).unwrap();
    assert_eq!(mcp_json["result"]["status"], "event_egress_delivered");
    assert_eq!(mcp_json["result"]["request"]["auth"], "hmac");
    assert_eq!(mcp_json["result"]["delivery"]["signed"], true);
    assert_eq!(
        mcp_json["result"]["delivery"]["signature_header"],
        "X-Partner-Signature"
    );
    assert_eq!(mcp_json["result"]["delivery"]["status_code"], 202);

    handle.join().unwrap();
}

#[test]
fn event_egress_adapter_injects_bearer_secret_without_reporting_value() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let addon_dir = temp.path().join("addons");
    fs::create_dir_all(&addon_dir).unwrap();
    let secret = "forge-bearer-secret";
    let (endpoint, handle) =
        start_bearer_event_egress_server(2, secret, Some("FORGE_TEST_BEARER_SECRET"));
    fs::write(
        addon_dir.join("bearer-partner.yaml"),
        format!(
            r#"
id: forge.addon.bearer_partner
name: Bearer Partner Addon
version: 0.1.0
permissions:
  - id: partner.bearer_notify
    risk: medium
    tools: [http-client]
    resources: [partner.bearer_notification]
    integrations: [partner.bearer_webhook]
    actions: [notify_partner]
capabilities:
  - id: bearer_partner_notifications
    title: Bearer partner notifications
    domains: [operations]
    keywords: [partner, notification, bearer]
event_types:
  - id: partner.bearer_notification
    title: Bearer partner notification
    transport: webhook
event_adapters:
  - id: partner.bearer_webhook_egress
    title: Partner Bearer Webhook Egress
    transport: webhook
    direction: egress
    origins: [codex]
    actions: [notify_partner]
    event_types: [partner.bearer_notification]
    schema: partner.bearer_notification.v1
    auth: bearer
    secret_env: FORGE_TEST_BEARER_SECRET
    permissions: [partner.bearer_notify]
    endpoint: "{endpoint}"
    allowed_hosts: [127.0.0.1]
    timeout_seconds: 5
    max_response_bytes: 65536
"#
        ),
    )
    .unwrap();

    let cli_output = forge()
        .env("FORGE_TEST_BEARER_SECRET", secret)
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "emit",
            "--addon-dir",
            addon_dir.to_str().unwrap(),
            "--addon",
            "forge.addon.bearer_partner",
            "--adapter",
            "partner.bearer_webhook_egress",
            "--event-type",
            "partner.bearer_notification",
            "--action",
            "notify_partner",
            "--origin",
            "codex",
            "--payload",
            r#"{"id":"bearer-cli-001","message":"ready"}"#,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let cli_json: Value = serde_json::from_slice(&cli_output).unwrap();
    assert_eq!(cli_json["status"], "event_egress_delivered");
    assert_eq!(cli_json["adapter_policy"]["status"], "matched");
    assert_eq!(cli_json["request"]["auth"], "bearer");
    assert_eq!(
        cli_json["request"]["secret_env"],
        "FORGE_TEST_BEARER_SECRET"
    );
    assert_eq!(cli_json["delivery"]["auth_scheme"], "bearer");
    assert_eq!(cli_json["delivery"]["signed"], false);
    assert_eq!(cli_json["delivery"]["signature_header"], "Authorization");
    assert_eq!(
        cli_json["delivery"]["secret_env"],
        "FORGE_TEST_BEARER_SECRET"
    );
    assert_eq!(cli_json["delivery"]["status_code"], 202);
    assert!(!String::from_utf8_lossy(&cli_output).contains(secret));

    let mcp_input = serde_json::json!({
        "addon_dirs": [addon_dir.display().to_string()],
        "addon_id": "forge.addon.bearer_partner",
        "adapter_id": "partner.bearer_webhook_egress",
        "event_type": "partner.bearer_notification",
        "action": "notify_partner",
        "origin": "codex",
        "payload": {"id": "bearer-mcp-001", "message": "ready"},
    })
    .to_string();
    let mcp_output = forge()
        .env("FORGE_TEST_BEARER_SECRET", secret)
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.events.emit",
            "--input",
            mcp_input.as_str(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let mcp_json: Value = serde_json::from_slice(&mcp_output).unwrap();
    assert_eq!(mcp_json["result"]["status"], "event_egress_delivered");
    assert_eq!(mcp_json["result"]["delivery"]["auth_scheme"], "bearer");
    assert_eq!(
        mcp_json["result"]["delivery"]["signature_header"],
        "Authorization"
    );
    assert!(!String::from_utf8_lossy(&mcp_output).contains(secret));

    handle.join().unwrap();
}

#[test]
fn event_egress_adapter_resolves_bearer_secret_from_credential_vault_without_reporting_value() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let addon_dir = temp.path().join("addons");
    let bin_dir = temp.path().join("bin");
    fs::create_dir_all(&addon_dir).unwrap();
    fs::create_dir_all(&bin_dir).unwrap();
    let vault_bin = write_fake_event_egress_credential_vault(&bin_dir);
    let vault_args = temp.path().join("credential-vault.args");
    let contract = temp.path().join("partner.contract.yaml");
    let data = temp.path().join("partner.data.yaml");
    fs::write(
        &contract,
        r#"version: 1
vault:
  id: partner-api
records:
  partner_api:
    title: Partner API
    fields:
      - id: token
        path: auth.token
        kind: password
        secret: true
"#,
    )
    .unwrap();
    fs::write(&data, "records: {}\n").unwrap();
    let secret = "forge-vault-bearer-secret";
    let (endpoint, handle) = start_bearer_event_egress_server(2, secret, None);
    fs::write(
        addon_dir.join("vault-bearer-partner.yaml"),
        format!(
            r#"
id: forge.addon.vault_bearer_partner
name: Vault Bearer Partner Addon
version: 0.1.0
permissions:
  - id: partner.vault_bearer_notify
    risk: medium
    tools: [http-client]
    resources: [partner.vault_bearer_notification]
    integrations: [partner.vault_bearer_webhook]
    actions: [notify_partner]
capabilities:
  - id: vault_bearer_partner_notifications
    title: Vault bearer partner notifications
    domains: [operations]
    keywords: [partner, notification, bearer, vault]
event_types:
  - id: partner.vault_bearer_notification
    title: Vault bearer partner notification
    transport: webhook
event_adapters:
  - id: partner.vault_bearer_webhook_egress
    title: Partner Vault Bearer Webhook Egress
    transport: webhook
    direction: egress
    origins: [codex]
    actions: [notify_partner]
    event_types: [partner.vault_bearer_notification]
    schema: partner.vault_bearer_notification.v1
    auth: bearer
    credential_vault:
      vault_bin: "{vault_bin}"
      contract: "{contract}"
      data: "{data}"
      record: partner_api
      field: auth.token
    permissions: [partner.vault_bearer_notify]
    endpoint: "{endpoint}"
    allowed_hosts: [127.0.0.1]
    timeout_seconds: 5
    max_response_bytes: 65536
"#,
            vault_bin = vault_bin.display(),
            contract = contract.display(),
            data = data.display(),
        ),
    )
    .unwrap();

    let cli_output = forge()
        .env("FORGE_FAKE_VAULT_SECRET", secret)
        .env("FORGE_FAKE_CREDENTIAL_VAULT_ARGS", &vault_args)
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "emit",
            "--addon-dir",
            addon_dir.to_str().unwrap(),
            "--addon",
            "forge.addon.vault_bearer_partner",
            "--adapter",
            "partner.vault_bearer_webhook_egress",
            "--event-type",
            "partner.vault_bearer_notification",
            "--action",
            "notify_partner",
            "--origin",
            "codex",
            "--payload",
            r#"{"id":"vault-bearer-cli-001","message":"ready"}"#,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let cli_text = String::from_utf8_lossy(&cli_output);
    let cli_json: Value = serde_json::from_slice(&cli_output).unwrap();
    assert_eq!(cli_json["status"], "event_egress_delivered");
    assert_eq!(cli_json["request"]["auth"], "bearer");
    assert!(cli_json["request"].get("secret_env").is_none());
    assert_eq!(
        cli_json["request"]["credential_vault"]["record"],
        "partner_api"
    );
    assert_eq!(
        cli_json["request"]["credential_vault"]["field"],
        "auth.token"
    );
    assert_eq!(cli_json["delivery"]["auth_scheme"], "bearer");
    assert_eq!(cli_json["delivery"]["secret_source"], "credential_vault");
    assert!(cli_json["delivery"].get("secret_env").is_none());
    assert_eq!(
        cli_json["delivery"]["credential_vault"]["record"],
        "partner_api"
    );
    assert_eq!(cli_json["delivery"]["status_code"], 202);
    assert!(!cli_text.contains(secret));
    let vault_args_text = fs::read_to_string(&vault_args).unwrap();
    assert!(vault_args_text.contains("--allow-secret-stdout"));
    assert!(vault_args_text.contains("--no-newline"));
    assert!(!vault_args_text.contains(secret));

    let mcp_input = serde_json::json!({
        "addon_dirs": [addon_dir.display().to_string()],
        "addon_id": "forge.addon.vault_bearer_partner",
        "adapter_id": "partner.vault_bearer_webhook_egress",
        "event_type": "partner.vault_bearer_notification",
        "action": "notify_partner",
        "origin": "codex",
        "payload": {"id": "vault-bearer-mcp-001", "message": "ready"},
    })
    .to_string();
    let mcp_output = forge()
        .env("FORGE_FAKE_VAULT_SECRET", secret)
        .env("FORGE_FAKE_CREDENTIAL_VAULT_ARGS", &vault_args)
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.events.emit",
            "--input",
            mcp_input.as_str(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let mcp_text = String::from_utf8_lossy(&mcp_output);
    let mcp_json: Value = serde_json::from_slice(&mcp_output).unwrap();
    assert_eq!(mcp_json["result"]["status"], "event_egress_delivered");
    assert_eq!(
        mcp_json["result"]["delivery"]["secret_source"],
        "credential_vault"
    );
    assert_eq!(
        mcp_json["result"]["delivery"]["credential_vault"]["field"],
        "auth.token"
    );
    assert_eq!(mcp_json["result"]["delivery"]["status_code"], 202);
    assert!(!mcp_text.contains(secret));

    handle.join().unwrap();
}

#[test]
fn event_egress_adapter_supports_https_endpoint_via_simulated_curl_transport() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let addon_dir = temp.path().join("addons");
    fs::create_dir_all(&addon_dir).unwrap();
    fs::write(
        addon_dir.join("https-partner.yaml"),
        r#"
id: forge.addon.https_partner
name: HTTPS Partner Addon
version: 0.1.0
source: test
lifecycle: enabled
permissions:
  - id: partner.notify
    description: Notify partner over HTTPS.
    risk: medium
    requires_human_approval: true
    tools: [partner_api]
    resources: [partner_notification]
    integrations: [partner.https]
    actions: [notify_partner]
    tenant_scopes: [organization]
capabilities:
  - id: partner_notifications
    title: Partner notifications
    domains: [operations]
    keywords: [partner, notification]
event_types:
  - id: partner.notification
    title: Partner notification
    transport: https
event_adapters:
  - id: partner.https_egress
    title: Partner HTTPS Egress
    transport: webhook
    direction: egress
    origins: [codex]
    actions: [notify_partner]
    event_types: [partner.notification]
    schema: partner.notification.v1
    auth: bearer
    secret_env: PARTNER_HTTPS_TOKEN
    permissions: [partner.notify]
    endpoint: "https://partner.example.test/forge"
    allowed_hosts: [partner.example.test]
    timeout_seconds: 5
    max_response_bytes: 65536
"#,
    )
    .unwrap();

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "authorize-permission",
            "--addon",
            "forge.addon.https_partner",
            "--permission",
            "partner.notify",
            "--risk",
            "medium",
            "--approved-by",
            "test",
            "--source",
            "test",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let cli_output = forge()
        .env("PARTNER_HTTPS_TOKEN", "partner-https-token-from-test")
        .env("FORGE_EVENT_EGRESS_HTTPS_MODE", "simulate")
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "emit",
            "--addon-dir",
            addon_dir.to_str().unwrap(),
            "--addon",
            "forge.addon.https_partner",
            "--adapter",
            "partner.https_egress",
            "--event-type",
            "partner.notification",
            "--action",
            "notify_partner",
            "--origin",
            "codex",
            "--payload",
            r#"{"id":"cli-https-001","message":"ready"}"#,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let cli_json: Value = serde_json::from_slice(&cli_output).unwrap();
    assert_eq!(cli_json["status"], "event_egress_delivered");
    assert_eq!(
        cli_json["delivery"]["endpoint"],
        "https://partner.example.test/forge"
    );
    assert_eq!(cli_json["delivery"]["auth_scheme"], "bearer");
    assert_eq!(cli_json["delivery"]["secret_env"], "PARTNER_HTTPS_TOKEN");
    assert_eq!(cli_json["delivery"]["status_code"], 202);
    assert_eq!(cli_json["delivery"]["success"], true);
    assert!(!String::from_utf8_lossy(&cli_output).contains("partner-https-token-from-test"));

    let mcp_input = serde_json::json!({
        "addon_dirs": [addon_dir.display().to_string()],
        "addon_id": "forge.addon.https_partner",
        "adapter_id": "partner.https_egress",
        "event_type": "partner.notification",
        "action": "notify_partner",
        "origin": "codex",
        "payload": {"id": "mcp-https-001", "message": "ready"}
    })
    .to_string();
    let mcp_output = forge()
        .env("PARTNER_HTTPS_TOKEN", "partner-https-token-from-test")
        .env("FORGE_EVENT_EGRESS_HTTPS_MODE", "simulate")
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.events.emit",
            "--input",
            mcp_input.as_str(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let mcp_json: Value = serde_json::from_slice(&mcp_output).unwrap();
    assert_eq!(mcp_json["result"]["status"], "event_egress_delivered");
    assert_eq!(mcp_json["result"]["delivery"]["status_code"], 202);
    assert_eq!(mcp_json["result"]["delivery"]["auth_scheme"], "bearer");
    assert!(!String::from_utf8_lossy(&mcp_output).contains("partner-https-token-from-test"));
}

#[test]
fn event_webhook_ingress_accepts_http_post_and_routes_through_declared_adapter() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let addon_dir = temp.path().join(".forge/addons");
    fs::create_dir_all(&addon_dir).unwrap();
    fs::write(
        addon_dir.join("logistics.yaml"),
        r#"
id: forge.addon.logistics_webhook
name: Logistics Webhook Addon
version: 0.1.0
permissions:
  - id: logistics.ingress
    risk: medium
    tools: [webhook-server]
    resources: [logistics.route_requested]
    integrations: [logistics.partner_webhook]
    actions: [start_workflow]
capabilities:
  - id: logistics_webhook_ingress
    title: Logistics webhook ingress
    domains: [logistics]
    keywords: [route, webhook]
event_types:
  - id: logistics.route_event.v1
    title: Logistics route event
    transport: webhook
event_adapters:
  - id: logistics.webhook_ingress
    title: Logistics Webhook Ingress
    transport: webhook
    direction: ingress
    origins: [logistics_partner_api]
    actions: [start_workflow]
    event_types: [logistics.route_event.v1]
    schema: logistics.route_event.v1
    auth: none
    permissions: [logistics.ingress]
"#,
    )
    .unwrap();
    let port = reserve_local_port();
    let child = StdCommand::new(assert_cmd::cargo::cargo_bin("forge"))
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "webhook-ingress",
            "--host",
            "127.0.0.1",
            "--port",
            &port.to_string(),
            "--path",
            "/webhook",
            "--origin",
            "logistics_partner_api",
            "--action",
            "start_workflow",
            "--schema",
            "logistics.route_event.v1",
            "--project-root",
            temp.path().to_str().unwrap(),
            "--route",
            "--max-requests",
            "1",
            "--output",
            "json",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let response = post_json_with_retry(
        port,
        "/webhook",
        r#"{"data":{"goal":"Create a logistics route workflow from webhook","customer":"acme"}}"#,
    );
    assert!(response.starts_with("HTTP/1.1 202 Accepted"));
    assert!(response.contains("webhook_event_ingested_and_routed"));

    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "webhook ingress command failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schema_version"], "forge.event_webhook_ingress.v1");
    assert_eq!(report["request_count"], 1);
    assert_eq!(report["ingested_count"], 1);
    assert_eq!(report["routed_count"], 1);
    assert_eq!(report["failed_count"], 0);
    assert_eq!(
        report["events"][0]["route"]["adapter_policy"]["status"],
        "matched"
    );
    assert_eq!(
        report["events"][0]["event"]["data"]["schema"],
        "logistics.route_event.v1"
    );
    assert_eq!(report["events"][0]["event"]["data"]["transport"], "webhook");
    assert!(report["events"][0]["route"]["workflow_id"].is_string());

    let inbox_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "inbox",
            "--status",
            "routed",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let inbox_json: Value = serde_json::from_slice(&inbox_output).unwrap();
    assert_eq!(inbox_json["event_count"], 1);
    assert_eq!(inbox_json["events"][0]["origin"], "logistics_partner_api");
    assert_eq!(inbox_json["events"][0]["action"], "start_workflow");
}

#[test]
fn event_webhook_ingress_verifies_hmac_before_routing_declared_adapter() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let addon_dir = temp.path().join(".forge/addons");
    fs::create_dir_all(&addon_dir).unwrap();
    fs::write(
        addon_dir.join("partner.yaml"),
        r#"
id: forge.addon.partner_signed_webhook
name: Partner Signed Webhook Addon
version: 0.1.0
permissions:
  - id: partner.ingress
    risk: medium
    tools: [webhook-server]
    resources: [partner.workflow_requested]
    integrations: [partner.signed_webhook]
    actions: [start_workflow]
capabilities:
  - id: partner_signed_webhook_ingress
    title: Partner signed webhook ingress
    domains: [operations]
    keywords: [partner, webhook, hmac]
event_types:
  - id: partner.workflow_requested.v1
    title: Partner workflow request
    transport: webhook
event_adapters:
  - id: partner.signed_webhook_ingress
    title: Partner Signed Webhook Ingress
    transport: webhook
    direction: ingress
    origins: [partner_gateway]
    actions: [start_workflow]
    event_types: [partner.workflow_requested.v1]
    schema: partner.workflow_requested.v1
    auth: hmac
    permissions: [partner.ingress]
"#,
    )
    .unwrap();

    let port = reserve_local_port();
    let secret = "forge-test-webhook-secret";
    let child = StdCommand::new(assert_cmd::cargo::cargo_bin("forge"))
        .env("FORGE_TEST_WEBHOOK_SECRET", secret)
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "webhook-ingress",
            "--host",
            "127.0.0.1",
            "--port",
            &port.to_string(),
            "--path",
            "/partner",
            "--origin",
            "partner_gateway",
            "--action",
            "start_workflow",
            "--schema",
            "partner.workflow_requested.v1",
            "--project-root",
            temp.path().to_str().unwrap(),
            "--route",
            "--max-requests",
            "2",
            "--hmac-secret-env",
            "FORGE_TEST_WEBHOOK_SECRET",
            "--signature-header",
            "X-Test-Signature",
            "--output",
            "json",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let bad_body = r#"{"data":{"goal":"This request must not route"}}"#;
    let bad_response = post_json_with_retry_headers(
        port,
        "/partner",
        bad_body,
        &[("X-Test-Signature", "sha256=00")],
    );
    assert!(bad_response.starts_with("HTTP/1.1 400 Bad Request"));
    assert!(bad_response.contains("webhook_ingress_failed"));
    assert!(bad_response.contains("webhook HMAC signature mismatch"));

    let signed_body =
        r#"{"data":{"goal":"Create a signed partner workflow from webhook","customer":"acme"}}"#;
    let signature = test_hmac_sha256_header(secret, signed_body);
    let signed_response = post_json_with_retry_headers(
        port,
        "/partner",
        signed_body,
        &[("X-Test-Signature", signature.as_str())],
    );
    assert!(signed_response.starts_with("HTTP/1.1 202 Accepted"));
    assert!(signed_response.contains("webhook_event_ingested_and_routed"));

    let output = child.wait_with_output().unwrap();
    assert!(
        !output.status.success(),
        "webhook ingress command should report failure when any request fails\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schema_version"], "forge.event_webhook_ingress.v1");
    assert_eq!(
        report["status"],
        "event_webhook_ingress_completed_with_failures"
    );
    assert_eq!(report["auth"]["required"], true);
    assert_eq!(report["auth"]["scheme"], "hmac_sha256");
    assert_eq!(report["auth"]["signature_header"], "X-Test-Signature");
    assert_eq!(report["auth"]["secret_env"], "FORGE_TEST_WEBHOOK_SECRET");
    assert_eq!(report["request_count"], 2);
    assert_eq!(report["ingested_count"], 1);
    assert_eq!(report["routed_count"], 1);
    assert_eq!(report["failed_count"], 1);

    assert_eq!(report["events"][0]["http_status"], 400);
    assert_eq!(report["events"][0]["auth_verified"], false);
    assert!(report["events"][0]["event_id"].is_null());
    assert_eq!(report["events"][1]["http_status"], 202);
    assert_eq!(report["events"][1]["auth_verified"], true);
    assert_eq!(report["events"][1]["event"]["data"]["auth_verified"], true);
    assert_eq!(
        report["events"][1]["route"]["adapter_policy"]["status"],
        "matched"
    );
    assert_eq!(
        report["events"][1]["route"]["adapter_policy"]["auth_verified"],
        true
    );
    assert_eq!(
        report["events"][1]["route"]["adapter_policy"]["matched_adapter"]["adapter"]["auth"],
        "hmac"
    );
    assert!(report["events"][1]["route"]["workflow_id"].is_string());
}

#[test]
fn event_service_plan_models_managed_worker_and_webhook_contracts_for_cli_and_mcp() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let worker_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "service-plan",
            "--kind",
            "worker",
            "--project-root",
            temp.path().to_str().unwrap(),
            "--status",
            "pending",
            "--limit",
            "2",
            "--max-cycles",
            "4",
            "--interval-seconds",
            "5",
            "--idle-exit",
            "--lease-seconds",
            "120",
            "--heartbeat-seconds",
            "30",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let worker_json: Value = serde_json::from_slice(&worker_output).unwrap();
    assert_eq!(worker_json["schema_version"], "forge.event_service_plan.v1");
    assert_eq!(worker_json["status"], "event_service_plan_created");
    assert_eq!(worker_json["service_kind"], "worker");
    assert_eq!(worker_json["mode"], "plan_only");
    assert_eq!(worker_json["settings"]["limit"], 2);
    assert_eq!(worker_json["settings"]["max_cycles"], 4);
    assert_eq!(worker_json["lease"]["ttl_seconds"], 120);
    assert_eq!(worker_json["lease"]["heartbeat_interval_seconds"], 30);
    assert!(worker_json["command"]
        .as_array()
        .unwrap()
        .contains(&Value::String("worker".to_string())));
    assert!(worker_json["global_event_id"].as_i64().unwrap() > 0);

    let webhook_input = serde_json::json!({
        "kind": "webhook_ingress",
        "project_root": temp.path().display().to_string(),
        "host": "127.0.0.1",
        "port": 9090,
        "path": "/partner",
        "origin": "partner_gateway",
        "action": "start_workflow",
        "schema": "partner.workflow_requested.v1",
        "route": true,
        "max_requests": 25,
        "hmac_secret_env": "FORGE_PARTNER_WEBHOOK_SECRET",
        "signature_header": "X-Partner-Signature",
        "lease_seconds": 600,
        "heartbeat_seconds": 60
    })
    .to_string();
    let webhook_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.events.service_plan",
            "--input",
            webhook_input.as_str(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let webhook_json: Value = serde_json::from_slice(&webhook_output).unwrap();
    let result = &webhook_json["result"];
    assert_eq!(result["schema_version"], "forge.event_service_plan.v1");
    assert_eq!(result["service_kind"], "webhook_ingress");
    assert_eq!(result["settings"]["path"], "/partner");
    assert_eq!(
        result["settings"]["hmac_secret_env"],
        "FORGE_PARTNER_WEBHOOK_SECRET"
    );
    assert_eq!(
        result["settings"]["signature_header"],
        "X-Partner-Signature"
    );
    assert_eq!(result["settings"]["route"], true);
    assert_eq!(result["lease"]["ttl_seconds"], 600);
    assert!(result["command"]
        .as_array()
        .unwrap()
        .contains(&Value::String("--hmac-secret-env".to_string())));
    assert!(result["command"]
        .as_array()
        .unwrap()
        .contains(&Value::String("FORGE_PARTNER_WEBHOOK_SECRET".to_string())));
    assert!(result["global_event_id"].as_i64().unwrap() > 0);

    let timeline_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "timeline",
            "--limit",
            "20",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let timeline_json: Value = serde_json::from_slice(&timeline_output).unwrap();
    let service_plan_events = timeline_json["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|event| {
            event["kind"] == "event_service_plan_created" && event["source"] == "event_service_plan"
        })
        .collect::<Vec<_>>();
    assert_eq!(service_plan_events.len(), 2);
}

#[test]
fn event_service_run_executes_worker_with_persistent_lease_and_mcp_status() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let ingest_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "ingest",
            "--origin",
            "service_test",
            "--action",
            "start_workflow",
            "--input",
            r#"{"goal":"Create workflow from managed service run"}"#,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let ingest_json: Value = serde_json::from_slice(&ingest_output).unwrap();
    assert_eq!(ingest_json["event"]["status"], "pending");
    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "ingest",
            "--origin",
            "service_test",
            "--action",
            "start_workflow",
            "--input",
            r#"{"goal":"Create second workflow from managed service run"}"#,
            "--output",
            "json",
        ])
        .assert()
        .success();

    let run_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "service-run",
            "--kind",
            "worker",
            "--project-root",
            temp.path().to_str().unwrap(),
            "--limit",
            "1",
            "--max-cycles",
            "2",
            "--interval-seconds",
            "0",
            "--idle-exit",
            "--lease-owner",
            "test-service-manager",
            "--lease-seconds",
            "60",
            "--heartbeat-seconds",
            "10",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let run_json: Value = serde_json::from_slice(&run_output).unwrap();
    assert_eq!(run_json["schema_version"], "forge.event_service_run.v1");
    assert_eq!(run_json["status"], "event_service_run_completed");
    assert_eq!(run_json["service"]["service_kind"], "worker");
    assert_eq!(run_json["service"]["status"], "completed");
    assert_eq!(run_json["service"]["lease_owner"], "test-service-manager");
    assert_eq!(run_json["worker_report"]["cycle_count"], 2);
    assert_eq!(run_json["worker_report"]["scanned_count"], 2);
    assert_eq!(run_json["worker_report"]["routed_count"], 2);
    assert_eq!(run_json["health"]["status"], "completed");
    assert!(run_json["health"]["heartbeat_count"].as_u64().unwrap() >= 3);
    assert!(run_json["health"]["lease_renewal_count"].as_u64().unwrap() >= 2);
    assert_eq!(
        run_json["service"]["data"]["health"]["heartbeat_count"],
        run_json["health"]["heartbeat_count"]
    );
    assert_eq!(
        run_json["service"]["data"]["heartbeat"]["last_heartbeat_at"],
        run_json["service"]["last_heartbeat_at"]
    );
    assert!(run_json["global_event_id"].as_i64().unwrap() > 0);

    let mcp_run_input = serde_json::json!({
        "kind": "worker",
        "project_root": temp.path().display().to_string(),
        "limit": 1,
        "max_cycles": 1,
        "interval_seconds": 0,
        "idle_exit": true,
        "lease_owner": "mcp-service-manager",
        "lease_seconds": 60,
        "heartbeat_seconds": 10
    })
    .to_string();
    let mcp_run_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.events.service_run",
            "--input",
            mcp_run_input.as_str(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let mcp_run_json: Value = serde_json::from_slice(&mcp_run_output).unwrap();
    assert_eq!(
        mcp_run_json["result"]["schema_version"],
        "forge.event_service_run.v1"
    );
    assert_eq!(mcp_run_json["result"]["service"]["status"], "completed");
    assert_eq!(mcp_run_json["result"]["worker_report"]["scanned_count"], 0);

    let services_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.events.services",
            "--input",
            r#"{"kind":"worker","limit":10}"#,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let services_json: Value = serde_json::from_slice(&services_output).unwrap();
    assert_eq!(
        services_json["result"]["schema_version"],
        "forge.event_services.v1"
    );
    assert_eq!(services_json["result"]["service_count"], 2);
    assert!(services_json["result"]["services"]
        .as_array()
        .unwrap()
        .iter()
        .all(|service| service["service_kind"] == "worker"
            && service["lease_id"]
                .as_str()
                .unwrap()
                .starts_with("evtlease_")
            && service["last_heartbeat_at"].is_string()));

    let timeline_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "timeline",
            "--limit",
            "20",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let timeline_json: Value = serde_json::from_slice(&timeline_output).unwrap();
    assert!(timeline_json["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|event| {
            event["kind"] == "event_service_run_completed" && event["source"] == "event_service_run"
        }));
}

#[test]
fn event_services_recover_marks_expired_running_leases_stale_for_cli_and_mcp() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let project_root = temp.path().join("project");
    fs::create_dir_all(&project_root).unwrap();
    drop(ForgeStore::open(&store).unwrap());

    let acquired_at = (Utc::now() - Duration::minutes(20)).to_rfc3339();
    let expires_at = (Utc::now() - Duration::minutes(10)).to_rfc3339();
    let heartbeat_at = (Utc::now() - Duration::minutes(11)).to_rfc3339();
    let data = serde_json::json!({
        "schema_version": "test.event_service_state.v1",
        "health": {
            "status": "running",
            "heartbeat_count": 1
        }
    });
    let connection = Connection::open(&store).unwrap();
    connection
        .execute(
            r#"
            INSERT INTO event_services (
                id,
                service_kind,
                status,
                lease_owner,
                lease_id,
                lease_acquired_at,
                lease_expires_at,
                last_heartbeat_at,
                heartbeat_ttl_seconds,
                data_json
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            "#,
            rusqlite::params![
                "svc-stale-worker",
                "worker",
                "running",
                "stale-owner",
                "lease-stale-worker",
                acquired_at,
                expires_at,
                heartbeat_at,
                60_i64,
                serde_json::to_string(&data).unwrap()
            ],
        )
        .unwrap();

    let recover_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "services-recover",
            "--project-root",
            project_root.to_str().unwrap(),
            "--kind",
            "worker",
            "--limit",
            "10",
            "--origin",
            "test_recovery",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let recover_json: Value = serde_json::from_slice(&recover_output).unwrap();
    assert_eq!(
        recover_json["schema_version"],
        "forge.event_services_recovery.v1"
    );
    assert_eq!(
        recover_json["status"],
        "event_services_recovered_stale_running_leases"
    );
    assert_eq!(recover_json["scanned_count"], 1);
    assert_eq!(recover_json["stale_running_count"], 1);
    assert_eq!(recover_json["recovered_count"], 1);
    assert_eq!(recover_json["services"][0]["id"], "svc-stale-worker");
    assert_eq!(recover_json["services"][0]["status"], "stale");
    assert_eq!(
        recover_json["services"][0]["data"]["recovery"]["previous_status"],
        "running"
    );
    assert_eq!(
        recover_json["services"][0]["data"]["recovery"]["origin"],
        "test_recovery"
    );
    assert!(recover_json["global_event_id"].as_i64().unwrap() > 0);

    let stale_services_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "services",
            "--kind",
            "worker",
            "--status",
            "stale",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let stale_services_json: Value = serde_json::from_slice(&stale_services_output).unwrap();
    assert_eq!(stale_services_json["service_count"], 1);
    assert_eq!(stale_services_json["services"][0]["id"], "svc-stale-worker");

    let mcp_input = serde_json::json!({
        "project_root": project_root.display().to_string(),
        "kind": "worker",
        "limit": 10,
        "origin": "mcp_recovery"
    })
    .to_string();
    let mcp_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.events.services_recover",
            "--input",
            mcp_input.as_str(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let mcp_json: Value = serde_json::from_slice(&mcp_output).unwrap();
    assert_eq!(
        mcp_json["result"]["schema_version"],
        "forge.event_services_recovery.v1"
    );
    assert_eq!(
        mcp_json["result"]["status"],
        "event_services_no_stale_running_leases"
    );
    assert_eq!(mcp_json["result"]["recovered_count"], 0);
}

#[test]
fn event_runtime_reconcile_and_daemon_can_recover_stale_worker_services() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let project_root = temp.path().join("project");
    fs::create_dir_all(&project_root).unwrap();
    drop(ForgeStore::open(&store).unwrap());

    insert_expired_event_service(&store, "svc-stale-runtime-reconcile", "worker");
    let reconcile_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "runtime-reconcile",
            "--project-root",
            project_root.to_str().unwrap(),
            "--service-limit",
            "10",
            "--recover-stale-services",
            "--lease-owner",
            "test-runtime-recovery",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let reconcile_json: Value = serde_json::from_slice(&reconcile_output).unwrap();
    assert_eq!(
        reconcile_json["schema_version"],
        "forge.event_runtime_reconcile.v1"
    );
    assert_eq!(reconcile_json["recover_stale_services"], true);
    assert_eq!(
        reconcile_json["service_recovery"]["schema_version"],
        "forge.event_services_recovery.v1"
    );
    assert_eq!(reconcile_json["service_recovery"]["recovered_count"], 1);
    assert_eq!(
        reconcile_json["service_recovery"]["services"][0]["id"],
        "svc-stale-runtime-reconcile"
    );
    assert_eq!(
        reconcile_json["service_recovery"]["services"][0]["status"],
        "stale"
    );
    assert_eq!(reconcile_json["services"]["running_count"], 0);
    assert_eq!(reconcile_json["services"]["active_lease_count"], 0);
    assert_eq!(reconcile_json["services"]["stale_running_count"], 0);
    assert!(reconcile_json["services"]["services"]
        .as_array()
        .unwrap()
        .iter()
        .any(|service| service["id"] == "svc-stale-runtime-reconcile"
            && service["status"] == "stale"));

    insert_expired_event_service(&store, "svc-stale-runtime-daemon", "worker");
    let daemon_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "runtime-daemon",
            "--project-root",
            project_root.to_str().unwrap(),
            "--service-limit",
            "10",
            "--recover-stale-services",
            "--max-cycles",
            "1",
            "--interval-seconds",
            "0",
            "--lease-owner",
            "test-runtime-daemon-recovery",
            "--lease-seconds",
            "60",
            "--heartbeat-seconds",
            "10",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let daemon_json: Value = serde_json::from_slice(&daemon_output).unwrap();
    assert_eq!(
        daemon_json["schema_version"],
        "forge.event_runtime_daemon.v1"
    );
    assert_eq!(daemon_json["recover_stale_services"], true);
    assert_eq!(daemon_json["cycle_count"], 1);
    assert_eq!(
        daemon_json["cycles"][0]["report"]["service_recovery"]["recovered_count"],
        1
    );
    assert_eq!(
        daemon_json["cycles"][0]["report"]["service_recovery"]["services"][0]["id"],
        "svc-stale-runtime-daemon"
    );
    assert_eq!(
        daemon_json["cycles"][0]["report"]["services"]["stale_running_count"],
        0
    );

    let stale_services_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "services",
            "--kind",
            "worker",
            "--status",
            "stale",
            "--limit",
            "10",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let stale_services_json: Value = serde_json::from_slice(&stale_services_output).unwrap();
    assert_eq!(stale_services_json["service_count"], 2);
    assert!(stale_services_json["services"]
        .as_array()
        .unwrap()
        .iter()
        .any(|service| service["id"] == "svc-stale-runtime-reconcile"));
    assert!(stale_services_json["services"]
        .as_array()
        .unwrap()
        .iter()
        .any(|service| service["id"] == "svc-stale-runtime-daemon"));
}

#[test]
fn event_service_supervisor_runs_bounded_worker_restarts_and_mcp_stop_file() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    for goal in [
        "Create first workflow from supervised worker",
        "Create second workflow from supervised worker",
    ] {
        forge()
            .args([
                "--store",
                store.to_str().unwrap(),
                "events",
                "ingest",
                "--origin",
                "supervisor_test",
                "--action",
                "start_workflow",
                "--input",
                &serde_json::json!({ "goal": goal }).to_string(),
                "--output",
                "json",
            ])
            .assert()
            .success();
    }

    let supervise_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "service-supervise",
            "--kind",
            "worker",
            "--project-root",
            temp.path().to_str().unwrap(),
            "--limit",
            "1",
            "--max-cycles",
            "1",
            "--interval-seconds",
            "0",
            "--max-runs",
            "2",
            "--backoff-initial-seconds",
            "0",
            "--backoff-max-seconds",
            "0",
            "--lease-owner",
            "test-supervisor",
            "--lease-seconds",
            "60",
            "--heartbeat-seconds",
            "10",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let supervise_json: Value = serde_json::from_slice(&supervise_output).unwrap();
    assert_eq!(
        supervise_json["schema_version"],
        "forge.event_service_supervisor.v1"
    );
    assert_eq!(
        supervise_json["status"],
        "event_service_supervisor_completed"
    );
    assert_eq!(supervise_json["service_kind"], "worker");
    assert_eq!(supervise_json["run_count"], 2);
    assert_eq!(supervise_json["success_count"], 2);
    assert_eq!(supervise_json["failure_count"], 0);
    assert_eq!(
        supervise_json["runs"][0]["report"]["worker_report"]["routed_count"],
        1
    );
    assert_eq!(
        supervise_json["runs"][1]["report"]["worker_report"]["routed_count"],
        1
    );
    assert!(supervise_json["global_event_id"].as_i64().unwrap() > 0);

    let services_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "services",
            "--kind",
            "worker",
            "--limit",
            "10",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let services_json: Value = serde_json::from_slice(&services_output).unwrap();
    assert_eq!(services_json["service_count"], 2);
    assert!(services_json["services"]
        .as_array()
        .unwrap()
        .iter()
        .all(|service| service["service_kind"] == "worker"
            && service["status"] == "completed"
            && service["lease_owner"] == "test-supervisor"));

    let stop_file = temp.path().join("supervisor.stop");
    fs::write(&stop_file, "stop").unwrap();
    let mcp_input = serde_json::json!({
        "kind": "worker",
        "project_root": temp.path().display().to_string(),
        "max_runs": 3,
        "interval_seconds": 0,
        "stop_file": stop_file.display().to_string(),
        "backoff_initial_seconds": 0,
        "backoff_max_seconds": 0,
        "lease_owner": "mcp-stop-supervisor",
        "lease_seconds": 60,
        "heartbeat_seconds": 10
    })
    .to_string();
    let mcp_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.events.service_supervise",
            "--input",
            mcp_input.as_str(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let mcp_json: Value = serde_json::from_slice(&mcp_output).unwrap();
    assert_eq!(
        mcp_json["result"]["schema_version"],
        "forge.event_service_supervisor.v1"
    );
    assert_eq!(
        mcp_json["result"]["status"],
        "event_service_supervisor_stopped"
    );
    assert_eq!(mcp_json["result"]["run_count"], 0);
    assert_eq!(mcp_json["result"]["stop_requested"], true);
    assert_eq!(mcp_json["result"]["stopped_reason"], "stop_file_requested");

    let timeline_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "timeline",
            "--limit",
            "50",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let timeline_json: Value = serde_json::from_slice(&timeline_output).unwrap();
    assert!(timeline_json["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|event| {
            event["kind"] == "event_service_supervisor_completed"
                && event["source"] == "event_service_supervisor"
        }));
    assert!(timeline_json["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|event| {
            event["kind"] == "event_service_supervisor_stopped"
                && event["source"] == "event_service_supervisor"
        }));
}

#[test]
fn event_runtime_reconcile_uses_registry_actions_and_can_execute_supervisor() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let project_root = temp.path().join("project");
    fs::create_dir_all(&project_root).unwrap();

    let run_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "request",
            "start",
            "--goal",
            "Operate recurring partner intake with event wakeups",
            "--origin",
            "codex",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let run_json: Value = serde_json::from_slice(&run_output).unwrap();
    let persistent_workflow_id = run_json["workflow_id"].as_str().unwrap();

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "ingest",
            "--origin",
            "partner_api",
            "--action",
            "start_workflow",
            "--input",
            &serde_json::json!({ "goal": "Create partner onboarding workflow" }).to_string(),
            "--output",
            "json",
        ])
        .assert()
        .success();

    let reconcile_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "runtime-reconcile",
            "--project-root",
            project_root.to_str().unwrap(),
            "--limit",
            "1",
            "--service-limit",
            "10",
            "--execute",
            "--max-cycles",
            "1",
            "--interval-seconds",
            "0",
            "--max-runs",
            "1",
            "--backoff-initial-seconds",
            "0",
            "--backoff-max-seconds",
            "0",
            "--lease-owner",
            "test-runtime-reconciler",
            "--lease-seconds",
            "60",
            "--heartbeat-seconds",
            "10",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let reconcile_json: Value = serde_json::from_slice(&reconcile_output).unwrap();
    assert_eq!(
        reconcile_json["schema_version"],
        "forge.event_runtime_reconcile.v1"
    );
    assert_eq!(reconcile_json["status"], "event_runtime_reconcile_executed");
    assert_eq!(reconcile_json["execute"], true);
    assert_eq!(
        reconcile_json["registry"]["schema_version"],
        "forge.event_runtime_registry_snapshot.v1"
    );
    assert_eq!(
        reconcile_json["registry"]["persistent_workflows"]
            .as_u64()
            .unwrap(),
        1
    );
    assert!(reconcile_json["registry"]["actionable_workflows"]
        .as_array()
        .unwrap()
        .iter()
        .any(|workflow| workflow["workflow_id"] == persistent_workflow_id
            && workflow["operator_action"] == "keep_event_listener_ready"));
    assert_eq!(reconcile_json["inbox"]["pending_event_count"], 1);
    assert_eq!(reconcile_json["recommendation_count"], 1);
    assert_eq!(
        reconcile_json["recommendations"][0]["action"],
        "start_event_worker_supervisor"
    );
    assert_eq!(reconcile_json["recommendations"][0]["required"], true);
    assert!(reconcile_json["recommendations"][0]["command"]
        .as_array()
        .unwrap()
        .iter()
        .any(|part| part == "service-supervise"));
    assert_eq!(reconcile_json["execution_count"], 1);
    assert_eq!(
        reconcile_json["executions"][0]["schema_version"],
        "forge.event_service_supervisor.v1"
    );
    assert_eq!(reconcile_json["executions"][0]["run_count"], 1);
    assert_eq!(reconcile_json["executions"][0]["success_count"], 1);

    let mcp_input = serde_json::json!({
        "project_root": project_root.display().to_string(),
        "limit": 1,
        "service_limit": 10,
        "execute": false,
        "interval_seconds": 0,
        "max_runs": 1,
        "backoff_initial_seconds": 0,
        "backoff_max_seconds": 0
    })
    .to_string();
    let mcp_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.events.runtime_reconcile",
            "--input",
            mcp_input.as_str(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let mcp_json: Value = serde_json::from_slice(&mcp_output).unwrap();
    assert_eq!(
        mcp_json["result"]["schema_version"],
        "forge.event_runtime_reconcile.v1"
    );
    assert_eq!(mcp_json["result"]["execute"], false);
    assert_eq!(mcp_json["result"]["execution_count"], 0);

    let timeline_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "timeline",
            "--limit",
            "50",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let timeline_json: Value = serde_json::from_slice(&timeline_output).unwrap();
    assert!(timeline_json["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|event| {
            event["kind"] == "event_runtime_reconcile_executed"
                && event["source"] == "event_runtime_reconcile"
        }));
}

#[test]
fn event_runtime_daemon_persists_service_heartbeat_and_stop_file() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let project_root = temp.path().join("project");
    fs::create_dir_all(&project_root).unwrap();

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "ingest",
            "--origin",
            "partner_api",
            "--action",
            "start_workflow",
            "--input",
            &serde_json::json!({ "goal": "Create daemon routed workflow" }).to_string(),
            "--output",
            "json",
        ])
        .assert()
        .success();

    let daemon_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "runtime-daemon",
            "--project-root",
            project_root.to_str().unwrap(),
            "--limit",
            "1",
            "--service-limit",
            "10",
            "--execute",
            "--max-cycles",
            "1",
            "--interval-seconds",
            "0",
            "--max-runs",
            "1",
            "--backoff-initial-seconds",
            "0",
            "--backoff-max-seconds",
            "0",
            "--lease-owner",
            "test-runtime-daemon",
            "--lease-seconds",
            "60",
            "--heartbeat-seconds",
            "10",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let daemon_json: Value = serde_json::from_slice(&daemon_output).unwrap();
    assert_eq!(
        daemon_json["schema_version"],
        "forge.event_runtime_daemon.v1"
    );
    assert_eq!(daemon_json["status"], "event_runtime_daemon_completed");
    assert_eq!(daemon_json["service"]["service_kind"], "runtime_reconcile");
    assert_eq!(daemon_json["service"]["status"], "completed");
    assert_eq!(daemon_json["service"]["lease_owner"], "test-runtime-daemon");
    assert_eq!(daemon_json["cycle_count"], 1);
    assert_eq!(daemon_json["execution_count"], 1);
    assert_eq!(
        daemon_json["cycles"][0]["report"]["schema_version"],
        "forge.event_runtime_reconcile.v1"
    );
    assert_eq!(
        daemon_json["cycles"][0]["report"]["status"],
        "event_runtime_reconcile_executed"
    );
    assert_eq!(
        daemon_json["cycles"][0]["report"]["executions"][0]["schema_version"],
        "forge.event_service_supervisor.v1"
    );

    let services_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "services",
            "--kind",
            "runtime_reconcile",
            "--limit",
            "10",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let services_json: Value = serde_json::from_slice(&services_output).unwrap();
    assert_eq!(services_json["service_count"], 1);
    assert_eq!(
        services_json["services"][0]["data"]["health"]["schema_version"],
        "forge.event_runtime_daemon_health.v1"
    );

    let stop_file = temp.path().join("runtime-daemon.stop");
    fs::write(&stop_file, "stop").unwrap();
    let mcp_input = serde_json::json!({
        "project_root": project_root.display().to_string(),
        "execute": true,
        "max_cycles": 3,
        "interval_seconds": 0,
        "stop_file": stop_file.display().to_string(),
        "lease_owner": "mcp-runtime-daemon",
        "lease_seconds": 60,
        "heartbeat_seconds": 10,
        "max_runs": 1,
        "backoff_initial_seconds": 0,
        "backoff_max_seconds": 0
    })
    .to_string();
    let mcp_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.events.runtime_daemon",
            "--input",
            mcp_input.as_str(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let mcp_json: Value = serde_json::from_slice(&mcp_output).unwrap();
    assert_eq!(
        mcp_json["result"]["schema_version"],
        "forge.event_runtime_daemon.v1"
    );
    assert_eq!(mcp_json["result"]["status"], "event_runtime_daemon_stopped");
    assert_eq!(mcp_json["result"]["cycle_count"], 0);
    assert_eq!(mcp_json["result"]["stop_requested"], true);
    assert_eq!(mcp_json["result"]["stopped_reason"], "stop_file_requested");
    assert_eq!(mcp_json["result"]["service"]["status"], "stopped");

    let timeline_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "timeline",
            "--limit",
            "50",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let timeline_json: Value = serde_json::from_slice(&timeline_output).unwrap();
    assert!(timeline_json["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|event| {
            event["kind"] == "event_runtime_daemon_completed"
                && event["source"] == "event_runtime_daemon"
        }));
    assert!(timeline_json["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|event| {
            event["kind"] == "event_runtime_daemon_stopped"
                && event["source"] == "event_runtime_daemon"
        }));
}

#[test]
fn event_runtime_daemon_continuous_mode_retains_bounded_cycle_reports() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let project_root = temp.path().join("project");
    fs::create_dir_all(&project_root).unwrap();

    let daemon_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "runtime-daemon",
            "--project-root",
            project_root.to_str().unwrap(),
            "--continuous",
            "--cycle-retention",
            "1",
            "--idle-exit",
            "--max-cycles",
            "99",
            "--interval-seconds",
            "0",
            "--lease-owner",
            "test-runtime-daemon-continuous",
            "--lease-seconds",
            "60",
            "--heartbeat-seconds",
            "10",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let daemon_json: Value = serde_json::from_slice(&daemon_output).unwrap();
    assert_eq!(
        daemon_json["schema_version"],
        "forge.event_runtime_daemon.v1"
    );
    assert_eq!(daemon_json["status"], "event_runtime_daemon_completed");
    assert_eq!(daemon_json["continuous"], true);
    assert_eq!(daemon_json["cycle_retention"], 1);
    assert_eq!(daemon_json["max_cycles"], 99);
    assert_eq!(daemon_json["cycle_count"], 1);
    assert_eq!(daemon_json["retained_cycle_count"], 1);
    assert_eq!(daemon_json["dropped_cycle_count"], 0);
    assert_eq!(daemon_json["stopped_reason"], "idle_exit");
    assert_eq!(
        daemon_json["cycles"][0]["report"]["status"],
        "event_runtime_reconcile_no_action"
    );

    let services_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "services",
            "--kind",
            "runtime_reconcile",
            "--limit",
            "10",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let services_json: Value = serde_json::from_slice(&services_output).unwrap();
    let service = services_json["services"]
        .as_array()
        .unwrap()
        .iter()
        .find(|service| service["lease_owner"] == "test-runtime-daemon-continuous")
        .unwrap();
    assert_eq!(service["data"]["continuous"], true);
    assert_eq!(service["data"]["cycle_retention"], 1);
    assert_eq!(service["data"]["health"]["cycle_count"], 1);
    assert_eq!(service["data"]["health"]["retained_cycle_count"], 1);

    let stop_file = temp.path().join("runtime-daemon-continuous.stop");
    fs::write(&stop_file, "stop").unwrap();
    let mcp_input = serde_json::json!({
        "project_root": project_root.display().to_string(),
        "continuous": true,
        "cycle_retention": 1,
        "max_cycles": 99,
        "interval_seconds": 0,
        "stop_file": stop_file.display().to_string(),
        "lease_owner": "mcp-runtime-daemon-continuous",
        "lease_seconds": 60,
        "heartbeat_seconds": 10
    })
    .to_string();
    let mcp_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.events.runtime_daemon",
            "--input",
            mcp_input.as_str(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let mcp_json: Value = serde_json::from_slice(&mcp_output).unwrap();
    assert_eq!(mcp_json["result"]["continuous"], true);
    assert_eq!(mcp_json["result"]["cycle_retention"], 1);
    assert_eq!(mcp_json["result"]["status"], "event_runtime_daemon_stopped");
    assert_eq!(mcp_json["result"]["cycle_count"], 0);
    assert_eq!(mcp_json["result"]["retained_cycle_count"], 0);
    assert_eq!(mcp_json["result"]["stop_requested"], true);
    assert_eq!(mcp_json["result"]["stopped_reason"], "stop_file_requested");
}

#[test]
fn event_runtime_daemon_rehydrates_due_schedules_when_enabled() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let project_root = temp.path().join("project");
    fs::create_dir_all(&project_root).unwrap();

    let created = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "schedule",
            "create-daily-goal-research",
            "--goal",
            "partner-demo",
            "--cron",
            "0 8 * * *",
            "--timezone",
            "America/Sao_Paulo",
            "--origin",
            "forge_cli",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let created_json: Value = serde_json::from_slice(&created).unwrap();
    let workflow_id = created_json["workflow_id"].as_str().unwrap().to_string();
    let schedule_task_id = created_json["workflow"]["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|task| task["schedule"].is_object())
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "schedule",
            "update",
            "--workflow",
            &workflow_id,
            "--task",
            &schedule_task_id,
            "--next-run-at",
            "2000-01-01T00:00:00Z",
            "--origin",
            "codex",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let daemon_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "runtime-daemon",
            "--project-root",
            project_root.to_str().unwrap(),
            "--execute",
            "--scan-schedules",
            "--schedule-executor",
            "test-runtime-scheduler",
            "--schedule-max-workers",
            "1",
            "--schedule-ttl-seconds",
            "60",
            "--max-cycles",
            "1",
            "--interval-seconds",
            "0",
            "--max-runs",
            "1",
            "--backoff-initial-seconds",
            "0",
            "--backoff-max-seconds",
            "0",
            "--lease-owner",
            "test-runtime-daemon-schedule",
            "--lease-seconds",
            "60",
            "--heartbeat-seconds",
            "10",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let daemon_json: Value = serde_json::from_slice(&daemon_output).unwrap();
    assert_eq!(
        daemon_json["schema_version"],
        "forge.event_runtime_daemon.v1"
    );
    assert_eq!(daemon_json["status"], "event_runtime_daemon_completed");
    assert_eq!(daemon_json["scan_schedules"], true);
    assert_eq!(daemon_json["schedule_execution_count"], 1);
    assert_eq!(daemon_json["execution_count"], 0);

    let cycle = &daemon_json["cycles"][0]["report"];
    assert_eq!(cycle["schema_version"], "forge.event_runtime_reconcile.v1");
    assert_eq!(cycle["status"], "event_runtime_reconcile_executed");
    assert_eq!(cycle["schedule_execution_count"], 1);
    assert_eq!(
        cycle["schedule"]["worker_status"]["schema_version"],
        "forge.schedule.worker_status.v1"
    );
    assert_eq!(
        cycle["schedule"]["scan_due"]["schema_version"],
        "forge.schedule.scan_due.v1"
    );
    assert_eq!(
        cycle["schedule"]["scan_due"]["summary"]["executed_workflows"],
        1
    );
    assert!(cycle["schedule"]["scan_due"]["results"]
        .as_array()
        .unwrap()
        .iter()
        .any(|result| result["workflow_id"] == workflow_id
            && result["run_due"]["status"] == "due_workflow_executed"));
}

#[test]
fn event_worker_and_service_run_honor_cooperative_stop_file() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let stop_file = temp.path().join("forge-event-worker.stop");
    fs::write(&stop_file, "stop").unwrap();

    let worker_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "worker",
            "--project-root",
            temp.path().to_str().unwrap(),
            "--max-cycles",
            "3",
            "--interval-seconds",
            "0",
            "--stop-file",
            stop_file.to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let worker_json: Value = serde_json::from_slice(&worker_output).unwrap();
    assert_eq!(worker_json["schema_version"], "forge.event_worker_loop.v1");
    assert_eq!(worker_json["status"], "event_worker_loop_stopped");
    assert_eq!(worker_json["cycle_count"], 0);
    assert_eq!(worker_json["stop_requested"], true);
    assert_eq!(worker_json["stopped_reason"], "stop_file_requested");
    assert_eq!(worker_json["stop_file"], stop_file.display().to_string());

    let mcp_input = serde_json::json!({
        "kind": "worker",
        "project_root": temp.path().display().to_string(),
        "max_cycles": 3,
        "interval_seconds": 0,
        "stop_file": stop_file.display().to_string(),
        "lease_owner": "mcp-stop-service-manager",
        "lease_seconds": 60,
        "heartbeat_seconds": 10
    })
    .to_string();
    let service_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.events.service_run",
            "--input",
            mcp_input.as_str(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let service_json: Value = serde_json::from_slice(&service_output).unwrap();
    assert_eq!(
        service_json["result"]["schema_version"],
        "forge.event_service_run.v1"
    );
    assert_eq!(
        service_json["result"]["status"],
        "event_service_run_stopped"
    );
    assert_eq!(service_json["result"]["service"]["status"], "stopped");
    assert_eq!(
        service_json["result"]["worker_report"]["status"],
        "event_worker_loop_stopped"
    );
    assert_eq!(
        service_json["result"]["worker_report"]["stop_file"],
        stop_file.display().to_string()
    );
    assert_eq!(service_json["result"]["health"]["stop_requested"], true);

    let webhook_port = reserve_local_port();
    let webhook_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "webhook-ingress",
            "--host",
            "127.0.0.1",
            "--port",
            &webhook_port.to_string(),
            "--path",
            "/stop-webhook",
            "--origin",
            "stop_webhook",
            "--action",
            "start_workflow",
            "--max-requests",
            "1",
            "--stop-file",
            stop_file.to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let webhook_json: Value = serde_json::from_slice(&webhook_output).unwrap();
    assert_eq!(
        webhook_json["schema_version"],
        "forge.event_webhook_ingress.v1"
    );
    assert_eq!(webhook_json["status"], "event_webhook_ingress_stopped");
    assert_eq!(webhook_json["request_count"], 0);
    assert_eq!(webhook_json["stop_requested"], true);
    assert_eq!(webhook_json["stopped_reason"], "stop_file_requested");
    assert_eq!(webhook_json["stop_file"], stop_file.display().to_string());

    let webhook_service_port = reserve_local_port();
    let webhook_mcp_input = serde_json::json!({
        "kind": "webhook_ingress",
        "project_root": temp.path().display().to_string(),
        "host": "127.0.0.1",
        "port": webhook_service_port,
        "path": "/stop-service",
        "origin": "stop_service_webhook",
        "action": "start_workflow",
        "max_requests": 1,
        "stop_file": stop_file.display().to_string(),
        "lease_owner": "mcp-stop-webhook-service-manager",
        "lease_seconds": 60,
        "heartbeat_seconds": 10
    })
    .to_string();
    let webhook_service_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.events.service_run",
            "--input",
            webhook_mcp_input.as_str(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let webhook_service_json: Value = serde_json::from_slice(&webhook_service_output).unwrap();
    assert_eq!(
        webhook_service_json["result"]["schema_version"],
        "forge.event_service_run.v1"
    );
    assert_eq!(
        webhook_service_json["result"]["status"],
        "event_service_run_stopped"
    );
    assert_eq!(
        webhook_service_json["result"]["service"]["status"],
        "stopped"
    );
    assert_eq!(
        webhook_service_json["result"]["webhook_report"]["status"],
        "event_webhook_ingress_stopped"
    );
    assert_eq!(
        webhook_service_json["result"]["webhook_report"]["stop_file"],
        stop_file.display().to_string()
    );
    assert_eq!(
        webhook_service_json["result"]["health"]["stop_requested"],
        true
    );

    let stopped_webhook_services_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "services",
            "--kind",
            "webhook_ingress",
            "--status",
            "stopped",
            "--limit",
            "10",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let stopped_webhook_services_json: Value =
        serde_json::from_slice(&stopped_webhook_services_output).unwrap();
    assert_eq!(stopped_webhook_services_json["service_count"], 1);
    assert_eq!(
        stopped_webhook_services_json["services"][0]["data"]["health"]["stop_requested"],
        true
    );
}

#[test]
fn event_service_run_executes_webhook_ingress_with_persistent_status_for_cli_and_mcp() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let addon_dir = temp.path().join(".forge/addons");
    fs::create_dir_all(&addon_dir).unwrap();
    fs::write(
        addon_dir.join("service-webhook.yaml"),
        r#"
id: forge.addon.service_webhook
name: Service Webhook Addon
version: 0.1.0
permissions:
  - id: service.ingress
    risk: medium
    tools: [webhook-server]
    resources: [service.workflow_requested]
    integrations: [service.webhook]
    actions: [start_workflow]
capabilities:
  - id: service_webhook_ingress
    title: Service webhook ingress
    domains: [operations]
    keywords: [service, webhook]
event_types:
  - id: service.workflow_requested.v1
    title: Service workflow request
    transport: webhook
event_adapters:
  - id: service.webhook_ingress
    title: Service Webhook Ingress
    transport: webhook
    direction: ingress
    origins: [service_webhook, mcp_service_webhook]
    actions: [start_workflow]
    event_types: [service.workflow_requested.v1]
    schema: service.workflow_requested.v1
    auth: none
    permissions: [service.ingress]
"#,
    )
    .unwrap();

    let cli_port = reserve_local_port();
    let cli_child = StdCommand::new(assert_cmd::cargo::cargo_bin("forge"))
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "service-run",
            "--kind",
            "webhook_ingress",
            "--host",
            "127.0.0.1",
            "--port",
            &cli_port.to_string(),
            "--path",
            "/service",
            "--origin",
            "service_webhook",
            "--action",
            "start_workflow",
            "--schema",
            "service.workflow_requested.v1",
            "--project-root",
            temp.path().to_str().unwrap(),
            "--route",
            "--max-requests",
            "1",
            "--lease-owner",
            "test-webhook-service-manager",
            "--lease-seconds",
            "60",
            "--heartbeat-seconds",
            "10",
            "--output",
            "json",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let cli_response = post_json_with_retry(
        cli_port,
        "/service",
        r#"{"data":{"goal":"Create a managed webhook service workflow","customer":"acme"}}"#,
    );
    assert!(cli_response.starts_with("HTTP/1.1 202 Accepted"));
    assert!(cli_response.contains("webhook_event_ingested_and_routed"));

    let cli_output = cli_child.wait_with_output().unwrap();
    assert!(
        cli_output.status.success(),
        "webhook service-run command failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&cli_output.stdout),
        String::from_utf8_lossy(&cli_output.stderr)
    );
    let cli_json: Value = serde_json::from_slice(&cli_output.stdout).unwrap();
    assert_eq!(cli_json["schema_version"], "forge.event_service_run.v1");
    assert_eq!(cli_json["service"]["service_kind"], "webhook_ingress");
    assert_eq!(cli_json["service"]["status"], "completed");
    assert!(cli_json.get("worker_report").is_none());
    assert_eq!(
        cli_json["webhook_report"]["schema_version"],
        "forge.event_webhook_ingress.v1"
    );
    assert_eq!(cli_json["webhook_report"]["request_count"], 1);
    assert_eq!(cli_json["webhook_report"]["ingested_count"], 1);
    assert_eq!(cli_json["webhook_report"]["routed_count"], 1);
    assert_eq!(cli_json["health"]["status"], "completed");
    assert_eq!(cli_json["health"]["request_count"], 1);
    assert!(
        cli_json["health"]["progress_heartbeat_count"]
            .as_u64()
            .unwrap()
            >= 2
    );
    assert!(cli_json["health"]["lease_renewal_count"].as_u64().unwrap() >= 3);
    assert_eq!(
        cli_json["service"]["data"]["webhook_report"]["request_count"],
        1
    );
    assert_eq!(
        cli_json["service"]["data"]["heartbeat"]["last_heartbeat_at"],
        cli_json["service"]["last_heartbeat_at"]
    );

    let mcp_port = reserve_local_port();
    let mcp_input = serde_json::json!({
        "kind": "webhook_ingress",
        "project_root": temp.path().display().to_string(),
        "host": "127.0.0.1",
        "port": mcp_port,
        "path": "/mcp-service",
        "origin": "mcp_service_webhook",
        "action": "start_workflow",
        "schema": "service.workflow_requested.v1",
        "route": true,
        "max_requests": 1,
        "lease_owner": "mcp-webhook-service-manager",
        "lease_seconds": 60,
        "heartbeat_seconds": 10
    })
    .to_string();
    let mcp_child = StdCommand::new(assert_cmd::cargo::cargo_bin("forge"))
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.events.service_run",
            "--input",
            mcp_input.as_str(),
            "--output",
            "json",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let mcp_response = post_json_with_retry(
        mcp_port,
        "/mcp-service",
        r#"{"data":{"goal":"Create a managed MCP webhook service workflow","customer":"beta"}}"#,
    );
    assert!(mcp_response.starts_with("HTTP/1.1 202 Accepted"));
    assert!(mcp_response.contains("webhook_event_ingested_and_routed"));

    let mcp_output = mcp_child.wait_with_output().unwrap();
    assert!(
        mcp_output.status.success(),
        "webhook service-run MCP command failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&mcp_output.stdout),
        String::from_utf8_lossy(&mcp_output.stderr)
    );
    let mcp_json: Value = serde_json::from_slice(&mcp_output.stdout).unwrap();
    assert_eq!(
        mcp_json["result"]["schema_version"],
        "forge.event_service_run.v1"
    );
    assert_eq!(
        mcp_json["result"]["service"]["service_kind"],
        "webhook_ingress"
    );
    assert_eq!(mcp_json["result"]["webhook_report"]["request_count"], 1);
    assert_eq!(mcp_json["result"]["webhook_report"]["routed_count"], 1);
    assert!(
        mcp_json["result"]["health"]["progress_heartbeat_count"]
            .as_u64()
            .unwrap()
            >= 2
    );

    let services_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "events",
            "services",
            "--kind",
            "webhook_ingress",
            "--limit",
            "10",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let services_json: Value = serde_json::from_slice(&services_output).unwrap();
    assert_eq!(services_json["schema_version"], "forge.event_services.v1");
    assert_eq!(services_json["service_count"], 2);
    assert!(services_json["services"]
        .as_array()
        .unwrap()
        .iter()
        .all(|service| service["service_kind"] == "webhook_ingress"
            && service["status"] == "completed"
            && service["data"]["webhook_report"]["request_count"] == 1));
}

#[test]
fn addon_runtime_dispatch_runs_safe_forge_core_builtin_contracts() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let addon_dir = temp.path().join("addons");
    fs::create_dir_all(&addon_dir).unwrap();
    fs::write(
        addon_dir.join("builtin.yaml"),
        r#"
id: forge.addon.builtin
name: Builtin Runtime Addon
version: 0.1.0
capabilities:
  - id: builtin_receipt
    title: Builtin receipt
    domains:
      - operations
    keywords:
      - receipt
runtime_contracts:
  - id: builtin.echo_executor
    title: Builtin Echo Executor
    contract_type: executor
    capability_id: builtin_receipt
    runtime: forge_core_builtin
    entrypoint: builtin:echo
    inputs:
      - payload
    outputs:
      - receipt
"#,
    )
    .unwrap();

    let dispatch_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "dispatch-contract",
            "--addon-dir",
            addon_dir.to_str().unwrap(),
            "--addon",
            "forge.addon.builtin",
            "--contract",
            "builtin.echo_executor",
            "--input",
            r#"{"payload":"ok"}"#,
            "--source",
            "test",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let dispatch_json: Value = serde_json::from_slice(&dispatch_output).unwrap();
    let dispatch_id = dispatch_json["dispatches"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let mcp_run_input = format!(
        r#"{{"addon_dirs":["{}"],"dispatch_id":"{}","worker":"mcp-test"}}"#,
        addon_dir.display(),
        dispatch_id
    );
    let run_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.addons.run_dispatch",
            "--input",
            &mcp_run_input,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let run_json: Value = serde_json::from_slice(&run_output).unwrap();
    assert_eq!(
        run_json["result"]["status"],
        "runtime_contract_dispatch_completed"
    );
    assert_eq!(run_json["result"]["completed_count"], 1);
    assert_eq!(run_json["result"]["dispatches"][0]["status"], "completed");
    assert_eq!(
        run_json["result"]["dispatches"][0]["data"]["runtime_processing"]["outcome"]["output"]
            ["kind"],
        "forge_core_builtin_echo"
    );
    assert_eq!(
        run_json["result"]["dispatches"][0]["data"]["runtime_processing"]["outcome"]["output"]
            ["input"]["payload"],
        "ok"
    );

    let completed_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "dispatches",
            "--status",
            "completed",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let completed_json: Value = serde_json::from_slice(&completed_output).unwrap();
    assert_eq!(completed_json["dispatch_count"], 1);
    assert_eq!(completed_json["completed_count"], 1);
}

#[test]
fn addon_validation_reports_missing_dependencies_without_planning() {
    let temp = tempdir().unwrap();
    let addon_dir = temp.path().join("addons");
    fs::create_dir_all(&addon_dir).unwrap();
    fs::write(
        addon_dir.join("broken.yaml"),
        r#"
id: forge.addon.broken
name: Broken Addon
version: 0.1.0
dependencies:
  - id: forge.addon.missing
    required: true
capabilities:
  - id: broken_capability
    title: Broken capability
    requires_capabilities:
      - missing_capability
runtime_contracts:
  - id: broken.validator
    title: Broken validator
    contract_type: validator
    capability_id: broken_capability
    runtime: external_api
    permissions:
      - broken.missing_permission
views:
  - id: broken.view
    title: Broken view
    surface: ops_console
    permissions:
      - broken.missing_permission
event_adapters:
  - id: broken.webhook
    title: Broken webhook
    transport: webhook
    direction: ingress
    permissions:
      - broken.missing_permission
compatibility:
  forge_version_req: ">=999.0.0"
  api_versions:
    - forge.addon_manifest.v999
  runtimes:
    - wasm
    - quantum
  features:
    - unknown_feature
  platforms:
    - plan9-mips
  migrations:
    - from_version: "1.0.0"
      to_version: "2.0.0"
      strategy: automatic
      requires_backup: true
"#,
    )
    .unwrap();
    fs::write(
        addon_dir.join("foundation.yaml"),
        r#"
id: forge.addon.foundation
name: Foundation Addon
version: 1.0.0
capabilities:
  - id: foundation_capability
    title: Foundation capability
"#,
    )
    .unwrap();
    fs::write(
        addon_dir.join("versioned.yaml"),
        r#"
id: forge.addon.versioned
name: Versioned Addon
version: 0.1.0
dependencies:
  - id: forge.addon.foundation
    version_req: ">=2.0.0"
    required: true
capabilities:
  - id: versioned_capability
    title: Versioned capability
"#,
    )
    .unwrap();

    let output = forge()
        .args([
            "addons",
            "validate",
            "--addon-dir",
            addon_dir.to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .failure()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["schema_version"], "forge.addon_validation.v1");
    assert_eq!(json["status"], "invalid");
    assert!(json["issues"]
        .as_array()
        .unwrap()
        .iter()
        .any(|issue| issue["code"] == "missing_required_addon_dependency"));
    assert!(json["issues"]
        .as_array()
        .unwrap()
        .iter()
        .any(|issue| issue["code"] == "missing_required_capability"));
    assert!(json["issues"]
        .as_array()
        .unwrap()
        .iter()
        .any(|issue| issue["code"] == "unsatisfied_addon_version_requirement"));
    assert!(json["issues"]
        .as_array()
        .unwrap()
        .iter()
        .any(|issue| issue["code"] == "undeclared_permission_reference"));
    for code in [
        "unsupported_forge_version_requirement",
        "unsupported_addon_api_version",
        "unsupported_addon_runtime",
        "unsupported_addon_feature",
        "unsupported_addon_platform",
        "runtime_contract_outside_declared_compatibility",
        "missing_addon_migration_rollback",
    ] {
        assert!(
            json["issues"]
                .as_array()
                .unwrap()
                .iter()
                .any(|issue| issue["code"] == code),
            "expected validation issue {code}"
        );
    }

    let mcp_input = format!(r#"{{"addon_dirs":["{}"]}}"#, addon_dir.display());
    let mcp_output = forge()
        .args([
            "mcp",
            "call",
            "forge.addons.validate",
            "--input",
            &mcp_input,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let mcp_json: Value = serde_json::from_slice(&mcp_output).unwrap();
    assert_eq!(mcp_json["result"]["status"], "invalid");
}

#[test]
fn addon_permissions_require_human_authorization_before_capability_exposure() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let manifest = temp.path().join("payments.yaml");
    fs::write(
        &manifest,
        r#"
id: forge.addon.payments
name: Payments Addon
version: 0.1.0
description: Payment workflow capability pack.
permissions:
  - id: payments.charge
    description: Charge customer payment methods.
    risk: high
    requires_human_approval: true
    tools:
      - appless_gateway
    resources:
      - customer_payment_method
      - invoice
    integrations:
      - appless.production
    actions:
      - charge
      - create_invoice
    tenant_scopes:
      - organization
      - product
capabilities:
  - id: payment_charge
    title: Payment charge
    description: Create and execute payment charge workflows.
    domains:
      - payments
    keywords:
      - cobrança
    workflow_extensions:
      - payment_charge_workflow
runtime_contracts:
  - id: payments.charge_executor
    title: Payments Charge Executor
    contract_type: executor
    capability_id: payment_charge
    workflow_extension_id: payment_charge_workflow
    runtime: external_api
    entrypoint: appless.production.charge
    inputs:
      - invoice
      - customer_payment_method
    outputs:
      - payment_receipt
    permissions:
      - payments.charge
"#,
    )
    .unwrap();

    let denied_stderr = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "install",
            "--manifest",
            manifest.to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .failure()
        .get_output()
        .stderr
        .clone();
    let denied_stderr = String::from_utf8(denied_stderr).unwrap();
    assert!(denied_stderr.contains("addon permission authorization required"));
    assert!(denied_stderr.contains("forge.addon.payments:payments.charge"));

    let denied_contracts = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "contracts",
            "--addon-dir",
            temp.path().to_str().unwrap(),
            "--addon",
            "forge.addon.payments",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let denied_contracts_json: Value = serde_json::from_slice(&denied_contracts).unwrap();
    assert_eq!(
        denied_contracts_json["contracts"][0]["permission_gate"]["status"],
        "missing_human_approval"
    );
    assert_eq!(
        denied_contracts_json["contracts"][0]["permission_gate"]["allowed"],
        false
    );
    let denied_contract_policy = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "contract-policy",
            "--addon-dir",
            temp.path().to_str().unwrap(),
            "--addon",
            "forge.addon.payments",
            "--contract",
            "payments.charge_executor",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let denied_contract_policy_json: Value =
        serde_json::from_slice(&denied_contract_policy).unwrap();
    assert_eq!(
        denied_contract_policy_json["schema_version"],
        "forge.addon_runtime_contract_policy.v1"
    );
    assert_eq!(
        denied_contract_policy_json["status"],
        "runtime_contract_policy_blocked"
    );
    assert_eq!(denied_contract_policy_json["blocked_count"], 1);
    assert_eq!(
        denied_contract_policy_json["contracts"][0]["status"],
        "missing_human_approval"
    );
    assert_eq!(
        denied_contract_policy_json["contracts"][0]["dispatch_allowed"],
        false
    );
    let denied_dispatch_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "dispatch-contract",
            "--addon-dir",
            temp.path().to_str().unwrap(),
            "--addon",
            "forge.addon.payments",
            "--contract",
            "payments.charge_executor",
            "--input",
            r#"{"invoice_id":"inv-001"}"#,
            "--source",
            "test",
            "--output",
            "json",
        ])
        .assert()
        .failure()
        .get_output()
        .stdout
        .clone();
    let denied_dispatch_json: Value = serde_json::from_slice(&denied_dispatch_output).unwrap();
    assert_eq!(
        denied_dispatch_json["status"],
        "runtime_contract_dispatch_blocked"
    );
    assert_eq!(denied_dispatch_json["blocked_count"], 1);
    assert_eq!(denied_dispatch_json["dispatches"][0]["status"], "blocked");

    let approve_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "authorize-permission",
            "--addon",
            "forge.addon.payments",
            "--permission",
            "payments.charge",
            "--risk",
            "high",
            "--approved-by",
            "arthur",
            "--source",
            "test",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let approve_json: Value = serde_json::from_slice(&approve_output).unwrap();
    assert_eq!(
        approve_json["schema_version"],
        "forge.addon_permission_authorizations.v1"
    );
    assert_eq!(approve_json["status"], "approved");
    assert_eq!(approve_json["authorization"]["approved_by"], "arthur");

    let approved_contracts = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "contracts",
            "--addon-dir",
            temp.path().to_str().unwrap(),
            "--addon",
            "forge.addon.payments",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let approved_contracts_json: Value = serde_json::from_slice(&approved_contracts).unwrap();
    assert_eq!(
        approved_contracts_json["contracts"][0]["permission_gate"]["status"],
        "allowed"
    );
    assert_eq!(
        approved_contracts_json["contracts"][0]["permission_gate"]["integrations"],
        serde_json::json!(["appless.production"])
    );
    assert_eq!(
        approved_contracts_json["contracts"][0]["permission_gate"]["resources"],
        serde_json::json!(["customer_payment_method", "invoice"])
    );
    let approved_contract_policy = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "contract-policy",
            "--addon-dir",
            temp.path().to_str().unwrap(),
            "--addon",
            "forge.addon.payments",
            "--contract",
            "payments.charge_executor",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let approved_contract_policy_json: Value =
        serde_json::from_slice(&approved_contract_policy).unwrap();
    assert_eq!(
        approved_contract_policy_json["status"],
        "runtime_contract_policy_ready"
    );
    assert_eq!(
        approved_contract_policy_json["contracts"][0]["dispatch_allowed"],
        true
    );

    let queued_payment_dispatch = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "dispatch-contract",
            "--addon-dir",
            temp.path().to_str().unwrap(),
            "--addon",
            "forge.addon.payments",
            "--contract",
            "payments.charge_executor",
            "--input",
            r#"{"invoice_id":"inv-queued"}"#,
            "--source",
            "test",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let queued_payment_json: Value = serde_json::from_slice(&queued_payment_dispatch).unwrap();
    let queued_payment_dispatch_id = queued_payment_json["dispatches"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "revoke-permission",
            "--addon",
            "forge.addon.payments",
            "--permission",
            "payments.charge",
            "--approved-by",
            "arthur",
            "--source",
            "test",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let revoked_run_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "run-dispatch",
            "--addon-dir",
            temp.path().to_str().unwrap(),
            "--dispatch",
            &queued_payment_dispatch_id,
            "--worker",
            "test-worker",
            "--output",
            "json",
        ])
        .assert()
        .failure()
        .get_output()
        .stdout
        .clone();
    let revoked_run_json: Value = serde_json::from_slice(&revoked_run_output).unwrap();
    assert_eq!(
        revoked_run_json["status"],
        "runtime_contract_dispatch_blocked"
    );
    assert_eq!(revoked_run_json["blocked_count"], 1);
    assert_eq!(revoked_run_json["dispatches"][0]["status"], "blocked");
    assert_eq!(
        revoked_run_json["dispatches"][0]["data"]["runtime_processing"]["outcome"]["outcome"],
        "policy_recheck_failed"
    );

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "authorize-permission",
            "--addon",
            "forge.addon.payments",
            "--permission",
            "payments.charge",
            "--risk",
            "high",
            "--approved-by",
            "arthur",
            "--source",
            "test-reapprove",
            "--output",
            "json",
        ])
        .assert()
        .success();

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "install",
            "--manifest",
            manifest.to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .success();

    let permissions_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.addons.permissions",
            "--input",
            r#"{"addon_id":"forge.addon.payments","status":"approved"}"#,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let permissions_json: Value = serde_json::from_slice(&permissions_output).unwrap();
    assert_eq!(
        permissions_json["result"]["schema_version"],
        "forge.addon_permission_authorizations.v1"
    );
    assert_eq!(permissions_json["result"]["authorization_count"], 1);

    let enabled_capabilities = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "capabilities",
            "--addon",
            "forge.addon.payments",
            "--lifecycle",
            "enabled",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let enabled_capabilities_json: Value = serde_json::from_slice(&enabled_capabilities).unwrap();
    assert_eq!(enabled_capabilities_json["capability_count"], 1);
    assert_eq!(enabled_capabilities_json["enabled_count"], 1);

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.addons.revoke_permission",
            "--input",
            r#"{"addon_id":"forge.addon.payments","permission_id":"payments.charge","approved_by":"arthur","source":"test"}"#,
            "--output",
            "json",
        ])
        .assert()
        .success();

    let unauthorized_capabilities = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "capabilities",
            "--addon",
            "forge.addon.payments",
            "--lifecycle",
            "unauthorized",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let unauthorized_capabilities_json: Value =
        serde_json::from_slice(&unauthorized_capabilities).unwrap();
    assert_eq!(unauthorized_capabilities_json["capability_count"], 1);
    assert_eq!(
        unauthorized_capabilities_json["capabilities"][0]["lifecycle"],
        "unauthorized"
    );

    let resolve_after_revoke = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "resolve",
            "--goal",
            "Criar cobrança para cliente",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let resolve_after_revoke_json: Value = serde_json::from_slice(&resolve_after_revoke).unwrap();
    assert!(!resolve_after_revoke_json["required_capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .any(|capability| capability["id"] == "payment_charge"));
}

#[test]
fn installed_addon_lifecycle_controls_planning_capabilities() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let manifest = temp.path().join("fiscal.yaml");
    let manifest_v2 = temp.path().join("fiscal-v2.yaml");
    fs::write(
        &manifest,
        r#"
id: forge.addon.fiscal
name: Fiscal Addon
version: 0.1.0
description: Fiscal workflow capability pack.
capabilities:
  - id: fiscal_audit
    title: Fiscal audit
    description: Build fiscal audit workflows.
    domains:
      - fiscal
    keywords:
      - auditoria fiscal
    workflow_extensions:
      - fiscal_audit_workflow
"#,
    )
    .unwrap();
    fs::write(
        &manifest_v2,
        r#"
id: forge.addon.fiscal
name: Fiscal Addon
version: 0.2.0
description: Fiscal workflow capability pack.
capabilities:
  - id: fiscal_audit
    title: Fiscal audit
    description: Build fiscal audit workflows.
    domains:
      - fiscal
    keywords:
      - auditoria fiscal
    workflow_extensions:
      - fiscal_audit_workflow
  - id: fiscal_invoice
    title: Fiscal invoice
    description: Build invoice issue workflows.
    domains:
      - fiscal
    keywords:
      - nota fiscal
    workflow_extensions:
      - fiscal_invoice_workflow
"#,
    )
    .unwrap();

    let package_path = temp.path().join("packages").join("fiscal.package.json");
    let package_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "package",
            "--manifest",
            manifest.to_str().unwrap(),
            "--repository",
            "https://example.com/forge/addons/fiscal.git",
            "--channel",
            "beta",
            "--signature",
            "sig-demo",
            "--public-key",
            "pub-demo",
            "--package-path",
            package_path.to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let package_json: Value = serde_json::from_slice(&package_output).unwrap();
    assert_eq!(package_json["schema_version"], "forge.addon_package.v1");
    assert_eq!(package_json["status"], "addon_package_ready");
    assert_eq!(package_json["package_id"], "forge.addon.fiscal@0.1.0");
    assert_eq!(package_json["manifest_sha256"].as_str().unwrap().len(), 64);
    assert_eq!(
        package_json["distribution"]["repository"],
        "https://example.com/forge/addons/fiscal.git"
    );
    assert_eq!(package_json["distribution"]["channel"], "beta");
    assert_eq!(package_json["signature"]["status"], "declared");
    assert_eq!(
        package_json["summary"]["capabilities"],
        serde_json::json!(["fiscal_audit"])
    );
    assert_eq!(
        package_json["written_package_path"],
        package_path.to_string_lossy().to_string()
    );
    assert!(package_path.exists());

    let package_mcp_input = format!(
        r#"{{"manifest":"{}","repository":"registry://forge/fiscal","channel":"stable"}}"#,
        manifest_v2.to_string_lossy().replace('\\', "\\\\")
    );
    let package_mcp_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.addons.package",
            "--input",
            &package_mcp_input,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let package_mcp_json: Value = serde_json::from_slice(&package_mcp_output).unwrap();
    assert_eq!(
        package_mcp_json["result"]["schema_version"],
        "forge.addon_package.v1"
    );
    assert_eq!(package_mcp_json["result"]["summary"]["capability_count"], 2);
    assert_eq!(
        package_mcp_json["result"]["signature"]["status"],
        "unsigned"
    );

    let install_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "install",
            "--manifest",
            manifest.to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let install_json: Value = serde_json::from_slice(&install_output).unwrap();
    assert_eq!(install_json["schema_version"], "forge.addon_lifecycle.v1");
    assert_eq!(install_json["status"], "installed");
    assert_eq!(install_json["addon"]["id"], "forge.addon.fiscal");

    let installed_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.addons.installed",
            "--input",
            "{}",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let installed_json: Value = serde_json::from_slice(&installed_output).unwrap();
    assert_eq!(
        installed_json["result"]["schema_version"],
        "forge.installed_addons.v1"
    );
    assert_eq!(installed_json["result"]["addon_count"], 1);

    let capabilities_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "capabilities",
            "--addon",
            "forge.addon.fiscal",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let capabilities_json: Value = serde_json::from_slice(&capabilities_output).unwrap();
    assert_eq!(
        capabilities_json["schema_version"],
        "forge.addon_capability_index.v1"
    );
    assert_eq!(capabilities_json["capability_count"], 1);
    assert_eq!(capabilities_json["enabled_count"], 1);
    assert_eq!(
        capabilities_json["capabilities"][0]["capability_id"],
        "fiscal_audit"
    );
    assert_eq!(capabilities_json["capabilities"][0]["lifecycle"], "enabled");

    let upgrade_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "upgrade",
            "--manifest",
            manifest_v2.to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let upgrade_json: Value = serde_json::from_slice(&upgrade_output).unwrap();
    assert_eq!(upgrade_json["schema_version"], "forge.addon_lifecycle.v1");
    assert_eq!(upgrade_json["status"], "upgraded");
    assert_eq!(upgrade_json["action"], "upgrade");
    assert_eq!(upgrade_json["addon"]["version"], "0.2.0");

    let upgraded_capabilities_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "capabilities",
            "--addon",
            "forge.addon.fiscal",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let upgraded_capabilities_json: Value =
        serde_json::from_slice(&upgraded_capabilities_output).unwrap();
    assert_eq!(upgraded_capabilities_json["capability_count"], 2);
    assert!(upgraded_capabilities_json["capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .all(|capability| capability["addon_version"] == "0.2.0"));
    assert!(upgraded_capabilities_json["capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .any(|capability| capability["capability_id"] == "fiscal_invoice"));

    let downgrade_input = format!(
        r#"{{"manifest":"{}"}}"#,
        manifest.to_string_lossy().replace('\\', "\\\\")
    );
    let downgrade_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.addons.downgrade",
            "--input",
            &downgrade_input,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let downgrade_json: Value = serde_json::from_slice(&downgrade_output).unwrap();
    assert_eq!(
        downgrade_json["result"]["schema_version"],
        "forge.addon_lifecycle.v1"
    );
    assert_eq!(downgrade_json["result"]["status"], "downgraded");
    assert_eq!(downgrade_json["result"]["action"], "downgrade");
    assert_eq!(downgrade_json["result"]["addon"]["version"], "0.1.0");

    let downgraded_capabilities_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "capabilities",
            "--addon",
            "forge.addon.fiscal",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let downgraded_capabilities_json: Value =
        serde_json::from_slice(&downgraded_capabilities_output).unwrap();
    assert_eq!(downgraded_capabilities_json["capability_count"], 1);
    assert_eq!(
        downgraded_capabilities_json["capabilities"][0]["addon_version"],
        "0.1.0"
    );

    let resolve_enabled = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "resolve",
            "--goal",
            "Criar auditoria fiscal para cliente",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let resolve_enabled_json: Value = serde_json::from_slice(&resolve_enabled).unwrap();
    assert!(resolve_enabled_json["required_capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .any(|capability| capability["id"] == "fiscal_audit"));

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "disable",
            "forge.addon.fiscal",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let resolve_disabled = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "resolve",
            "--goal",
            "Criar auditoria fiscal para cliente",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let resolve_disabled_json: Value = serde_json::from_slice(&resolve_disabled).unwrap();
    assert!(!resolve_disabled_json["required_capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .any(|capability| capability["id"] == "fiscal_audit"));

    let disabled_capabilities_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.addons.capabilities",
            "--input",
            r#"{"addon_id":"forge.addon.fiscal","lifecycle":"disabled"}"#,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let disabled_capabilities_json: Value =
        serde_json::from_slice(&disabled_capabilities_output).unwrap();
    assert_eq!(
        disabled_capabilities_json["result"]["schema_version"],
        "forge.addon_capability_index.v1"
    );
    assert_eq!(disabled_capabilities_json["result"]["capability_count"], 1);
    assert_eq!(disabled_capabilities_json["result"]["disabled_count"], 1);

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.addons.enable",
            "--input",
            r#"{"id":"forge.addon.fiscal"}"#,
            "--output",
            "json",
        ])
        .assert()
        .success();

    let resolve_reenabled = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "resolve",
            "--goal",
            "Criar auditoria fiscal para cliente",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let resolve_reenabled_json: Value = serde_json::from_slice(&resolve_reenabled).unwrap();
    assert!(resolve_reenabled_json["required_capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .any(|capability| capability["id"] == "fiscal_audit"));

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "uninstall",
            "forge.addon.fiscal",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let installed_empty = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "installed",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let installed_empty_json: Value = serde_json::from_slice(&installed_empty).unwrap();
    assert_eq!(installed_empty_json["addon_count"], 0);

    let capabilities_empty = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "capabilities",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let capabilities_empty_json: Value = serde_json::from_slice(&capabilities_empty).unwrap();
    assert_eq!(capabilities_empty_json["capability_count"], 0);
}

#[test]
fn addon_fetch_package_caches_and_indexes_trusted_package_sources() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let manifest = temp.path().join("fiscal.yaml");
    fs::write(
        &manifest,
        r#"
id: forge.addon.fiscal
name: Fiscal Addon
version: 1.2.0
description: Fiscal workflow capability pack.
capabilities:
  - id: fiscal_invoice
    title: Fiscal invoice
    description: Emit invoice workflows.
    domains:
      - fiscal
    keywords:
      - nota fiscal
"#,
    )
    .unwrap();

    let repository = "registry://forge/fiscal";
    let channel = "stable";
    let package_id = "forge.addon.fiscal@1.2.0";
    let signing_key = SigningKey::from_bytes(&[23u8; 32]);
    let public_key = test_hex_encode(signing_key.verifying_key().as_bytes());
    let payload = addon_package_payload_bytes(
        &manifest,
        package_id,
        "forge.addon.fiscal",
        "1.2.0",
        repository,
        channel,
    );
    let signature = test_hex_encode(&signing_key.sign(&payload).to_bytes());
    let package_path = temp.path().join("packages").join("fiscal.package.json");

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "package",
            "--manifest",
            manifest.to_str().unwrap(),
            "--repository",
            repository,
            "--channel",
            channel,
            "--signature",
            &signature,
            "--public-key",
            &public_key,
            "--package-path",
            package_path.to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .success();
    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "trust-key",
            "--repository",
            repository,
            "--channel",
            channel,
            "--public-key",
            &public_key,
            "--approved-by",
            "operator",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let expected_sha256 = hex_sha256(&fs::read(&package_path).unwrap());
    let cache_dir = temp.path().join("package-cache");
    let source = format!("file://{}", package_path.display());
    let fetch_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "fetch-package",
            "--source",
            &source,
            "--cache-dir",
            cache_dir.to_str().unwrap(),
            "--expected-sha256",
            &expected_sha256,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let fetch_json: Value = serde_json::from_slice(&fetch_output).unwrap();
    assert_eq!(fetch_json["schema_version"], "forge.addon_package_fetch.v1");
    assert_eq!(fetch_json["status"], "fetched");
    assert_eq!(fetch_json["source_kind"], "file_uri");
    assert_eq!(fetch_json["sha256"], expected_sha256);
    assert_eq!(
        fetch_json["marketplace"]["package"]["policy"]["status"],
        "install_allowed"
    );
    assert_eq!(
        fetch_json["marketplace"]["package"]["source"],
        cache_dir
            .join(format!("{expected_sha256}.package.json"))
            .to_str()
            .unwrap()
    );

    let mcp_input = serde_json::json!({
        "source": package_path,
        "cache_dir": temp.path().join("mcp-package-cache"),
        "expected_sha256": expected_sha256,
        "max_bytes": 10 * 1024 * 1024
    });
    let mcp_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.addons.fetch_package",
            "--input",
            &mcp_input.to_string(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let mcp_json: Value = serde_json::from_slice(&mcp_output).unwrap();
    assert_eq!(
        mcp_json["result"]["schema_version"],
        "forge.addon_package_fetch.v1"
    );
    assert_eq!(
        mcp_json["result"]["marketplace"]["package"]["package_id"],
        package_id
    );
}

#[test]
fn addon_sync_registry_fetches_indexed_packages_into_marketplace_cache() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let manifest = temp.path().join("notifications.yaml");
    fs::write(
        &manifest,
        r#"
id: forge.addon.notifications.email
name: Email Notification Addon
version: 0.3.0
capabilities:
  - id: email_notification
    title: Email notification
    domains:
      - notification
    keywords:
      - email
"#,
    )
    .unwrap();

    let repository = "registry://forge/notifications";
    let channel = "stable";
    let package_id = "forge.addon.notifications.email@0.3.0";
    let signing_key = SigningKey::from_bytes(&[29u8; 32]);
    let public_key = test_hex_encode(signing_key.verifying_key().as_bytes());
    let payload = addon_package_payload_bytes(
        &manifest,
        package_id,
        "forge.addon.notifications.email",
        "0.3.0",
        repository,
        channel,
    );
    let signature = test_hex_encode(&signing_key.sign(&payload).to_bytes());
    let package_path = temp
        .path()
        .join("packages")
        .join("notifications.package.json");

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "package",
            "--manifest",
            manifest.to_str().unwrap(),
            "--repository",
            repository,
            "--channel",
            channel,
            "--signature",
            &signature,
            "--public-key",
            &public_key,
            "--package-path",
            package_path.to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .success();
    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "trust-key",
            "--repository",
            repository,
            "--channel",
            channel,
            "--public-key",
            &public_key,
            "--approved-by",
            "operator",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let expected_sha256 = hex_sha256(&fs::read(&package_path).unwrap());
    let registry_index = temp.path().join("registry-index.json");
    fs::write(
        &registry_index,
        serde_json::to_vec_pretty(&serde_json::json!({
            "schema_version": "forge.addon_registry_index.v1",
            "packages": [
                {
                    "source": format!("file://{}", package_path.display()),
                    "expected_sha256": expected_sha256.clone(),
                    "max_bytes": 10 * 1024 * 1024
                }
            ]
        }))
        .unwrap(),
    )
    .unwrap();
    let cache_dir = temp.path().join("registry-cache");
    let sync_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "sync-registry",
            "--source",
            registry_index.to_str().unwrap(),
            "--cache-dir",
            cache_dir.to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let sync_json: Value = serde_json::from_slice(&sync_output).unwrap();
    assert_eq!(sync_json["schema_version"], "forge.addon_registry_sync.v1");
    assert_eq!(sync_json["status"], "synced");
    assert_eq!(sync_json["package_count"], 1);
    assert_eq!(sync_json["fetched_count"], 1);
    assert_eq!(sync_json["blocked_count"], 0);
    assert_eq!(
        sync_json["fetches"][0]["marketplace"]["package"]["package_id"],
        package_id
    );

    let mcp_input = serde_json::json!({
        "source": registry_index,
        "cache_dir": temp.path().join("registry-mcp-cache"),
        "max_packages": 10
    });
    let mcp_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.addons.sync_registry",
            "--input",
            &mcp_input.to_string(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let mcp_json: Value = serde_json::from_slice(&mcp_output).unwrap();
    assert_eq!(
        mcp_json["result"]["schema_version"],
        "forge.addon_registry_sync.v1"
    );
    assert_eq!(mcp_json["result"]["status"], "synced");
    assert_eq!(
        mcp_json["result"]["fetches"][0]["marketplace"]["package"]["package_id"],
        package_id
    );

    let lock_path = temp.path().join("package-lock.json");
    let lock_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "package-lock",
            "--repository",
            repository,
            "--write",
            lock_path.to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let lock_json: Value = serde_json::from_slice(&lock_output).unwrap();
    assert_eq!(lock_json["schema_version"], "forge.addon_package_lock.v1");
    assert_eq!(lock_json["package_count"], 1);
    assert_eq!(lock_json["packages"][0]["package_id"], package_id);
    assert_eq!(lock_json["packages"][0]["package_sha256"], expected_sha256);
    assert_eq!(lock_json["written_lock_path"], lock_path.to_str().unwrap());
    assert_eq!(lock_json["written_lock_sha256"].as_str().unwrap().len(), 64);

    let locked_cache_dir = temp.path().join("registry-locked-cache");
    let locked_sync_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "sync-registry",
            "--source",
            registry_index.to_str().unwrap(),
            "--cache-dir",
            locked_cache_dir.to_str().unwrap(),
            "--lock",
            lock_path.to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let locked_sync_json: Value = serde_json::from_slice(&locked_sync_output).unwrap();
    assert_eq!(locked_sync_json["status"], "synced");
    assert_eq!(
        locked_sync_json["fetches"][0]["lock"]["schema_version"],
        "forge.addon_package_lock_enforcement.v1"
    );
    assert_eq!(locked_sync_json["fetches"][0]["lock"]["status"], "matched");

    let mcp_lock_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.addons.package_lock",
            "--input",
            &serde_json::json!({"repository": repository}).to_string(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let mcp_lock_json: Value = serde_json::from_slice(&mcp_lock_output).unwrap();
    assert_eq!(
        mcp_lock_json["result"]["schema_version"],
        "forge.addon_package_lock.v1"
    );
    assert_eq!(
        mcp_lock_json["result"]["packages"][0]["package_id"],
        package_id
    );

    let locked_mcp_input = serde_json::json!({
        "source": registry_index,
        "cache_dir": temp.path().join("registry-locked-mcp-cache"),
        "lock_path": lock_path,
        "max_packages": 10
    });
    let locked_mcp_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.addons.sync_registry",
            "--input",
            &locked_mcp_input.to_string(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let locked_mcp_json: Value = serde_json::from_slice(&locked_mcp_output).unwrap();
    assert_eq!(
        locked_mcp_json["result"]["fetches"][0]["lock"]["schema_version"],
        "forge.addon_package_lock_enforcement.v1"
    );
}

#[test]
fn addon_marketplace_installs_only_trusted_signed_packages() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let manifest = temp.path().join("payments.yaml");
    fs::write(
        &manifest,
        r#"
id: forge.addon.payments
name: Payments Addon
version: 1.0.0
description: Payment workflow capability pack.
capabilities:
  - id: payment_checkout
    title: Payment checkout
    description: Build checkout workflows.
    domains:
      - payments
    keywords:
      - pagamento
      - checkout
    workflow_extensions:
      - payment_checkout_workflow
"#,
    )
    .unwrap();

    let repository = "registry://forge/payments";
    let channel = "stable";
    let package_id = "forge.addon.payments@1.0.0";
    let signing_key = SigningKey::from_bytes(&[13u8; 32]);
    let public_key = test_hex_encode(signing_key.verifying_key().as_bytes());
    let payload = addon_package_payload_bytes(
        &manifest,
        package_id,
        "forge.addon.payments",
        "1.0.0",
        repository,
        channel,
    );
    let signature = test_hex_encode(&signing_key.sign(&payload).to_bytes());
    let package_path = temp.path().join("packages").join("payments.package.json");

    let package_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "package",
            "--manifest",
            manifest.to_str().unwrap(),
            "--repository",
            repository,
            "--channel",
            channel,
            "--signature",
            &signature,
            "--public-key",
            &public_key,
            "--package-path",
            package_path.to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let package_json: Value = serde_json::from_slice(&package_output).unwrap();
    assert_eq!(package_json["signature"]["status"], "declared");
    assert_eq!(
        package_json["manifest_canonical_sha256"]
            .as_str()
            .unwrap()
            .len(),
        64
    );

    let publish_blocked = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "publish-package",
            "--package",
            package_path.to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let publish_blocked_json: Value = serde_json::from_slice(&publish_blocked).unwrap();
    assert_eq!(
        publish_blocked_json["schema_version"],
        "forge.addon_marketplace.v1"
    );
    assert_eq!(publish_blocked_json["package"]["status"], "blocked");
    assert_eq!(
        publish_blocked_json["package"]["policy"]["signature"]["verification_status"],
        "verified"
    );
    assert!(publish_blocked_json["package"]["policy"]["issues"]
        .as_array()
        .unwrap()
        .iter()
        .any(|issue| issue == "public_key_not_trusted_for_repository_channel"));

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "install-package",
            "--package",
            package_path.to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .failure();

    let trust_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "trust-key",
            "--repository",
            repository,
            "--channel",
            channel,
            "--public-key",
            &public_key,
            "--approved-by",
            "operator",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let trust_json: Value = serde_json::from_slice(&trust_output).unwrap();
    assert_eq!(trust_json["schema_version"], "forge.addon_trust_store.v1");
    assert_eq!(trust_json["key"]["status"], "trusted");
    assert_eq!(trust_json["key"]["public_key"], public_key);

    let publish_allowed = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "publish-package",
            "--package",
            package_path.to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let publish_allowed_json: Value = serde_json::from_slice(&publish_allowed).unwrap();
    assert_eq!(publish_allowed_json["package"]["status"], "installable");
    assert_eq!(
        publish_allowed_json["package"]["policy"]["status"],
        "install_allowed"
    );
    assert_eq!(
        publish_allowed_json["package"]["policy"]["trusted_key_count"],
        1
    );

    let marketplace_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.addons.marketplace",
            "--input",
            r#"{"repository":"registry://forge/payments","channel":"stable"}"#,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let marketplace_json: Value = serde_json::from_slice(&marketplace_output).unwrap();
    assert_eq!(
        marketplace_json["result"]["schema_version"],
        "forge.addon_marketplace.v1"
    );
    assert_eq!(marketplace_json["result"]["installable_count"], 1);

    let lock_path = temp.path().join("payments-package-lock.json");
    let lock_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "package-lock",
            "--repository",
            repository,
            "--channel",
            channel,
            "--write",
            lock_path.to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let lock_json: Value = serde_json::from_slice(&lock_output).unwrap();
    assert_eq!(lock_json["schema_version"], "forge.addon_package_lock.v1");
    assert_eq!(lock_json["packages"][0]["package_id"], package_id);

    let bad_lock_path = temp.path().join("payments-bad-package-lock.json");
    let mut bad_lock_json = lock_json.clone();
    bad_lock_json["written_lock_path"] = Value::Null;
    bad_lock_json["written_lock_sha256"] = Value::Null;
    bad_lock_json["packages"][0]["package_sha256"] = Value::String("0".repeat(64));
    fs::write(
        &bad_lock_path,
        serde_json::to_vec_pretty(&bad_lock_json).unwrap(),
    )
    .unwrap();
    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "install-package",
            "--package",
            package_path.to_str().unwrap(),
            "--lock",
            bad_lock_path.to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "addon package lock blocked install",
        ));

    let install_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.addons.install_package",
            "--input",
            &format!(
                r#"{{"package":"{}","lock":"{}"}}"#,
                package_path.to_string_lossy().replace('\\', "\\\\"),
                lock_path.to_string_lossy().replace('\\', "\\\\")
            ),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let install_json: Value = serde_json::from_slice(&install_output).unwrap();
    assert_eq!(
        install_json["result"]["schema_version"],
        "forge.addon_package_install.v1"
    );
    assert_eq!(install_json["result"]["status"], "installed");
    assert_eq!(
        install_json["result"]["lock"]["schema_version"],
        "forge.addon_package_lock_enforcement.v1"
    );
    assert_eq!(install_json["result"]["lock"]["status"], "matched");
    assert_eq!(
        install_json["result"]["package"]["policy"]["signature"]["verification_status"],
        "verified"
    );
    assert_eq!(
        install_json["result"]["lifecycle"]["addon"]["source"]
            .as_str()
            .unwrap()
            .starts_with(
                "marketplace:registry://forge/payments:stable:forge.addon.payments@1.0.0#"
            ),
        true
    );

    let capabilities_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "capabilities",
            "--addon",
            "forge.addon.payments",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let capabilities_json: Value = serde_json::from_slice(&capabilities_output).unwrap();
    assert_eq!(capabilities_json["capability_count"], 1);
    assert_eq!(
        capabilities_json["capabilities"][0]["capability_id"],
        "payment_checkout"
    );
}

#[test]
fn addon_major_upgrade_requires_migration_plan_with_rollback() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let manifest_v1 = temp.path().join("analytics-v1.yaml");
    let manifest_v2 = temp.path().join("analytics-v2.yaml");
    let manifest_v2_migrated = temp.path().join("analytics-v2-migrated.yaml");
    fs::write(
        &manifest_v1,
        r#"
id: forge.addon.analytics
name: Analytics Addon
version: 1.0.0
capabilities:
  - id: analytics_report
    title: Analytics report
"#,
    )
    .unwrap();
    fs::write(
        &manifest_v2,
        r#"
id: forge.addon.analytics
name: Analytics Addon
version: 2.0.0
capabilities:
  - id: analytics_report
    title: Analytics report
  - id: analytics_forecast
    title: Analytics forecast
"#,
    )
    .unwrap();
    fs::write(
        &manifest_v2_migrated,
        r#"
id: forge.addon.analytics
name: Analytics Addon
version: 2.0.0
capabilities:
  - id: analytics_report
    title: Analytics report
  - id: analytics_forecast
    title: Analytics forecast
compatibility:
  migrations:
    - from_version: "1.0.0"
      to_version: "2.0.0"
      strategy: assisted_sqlite_projection_rebuild
      data_migration: capability_index_rebuild
      rollback: restore_previous_manifest_and_capability_index
      requires_backup: true
"#,
    )
    .unwrap();

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "install",
            "--manifest",
            manifest_v1.to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .success();

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "upgrade",
            "--manifest",
            manifest_v2.to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .failure();

    let upgrade_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "upgrade",
            "--manifest",
            manifest_v2_migrated.to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let upgrade_json: Value = serde_json::from_slice(&upgrade_output).unwrap();
    assert_eq!(upgrade_json["status"], "upgraded");
    assert_eq!(upgrade_json["addon"]["version"], "2.0.0");
    assert_eq!(
        upgrade_json["migration_workflow"]["schema_version"],
        "forge.addon_migration_workflow.v1"
    );
    assert_eq!(
        upgrade_json["migration_workflow"]["status"],
        "addon_migration_workflow_created"
    );
    assert_eq!(
        upgrade_json["migration_workflow"]["migration_strategy"],
        "assisted_sqlite_projection_rebuild"
    );
    assert_eq!(upgrade_json["migration_workflow"]["task_count"], 5);
    assert_eq!(
        upgrade_json["migration_workflow"]["tasks"][3]["title"],
        "Prepare Addon rollback path"
    );
    let migration_workflow_id = upgrade_json["migration_workflow"]["workflow_id"]
        .as_str()
        .unwrap()
        .to_string();

    let workflow_list_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "list",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let workflow_list_json: Value = serde_json::from_slice(&workflow_list_output).unwrap();
    assert!(workflow_list_json["workflows"]
        .as_array()
        .unwrap()
        .iter()
        .any(|workflow| workflow["workflow_id"] == migration_workflow_id
            && workflow["runtime"]["lifecycle_kind"] == "persistent_workflow"));

    let package_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "addons",
            "package",
            "--manifest",
            manifest_v2_migrated.to_str().unwrap(),
            "--repository",
            "registry://forge/analytics",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let package_json: Value = serde_json::from_slice(&package_output).unwrap();
    assert_eq!(
        package_json["summary"]["compatibility"]["migration_count"],
        1
    );

    let explicit_workflow_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.addons.migration_workflow",
            "--input",
            &format!(
                r#"{{"from_manifest":"{}","to_manifest":"{}","action":"upgrade"}}"#,
                manifest_v1.to_string_lossy().replace('\\', "\\\\"),
                manifest_v2_migrated.to_string_lossy().replace('\\', "\\\\")
            ),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let explicit_workflow_json: Value = serde_json::from_slice(&explicit_workflow_output).unwrap();
    assert_eq!(
        explicit_workflow_json["result"]["schema_version"],
        "forge.addon_migration_workflow.v1"
    );
    assert_eq!(explicit_workflow_json["result"]["task_count"], 5);
}

#[test]
fn plan_records_capability_context_and_event_policy_in_intent() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Criar whiteboard colaborativo com sistema de design e workflows dinâmicos",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_slice(&output).unwrap();
    let intent = &json["intent"];
    assert_eq!(intent["schema_version"], "forge.intent.v2");
    assert_eq!(
        intent["capability_resolution"]["planning_strategy"],
        "capability_first_addon_registry"
    );
    assert_eq!(intent["capability_resolution"]["status"], "resolved");
    assert!(intent["required_capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .any(|capability| capability["id"] == "visual_workspace"
            && capability["source_addon"] == "forge.addon.visual_workspace"));
    assert!(intent["capability_resolution"]["runtime_contracts"]
        .as_array()
        .unwrap()
        .iter()
        .any(
            |contract| contract["id"] == "creative_workspace.planning_strategy"
                && contract["contract_type"] == "planning_strategy"
                && contract["source_addon"] == "forge.addon.visual_workspace"
        ));
    assert_eq!(
        intent["operating_context"]["organization"]["scope"],
        "organization"
    );
    assert!(intent["event_policy"]["allowed_actions"]
        .as_array()
        .unwrap()
        .contains(&Value::String("modify_workflow".to_string())));
    assert_eq!(intent["workflow_mode"]["kind"], "ephemeral_workflow");
}
