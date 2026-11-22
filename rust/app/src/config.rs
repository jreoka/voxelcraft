use std::fs;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Debug)]
pub struct CliOptions {
    pub no_console: bool,
    pub no_client: bool,
    pub data_dir: PathBuf,
    pub client_html_path: Option<PathBuf>,
    pub allowed_origin: Option<String>,
    pub cli_host: Option<String>,
    pub cli_port: Option<u16>,
}

pub fn normalize_origin(raw: &str) -> Option<String> {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    if let Some(scheme_end) = trimmed.find("://") {
        let after_scheme = &trimmed[scheme_end + 3..];
        let host_end = after_scheme
            .find('/')
            .map(|i| scheme_end + 3 + i)
            .unwrap_or(trimmed.len());
        Some(trimmed[..host_end].to_ascii_lowercase())
    } else {
        Some(trimmed.to_ascii_lowercase())
    }
}

fn env_flag(name: &str) -> Option<bool> {
    std::env::var(name).ok().and_then(|v| match v.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    })
}

impl CliOptions {
    pub fn parse() -> Self {
        let argv: Vec<String> = std::env::args().skip(1).collect();
        let get_arg = |name: &str| -> Option<String> {
            let flag = format!("--{}", name);
            argv.iter()
                .position(|a| a == &flag)
                .and_then(|i| argv.get(i + 1).cloned())
        };
        let no_console = argv.iter().any(|a| a == "--no-console") || !std::io::stdin().is_terminal();
        let no_client = argv.iter().any(|a| a == "--no-client") || env_flag("NO_CLIENT").unwrap_or(false);
        let allowed_origin = get_arg("url")
            .or_else(|| std::env::var("URL").ok())
            .and_then(|u| normalize_origin(&u));
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let data_dir = get_arg("data-dir")
            .or_else(|| std::env::var("DATA_DIR").ok())
            .map(PathBuf::from)
            .unwrap_or_else(|| cwd.clone());
        let data_dir = if data_dir.is_absolute() {
            data_dir
        } else {
            cwd.join(data_dir)
        };
        let client_html_path = if no_client {
            None
        } else if let Some(explicit) = get_arg("client").or_else(|| std::env::var("CLIENT_HTML").ok()) {
            let p = PathBuf::from(explicit);
            Some(if p.is_absolute() { p } else { cwd.join(p) })
        } else {
            let mut found = None;
            for name in ["index.html", "voxelcraft.html"] {
                let cand = cwd.join(name);
                if cand.exists() {
                    found = Some(cand);
                    break;
                }
            }
            if found.is_some() {
                found
            } else if allowed_origin.is_some() {
                None
            } else {
                Some(cwd.join("index.html"))
            }
        };
        let cli_host = get_arg("host");
        let cli_port = get_arg("port").and_then(|s| s.parse::<u16>().ok());
        Self {
            no_console,
            no_client: no_client || client_html_path.is_none(),
            data_dir,
            client_html_path,
            allowed_origin,
            cli_host,
            cli_port,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AntiCheatConfig {
    pub enabled: bool,
    #[serde(rename = "warnVL")]
    pub warn_vl: f64,
    #[serde(rename = "kickVL")]
    pub kick_vl: f64,
    pub vl_decay_per_sec: f64,
    pub max_reach: f64,
    pub max_pvp_reach: f64,
    pub max_horiz_speed: f64,
    pub max_creative_speed: f64,
    pub max_teleport_dist: f64,
    pub max_air_hover_sec: f64,
    pub max_jump_gain: f64,
    pub max_blocks_per_sec: usize,
    pub max_packets_per_sec: usize,
    pub log_file: String,
}

impl Default for AntiCheatConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            warn_vl: 8.0,
            kick_vl: 18.0,
            vl_decay_per_sec: 0.25,
            max_reach: 6.9,
            max_pvp_reach: 4.8,
            max_horiz_speed: 11.8,
            max_creative_speed: 20.0,
            max_teleport_dist: 14.5,
            max_air_hover_sec: 3.0,
            max_jump_gain: 4.8,
            max_blocks_per_sec: 12,
            max_packets_per_sec: 80,
            log_file: "anticheat.log".to_string(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub server_name: String,
    pub motd: String,
    pub seed: String,
    pub mode: String,
    pub difficulty: String,
    pub pvp: bool,
    pub max_players: usize,
    pub max_per_ip: usize,
    pub trust_proxy: bool,
    pub save_interval_sec: u64,
    pub ops: Vec<String>,
    pub bans: Vec<String>,
    pub anticheat: AntiCheatConfig,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 25565,
            server_name: "VoxelCraft Survival Server".to_string(),
            motd: "Welcome to VoxelCraft! Press T to chat, /help for commands.".to_string(),
            seed: "voxelcraft-server-2026".to_string(),
            mode: "survival".to_string(),
            difficulty: "normal".to_string(),
            pvp: true,
            max_players: 32,
            max_per_ip: 4,
            trust_proxy: false,
            save_interval_sec: 60,
            ops: Vec::new(),
            bans: Vec::new(),
            anticheat: AntiCheatConfig::default(),
        }
    }
}

fn deep_merge(base: &mut Value, src: &Value) {
    if let (Some(bo), Some(so)) = (base.as_object_mut(), src.as_object()) {
        for (k, v) in so {
            if v.is_object() && bo.get(k).is_some_and(|bv| bv.is_object()) {
                deep_merge(bo.get_mut(k).unwrap(), v);
            } else {
                bo.insert(k.clone(), v.clone());
            }
        }
    }
}

pub fn atomic_write_json<T: Serialize>(path: &Path, data: &T, pretty: bool) -> std::io::Result<()> {
    let text = if pretty {
        serde_json::to_string_pretty(data).map_err(std::io::Error::other)?
    } else {
        serde_json::to_string(data).map_err(std::io::Error::other)?
    };
    let tmp = PathBuf::from(format!("{}.tmp", path.display()));
    fs::write(&tmp, &text)?;
    if fs::rename(&tmp, path).is_err() {
        fs::write(path, &text)?;
        let _ = fs::remove_file(&tmp);
    }
    Ok(())
}

pub fn load_config(opts: &CliOptions) -> ServerConfig {
    let _ = fs::create_dir_all(&opts.data_dir);
    let cfg_path = opts.data_dir.join("server-config.json");
    let mut cfg = ServerConfig::default();

    if cfg_path.exists() {
        match fs::read_to_string(&cfg_path)
            .ok()
            .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        {
            Some(raw) => {
                let mut base = serde_json::to_value(&cfg).unwrap();
                deep_merge(&mut base, &raw);
                if let Ok(parsed) = serde_json::from_value::<ServerConfig>(base) {
                    cfg = parsed;
                }
            }
            None => {
                eprintln!("[Config] Failed to parse server-config.json, using defaults.");
            }
        }
    } else if let Err(e) = atomic_write_json(&cfg_path, &cfg, true) {
        eprintln!("[Config] Could not write default config: {}", e);
    } else {
        println!("[Config] Created default config at {}", cfg_path.display());
    }

    if let Ok(h) = std::env::var("HOST")
        && !h.is_empty() {
            cfg.host = h;
        }
    if let Ok(p) = std::env::var("PORT")
        && let Ok(pn) = p.parse::<u16>()
            && pn > 0 {
                cfg.port = pn;
            }
    if let Some(tp) = env_flag("TRUST_PROXY") {
        cfg.trust_proxy = tp;
    }
    if let Some(h) = &opts.cli_host {
        cfg.host = h.clone();
    }
    if let Some(p) = opts.cli_port
        && p > 0 {
            cfg.port = p;
        }
    cfg
}

pub fn save_config(data_dir: &Path, cfg: &ServerConfig) {
    let cfg_path = data_dir.join("server-config.json");
    if let Err(e) = atomic_write_json(&cfg_path, cfg, true) {
        eprintln!("[Config] Save failed: {}", e);
    }
}
