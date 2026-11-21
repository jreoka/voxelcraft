mod accounts;
mod anticheat;
mod config;
mod game;
mod ledger;
mod server;
mod world_state;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::serve::ListenerExt;
use serde_json::json;
use tokio::io::{AsyncBufReadExt, BufReader};
use voxelcraft_core::day_night::DAY_LENGTH_SEC;

use crate::accounts::AccountStore;
use crate::anticheat::{AntiCheat, now_ms};
use crate::config::{CliOptions, SERVER_VERSION, load_config};
use crate::game::{ServerState, WsOut};
use crate::server::{AppCtx, build_router, execute_console_command};

#[tokio::main]
async fn main() {
    let opts = CliOptions::parse();
    let cfg = load_config(&opts);

    if !opts.client_html_path.exists() {
        eprintln!(
            "[Fatal] Client file not found: {}\nPlace index.html next to the server or pass --client <path>.",
            opts.client_html_path.display()
        );
        std::process::exit(1);
    }

    let accounts = AccountStore::new(opts.data_dir.join("accounts.json"));
    accounts.load();
    let accounts_count = accounts.len();

    let state = ServerState::new(cfg.clone(), opts.data_dir.clone(), opts.client_html_path.clone());
    let seed = state.world.seed.clone();
    let deltas_len = state.world.tables.deltas.len();
    let world_path = state.world.world_path.clone();
    let ctx = AppCtx::new(state, accounts);

    let bind_addr: SocketAddr = format!("{}:{}", cfg.host, cfg.port)
        .parse()
        .unwrap_or_else(|_| SocketAddr::from(([0, 0, 0, 0], cfg.port)));
    let listener = tokio::net::TcpListener::bind(bind_addr)
        .await
        .expect("Failed to bind server port");
    let listener = listener.tap_io(|tcp| {
        let _ = tcp.set_nodelay(true);
    });

    println!("============================================================");
    println!("  VoxelCraft Server v{} (Rust) — Running on {}:{}", SERVER_VERSION, cfg.host, cfg.port);
    println!("  Web Client : http://localhost:{}/", cfg.port);
    println!("  WebSocket  : ws://<your-server-address>:{}", cfg.port);
    println!("  Status API : http://localhost:{}/status", cfg.port);
    println!("  World Seed : {} | Mode: {} | PvP: {}", seed, cfg.mode, if cfg.pvp { "ON" } else { "OFF" });
    println!("  World File : {} ({} deltas)", world_path.display(), deltas_len);
    println!("  Accounts   : {} claimed", accounts_count);
    println!("  Data Dir   : {}", opts.data_dir.display());
    println!("  Client     : {}", opts.client_html_path.display());
    println!("============================================================");
    if !opts.no_console {
        println!("Type /help in this terminal for server console commands.");
    }

    // 1. Player positions snapshot broadcast @ 15 Hz
    {
        let c = Arc::clone(&ctx);
        tokio::spawn(async move {
            let mut iv = tokio::time::interval(Duration::from_millis(66));
            loop {
                iv.tick().await;
                let st = c.state.lock().unwrap();
                if st.stopping { break; }
                st.send_player_snapshots();
            }
        });
    }

    // 2. Game simulation @ 20 Hz
    {
        let c = Arc::clone(&ctx);
        tokio::spawn(async move {
            let mut iv = tokio::time::interval(Duration::from_millis(50));
            let mut last = Instant::now();
            loop {
                iv.tick().await;
                let now = Instant::now();
                let dt = now.duration_since(last).as_secs_f64().clamp(0.0, 0.25);
                last = now;
                let mut st = c.state.lock().unwrap();
                if st.stopping { break; }
                st.tick(dt);
            }
        });
    }

    // 3. World clock + Anti-Cheat VL decay @ 1 Hz
    {
        let c = Arc::clone(&ctx);
        tokio::spawn(async move {
            let mut iv = tokio::time::interval(Duration::from_secs(1));
            let mut last = Instant::now();
            loop {
                iv.tick().await;
                let now = Instant::now();
                let dt = now.duration_since(last).as_secs_f64().clamp(0.0, 5.0);
                last = now;
                let mut st = c.state.lock().unwrap();
                if st.stopping { break; }
                st.world.update_time(dt, DAY_LENGTH_SEC);
                let ac_cfg = st.config.anticheat.clone();
                for p in st.players.values_mut() {
                    AntiCheat::decay_player(&ac_cfg, &mut p.ac, dt);
                }
                let t_ms = now_ms();
                st.failed_logins.retain(|_, &mut (_, first)| t_ms - first <= 120_000);
            }
        });
    }

    // 4. Time/weather sync & keepalive pings @ 5s
    {
        let c = Arc::clone(&ctx);
        tokio::spawn(async move {
            let mut iv = tokio::time::interval(Duration::from_secs(5));
            loop {
                iv.tick().await;
                let st = c.state.lock().unwrap();
                if st.stopping { break; }
                let day = st.world.day;
                let time = (st.world.time * 10000.0).round() / 10000.0;
                let weather = st.world.weather.clone();
                st.broadcast(&json!({ "type": "time_sync", "day": day, "time": time, "weather": weather }), None);
                let now = now_ms();
                let mut to_kick = Vec::new();
                for p in st.players.values() {
                    if now - p.last_activity > 45_000 {
                        to_kick.push(p.id.clone());
                    } else {
                        let _ = p.tx.send(WsOut::Ping);
                    }
                }
                for pid in to_kick {
                    st.kick_player(&pid, "Timed out.");
                }
            }
        });
    }

    // 5. Periodic world auto-save
    {
        let c = Arc::clone(&ctx);
        let save_sec = cfg.save_interval_sec.max(15);
        tokio::spawn(async move {
            let mut iv = tokio::time::interval(Duration::from_secs(save_sec));
            iv.tick().await; // skip immediate tick
            loop {
                iv.tick().await;
                let mut st = c.state.lock().unwrap();
                if st.stopping { break; }
                st.snapshot_online_players();
                if (st.world.tables.dirty || !st.players.is_empty()) && st.world.save() {
                    println!(
                        "[Save] Saved world ({} deltas, {} online).",
                        st.world.tables.deltas.len(),
                        st.players.len()
                    );
                }
            }
        });
    }

    // 6. Interactive console
    if !opts.no_console {
        let c = Arc::clone(&ctx);
        tokio::spawn(async move {
            let stdin = tokio::io::stdin();
            let mut lines = BufReader::new(stdin).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    execute_console_command(&c, trimmed);
                }
            }
        });
    }

    let app = build_router(Arc::clone(&ctx));
    let shutdown_ctx = Arc::clone(&ctx);
    let shutdown_fut = async move {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                shutdown_ctx.stop();
            }
            _ = shutdown_ctx.shutdown.notified() => {}
        }
    };

    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(shutdown_fut)
        .await
        .expect("Server error");
}
