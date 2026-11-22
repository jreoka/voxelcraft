use std::fs;
use std::path::PathBuf;

use chrono::Utc;
use indexmap::IndexMap;
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use voxelcraft_core::enchants::ItemStack;
use voxelcraft_core::math::{abs_coord_json, abs_delta_key};
use voxelcraft_core::world::{BlockEntity, SharedWorldTables};

use crate::config::{ServerConfig, atomic_write_json};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedLocation {
    pub xi: String,
    pub xf: f64,
    pub y: f64,
    pub zi: String,
    pub zf: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedPlayerProfile {
    pub name: String,
    #[serde(default)]
    pub ox: Value,
    #[serde(default)]
    pub oz: Value,
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub z: Option<f64>,
    pub health: Option<f64>,
    pub hunger: Option<f64>,
    pub saturation: Option<f64>,
    pub xp: Option<i32>,
    pub level: Option<i32>,
    pub dead: Option<bool>,
    #[serde(default)]
    pub slots: Vec<Option<ItemStack>>,
    #[serde(default)]
    pub armor: Vec<Option<ItemStack>>,
    #[serde(default)]
    pub limbo: Vec<ItemStack>,
    pub mode: Option<String>,
    pub last_seen: Option<i64>,
    #[serde(default)]
    pub homes: IndexMap<String, SavedLocation>,
}

pub struct WorldState {
    pub world_path: PathBuf,
    pub seed: String,
    pub day: i32,
    pub time: f64,
    pub weather: String,
    pub weather_timer: f64,
    pub spawn: Option<SavedLocation>,
    pub tables: SharedWorldTables,
    pub saved_players: IndexMap<String, SavedPlayerProfile>,
}

impl WorldState {
    pub fn new(cfg: &ServerConfig, world_path: PathBuf) -> Self {
        Self {
            world_path,
            seed: if cfg.seed.is_empty() {
                "voxelcraft-server-2026".to_string()
            } else {
                cfg.seed.clone()
            },
            day: 1,
            time: 0.28,
            weather: "clear".to_string(),
            weather_timer: 240.0,
            spawn: None,
            tables: SharedWorldTables::default(),
            saved_players: IndexMap::new(),
        }
    }

    pub fn load(&mut self) {
        if self.world_path.exists() {
            match fs::read_to_string(&self.world_path)
                .ok()
                .and_then(|s| serde_json::from_str::<Value>(&s).ok())
            {
                Some(data) => {
                    if let Some(s) = data.get("seed").and_then(|v| v.as_str())
                        && !s.is_empty() {
                            self.seed = s.to_string();
                        }
                    if let Some(d) = data.get("day").and_then(|v| v.as_f64())
                        && d.is_finite() {
                            self.day = d as i32;
                        }
                    if let Some(t) = data.get("time").and_then(|v| v.as_f64())
                        && t.is_finite() {
                            self.time = t;
                        }
                    if let Some(w) = data.get("weather").and_then(|v| v.as_str()) {
                        self.weather = w.to_string();
                    }
                    if let Some(sp) = data.get("spawn")
                        && let Ok(loc) = serde_json::from_value::<SavedLocation>(sp.clone()) {
                            self.spawn = Some(loc);
                        }
                    if let Some(deltas) = data.get("deltas").and_then(|v| v.as_array()) {
                        for d in deltas {
                            if let Some(arr) = d.as_array()
                                && arr.len() >= 4 {
                                    let Some(wx) = abs_coord_json(&arr[0]) else { continue };
                                    let Some(wz) = abs_coord_json(&arr[2]) else { continue };
                                    let wy = arr[1].as_i64().unwrap_or(0) as i32;
                                    let id = arr[3].as_i64().unwrap_or(0) as u8;
                                    let meta = arr.get(4).and_then(|v| v.as_i64()).unwrap_or(0) as u8;
                                    let wx_s = match &wx {
                                        Value::String(s) => s.clone(),
                                        other => other.to_string(),
                                    };
                                    let wz_s = match &wz {
                                        Value::String(s) => s.clone(),
                                        other => other.to_string(),
                                    };
                                    self.tables
                                        .deltas
                                        .insert(format!("{},{},{}", wx_s, wy, wz_s), (wx, wy, wz, id, meta));
                                }
                        }
                    }
                    if let Some(bes) = data.get("blockEntities").and_then(|v| v.as_array()) {
                        for item in bes {
                            if let Some(pair) = item.as_array()
                                && pair.len() >= 2
                                    && let Some(k) = pair[0].as_str()
                                        && let Ok(ent) = serde_json::from_value::<BlockEntity>(pair[1].clone()) {
                                            self.tables.block_entities.insert(k.to_string(), ent);
                                        }
                        }
                    }
                    if let Some(pls) = data.get("players").and_then(|v| v.as_object()) {
                        for (k, v) in pls {
                            if let Ok(prof) = serde_json::from_value::<SavedPlayerProfile>(v.clone()) {
                                self.saved_players.insert(k.to_lowercase(), prof);
                            }
                        }
                    }
                    println!(
                        "[World] Loaded {} block deltas, {} containers, {} player profiles (Seed: {}, Day {}).",
                        self.tables.deltas.len(),
                        self.tables.block_entities.len(),
                        self.saved_players.len(),
                        self.seed,
                        self.day
                    );
                }
                None => {
                    eprintln!("[World] Failed to load world-data.json");
                }
            }
            self.init_modifications();
        } else {
            self.init_modifications();
            self.save();
            println!(
                "[World] Created new server world at {} (Seed: {}).",
                self.world_path.display(),
                self.seed
            );
        }
    }

    fn init_modifications(&mut self) {
        for (wx, wy, wz, id, meta) in self.tables.deltas.values() {
            let (key, idx) = abs_delta_key(wx, *wy, wz);
            self.tables
                .modifications
                .entry(key)
                .or_default()
                .insert(idx, (*id, *meta));
        }
    }

    pub fn save(&mut self) -> bool {
        let deltas: Vec<Value> = self
            .tables
            .deltas
            .values()
            .map(|(wx, wy, wz, id, meta)| json!([wx, wy, wz, id, meta]))
            .collect();
        let block_entities: Vec<Value> = self
            .tables
            .block_entities
            .iter()
            .map(|(k, ent)| json!([k, ent]))
            .collect();
        let time_rounded = (self.time * 10000.0).round() / 10000.0;
        let payload = json!({
            "version": 3,
            "seed": self.seed,
            "day": self.day,
            "time": time_rounded,
            "weather": self.weather,
            "spawn": self.spawn,
            "deltas": deltas,
            "blockEntities": block_entities,
            "players": self.saved_players,
            "savedAt": Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        });
        match atomic_write_json(&self.world_path, &payload, false) {
            Ok(()) => {
                self.tables.dirty = false;
                true
            }
            Err(e) => {
                eprintln!("[World] Save failed: {}", e);
                false
            }
        }
    }

    pub fn update_time(&mut self, dt: f64, day_length: f64) {
        self.time += dt / day_length;
        if self.time >= 1.0 {
            self.time -= 1.0;
            self.day += 1;
        }
        self.weather_timer -= dt;
        if self.weather_timer <= 0.0 {
            let mut rng = rand::thread_rng();
            let r: f64 = rng.r#gen();
            if self.weather == "clear" {
                self.weather = if r < 0.72 {
                    "clear".to_string()
                } else if r < 0.90 {
                    "rain".to_string()
                } else {
                    "thunder".to_string()
                };
                self.weather_timer = 140.0 + rng.r#gen::<f64>() * 200.0;
            } else {
                self.weather = "clear".to_string();
                self.weather_timer = 220.0 + rng.r#gen::<f64>() * 260.0;
            }
        }
    }
}
