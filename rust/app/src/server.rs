use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade, rejection::WebSocketUpgradeRejection};
use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use futures_util::{SinkExt, StreamExt};
use num_bigint::BigInt;
use serde_json::{Value, json};
use tokio::sync::{Notify, mpsc};
use voxelcraft_core::enchants::ItemStack;
use voxelcraft_core::items::ITEMS;
use voxelcraft_core::math::{big_to_num, format_abs_coord, parse_abs_coord};

use crate::accounts::AccountStore;
use crate::anticheat::now_ms;
use crate::config::{normalize_origin, save_config};
use crate::game::{ServerState, WsOut};

const MAX_WS_PAYLOAD: usize = 256 * 1024;

pub struct AppCtx {
    pub state: Mutex<ServerState>,
    pub accounts: AccountStore,
    pub shutdown: Notify,
}

impl AppCtx {
    pub fn new(state: ServerState, accounts: AccountStore) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(state),
            accounts,
            shutdown: Notify::new(),
        })
    }

    pub fn stop(&self) {
        let mut st = self.state.lock().unwrap();
        if st.stopping {
            return;
        }
        st.stopping = true;
        println!("[Server] Shutting down and saving world...");
        st.snapshot_online_players();
        st.world.save();
        self.accounts.save();
        for p in st.players.values() {
            p.send_json(&json!({ "type": "kick", "reason": "Server shutting down." }));
            p.close(1001, "Server shutting down");
        }
        st.players.clear();
        self.shutdown.notify_waiters();
    }
}

pub fn build_router(ctx: Arc<AppCtx>) -> Router {
    Router::new()
        .route("/status", get(handle_status))
        .route("/api/status", get(handle_status))
        .route("/", get(handle_root_or_ws))
        .route("/index.html", get(handle_html))
        .route("/voxelcraft.html", get(handle_html))
        .fallback(handle_fallback)
        .with_state(ctx)
}

fn extract_ip(headers: &HeaderMap, addr: SocketAddr, trust_proxy: bool) -> String {
    if trust_proxy {
        if let Some(fly_ip) = headers.get("fly-client-ip").and_then(|v| v.to_str().ok()) {
            let t = fly_ip.trim();
            if !t.is_empty() {
                return t.strip_prefix("::ffff:").unwrap_or(t).to_string();
            }
        }
        if let Some(fwd) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok())
            && let Some(first) = fwd.split(',').next()
        {
            let t = first.trim();
            if !t.is_empty() {
                return t.strip_prefix("::ffff:").unwrap_or(t).to_string();
            }
        }
    }
    let raw = addr.ip().to_string();
    raw.strip_prefix("::ffff:").unwrap_or(&raw).to_string()
}

fn origin_allowed(headers: &HeaderMap, allowed_origin: Option<&str>) -> bool {
    let Some(expected) = allowed_origin else {
        return true;
    };
    headers
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .and_then(normalize_origin)
        .is_some_and(|origin| origin == expected)
}

fn forbidden_response() -> Response {
    let mut resp = Response::new("Forbidden".into());
    *resp.status_mut() = StatusCode::FORBIDDEN;
    resp.headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static("text/plain; charset=utf-8"));
    resp
}

async fn handle_status(
    State(ctx): State<Arc<AppCtx>>,
    ws: Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    if let Ok(ws) = ws {
        return handle_ws_upgrade(ctx, ws, addr, headers);
    }
    let allowed_origin = ctx.state.lock().unwrap().allowed_origin.clone();
    if !origin_allowed(&headers, allowed_origin.as_deref()) {
        return forbidden_response();
    }
    let body = {
        #[derive(serde::Serialize)]
        #[serde(rename_all = "camelCase")]
        struct AcStatus {
            enabled: bool,
            total_flags: u64,
            total_kicks: u64,
        }
        #[derive(serde::Serialize)]
        #[serde(rename_all = "camelCase")]
        struct StatusResponse<'a> {
            ok: bool,
            server_name: &'a str,
            motd: &'a str,
            mode: &'a str,
            difficulty: &'a str,
            pvp: bool,
            players_online: usize,
            max_players: usize,
            player_names: Vec<&'a str>,
            shards: usize,
            mobs: usize,
            drops: usize,
            day: i32,
            weather: &'a str,
            uptime_sec: i64,
            anticheat: AcStatus,
        }
        let st = ctx.state.lock().unwrap();
        let mut mob_count = 0usize;
        let mut drop_count = 0usize;
        for s in st.shards.values() {
            mob_count += s.mobs.mobs.len();
            drop_count += s.drops.len();
        }
        let resp = StatusResponse {
            ok: true,
            server_name: &st.config.server_name,
            motd: &st.config.motd,
            mode: &st.config.mode,
            difficulty: &st.config.difficulty,
            pvp: st.config.pvp,
            players_online: st.players.len(),
            max_players: st.config.max_players,
            player_names: st.players.values().map(|p| p.name.as_str()).collect(),
            shards: st.shards.len(),
            mobs: mob_count,
            drops: drop_count,
            day: st.world.day,
            weather: &st.world.weather,
            uptime_sec: ((now_ms() - st.started_at) / 1000).max(0),
            anticheat: AcStatus {
                enabled: st.config.anticheat.enabled,
                total_flags: st.ac.total_flags,
                total_kicks: st.ac.total_kicks,
            },
        };
        serde_json::to_string(&resp).unwrap_or_else(|_| "{}".to_string())
    };

    let mut resp = Response::new(body.into());
    *resp.status_mut() = StatusCode::OK;
    let h = resp.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json; charset=utf-8"));
    if let Some(origin) = allowed_origin {
        if let Ok(val) = HeaderValue::from_str(&origin) {
            h.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, val);
            h.insert(header::VARY, HeaderValue::from_static("Origin"));
        }
    } else {
        h.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));
    }
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    resp
}

async fn handle_root_or_ws(
    State(ctx): State<Arc<AppCtx>>,
    ws: Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    if let Ok(ws) = ws {
        return handle_ws_upgrade(ctx, ws, addr, headers);
    }
    serve_client_html(&ctx, &headers)
}

async fn handle_html(
    State(ctx): State<Arc<AppCtx>>,
    ws: Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    if let Ok(ws) = ws {
        return handle_ws_upgrade(ctx, ws, addr, headers);
    }
    serve_client_html(&ctx, &headers)
}

async fn handle_fallback(
    State(ctx): State<Arc<AppCtx>>,
    ws: Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    if let Ok(ws) = ws {
        return handle_ws_upgrade(ctx, ws, addr, headers);
    }
    let allowed_origin = ctx.state.lock().unwrap().allowed_origin.clone();
    if !origin_allowed(&headers, allowed_origin.as_deref()) {
        return forbidden_response();
    }
    let mut resp = Response::new("Not found.".into());
    *resp.status_mut() = StatusCode::NOT_FOUND;
    resp.headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static("text/plain; charset=utf-8"));
    resp
}

fn serve_client_html(ctx: &Arc<AppCtx>, headers: &HeaderMap) -> Response {
    let (path, allowed_origin) = {
        let st = ctx.state.lock().unwrap();
        (st.client_html_path.clone(), st.allowed_origin.clone())
    };
    let Some(path) = path else {
        if !origin_allowed(headers, allowed_origin.as_deref()) {
            return forbidden_response();
        }
        let mut resp = Response::new("Web client is not hosted on this server.".into());
        *resp.status_mut() = StatusCode::NOT_FOUND;
        resp.headers_mut()
            .insert(header::CONTENT_TYPE, HeaderValue::from_static("text/plain; charset=utf-8"));
        return resp;
    };
    match std::fs::read(&path) {
        Ok(bytes) => {
            let mut resp = Response::new(bytes.into());
            *resp.status_mut() = StatusCode::OK;
            let h = resp.headers_mut();
            h.insert(header::CONTENT_TYPE, HeaderValue::from_static("text/html; charset=utf-8"));
            h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
            resp
        }
        Err(_) => {
            let body = format!(
                "Client file not found: {}\nPlace index.html next to the server or pass --client <path>.",
                path.display()
            );
            let mut resp = Response::new(body.into());
            *resp.status_mut() = StatusCode::NOT_FOUND;
            resp.headers_mut()
                .insert(header::CONTENT_TYPE, HeaderValue::from_static("text/plain; charset=utf-8"));
            resp
        }
    }
}

fn handle_ws_upgrade(
    ctx: Arc<AppCtx>,
    ws: WebSocketUpgrade,
    addr: SocketAddr,
    headers: HeaderMap,
) -> Response {
    let (ip, origin_ok, rate_ok) = {
        let mut st = ctx.state.lock().unwrap();
        if !origin_allowed(&headers, st.allowed_origin.as_deref()) {
            (String::new(), false, false)
        } else {
            let ip = extract_ip(&headers, addr, st.config.trust_proxy);
            let cur = *st.ip_counts.get(&ip).unwrap_or(&0);
            let max_per_ip = st.config.max_per_ip.max(1);
            if cur >= max_per_ip {
                (ip, true, false)
            } else {
                st.ip_counts.insert(ip.clone(), cur + 1);
                (ip, true, true)
            }
        }
    };
    if !origin_ok {
        return forbidden_response();
    }
    if !rate_ok {
        return (StatusCode::TOO_MANY_REQUESTS, "").into_response();
    }
    ws.max_message_size(MAX_WS_PAYLOAD)
        .on_upgrade(move |socket| handle_ws_connection(ctx, socket, ip))
}

async fn handle_ws_connection(ctx: Arc<AppCtx>, socket: WebSocket, ip: String) {
    let (mut sink, mut stream) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<WsOut>();

    let writer = tokio::spawn(async move {
        while let Some(out) = rx.recv().await {
            match out {
                WsOut::Text(s) => {
                    if sink.send(Message::Text(s.into())).await.is_err() {
                        break;
                    }
                }
                WsOut::Ping => {
                    if sink.send(Message::Ping(Vec::new().into())).await.is_err() {
                        break;
                    }
                }
                WsOut::Close(code, reason) => {
                    let _ = sink
                        .send(Message::Close(Some(CloseFrame {
                            code,
                            reason: reason.chars().take(120).collect::<String>().into(),
                        })))
                        .await;
                    let _ = sink.close().await;
                    break;
                }
            }
        }
    });

    let mut session_id: Option<String> = None;
    let join_deadline = tokio::time::sleep(Duration::from_secs(8));
    tokio::pin!(join_deadline);

    loop {
        let next_msg = if session_id.is_none() {
            tokio::select! {
                _ = &mut join_deadline => {
                    let _ = tx.send(WsOut::Close(4000, "Join handshake timeout".to_string()));
                    break;
                }
                m = stream.next() => m,
            }
        } else {
            stream.next().await
        };

        let Some(Ok(msg)) = next_msg else {
            break;
        };

        if let Some(pid) = &session_id {
            let mut st = ctx.state.lock().unwrap();
            if let Some(p) = st.players.get_mut(pid) {
                p.last_activity = now_ms();
            }
        }

        match msg {
            Message::Text(raw) => {
                if raw.len() > MAX_WS_PAYLOAD {
                    let _ = tx.send(WsOut::Close(1009, "Message too large".to_string()));
                    break;
                }
                let Ok(mut val) = serde_json::from_str::<Value>(&raw) else { continue };
                if !val.is_object() { continue; }

                if session_id.is_none() {
                    if val.get("type").and_then(|v| v.as_str()) == Some("join") {
                        match handle_join(&ctx, &tx, &ip, &val).await {
                            Some(pid) => {
                                session_id = Some(pid);
                            }
                            None => break,
                        }
                    }
                    continue;
                }

                let pid = session_id.as_ref().unwrap();
                handle_packet(&ctx, pid, &mut val);
            }
            Message::Ping(_) | Message::Pong(_) => {}
            Message::Close(_) => break,
            Message::Binary(_) => {}
        }
    }

    // Cleanup on disconnect
    {
        let mut st = ctx.state.lock().unwrap();
        if let Some(c) = st.ip_counts.get_mut(&ip) {
            if *c <= 1 {
                st.ip_counts.remove(&ip);
            } else {
                *c -= 1;
            }
        }
        if let Some(pid) = session_id {
            let is_current = st.players.contains_key(&pid);
            if is_current {
                if let Some(p) = st.players.get_mut(&pid) {
                    p.online = false;
                }
                st.save_player_profile(&pid);
                if let Some(p) = st.players.shift_remove(&pid) {
                    if let Some(sh) = st.shards.get_mut(&p.shard_id) {
                        sh.player_ids.retain(|id| id != &pid);
                    }
                    if st.world.tables.dirty {
                        st.schedule_world_save(1000);
                    }
                    println!("[Leave] {} ({}) disconnected. Online: {}", p.name, ip, st.players.len());
                    st.broadcast(&json!({ "type": "player_leave", "id": p.id, "name": p.name }), None);
                }
            }
        }
    }
    drop(tx);
    let _ = writer.await;
}

async fn handle_join(
    ctx: &Arc<AppCtx>,
    tx: &mpsc::UnboundedSender<WsOut>,
    ip: &str,
    msg: &Value,
) -> Option<String> {
    let reject = |code: u16, reason: &str| -> Option<String> {
        let _ = tx.send(WsOut::Text(json!({ "type": "auth_error", "reason": reason }).to_string()));
        let _ = tx.send(WsOut::Close(code, reason.to_string()));
        None
    };

    {
        let mut st = ctx.state.lock().unwrap();
        if st.is_banned(None, Some(ip)) {
            return reject(4003, "You are banned from this server.");
        }
        let throttled = if let Some(&(cnt, first_ms)) = st.failed_logins.get(ip) {
            if now_ms() - first_ms > 120_000 {
                st.failed_logins.remove(ip);
                false
            } else {
                cnt >= 6
            }
        } else {
            false
        };
        if throttled {
            return reject(4006, "Too many failed login attempts. Try again in a couple of minutes.");
        }
    }

    let auth_mode = if msg.get("authMode").and_then(|v| v.as_str()) == Some("register") {
        "register"
    } else {
        "login"
    };
    let raw_name = msg.get("name").and_then(|v| v.as_str()).unwrap_or("").trim();
    let raw_pw = msg.get("password").and_then(|v| v.as_str()).unwrap_or("");

    let auth_res = if auth_mode == "register" {
        ctx.accounts.register(raw_name, raw_pw).await
    } else {
        ctx.accounts.login(raw_name, raw_pw).await
    };

    if tx.is_closed() {
        return None;
    }

    if !auth_res.ok {
        if auth_res.bad_password {
            let mut st = ctx.state.lock().unwrap();
            let now = now_ms();
            let entry = st.failed_logins.entry(ip.to_string()).or_insert((0, now));
            if now - entry.1 > 120_000 {
                *entry = (1, now);
            } else {
                entry.0 += 1;
            }
        }
        return reject(4004, &auth_res.reason);
    }

    let account = auth_res.account.unwrap();
    let name = account.name;

    let mut st = ctx.state.lock().unwrap();
    st.failed_logins.remove(ip);
    if st.is_banned(Some(&name), Some(ip)) {
        return reject(4003, "You are banned from this server.");
    }

    if let Some(ex_id) = st.find_player_id_by_name(&name) {
        st.save_player_profile(&ex_id);
        if let Some(ex) = st.players.shift_remove(&ex_id) {
            if let Some(sh) = st.shards.get_mut(&ex.shard_id) {
                sh.player_ids.retain(|id| id != &ex_id);
            }
            st.broadcast(&json!({ "type": "player_leave", "id": ex.id, "name": ex.name }), None);
            println!("[Kick] {}: Logged in from another location.", ex.name);
            ex.send_json(&json!({ "type": "kick", "reason": "Logged in from another location." }));
            ex.close(4001, "Logged in from another location.");
        }
    }

    let max_p = st.config.max_players.max(1);
    if st.players.len() >= max_p {
        return reject(4002, "Server is full.");
    }

    let (pid, has_pos) = st.init_player_session(name.clone(), ip.to_string(), tx.clone());
    let welcome = st.build_welcome_json(&pid, auth_res.created, has_pos);
    st.players[&pid].send_json(&welcome);

    println!(
        "[Join] {} ({}) {}joined as {}. Online: {}/{}",
        name,
        ip,
        if auth_res.created { "claimed new account & " } else { "" },
        pid,
        st.players.len(),
        max_p
    );
    st.broadcast(&json!({ "type": "player_join", "id": pid, "name": name }), Some(&pid));
    Some(pid)
}

fn handle_packet(ctx: &Arc<AppCtx>, pid: &str, msg: &mut Value) {
    let mut async_passwd: Option<(String, String, String)> = None;
    {
        let mut st = ctx.state.lock().unwrap();
        if !st.players.contains_key(pid) {
            return;
        }
        // Check packet rate
        if st.config.anticheat.enabled {
            let now = now_ms();
            let limit = st.config.anticheat.max_packets_per_sec.max(1);
            let (over, cnt) = {
                let p = st.players.get_mut(pid).unwrap();
                if now - p.ac.packet_window_start >= 1000 {
                    p.ac.packet_window_start = now;
                    p.ac.packet_count = 0;
                }
                p.ac.packet_count += 1;
                (p.ac.packet_count > limit, p.ac.packet_count)
            };
            if over {
                if cnt == limit + 1 || cnt % 25 == 0 {
                    st.flag_player(pid, "PACKET_FLOOD", 3.0, &format!("{} pkt/s", cnt));
                }
                return;
            }
        }

        let mtype = msg.get("type").and_then(|v| v.as_str()).unwrap_or("").to_string();
        if mtype == "rebase_ack" {
            let seq = msg.get("seq").and_then(|v| v.as_u64()).unwrap_or(0);
            if let Some(p) = st.players.get_mut(pid) {
                p.pending_frames.retain(|f| f.seq > seq);
            }
            return;
        }
        if !st.translate_incoming(pid, msg) {
            return;
        }

        match mtype.as_str() {
            "ping" => {
                let t = msg.get("t").and_then(|v| v.as_i64()).unwrap_or_else(now_ms);
                let p = st.players.get_mut(pid).unwrap();
                if let Some(pg) = msg.get("ping").and_then(|v| v.as_f64())
                    && pg.is_finite() && pg > 0.0 {
                        p.ping = (pg.round() as i32).clamp(1, 9999);
                    }
                p.send_json(&json!({ "type": "pong", "t": t }));
            }
            "move" => {
                st.handle_move(pid, msg);
            }
            "chat" => {
                let raw = msg.get("text").and_then(|v| v.as_str()).unwrap_or("");
                let clean: String = raw
                    .chars()
                    .filter(|c| !matches!(*c, '\r' | '\n' | '\x00'..='\x1f'))
                    .collect();
                let text: String = clean.trim().chars().take(200).collect();
                if text.is_empty() {
                    return;
                }
                // Chat rate limit
                let now = now_ms();
                let rate_ok = {
                    let p = st.players.get_mut(pid).unwrap();
                    p.ac.chat_timestamps.retain(|&t| now - t < 6000);
                    if p.ac.chat_timestamps.len() >= 5 {
                        p.send_json(&json!({ "type": "chat", "kind": "warn", "text": "Slow down! You are sending messages too quickly." }));
                        false
                    } else {
                        p.ac.chat_timestamps.push(now);
                        true
                    }
                };
                if !rate_ok {
                    return;
                }
                if text.starts_with('/') {
                    async_passwd = execute_command_locked(&mut st, &text, Some(pid));
                } else {
                    let pname = st.players[pid].name.clone();
                    st.broadcast_chat(&text, "normal", Some(&pname));
                }
            }
            _ => {
                st.handle_game_packet(pid, msg);
            }
        }
    }

    if let Some((pname, old_pw, new_pw)) = async_passwd {
        let ctx_c = Arc::clone(ctx);
        let pid_s = pid.to_string();
        tokio::spawn(async move {
            let res = ctx_c.accounts.change_password(&pname, &old_pw, &new_pw).await;
            let st = ctx_c.state.lock().unwrap();
            if let Some(p) = st.players.get(&pid_s) {
                match res {
                    Ok(()) => p.send_json(&json!({ "type": "chat", "text": "Account password updated.", "kind": "sys" })),
                    Err(r) => p.send_json(&json!({ "type": "chat", "text": r, "kind": "err" })),
                }
            }
        });
    }
}

pub fn execute_console_command(ctx: &Arc<AppCtx>, raw: &str) {
    let should_stop = {
        let mut st = ctx.state.lock().unwrap();
        let trimmed = raw.trim().trim_start_matches('/');
        let cmd = trimmed.split_whitespace().next().unwrap_or("").to_lowercase();
        if cmd == "stop" {
            true
        } else {
            execute_command_locked(&mut st, raw, None);
            false
        }
    };
    if should_stop {
        ctx.stop();
    }
}

/// Executes a slash command while holding the state lock. Returns `Some((name, old_pw, new_pw))` if `/passwd` needs async hashing.
fn execute_command_locked(st: &mut ServerState, raw: &str, sender_pid: Option<&str>) -> Option<(String, String, String)> {
    let is_console = sender_pid.is_none();
    let is_op = is_console
        || sender_pid
            .and_then(|id| st.players.get(id))
            .is_some_and(|p| st.is_op(&p.name));

    let reply = |st: &ServerState, text: &str, kind: &str| {
        if let Some(pid) = sender_pid {
            if let Some(p) = st.players.get(pid) {
                p.send_json(&json!({ "type": "chat", "text": text, "kind": kind }));
            }
        } else {
            println!("[Console] {}", text);
        }
    };

    let stripped = raw.trim().trim_start_matches('/').trim();
    let parts: Vec<&str> = stripped.split_whitespace().collect();
    let cmd = parts.first().copied().unwrap_or("").to_lowercase();
    let args = if parts.len() > 1 { &parts[1..] } else { &[] };

    match cmd.as_str() {
        "help" => {
            reply(st, "Player cmds: /list, /ping, /pos, /msg <player> <text>, /passwd <old> <new>", "sys");
            if is_op {
                let extra = if is_console { ", /stop" } else { "" };
                reply(
                    st,
                    &format!("Admin cmds: /op, /deop, /kick, /ban, /unban, /tp, /gamemode, /give, /time, /weather, /pvp, /ac, /save, /say{}", extra),
                    "sys",
                );
            }
            return None;
        }
        "passwd" if !is_console => {
            let old_pw = args.first().copied().unwrap_or("");
            let new_pw = args.get(1).copied().unwrap_or("");
            if old_pw.is_empty() || new_pw.is_empty() {
                reply(st, "Usage: /passwd <currentPassword> <newPassword>", "warn");
                return None;
            }
            let pname = st.players[sender_pid.unwrap()].name.clone();
            return Some((pname, old_pw.to_string(), new_pw.to_string()));
        }
        "list" => {
            let names: Vec<String> = st
                .players
                .values()
                .map(|p| format!("{}{}", p.name, if st.is_op(&p.name) { "*" } else { "" }))
                .collect();
            let joined = if names.is_empty() { "none".to_string() } else { names.join(", ") };
            reply(st, &format!("Online ({}/{}): {}", names.len(), st.config.max_players, joined), "sys");
            return None;
        }
        "ping" => {
            if let Some(pid) = sender_pid {
                let vl = st.players[pid].ac.vl;
                reply(st, &format!("Your connection is active (VL: {:.1}).", vl), "sys");
            } else {
                reply(st, "Server is running smoothly.", "sys");
            }
            return None;
        }
        "pos" if !is_console => {
            let p = &st.players[sender_pid.unwrap()];
            let sh = &st.shards[&p.shard_id];
            let sx = format_abs_coord(&sh.ox, big_to_num(&sh.ox), p.x, 2);
            let sz = format_abs_coord(&sh.oz, big_to_num(&sh.oz), p.z, 2);
            reply(st, &format!("Position: X {}, Y {:.2}, Z {}", sx, p.y, sz), "sys");
            return None;
        }
        "msg" | "w" | "tell" => {
            let target_id = args.first().and_then(|n| st.find_player_id_by_name(n));
            let body = if args.len() > 1 { args[1..].join(" ") } else { String::new() };
            let Some(tid) = target_id.filter(|_| !body.is_empty()) else {
                reply(st, "Usage: /msg <player> <message>", "warn");
                return None;
            };
            let from_name = sender_pid.map_or("Server".to_string(), |id| st.players[id].name.clone());
            let tname = st.players[&tid].name.clone();
            st.players[&tid].send_json(&json!({ "type": "chat", "kind": "sys", "text": format!("[PM from {}] {}", from_name, body) }));
            reply(st, &format!("[PM to {}] {}", tname, body), "sys");
            return None;
        }
        _ => {}
    }

    if !is_op {
        reply(st, "Permission denied. You must be a server OP (run \"/op <name>\" in server console).", "err");
        return None;
    }

    match cmd.as_str() {
        "op" => {
            let Some(&name) = args.first() else {
                reply(st, "Usage: /op <player>", "warn");
                return None;
            };
            if !st.is_op(name) {
                st.config.ops.push(name.to_string());
                save_config(&st.data_dir, &st.config);
            }
            reply(st, &format!("Granted OP to {}.", name), "sys");
            if let Some(tid) = st.find_player_id_by_name(name) {
                st.players[&tid].send_json(&json!({ "type": "chat", "kind": "sys", "text": "You were granted Server OP privileges!" }));
            }
        }
        "deop" => {
            let low = args.first().copied().unwrap_or("").to_lowercase();
            st.config.ops.retain(|o| o.to_lowercase() != low);
            save_config(&st.data_dir, &st.config);
            reply(st, &format!("Removed OP from {}.", args.first().copied().unwrap_or("")), "sys");
        }
        "kick" => {
            let Some(tid) = args.first().and_then(|n| st.find_player_id_by_name(n)) else {
                reply(st, "Player not found.", "warn");
                return None;
            };
            let reason = if args.len() > 1 { args[1..].join(" ") } else { "Kicked by an administrator.".to_string() };
            let tname = st.players[&tid].name.clone();
            st.kick_player(&tid, &reason);
            reply(st, &format!("Kicked {}: {}", tname, reason), "sys");
        }
        "ban" => {
            let Some(&name) = args.first() else {
                reply(st, "Usage: /ban <player> [reason]", "warn");
                return None;
            };
            if !st.is_banned(Some(name), None) {
                st.config.bans.push(name.to_string());
                save_config(&st.data_dir, &st.config);
            }
            if let Some(tid) = st.find_player_id_by_name(name) {
                let reason = if args.len() > 1 { args[1..].join(" ") } else { "Banned by an administrator.".to_string() };
                st.kick_player(&tid, &reason);
            }
            reply(st, &format!("Banned {}.", name), "sys");
        }
        "unban" => {
            let low = args.first().copied().unwrap_or("").to_lowercase();
            st.config.bans.retain(|b| b.to_lowercase() != low);
            save_config(&st.data_dir, &st.config);
            reply(st, &format!("Unbanned {}.", args.first().copied().unwrap_or("")), "sys");
        }
        "gamemode" | "gm" => {
            let m = args.first().copied().unwrap_or("").to_lowercase();
            let mode = match m.as_str() {
                "c" | "creative" | "1" => Some("creative"),
                "s" | "survival" | "0" => Some("survival"),
                _ => None,
            };
            let Some(mode) = mode else {
                reply(st, "Usage: /gamemode <survival|creative> [player]", "warn");
                return None;
            };
            let target_id = match args.get(1) {
                Some(n) => st.find_player_id_by_name(n),
                None => sender_pid.map(|s| s.to_string()),
            };
            let Some(tid) = target_id else {
                reply(st, "Specify an online player.", "warn");
                return None;
            };
            let tname = {
                let p = st.players.get_mut(&tid).unwrap();
                p.mode = mode.to_string();
                p.pl.mode = mode.to_string();
                p.accept_next_sync = true;
                p.send_json(&json!({ "type": "mode_change", "mode": mode }));
                p.name.clone()
            };
            reply(st, &format!("Set {} to {} mode.", tname, mode), "sys");
        }
        "give" => {
            let tid = args.first().and_then(|n| st.find_player_id_by_name(n));
            let id = args.get(1).and_then(|s| s.parse::<i32>().ok());
            let cnt = args.get(2).and_then(|s| s.parse::<i32>().ok()).unwrap_or(1).clamp(1, 64);
            let (Some(tid), Some(id)) = (tid, id.filter(|&i| ITEMS.get(i).is_some())) else {
                reply(st, "Usage: /give <player> <itemId> [count]", "warn");
                return None;
            };
            let tname = {
                let p = st.players.get_mut(&tid).unwrap();
                if p.mode != "creative" {
                    p.ledger.add(&ItemStack { id, count: cnt, dur: None, enchant: None });
                }
                p.send_json(&json!({ "type": "give_item", "id": id, "count": cnt }));
                p.name.clone()
            };
            reply(st, &format!("Gave {}x item #{} to {}.", cnt, id, tname), "sys");
        }
        "tp" => {
            if args.len() == 2 {
                let (Some(aid), Some(bid)) = (st.find_player_id_by_name(args[0]), st.find_player_id_by_name(args[1])) else {
                    reply(st, "Player(s) not found.", "warn");
                    return None;
                };
                let (aname, bname, xi, xf, by, zi, zf) = {
                    let a = &st.players[&aid];
                    let b = &st.players[&bid];
                    let sh = &st.shards[&b.shard_id];
                    let xi = &sh.ox + BigInt::from(b.x.floor() as i64);
                    let zi = &sh.oz + BigInt::from(b.z.floor() as i64);
                    (a.name.clone(), b.name.clone(), xi, b.x - b.x.floor(), b.y + 0.5, zi, b.z - b.z.floor())
                };
                st.place_abs(&aid, &xi, xf, by, &zi, zf, Some(&format!("Teleported to {}.", bname)), true);
                reply(st, &format!("Teleported {} to {}.", aname, bname), "sys");
            } else if args.len() == 3 && !is_console {
                let (Some((xi, xf)), Ok(ty), Some((zi, zf))) = (parse_abs_coord(args[0]), args[1].parse::<f64>(), parse_abs_coord(args[2])) else {
                    reply(st, "Usage: /tp <x> <y> <z>", "warn");
                    return None;
                };
                if !ty.is_finite() { reply(st, "Usage: /tp <x> <y> <z>", "warn"); return None; }
                let y = ty.clamp(-64.0, 320.0);
                let sx = format_abs_coord(&xi, big_to_num(&xi), xf, 1);
                let sz = format_abs_coord(&zi, big_to_num(&zi), zf, 1);
                let pid = sender_pid.unwrap();
                let pname = st.players[pid].name.clone();
                st.place_abs(pid, &xi, xf, y, &zi, zf, Some(&format!("Teleported to {}, {:.1}, {}.", sx, y, sz)), true);
                reply(st, &format!("Teleported {} to {}, {:.1}, {}.", pname, sx, y, sz), "sys");
            } else if args.len() == 4 {
                let (Some(aid), Some((xi, xf)), Ok(ty), Some((zi, zf))) = (
                    st.find_player_id_by_name(args[0]),
                    parse_abs_coord(args[1]),
                    args[2].parse::<f64>(),
                    parse_abs_coord(args[3]),
                ) else {
                    reply(st, "Usage: /tp <player> <x> <y> <z>", "warn");
                    return None;
                };
                if !ty.is_finite() { reply(st, "Usage: /tp <player> <x> <y> <z>", "warn"); return None; }
                let y = ty.clamp(-64.0, 320.0);
                let sx = format_abs_coord(&xi, big_to_num(&xi), xf, 1);
                let sz = format_abs_coord(&zi, big_to_num(&zi), zf, 1);
                let aname = st.players[&aid].name.clone();
                st.place_abs(&aid, &xi, xf, y, &zi, zf, Some(&format!("Teleported to {}, {:.1}, {}.", sx, y, sz)), true);
                reply(st, &format!("Teleported {} to {}, {:.1}, {}.", aname, sx, y, sz), "sys");
            } else {
                reply(st, "Usage: /tp <p1> <p2> OR /tp [player] <x> <y> <z>", "warn");
            }
        }
        "time" => {
            let arg = args.first().copied().unwrap_or("");
            let t = match arg {
                "day" => Some(0.28),
                "sunrise" => Some(0.24),
                "noon" => Some(0.50),
                "sunset" => Some(0.75),
                "night" => Some(0.88),
                _ => arg.parse::<f64>().ok().filter(|v| v.is_finite()),
            };
            let Some(t) = t else {
                reply(st, "Usage: /time <day|noon|sunset|night|0..1>", "warn");
                return None;
            };
            st.world.time = ((t % 1.0) + 1.0) % 1.0;
            let day = st.world.day;
            let time = st.world.time;
            let weather = st.world.weather.clone();
            st.broadcast(&json!({ "type": "time_sync", "day": day, "time": time, "weather": weather }), None);
            st.broadcast_chat(&format!("Time set to {}.", arg), "sys", None);
        }
        "weather" => {
            let w = args.first().copied().unwrap_or("").to_lowercase();
            if !matches!(w.as_str(), "clear" | "rain" | "thunder") {
                reply(st, "Usage: /weather <clear|rain|thunder>", "warn");
                return None;
            }
            st.world.weather = w.clone();
            let day = st.world.day;
            let time = st.world.time;
            st.broadcast(&json!({ "type": "time_sync", "day": day, "time": time, "weather": w }), None);
            st.broadcast_chat(&format!("Weather set to {}.", w), "sys", None);
        }
        "pvp" => {
            let v = args.first().copied().unwrap_or("").to_lowercase();
            match v.as_str() {
                "on" | "true" => st.config.pvp = true,
                "off" | "false" => st.config.pvp = false,
                _ => {
                    reply(st, &format!("PvP is currently {}. Usage: /pvp <on|off>", if st.config.pvp { "ON" } else { "OFF" }), "sys");
                    return None;
                }
            }
            save_config(&st.data_dir, &st.config);
            st.broadcast_chat(&format!("PvP is now {}.", if st.config.pvp { "ENABLED" } else { "DISABLED" }), "sys", None);
        }
        "ac" | "anticheat" => {
            let sub = args.first().copied().unwrap_or("status").to_lowercase();
            if sub == "toggle" {
                st.config.anticheat.enabled = !st.config.anticheat.enabled;
                save_config(&st.data_dir, &st.config);
                reply(st, &format!("Anti-Cheat is now {}.", if st.config.anticheat.enabled { "ENABLED" } else { "DISABLED" }), "sys");
            } else if sub == "reset" && args.len() > 1 {
                let Some(tid) = st.find_player_id_by_name(args[1]) else {
                    reply(st, "Player not found.", "warn");
                    return None;
                };
                let tname = {
                    let p = st.players.get_mut(&tid).unwrap();
                    p.ac.vl = 0.0;
                    p.ac.warned = false;
                    p.ac.flags.clear();
                    p.name.clone()
                };
                reply(st, &format!("Reset Anti-Cheat VL for {}.", tname), "sys");
            } else {
                let list: Vec<String> = st.players.values().map(|pl| format!("{}: VL {:.1}", pl.name, pl.ac.vl)).collect();
                let joined = if list.is_empty() { "none".to_string() } else { list.join(", ") };
                reply(
                    st,
                    &format!(
                        "Anti-Cheat {} | Flags: {} | Kicks: {} | Players: {}",
                        if st.config.anticheat.enabled { "ON" } else { "OFF" },
                        st.ac.total_flags,
                        st.ac.total_kicks,
                        joined
                    ),
                    "sys",
                );
            }
        }
        "save" => {
            st.snapshot_online_players();
            let ok = st.world.save();
            if ok {
                reply(st, &format!("Saved world ({} deltas).", st.world.tables.deltas.len()), "sys");
            } else {
                reply(st, "World save failed!", "err");
            }
        }
        "say" => {
            let msg = args.join(" ");
            if !msg.is_empty() {
                st.broadcast_chat(&format!("[Server] {}", msg), "sys", None);
            }
        }
        _ => {
            reply(st, &format!("Unknown command \"/{}\". Type /help.", cmd), "warn");
        }
    }
    None
}
