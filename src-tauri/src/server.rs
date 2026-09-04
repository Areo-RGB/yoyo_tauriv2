use local_ip_address::list_afinet_netifas;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tiny_http::{Header, Response, Server};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RemoteAction {
    pub id: String,
    pub action: String,
    pub athlete_id: Option<String>,
    pub timestamp: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PeerSyncPayload {
    pub state: Option<String>,
    pub actions: Option<Vec<String>>,
}

pub struct ServerControl {
    pub running: bool,
    pub port: u16,
    pub latest_state: String,
    pub pending_actions: Vec<String>,
    pub known_peers: HashSet<String>,
}

impl Default for ServerControl {
    fn default() -> Self {
        Self {
            running: false,
            port: 8080,
            latest_state: r#"{"status":"idle","athletes":[],"level":1,"shuttle":1,"distance":0}"#
                .to_string(),
            pending_actions: Vec::new(),
            known_peers: HashSet::new(),
        }
    }
}

pub type SharedServerState = Arc<Mutex<ServerControl>>;

pub fn create_shared_state() -> SharedServerState {
    Arc::new(Mutex::new(ServerControl::default()))
}

pub fn get_local_ips() -> Vec<String> {
    let mut ips = Vec::new();
    if let Ok(netifs) = list_afinet_netifas() {
        for (_name, ip) in netifs {
            if let IpAddr::V4(ipv4) = ip {
                if !ipv4.is_loopback() {
                    ips.push(ipv4.to_string());
                }
            }
        }
    }
    if ips.is_empty() {
        ips.push("127.0.0.1".to_string());
    }
    ips
}

fn http_get_raw(addr: &str, path: &str, timeout_ms: u64) -> Result<String, String> {
    let socket_addr: SocketAddr = addr.parse().map_err(|e| format!("{}", e))?;
    let mut stream = TcpStream::connect_timeout(&socket_addr, Duration::from_millis(timeout_ms))
        .map_err(|e| format!("{}", e))?;
    stream
        .set_read_timeout(Some(Duration::from_millis(timeout_ms)))
        .ok();
    stream
        .set_write_timeout(Some(Duration::from_millis(timeout_ms)))
        .ok();

    let request = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        path, addr
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|e| format!("{}", e))?;

    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|e| format!("{}", e))?;

    if let Some(body_start) = response.find("\r\n\r\n") {
        Ok(response[body_start + 4..].to_string())
    } else {
        Ok(response)
    }
}

fn http_post_raw(addr: &str, path: &str, body: &str, timeout_ms: u64) -> Result<String, String> {
    let socket_addr: SocketAddr = addr.parse().map_err(|e| format!("{}", e))?;
    let mut stream = TcpStream::connect_timeout(&socket_addr, Duration::from_millis(timeout_ms))
        .map_err(|e| format!("{}", e))?;
    stream
        .set_read_timeout(Some(Duration::from_millis(timeout_ms)))
        .ok();
    stream
        .set_write_timeout(Some(Duration::from_millis(timeout_ms)))
        .ok();

    let request = format!(
        "POST {} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        path,
        addr,
        body.len(),
        body
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|e| format!("{}", e))?;

    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|e| format!("{}", e))?;

    if let Some(body_start) = response.find("\r\n\r\n") {
        Ok(response[body_start + 4..].to_string())
    } else {
        Ok(response)
    }
}

pub fn start_http_server(state: SharedServerState, port: u16) -> Result<String, String> {
    {
        let mut lock = state.lock().map_err(|e| e.to_string())?;
        if lock.running {
            let primary_ip = get_local_ips().first().cloned().unwrap_or_default();
            return Ok(format!("http://{}:{}", primary_ip, lock.port));
        }
        lock.running = true;
        lock.port = port;
    }

    let addr = format!("0.0.0.0:{}", port);
    let server =
        Server::http(&addr).map_err(|e| format!("Failed to bind server to {}: {}", addr, e))?;

    let state_clone = Arc::clone(&state);

    // 1. HTTP Server Listening Thread
    thread::spawn(move || {
        for mut request in server.incoming_requests() {
            let state = Arc::clone(&state_clone);
            let url = request.url().to_string();
            let method = request.method().to_string();

            // Track remote client peer if non-loopback
            if let Some(remote) = request.remote_addr() {
                let remote_ip = remote.ip().to_string();
                if remote_ip != "127.0.0.1" {
                    let peer_addr = format!("{}:{}", remote_ip, port);
                    if let Ok(mut lock) = state.lock() {
                        lock.known_peers.insert(peer_addr);
                    }
                }
            }

            if method == "OPTIONS" {
                let response = Response::empty(200)
                    .with_header(
                        Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap(),
                    )
                    .with_header(
                        Header::from_bytes(
                            &b"Access-Control-Allow-Methods"[..],
                            &b"GET, POST, OPTIONS"[..],
                        )
                        .unwrap(),
                    )
                    .with_header(
                        Header::from_bytes(
                            &b"Access-Control-Allow-Headers"[..],
                            &b"Content-Type"[..],
                        )
                        .unwrap(),
                    );
                let _ = request.respond(response);
                continue;
            }

            if url == "/api/ip" {
                let ips = get_local_ips();
                let json = serde_json::to_string(&ips).unwrap_or_default();
                let response = Response::from_string(json)
                    .with_header(
                        Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
                    )
                    .with_header(
                        Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap(),
                    );
                let _ = request.respond(response);
                continue;
            }

            if url == "/api/state" {
                let current_json = {
                    let lock = state.lock().unwrap();
                    lock.latest_state.clone()
                };
                let response = Response::from_string(current_json)
                    .with_header(
                        Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
                    )
                    .with_header(
                        Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap(),
                    );
                let _ = request.respond(response);
                continue;
            }

            if url == "/api/action" && method == "POST" {
                let mut content = String::new();
                let _ = request.as_reader().read_to_string(&mut content);
                if !content.is_empty() {
                    if let Ok(mut lock) = state.lock() {
                        lock.pending_actions.push(content);
                    }
                }
                let response = Response::from_string(r#"{"status":"ok"}"#)
                    .with_header(
                        Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
                    )
                    .with_header(
                        Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap(),
                    );
                let _ = request.respond(response);
                continue;
            }

            if url == "/api/sync_peer" && method == "POST" {
                let mut content = String::new();
                let _ = request.as_reader().read_to_string(&mut content);
                let mut resp_state = String::new();
                let mut resp_actions = Vec::new();

                if let Ok(payload) = serde_json::from_str::<PeerSyncPayload>(&content) {
                    if let Ok(mut lock) = state.lock() {
                        if let Some(incoming_state) = payload.state {
                            if !incoming_state.is_empty() {
                                // If peer is running or local state is idle, accept peer state
                                let peer_is_running = incoming_state.contains(r#""status":"running""#)
                                    || incoming_state.contains(r#""status":"starting""#);
                                let local_is_idle = lock.latest_state.contains(r#""status":"idle""#);

                                if peer_is_running || local_is_idle {
                                    lock.latest_state = incoming_state;
                                }
                            }
                        }
                        if let Some(incoming_actions) = payload.actions {
                            for act in incoming_actions {
                                lock.pending_actions.push(act);
                            }
                        }
                        resp_state = lock.latest_state.clone();
                        resp_actions = lock.pending_actions.clone();
                    }
                }

                let response_payload = serde_json::json!({
                    "status": "ok",
                    "state": resp_state,
                    "actions": resp_actions
                });

                let response = Response::from_string(response_payload.to_string())
                    .with_header(
                        Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
                    )
                    .with_header(
                        Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap(),
                    );
                let _ = request.respond(response);
                continue;
            }

            if url.starts_with("/api/events") {
                let current_json = {
                    let lock = state.lock().unwrap();
                    lock.latest_state.clone()
                };
                let sse_payload = format!("data: {}\n\n", current_json);
                let response = Response::from_string(sse_payload)
                    .with_header(
                        Header::from_bytes(&b"Content-Type"[..], &b"text/event-stream"[..])
                            .unwrap(),
                    )
                    .with_header(
                        Header::from_bytes(&b"Cache-Control"[..], &b"no-cache"[..]).unwrap(),
                    )
                    .with_header(
                        Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap(),
                    );
                let _ = request.respond(response);
                continue;
            }

            // Default route: Mobile Web Dashboard for browser clients
            let html = get_web_client_html();
            let response = Response::from_string(html)
                .with_header(
                    Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..])
                        .unwrap(),
                )
                .with_header(
                    Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap(),
                );
            let _ = request.respond(response);
        }
    });

    // 2. Background Auto-Discovery & Auto-Peer-Sync Thread
    let state_discovery = Arc::clone(&state);
    thread::spawn(move || {
        let mut scan_counter = 0;

        loop {
            thread::sleep(Duration::from_millis(500));
            scan_counter += 1;

            let local_ips = get_local_ips();

            // Periodic Subnet Scan every ~3s (6 * 500ms)
            if scan_counter % 6 == 0 {
                for local_ip in &local_ips {
                    if local_ip == "127.0.0.1" {
                        continue;
                    }

                    let parts: Vec<&str> = local_ip.split('.').collect();
                    if parts.len() == 4 {
                        let prefix = format!("{}.{}.{}", parts[0], parts[1], parts[2]);

                        // Check primary gateways and common peer IP range
                        let mut candidates = vec![
                            format!("{}.1:{}", prefix, port),
                            format!("{}.100:{}", prefix, port),
                        ];

                        // Sweep first 30 host addresses
                        for i in 1..=30 {
                            candidates.push(format!("{}.{}:{}", prefix, i, port));
                        }

                        for candidate in candidates {
                            if local_ips.iter().any(|ip| candidate.starts_with(ip)) {
                                continue;
                            }

                            if let Ok(res) = http_get_raw(&candidate, "/api/ip", 120) {
                                if res.contains('[') && res.contains(']') {
                                    if let Ok(mut lock) = state_discovery.lock() {
                                        lock.known_peers.insert(candidate);
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Continuous Peer State Sync every 500ms
            let (peers, cur_state, cur_actions) = {
                if let Ok(lock) = state_discovery.lock() {
                    (
                        lock.known_peers.iter().cloned().collect::<Vec<_>>(),
                        lock.latest_state.clone(),
                        lock.pending_actions.clone(),
                    )
                } else {
                    (Vec::new(), String::new(), Vec::new())
                }
            };

            if peers.is_empty() {
                continue;
            }

            let sync_req = serde_json::json!({
                "state": cur_state,
                "actions": cur_actions
            })
            .to_string();

            for peer in peers {
                if let Ok(res) = http_post_raw(&peer, "/api/sync_peer", &sync_req, 250) {
                    if let Ok(val) = serde_json::from_str::<Value>(&res) {
                        if let Some(peer_state) = val.get("state").and_then(|s| s.as_str()) {
                            if !peer_state.is_empty() {
                                let peer_is_running = peer_state.contains(r#""status":"running""#)
                                    || peer_state.contains(r#""status":"starting""#);

                                if let Ok(mut lock) = state_discovery.lock() {
                                    let local_is_idle = lock.latest_state.contains(r#""status":"idle""#);
                                    if peer_is_running || local_is_idle {
                                        lock.latest_state = peer_state.to_string();
                                    }
                                }
                            }
                        }

                        if let Some(peer_actions) = val.get("actions").and_then(|a| a.as_array()) {
                            if let Ok(mut lock) = state_discovery.lock() {
                                for act_val in peer_actions {
                                    if let Some(act_str) = act_val.as_str() {
                                        if !lock.pending_actions.contains(&act_str.to_string()) {
                                            lock.pending_actions.push(act_str.to_string());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    });

    let primary_ip = get_local_ips()
        .first()
        .cloned()
        .unwrap_or_else(|| "127.0.0.1".to_string());
    Ok(format!("http://{}:{}", primary_ip, port))
}

fn get_web_client_html() -> &'static str {
    r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Yo-Yo Sync Remote</title>
  <style>
    :root {
      --bg: #161616;
      --card-bg: #262626;
      --text: #f4f4f4;
      --primary: #0f62fe;
      --success: #24a148;
      --warning: #f1c21b;
      --danger: #da1e28;
    }
    body {
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
      background: var(--bg);
      color: var(--text);
      margin: 0;
      padding: 1rem;
    }
    .header {
      display: flex;
      justify-content: space-between;
      align-items: center;
      padding-bottom: 1rem;
      border-bottom: 1px solid #393939;
    }
    .title { font-size: 1.2rem; font-weight: bold; }
    .badge {
      padding: 0.25rem 0.5rem;
      border-radius: 4px;
      font-size: 0.8rem;
      text-transform: uppercase;
      font-weight: bold;
    }
    .badge.running { background: var(--success); color: #fff; }
    .badge.idle { background: #525252; color: #fff; }
    .metrics {
      display: grid;
      grid-template-columns: repeat(3, 1fr);
      gap: 0.5rem;
      margin: 1rem 0;
    }
    .metric-card {
      background: var(--card-bg);
      padding: 0.75rem;
      border-radius: 8px;
      text-align: center;
    }
    .metric-card small { display: block; opacity: 0.7; font-size: 0.75rem; }
    .metric-card strong { display: block; font-size: 1.5rem; margin-top: 0.25rem; }
    .athlete-grid {
      display: grid;
      grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
      gap: 0.75rem;
    }
    .athlete-card {
      background: var(--card-bg);
      padding: 0.75rem;
      border-radius: 8px;
      display: flex;
      flex-direction: column;
      justify-content: space-between;
      border-left: 4px solid transparent;
    }
    .athlete-card.status-running { border-left-color: var(--success); }
    .athlete-card.status-warned { border-left-color: var(--danger); }
    .athlete-card.status-eliminated { border-left-color: #525252; opacity: 0.6; }
    .athlete-name { font-weight: bold; font-size: 1rem; }
    .athlete-status { font-size: 0.75rem; opacity: 0.8; margin-bottom: 0.5rem; }
    .actions { display: flex; gap: 0.35rem; }
    button {
      flex: 1;
      padding: 0.5rem 0.25rem;
      border: none;
      border-radius: 4px;
      font-weight: bold;
      cursor: pointer;
      font-size: 0.8rem;
    }
    button.btn-warn { background: var(--warning); color: #000; }
    button.btn-out { background: var(--danger); color: #fff; }
    button:disabled { opacity: 0.3; cursor: not-allowed; }
  </style>
</head>
<body>
  <div class="header">
    <div class="title">Yo-Yo Remote Dashboard</div>
    <div id="statusBadge" class="badge idle">Connecting...</div>
  </div>

  <div class="metrics">
    <div class="metric-card"><small>LEVEL</small><strong id="mLevel">-</strong></div>
    <div class="metric-card"><small>SHUTTLE</small><strong id="mShuttle">-</strong></div>
    <div class="metric-card"><small>DISTANCE</small><strong id="mDist">0 m</strong></div>
  </div>

  <div class="athlete-grid" id="athleteGrid"></div>

  <script>
    let currentState = null;

    function sendAction(action, athleteId) {
      fetch('/api/action', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ action, athleteId, timestamp: Date.now() })
      }).catch(err => console.error(err));
    }

    function renderState(state) {
      currentState = state;
      document.getElementById('mLevel').textContent = state.level ?? 1;
      document.getElementById('mShuttle').textContent = state.shuttle ?? 1;
      document.getElementById('mDist').textContent = (state.distance ?? 0) + ' m';

      const badge = document.getElementById('statusBadge');
      badge.textContent = state.status ?? 'idle';
      badge.className = 'badge ' + (state.status === 'running' ? 'running' : 'idle');

      const grid = document.getElementById('athleteGrid');
      grid.innerHTML = '';

      const athletes = state.athletes || [];
      athletes.forEach(a => {
        const card = document.createElement('div');
        card.className = 'athlete-card status-' + a.status;

        const isRunning = state.status === 'running' && a.status !== 'eliminated';

        card.innerHTML = `
          <div>
            <div class="athlete-name">${a.name}</div>
            <div class="athlete-status">${a.status}</div>
          </div>
          <div class="actions">
            <button class="btn-warn" ${!isRunning ? 'disabled' : ''} onclick="sendAction('mark_miss', '${a.id}')">Miss</button>
            <button class="btn-out" ${!isRunning ? 'disabled' : ''} onclick="sendAction('eliminate', '${a.id}')">Out</button>
          </div>
        `;
        grid.appendChild(card);
      });
    }

    function pollState() {
      fetch('/api/state')
        .then(r => r.json())
        .then(data => { renderState(data); })
        .catch(err => console.error(err));
    }

    setInterval(pollState, 1000);
    pollState();
  </script>
</body>
</html>"#
}
