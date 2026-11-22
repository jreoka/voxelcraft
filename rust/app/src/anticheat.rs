use std::collections::{HashMap, HashSet};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

use chrono::Utc;
use serde_json::{Value, json};
use voxelcraft_core::enchants::{EnchantEntry, ItemEnchant, is_valid_enchant_name};

use crate::config::AntiCheatConfig;

pub fn now_ms() -> i64 {
    Utc::now().timestamp_millis()
}

#[derive(Clone, Debug)]
pub struct PlayerAC {
    pub vl: f64,
    pub flags: HashMap<String, u32>,
    pub last_move_ms: i64,
    pub last_safe_x: f64,
    pub last_safe_y: f64,
    pub last_safe_z: f64,
    pub last_ground_y: f64,
    pub air_hover_sec: f64,
    pub jump_start_y: f64,
    pub speed_burst: f64,
    pub speed_win_dist: f64,
    pub speed_win_dt: f64,
    pub packet_window_start: i64,
    pub packet_count: usize,
    pub break_timestamps: Vec<i64>,
    pub chat_timestamps: Vec<i64>,
    pub last_pvp_ms: i64,
    pub knockback_until_ms: i64,
    pub slime_bounce_until_ms: i64,
    pub dodge_grace_until_ms: i64,
    pub last_dodge_ms: i64,
    pub mobility_grace_until_ms: i64,
    pub teleport_grace_until_ms: i64,
    pub warned: bool,
}

impl PlayerAC {
    pub fn new(x: f64, y: f64, z: f64) -> Self {
        let now = now_ms();
        Self {
            vl: 0.0,
            flags: HashMap::new(),
            last_move_ms: 0,
            last_safe_x: x,
            last_safe_y: y,
            last_safe_z: z,
            last_ground_y: y,
            air_hover_sec: 0.0,
            jump_start_y: y,
            speed_burst: 0.0,
            speed_win_dist: 0.0,
            speed_win_dt: 0.0,
            packet_window_start: now,
            packet_count: 0,
            break_timestamps: Vec::new(),
            chat_timestamps: Vec::new(),
            last_pvp_ms: 0,
            knockback_until_ms: 0,
            slime_bounce_until_ms: 0,
            dodge_grace_until_ms: 0,
            last_dodge_ms: 0,
            mobility_grace_until_ms: 0,
            teleport_grace_until_ms: now + 2500,
            warned: false,
        }
    }

    pub fn grant_teleport_grace(&mut self, x: f64, y: f64, z: f64, ms: i64) {
        let now = now_ms();
        self.teleport_grace_until_ms = now + ms;
        self.last_safe_x = x;
        self.last_safe_y = y;
        self.last_safe_z = z;
        self.last_ground_y = y;
        self.jump_start_y = y;
        self.air_hover_sec = 0.0;
        self.speed_burst = 0.0;
        self.speed_win_dist = 0.0;
        self.speed_win_dt = 0.0;
        self.last_move_ms = now;
    }
}

pub struct FlagOutcome {
    pub kicked: Option<String>,
    pub warn_player: Option<Value>,
    pub op_notice: Value,
}

#[derive(Default)]
pub struct AntiCheat {
    pub total_flags: u64,
    pub total_kicks: u64,
    pub recent_logs: Vec<String>,
}

impl AntiCheat {
    pub fn new() -> Self {
        Self::default()
    }

    #[expect(clippy::too_many_arguments, reason = "Direct port of JS AntiCheat.flag without global server ref")]
    pub fn flag(
        &mut self,
        cfg: &AntiCheatConfig,
        data_dir: &Path,
        player_name: &str,
        player_ip: &str,
        ac: &mut PlayerAC,
        check: &str,
        points: f64,
        detail: &str,
    ) -> Option<FlagOutcome> {
        if !cfg.enabled {
            return None;
        }
        ac.vl += points;
        *ac.flags.entry(check.to_string()).or_insert(0) += 1;
        self.total_flags += 1;

        let ts = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let line = format!(
            "[{}] [AC] {} ({}) failed {} (+{} -> VL {:.1}): {}",
            ts, player_name, player_ip, check, points, ac.vl, detail
        );
        println!("{}", line);
        self.recent_logs.push(line.clone());
        if self.recent_logs.len() > 100 {
            self.recent_logs.remove(0);
        }

        if !cfg.log_file.is_empty()
            && let Ok(mut f) = OpenOptions::new()
                .create(true)
                .append(true)
                .open(data_dir.join(&cfg.log_file))
            {
                let _ = writeln!(f, "{}", line);
            }

        let op_notice = json!({
            "type": "chat",
            "kind": "warn",
            "text": format!("[AC] {} failed {} (VL {:.1}): {}", player_name, check, ac.vl, detail)
        });

        let warn_vl = if cfg.warn_vl > 0.0 { cfg.warn_vl } else { 8.0 };
        let kick_vl = if cfg.kick_vl > 0.0 { cfg.kick_vl } else { 18.0 };

        if ac.vl >= kick_vl {
            self.total_kicks += 1;
            return Some(FlagOutcome {
                kicked: Some(format!(
                    "Anti-Cheat kicked you ({} · VL {})",
                    check,
                    ac.vl.round() as i64
                )),
                warn_player: None,
                op_notice,
            });
        }

        let warn_player = if ac.vl >= warn_vl && !ac.warned {
            ac.warned = true;
            Some(json!({
                "type": "chat",
                "kind": "warn",
                "text": format!("[Anti-Cheat] Warning: Unusual behavior detected ({}). Continued flags will kick you.", check)
            }))
        } else {
            None
        };

        Some(FlagOutcome {
            kicked: None,
            warn_player,
            op_notice,
        })
    }

    pub fn decay_player(cfg: &AntiCheatConfig, ac: &mut PlayerAC, dt: f64) {
        if !cfg.enabled || ac.vl <= 0.0 {
            return;
        }
        let rate = if cfg.vl_decay_per_sec > 0.0 { cfg.vl_decay_per_sec } else { 0.14 };
        ac.vl = (ac.vl - rate * dt).max(0.0);
        let warn_vl = if cfg.warn_vl > 0.0 { cfg.warn_vl } else { 8.0 };
        if ac.vl < warn_vl * 0.5 {
            ac.warned = false;
        }
    }

    pub fn check_reach(cfg: &AntiCheatConfig, px: f64, py: f64, pz: f64, bx: i32, by: i32, bz: i32) -> (bool, f64, f64) {
        if !cfg.enabled {
            return (true, 0.0, 999.0);
        }
        let eye_y = py + 1.62;
        let dx = (bx as f64 + 0.5) - px;
        let dy = (by as f64 + 0.5) - eye_y;
        let dz = (bz as f64 + 0.5) - pz;
        let dist = (dx * dx + dy * dy + dz * dz).sqrt();
        let max_r = if cfg.max_reach > 0.0 { cfg.max_reach } else { 6.9 };
        (dist <= max_r, dist, max_r)
    }
}

pub fn sanitize_enchant(v: &Value) -> Option<ItemEnchant> {
    let obj = v.as_object()?;
    let raw: Vec<&Value> = match obj.get("list").and_then(|l| l.as_array()) {
        Some(arr) => arr.iter().take(8).collect(),
        None => vec![v],
    };
    let mut list = Vec::new();
    let mut seen = HashSet::new();
    for x in raw {
        let Some(xo) = x.as_object() else { continue };
        let Some(name) = xo.get("name").and_then(|n| n.as_str()) else { continue };
        if !is_valid_enchant_name(name) || seen.contains(name) {
            continue;
        }
        let lvl = xo.get("lvl").and_then(|l| l.as_i64()).unwrap_or(0) as i32;
        if lvl < 1 {
            continue;
        }
        seen.insert(name.to_string());
        list.push(EnchantEntry {
            name: name.to_string(),
            lvl: lvl.min(5),
        });
    }
    if list.is_empty() {
        return None;
    }
    Some(ItemEnchant {
        name: list[0].name.clone(),
        lvl: list[0].lvl,
        list: Some(list),
    })
}
