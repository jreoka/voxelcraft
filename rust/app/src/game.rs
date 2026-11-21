use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use indexmap::IndexMap;
use num_bigint::BigInt;
use num_traits::{ToPrimitive, Zero};
use rand::Rng;
use serde_json::{Value, json};
use tokio::sync::mpsc::UnboundedSender;
use voxelcraft_core::blocks::{B, BLOCKS};
use voxelcraft_core::crafting::CRAFTING;
use voxelcraft_core::day_night::DayNightSystem;
use voxelcraft_core::enchants::{
    ItemStack, compute_block_drops, compute_enchant_offers, compute_enchant_power, ench_kinds_of,
    ench_lvl, make_enchant,
};
use voxelcraft_core::inventory::Inventory;
use voxelcraft_core::items::{IT, ITEMS, TRADER_TRADES, campfire_cook};
use voxelcraft_core::math::{
    CHUNK_SIZE, REBASE_DIST, big_to_num, floor_div, snap_origin, to_big_value,
};
use voxelcraft_core::mobs::{MOB_DEFS, MOB_TYPES, MobEvents, MobManager};
use voxelcraft_core::player::CorePlayer;
use voxelcraft_core::world::{BlockEntity, CoreRedstoneAdapter, CoreWorld, container_type};

use crate::anticheat::{AntiCheat, PlayerAC, now_ms, sanitize_enchant};
use crate::config::ServerConfig;
use crate::ledger::{Ledger, stack_key};
use crate::world_state::{SavedPlayerProfile, WorldState};

const MAX_DROPS: usize = 1500;
const MERGE_DIST: f64 = (1_i64 << 20) as f64;
const JOIN_DIST: f64 = (1_i64 << 26) as f64;
const SPLIT_LOCAL: f64 = (1_i64 << 28) as f64;
const SHARD_CHECK_SEC: f64 = 1.0;
const DROP_LIFETIME_MS: i64 = 240_000;
const LOAD_RADIUS_H: i32 = 3;
const LOAD_RADIUS_V: i32 = 2;

const SELF_DAMAGE_REASONS: &[&str] = &[
    "Fell from a high place.",
    "Stepped on molten Magma.",
    "Pricked to death by a Cactus.",
    "Drowned underwater.",
    "Tried to swim in lava.",
];

const COORD_PACKETS: &[&str] = &[
    "move",
    "enchant",
    "cook",
    "container_open",
    "container_commit",
    "block_break",
    "block_place",
    "bonemeal",
    "block_interact",
    "shoot",
    "drop_item",
];

#[derive(Clone, Debug)]
pub enum WsOut {
    Text(String),
    Ping,
    Close(u16, String),
}

pub fn block_coord(v: Option<&Value>) -> i32 {
    let Some(v) = v else { return 0 };
    if let Some(i) = v.as_i64() {
        return i as i32;
    }
    if let Some(f) = v.as_f64()
        && f.is_finite() {
            let fl = f.floor();
            if fl.abs() <= 9_007_199_254_740_991.0 {
                return fl as i32;
            }
        }
    0
}

fn round_dec(v: f64, mult: f64) -> f64 {
    (v * mult).round() / mult
}

#[derive(Clone, Debug)]
pub struct DropEntity {
    pub id: i32,
    pub s: ItemStack,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub gy: f64,
    pub born: i64,
    pub delay: f64,
}

#[derive(Clone, Debug)]
pub struct TntItem {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub t: f64,
}

#[derive(Clone, Debug)]
pub struct PendingFrame {
    pub seq: u64,
    pub dx: f64,
    pub dz: f64,
    pub far: bool,
}

pub struct PlayerSession {
    pub id: String,
    pub name: String,
    pub ip: String,
    pub tx: UnboundedSender<WsOut>,
    pub shard_id: u64,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub yaw: f64,
    pub pitch: f64,
    pub crouch: bool,
    pub swing: f64,
    pub held_id: i32,
    pub on_ground: bool,
    pub has_moved: bool,
    pub mode: String,
    pub ping: i32,
    pub online: bool,
    pub last_activity: i64,
    pub sel: usize,
    pub pl: CorePlayer,
    pub limbo: Vec<Option<ItemStack>>,
    pub ledger: Ledger,
    pub accept_next_sync: bool,
    pub ac: PlayerAC,
    pub frame_ox: BigInt,
    pub frame_oz: BigInt,
    pub frame_seq: u64,
    pub pending_frames: Vec<PendingFrame>,
    pub last_stats_ms: i64,
    pub last_respawn_ms: i64,
    pub last_eat_ms: i64,
    pub last_break_ms: i64,
    pub last_mob_hit_ms: i64,
    pub last_shot_ms: i64,
}

impl PlayerSession {
    pub fn send_json(&self, val: &Value) {
        if let Ok(s) = serde_json::to_string(val) {
            let _ = self.tx.send(WsOut::Text(s));
        }
    }

    pub fn send_text(&self, raw: String) {
        let _ = self.tx.send(WsOut::Text(raw));
    }

    pub fn close(&self, code: u16, reason: &str) {
        let _ = self.tx.send(WsOut::Close(code, reason.to_string()));
    }

    pub fn is_survival(&self) -> bool {
        self.mode != "creative"
    }

    pub fn eye(&self) -> (f64, f64, f64) {
        (self.x, self.y + if self.crouch { 1.32 } else { 1.6 }, self.z)
    }

    pub fn look(&self) -> (f64, f64, f64) {
        let cp = self.pitch.cos();
        (-self.yaw.sin() * cp, self.pitch.sin(), -self.yaw.cos() * cp)
    }

    pub fn held_stack(&self, claimed_id: i32) -> Option<ItemStack> {
        let s = self.pl.inventory.slots.get(self.sel).and_then(|x| x.as_ref());
        if claimed_id == 0 {
            return s.cloned();
        }
        if let Some(st) = s
            && st.id == claimed_id {
                return Some(st.clone());
            }
        for st in self.pl.inventory.slots.iter().flatten() {
            if st.id == claimed_id {
                return Some(st.clone());
            }
        }
        None
    }

    pub fn set_inventory(
        &mut self,
        slots: Vec<Option<ItemStack>>,
        armor: Vec<Option<ItemStack>>,
        limbo: Vec<Option<ItemStack>>,
    ) {
        self.pl.inventory.slots = slots;
        self.pl.inventory.armor = armor;
        self.pl.inventory.selected = self.sel;
        self.limbo = limbo;
        self.ledger = Ledger::from_stacks(&[
            &self.pl.inventory.slots,
            &self.pl.inventory.armor,
            &self.limbo,
        ]);
    }

    pub fn send_inventory(&self) {
        self.send_json(&json!({
            "type": "inv_set",
            "slots": self.pl.inventory.slots,
            "armor": self.pl.inventory.armor,
            "limbo": self.limbo,
        }));
    }

    pub fn stats_json(&self) -> Value {
        json!({
            "type": "stats",
            "h": round_dec(self.pl.health, 100.0),
            "f": round_dec(self.pl.hunger, 100.0),
            "s": round_dec(self.pl.saturation, 100.0),
            "x": self.pl.xp,
            "l": self.pl.level,
            "d": self.pl.dead,
            "e": self.pl.ench_seed,
        })
    }

    pub fn send_stats(&mut self) {
        self.pl.stats_dirty = false;
        self.last_stats_ms = now_ms();
        let val = self.stats_json();
        self.send_json(&val);
    }
}

pub struct Shard {
    pub id: u64,
    pub ox: BigInt,
    pub oz: BigInt,
    pub core: CoreWorld,
    pub mobs: MobManager,
    pub drops: IndexMap<i32, DropEntity>,
    pub tnt_queue: Vec<TntItem>,
    pub player_ids: Vec<String>,
}

pub struct ServerState {
    pub config: ServerConfig,
    pub data_dir: PathBuf,
    pub client_html_path: PathBuf,
    pub world: WorldState,
    pub ac: AntiCheat,
    pub players: IndexMap<String, PlayerSession>,
    pub shards: IndexMap<u64, Shard>,
    pub home_shard_id: u64,
    pub next_shard_id: u64,
    pub next_player_id: u64,
    pub next_drop_id: i32,
    pub day_night: DayNightSystem,
    pub ip_counts: HashMap<String, usize>,
    pub failed_logins: HashMap<String, (u32, i64)>,
    pub started_at: i64,
    pub save_pending_at: Option<i64>,
    pub stopping: bool,
    snap_accum: f64,
    unload_accum: f64,
    shard_accum: f64,
    crop_accum: f64,
}

impl ServerState {
    pub fn new(config: ServerConfig, data_dir: PathBuf, client_html_path: PathBuf) -> Self {
        let world_path = data_dir.join("world-data.json");
        let mut world = WorldState::new(&config, world_path);
        world.load();
        let home_core = CoreWorld::new(&world.seed, BigInt::zero(), BigInt::zero());
        let home_shard = Shard {
            id: 1,
            ox: BigInt::zero(),
            oz: BigInt::zero(),
            core: home_core,
            mobs: MobManager::new(),
            drops: IndexMap::new(),
            tnt_queue: Vec::new(),
            player_ids: Vec::new(),
        };
        let mut shards = IndexMap::new();
        shards.insert(1, home_shard);
        Self {
            config,
            data_dir,
            client_html_path,
            world,
            ac: AntiCheat::new(),
            players: IndexMap::new(),
            shards,
            home_shard_id: 1,
            next_shard_id: 2,
            next_player_id: 1,
            next_drop_id: 1,
            day_night: DayNightSystem::new(),
            ip_counts: HashMap::new(),
            failed_logins: HashMap::new(),
            started_at: now_ms(),
            save_pending_at: None,
            stopping: false,
            snap_accum: 0.0,
            unload_accum: 0.0,
            shard_accum: 0.0,
            crop_accum: 0.0,
        }
    }

    pub fn is_op(&self, name: &str) -> bool {
        let low = name.to_lowercase();
        self.config.ops.iter().any(|o| o.to_lowercase() == low)
    }

    pub fn is_banned(&self, name: Option<&str>, ip: Option<&str>) -> bool {
        let low_n = name.map(|n| n.to_lowercase());
        let low_i = ip.map(|i| i.to_lowercase());
        self.config.bans.iter().any(|b| {
            let bl = b.to_lowercase();
            low_n.as_deref() == Some(bl.as_str()) || low_i.as_deref() == Some(bl.as_str())
        })
    }

    pub fn find_player_id_by_name(&self, name: &str) -> Option<String> {
        let low = name.to_lowercase();
        self.players
            .values()
            .find(|p| p.name.to_lowercase() == low)
            .map(|p| p.id.clone())
    }

    pub fn broadcast(&self, val: &Value, exclude_id: Option<&str>) {
        if let Ok(raw) = serde_json::to_string(val) {
            for p in self.players.values() {
                if exclude_id != Some(p.id.as_str()) {
                    p.send_text(raw.clone());
                }
            }
        }
    }

    pub fn broadcast_chat(&self, text: &str, kind: &str, sender: Option<&str>) {
        let val = match sender {
            Some(s) => json!({ "type": "chat", "text": text, "kind": kind, "sender": s }),
            None => json!({ "type": "chat", "text": text, "kind": kind, "sender": Value::Null }),
        };
        self.broadcast(&val, None);
        if let Some(s) = sender {
            println!("[Chat] <{}> {}", s, text);
        } else {
            println!("[Server] {}", text);
        }
    }

    pub fn bcast_shard(&self, shard_id: u64, val: &Value, exclude_id: Option<&str>) {
        let Some(shard) = self.shards.get(&shard_id) else { return };
        let Ok(raw) = serde_json::to_string(val) else { return };
        for pid in &shard.player_ids {
            if exclude_id == Some(pid.as_str()) { continue; }
            if let Some(p) = self.players.get(pid) {
                p.send_text(raw.clone());
            }
        }
    }

    pub fn broadcast_near(&self, shard_id: u64, x: f64, z: f64, radius: f64, val: &Value, exclude_id: Option<&str>) {
        let Some(shard) = self.shards.get(&shard_id) else { return };
        let Ok(raw) = serde_json::to_string(val) else { return };
        for pid in &shard.player_ids {
            if exclude_id == Some(pid.as_str()) { continue; }
            if let Some(p) = self.players.get(pid)
                && (p.x - x).abs() <= radius && (p.z - z).abs() <= radius {
                    p.send_text(raw.clone());
                }
        }
    }

    pub fn kick_player(&self, pid: &str, reason: &str) {
        if let Some(p) = self.players.get(pid) {
            println!("[Kick] {}: {}", p.name, reason);
            p.send_json(&json!({ "type": "kick", "reason": reason }));
            p.close(4001, reason);
        }
    }

    pub fn schedule_world_save(&mut self, delay_ms: i64) {
        if self.save_pending_at.is_none() {
            self.save_pending_at = Some(now_ms() + delay_ms);
        }
    }

    pub fn flag_player(&mut self, pid: &str, check: &str, points: f64, detail: &str) -> bool {
        let (pname, pip) = match self.players.get(pid) {
            Some(p) => (p.name.clone(), p.ip.clone()),
            None => return false,
        };
        let outcome = {
            let p = self.players.get_mut(pid).unwrap();
            self.ac.flag(
                &self.config.anticheat,
                &self.data_dir,
                &pname,
                &pip,
                &mut p.ac,
                check,
                points,
                detail,
            )
        };
        let Some(outcome) = outcome else { return false };
        for op in self.players.values() {
            if self.is_op(&op.name) && op.id != pid {
                op.send_json(&outcome.op_notice);
            }
        }
        if let Some(reason) = outcome.kicked {
            self.kick_player(pid, &reason);
            return true;
        }
        if let Some(warn) = outcome.warn_player
            && let Some(p) = self.players.get(pid) {
                p.send_json(&warn);
            }
        false
    }

    pub fn save_player_profile(&mut self, pid: &str) {
        let Some(p) = self.players.get(pid) else { return };
        let Some(shard) = self.shards.get(&p.shard_id) else { return };
        let mut prof = SavedPlayerProfile {
            name: p.name.clone(),
            ox: Value::String(shard.ox.to_string()),
            oz: Value::String(shard.oz.to_string()),
            x: Some(round_dec(p.x, 1000.0)),
            y: Some(round_dec(p.y, 1000.0)),
            z: Some(round_dec(p.z, 1000.0)),
            health: Some(p.pl.health),
            hunger: Some(p.pl.hunger),
            saturation: Some(p.pl.saturation),
            xp: Some(p.pl.xp),
            level: Some(p.pl.level),
            dead: Some(p.pl.dead),
            slots: p.pl.inventory.slots.clone(),
            armor: p.pl.inventory.armor.clone(),
            limbo: p.limbo.iter().flatten().cloned().collect(),
            mode: Some(p.mode.clone()),
            last_seen: Some(now_ms()),
        };
        if !p.has_moved {
            if let Some(old) = self.world.saved_players.get(&p.name.to_lowercase()) {
                prof.ox = old.ox.clone();
                prof.oz = old.oz.clone();
                prof.x = old.x;
                prof.y = old.y;
                prof.z = old.z;
            } else {
                prof.x = None;
            }
        }
        self.world.saved_players.insert(p.name.to_lowercase(), prof);
        self.world.tables.dirty = true;
    }

    pub fn snapshot_online_players(&mut self) {
        let ids: Vec<String> = self.players.keys().cloned().collect();
        for id in ids {
            self.save_player_profile(&id);
        }
    }

    // ---- Shards ----

    pub fn make_shard(&mut self, ox: BigInt, oz: BigInt) -> u64 {
        let id = self.next_shard_id;
        self.next_shard_id += 1;
        let core = CoreWorld::new(&self.world.seed, ox.clone(), oz.clone());
        self.shards.insert(
            id,
            Shard {
                id,
                ox,
                oz,
                core,
                mobs: MobManager::new(),
                drops: IndexMap::new(),
                tnt_queue: Vec::new(),
                player_ids: Vec::new(),
            },
        );
        id
    }

    pub fn find_shard_near(&self, xi: &BigInt, zi: &BigInt, exclude: Option<u64>) -> Option<u64> {
        let mut best = None;
        let mut best_d = JOIN_DIST;
        for s in self.shards.values() {
            if exclude == Some(s.id) { continue; }
            let Some(dx) = big_to_num(&(xi - &s.ox)) else { continue };
            let Some(dz) = big_to_num(&(zi - &s.oz)) else { continue };
            let d = (dx.abs().max(dz.abs())) as f64;
            if d < best_d && (!s.player_ids.is_empty() || s.id == self.home_shard_id) {
                best = Some(s.id);
                best_d = d;
            }
        }
        best
    }

    #[expect(clippy::too_many_arguments, reason = "Matches JS Game.placeAbs signature")]
    pub fn place_abs(
        &mut self,
        pid: &str,
        xi: &BigInt,
        xf: f64,
        y: f64,
        zi: &BigInt,
        zf: f64,
        reason: Option<&str>,
        announce: bool,
    ) {
        let dst = self
            .find_shard_near(xi, zi, None)
            .unwrap_or_else(|| self.make_shard(snap_origin(xi), snap_origin(zi)));
        let (dst_ox, dst_oz) = {
            let s = self.shards.get(&dst).unwrap();
            (s.ox.clone(), s.oz.clone())
        };
        let lx = (xi - &dst_ox).to_f64().unwrap_or(0.0) + xf;
        let lz = (zi - &dst_oz).to_f64().unwrap_or(0.0) + zf;
        self.move_player_to_shard(pid, dst, lx, y, lz, reason, announce);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn move_player_to_shard(
        &mut self,
        pid: &str,
        dst: u64,
        x: f64,
        y: f64,
        z: f64,
        reason: Option<&str>,
        announce: bool,
    ) {
        let src = self.players.get(pid).map(|p| p.shard_id);
        if let Some(sid) = src
            && sid != dst
                && let Some(s) = self.shards.get_mut(&sid) {
                    s.player_ids.retain(|id| id != pid);
                }
        if let Some(ds) = self.shards.get_mut(&dst)
            && !ds.player_ids.iter().any(|id| id == pid) {
                ds.player_ids.push(pid.to_string());
            }
        let (dst_ox, dst_oz) = {
            let ds = self.shards.get(&dst).unwrap();
            (ds.ox.clone(), ds.oz.clone())
        };
        if let Some(p) = self.players.get_mut(pid) {
            p.shard_id = dst;
            p.x = x;
            p.y = y;
            p.z = z;
            p.has_moved = true;
            p.pl.x = x;
            p.pl.y = y;
            p.pl.z = z;
            if !announce {
                p.frame_ox = dst_ox;
                p.frame_oz = dst_oz;
                return;
            }
        }
        self.send_frame(pid);
        if let Some(p) = self.players.get_mut(pid) {
            p.ac.grant_teleport_grace(x, y, z, 2500);
            p.send_json(&json!({ "type": "rubberband", "x": x, "y": y, "z": z, "reason": reason }));
        }
        if src != Some(dst) {
            self.send_shard_state(pid);
        }
    }

    pub fn send_frame(&mut self, pid: &str) {
        let Some(shard_id) = self.players.get(pid).map(|p| p.shard_id) else { return };
        let Some((sox, soz)) = self.shards.get(&shard_id).map(|s| (s.ox.clone(), s.oz.clone())) else { return };
        let Some(p) = self.players.get_mut(pid) else { return };
        if p.frame_ox == sox && p.frame_oz == soz {
            return;
        }
        let dx = big_to_num(&(&sox - &p.frame_ox));
        let dz = big_to_num(&(&soz - &p.frame_oz));
        let far = match (dx, dz) {
            (Some(x), Some(z)) => x.abs() > (1_i64 << 40) || z.abs() > (1_i64 << 40),
            _ => true,
        };
        p.frame_seq += 1;
        let seq = p.frame_seq;
        p.pending_frames.push(PendingFrame {
            seq,
            dx: if far { 0.0 } else { dx.unwrap() as f64 },
            dz: if far { 0.0 } else { dz.unwrap() as f64 },
            far,
        });
        p.frame_ox = sox.clone();
        p.frame_oz = soz.clone();
        p.send_json(&json!({
            "type": "rebase",
            "ox": sox.to_string(),
            "oz": soz.to_string(),
            "seq": seq,
        }));
    }

    pub fn send_shard_state(&self, pid: &str) {
        let Some(p) = self.players.get(pid) else { return };
        let Some(s) = self.shards.get(&p.shard_id) else { return };
        let drops: Vec<Value> = s
            .drops
            .values()
            .map(|d| json!([d.id, d.x, d.y, d.z, d.s]))
            .collect();
        p.send_json(&json!({ "type": "shard_state", "drops": drops }));
    }

    pub fn translate_incoming(&self, pid: &str, msg: &mut Value) -> bool {
        let Some(p) = self.players.get(pid) else { return false };
        if p.pending_frames.is_empty() {
            return true;
        }
        let mtype = msg.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if !COORD_PACKETS.contains(&mtype) {
            return true;
        }
        let (mut dx, mut dz) = (0.0, 0.0);
        for f in &p.pending_frames {
            if f.far {
                return false;
            }
            dx += f.dx;
            dz += f.dz;
        }
        if let Some(obj) = msg.as_object_mut() {
            if let Some(x) = obj.get("x").and_then(|v| v.as_f64()) {
                obj.insert("x".into(), json!(x - dx));
            }
            if let Some(z) = obj.get("z").and_then(|v| v.as_f64()) {
                obj.insert("z".into(), json!(z - dz));
            }
            if let Some(plants) = obj.get_mut("plants").and_then(|v| v.as_array_mut()) {
                for pl in plants {
                    if let Some(arr) = pl.as_array_mut()
                        && arr.len() >= 3 {
                            if let Some(px) = arr[0].as_f64() { arr[0] = json!(px - dx); }
                            if let Some(pz) = arr[2].as_f64() { arr[2] = json!(pz - dz); }
                        }
                }
            }
            if let Some(o) = obj.get_mut("o").and_then(|v| v.as_array_mut())
                && o.len() >= 3 {
                    if let Some(ox) = o[0].as_f64() { o[0] = json!(ox - dx); }
                    if let Some(oz) = o[2].as_f64() { o[2] = json!(oz - dz); }
                }
            if let Some(src) = obj.get_mut("src").and_then(|v| v.as_object_mut()) {
                if let Some(sx) = src.get("x").and_then(|v| v.as_f64()) {
                    src.insert("x".into(), json!(sx - dx));
                }
                if let Some(sz) = src.get("z").and_then(|v| v.as_f64()) {
                    src.insert("z".into(), json!(sz - dz));
                }
            }
        }
        true
    }

    pub fn maintain_shards(&mut self) {
        // 1. Merge nearby shards
        let shard_ids: Vec<u64> = self.shards.keys().copied().collect();
        for &aid in &shard_ids {
            for &bid in &shard_ids {
                if aid == bid || !self.shards.contains_key(&aid) || !self.shards.contains_key(&bid) {
                    continue;
                }
                let (a_empty, b_empty) = (
                    self.shards[&aid].player_ids.is_empty(),
                    self.shards[&bid].player_ids.is_empty(),
                );
                if a_empty || b_empty { continue; }
                let (a_ox, a_oz) = (self.shards[&aid].ox.clone(), self.shards[&aid].oz.clone());
                let (b_ox, b_oz) = (self.shards[&bid].ox.clone(), self.shards[&bid].oz.clone());
                let (Some(ddx), Some(ddz)) = (big_to_num(&(&b_ox - &a_ox)), big_to_num(&(&b_oz - &a_oz))) else {
                    continue;
                };
                let mut near = false;
                for pa_id in &self.shards[&aid].player_ids {
                    let Some(pa) = self.players.get(pa_id) else { continue };
                    for pb_id in &self.shards[&bid].player_ids {
                        let Some(pb) = self.players.get(pb_id) else { continue };
                        let gap = ((ddx as f64) + pb.x - pa.x)
                            .abs()
                            .max(((ddz as f64) + pb.z - pa.z).abs());
                        if gap < MERGE_DIST {
                            near = true;
                            break;
                        }
                    }
                    if near { break; }
                }
                if !near { continue; }
                let (dst, src) = if aid == self.home_shard_id {
                    (aid, bid)
                } else if bid == self.home_shard_id {
                    (bid, aid)
                } else if self.shards[&aid].player_ids.len() >= self.shards[&bid].player_ids.len() {
                    (aid, bid)
                } else {
                    (bid, aid)
                };
                self.merge_shard(src, dst);
            }
        }

        // 2. Recenter & 3. Split
        let shard_ids: Vec<u64> = self.shards.keys().copied().collect();
        for sid in shard_ids {
            let Some(s) = self.shards.get(&sid) else { continue };
            if s.player_ids.is_empty() {
                if sid != self.home_shard_id {
                    self.shards.shift_remove(&sid);
                }
                continue;
            }
            let pids = s.player_ids.clone();
            let (mut cx, mut cz, mut cnt) = (0.0, 0.0, 0.0);
            for pid in &pids {
                if let Some(p) = self.players.get(pid) {
                    cx += p.x;
                    cz += p.z;
                    cnt += 1.0;
                }
            }
            if cnt > 0.0 {
                cx /= cnt;
                cz /= cnt;
            }
            if cx.abs() > REBASE_DIST || cz.abs() > REBASE_DIST {
                let (new_ox, new_oz) = {
                    let s = &self.shards[&sid];
                    (
                        snap_origin(&(&s.ox + BigInt::from(cx.floor() as i64))),
                        snap_origin(&(&s.oz + BigInt::from(cz.floor() as i64))),
                    )
                };
                let r = self.shards.get_mut(&sid).unwrap().core.set_origin(new_ox.clone(), new_oz.clone());
                let s = self.shards.get_mut(&sid).unwrap();
                s.ox = new_ox;
                s.oz = new_oz;
                let (dx, dz) = (r.dx as f64, r.dz as f64);
                s.mobs.shift_local(dx, dz);
                for d in s.drops.values_mut() { d.x -= dx; d.z -= dz; }
                for q in &mut s.tnt_queue { q.x -= r.dx as i32; q.z -= r.dz as i32; }
                let spids = s.player_ids.clone();
                for pid in &spids {
                    if let Some(p) = self.players.get_mut(pid) {
                        p.x -= dx; p.z -= dz;
                        p.pl.x -= dx; p.pl.z -= dz;
                        p.ac.last_safe_x -= dx; p.ac.last_safe_z -= dz;
                    }
                    self.send_frame(pid);
                }
            }

            if self.shards.get(&sid).is_none_or(|s| s.player_ids.len() < 2) {
                continue;
            }
            let spids = self.shards[&sid].player_ids.clone();
            for pid in spids {
                if self.shards[&sid].player_ids.len() < 2 { break; }
                let Some((px, py, pz)) = self.players.get(&pid).map(|p| (p.x, p.y, p.z)) else { continue };
                if px.abs() <= SPLIT_LOCAL && pz.abs() <= SPLIT_LOCAL { continue; }
                let (sox, soz) = (self.shards[&sid].ox.clone(), self.shards[&sid].oz.clone());
                let xi = &sox + BigInt::from(px.floor() as i64);
                let zi = &soz + BigInt::from(pz.floor() as i64);
                let dst = self
                    .find_shard_near(&xi, &zi, Some(sid))
                    .unwrap_or_else(|| self.make_shard(snap_origin(&xi), snap_origin(&zi)));
                let (dox, doz) = (self.shards[&dst].ox.clone(), self.shards[&dst].oz.clone());
                let lx = (&xi - &dox).to_f64().unwrap_or(0.0) + (px - px.floor());
                let lz = (&zi - &doz).to_f64().unwrap_or(0.0) + (pz - pz.floor());
                self.move_player_to_shard(&pid, dst, lx, py, lz, None, true);
            }
        }
    }

    fn merge_shard(&mut self, src_id: u64, dst_id: u64) {
        let Some(src) = self.shards.shift_remove(&src_id) else { return };
        let Some(dst) = self.shards.get_mut(&dst_id) else { return };
        let dx = (&src.ox - &dst.ox).to_f64().unwrap_or(0.0);
        let dz = (&src.oz - &dst.oz).to_f64().unwrap_or(0.0);
        for mut m in src.mobs.mobs { m.x += dx; m.z += dz; dst.mobs.mobs.push(m); }
        for mut pr in src.mobs.projectiles { pr.x += dx; pr.z += dz; dst.mobs.projectiles.push(pr); }
        for (id, mut d) in src.drops { d.x += dx; d.z += dz; dst.drops.insert(id, d); }
        for mut q in src.tnt_queue { q.x += dx as i32; q.z += dz as i32; dst.tnt_queue.push(q); }
        let moved_pids = src.player_ids;
        for pid in &moved_pids {
            dst.player_ids.push(pid.clone());
            if let Some(p) = self.players.get_mut(pid) {
                p.shard_id = dst_id;
                p.x += dx; p.z += dz;
                p.pl.x += dx; p.pl.z += dz;
                p.ac.last_safe_x += dx; p.ac.last_safe_z += dz;
            }
        }
        let all_dst = self.shards[&dst_id].player_ids.clone();
        for pid in &moved_pids {
            self.send_frame(pid);
        }
        for pid in &all_dst {
            self.send_shard_state(pid);
        }
    }

    // ---- Validation helpers ----

    pub fn clean_stack(v: &Value) -> Option<ItemStack> {
        let obj = v.as_object()?;
        let id = obj.get("id").and_then(|x| x.as_i64())? as i32;
        let it = ITEMS.get(id)?;
        let max = it.stack.max(1);
        let count = (obj.get("count").and_then(|x| x.as_i64()).unwrap_or(0) as i32).min(max);
        if count <= 0 {
            return None;
        }
        let dur = if it.max_dur > 0 {
            Some(
                obj.get("dur")
                    .and_then(|d| d.as_f64())
                    .filter(|d| d.is_finite())
                    .map_or(it.max_dur, |d| (d as i32).clamp(0, it.max_dur)),
            )
        } else {
            None
        };
        let enchant = obj
            .get("enchant")
            .and_then(sanitize_enchant)
            .filter(|_| !ench_kinds_of(it).is_empty());
        Some(ItemStack { id, count, dur, enchant })
    }

    pub fn clean_array(v: Option<&Value>, len: usize) -> Vec<Option<ItemStack>> {
        let mut out = vec![None; len];
        if let Some(arr) = v.and_then(|x| x.as_array()) {
            for i in 0..len.min(arr.len()) {
                out[i] = Self::clean_stack(&arr[i]);
            }
        }
        out
    }

    pub fn clean_block_entity(v: Option<&Value>, block_id: u8) -> Option<BlockEntity> {
        let kind = container_type(block_id)?;
        let obj = v?.as_object()?;
        match kind {
            "furnace" => Some(BlockEntity::Furnace {
                input: obj.get("input").and_then(Self::clean_stack),
                fuel: obj.get("fuel").and_then(Self::clean_stack),
                output: obj.get("output").and_then(Self::clean_stack),
                burn_time: 0.0,
                burn_max: 1.0,
                progress: 0.0,
            }),
            "barrel" => Some(BlockEntity::Barrel {
                slots: Self::clean_array(obj.get("slots"), 27),
            }),
            _ => Some(BlockEntity::Chest {
                slots: Self::clean_array(obj.get("slots"), 27),
            }),
        }
    }

    pub fn default_entity(block_id: u8) -> BlockEntity {
        match container_type(block_id) {
            Some("furnace") => BlockEntity::Furnace {
                input: None,
                fuel: None,
                output: None,
                burn_time: 0.0,
                burn_max: 1.0,
                progress: 0.0,
            },
            Some("barrel") => BlockEntity::Barrel { slots: vec![None; 27] },
            _ => BlockEntity::Chest { slots: vec![None; 27] },
        }
    }

    // ---- Drops & Explosions ----

    #[expect(clippy::too_many_arguments, reason = "Matches JS Game.spawnDrop signature")]
    pub fn spawn_drop(
        &mut self,
        shard_id: u64,
        x: f64,
        y: f64,
        z: f64,
        stack: ItemStack,
        vel: Option<(f64, f64, f64)>,
        delay: f64,
    ) {
        let Some(s) = Self::clean_stack(&serde_json::to_value(&stack).unwrap_or(Value::Null)) else {
            return;
        };
        let id = self.next_drop_id;
        self.next_drop_id += 1;
        let mut rng = rand::thread_rng();
        let (vx, vy, vz) = vel.unwrap_or_else(|| {
            (
                (rng.r#gen::<f64>() - 0.5) * 2.4,
                3.2 + rng.r#gen::<f64>() * 1.4,
                (rng.r#gen::<f64>() - 0.5) * 2.4,
            )
        });
        let gy = {
            let (bx, bz) = (x.floor() as i32, z.floor() as i32);
            let mut res = y;
            if let Some(sh) = self.shards.get_mut(&shard_id) {
                let start = y.floor() as i32;
                for yy in ((start - 47)..=start).rev() {
                    if BLOCKS.get(sh.core.get_block_ensured(bx, yy - 1, bz, &mut self.world.tables)).solid {
                        res = yy as f64;
                        break;
                    }
                }
            }
            res
        };
        let d = DropEntity {
            id,
            s: s.clone(),
            x: round_dec(x, 100.0),
            y: round_dec(y, 100.0),
            z: round_dec(z, 100.0),
            gy,
            born: now_ms(),
            delay,
        };
        let mut oldest_removed = None;
        if let Some(sh) = self.shards.get_mut(&shard_id) {
            sh.drops.insert(id, d.clone());
            if sh.drops.len() > MAX_DROPS
                && let Some((old_id, _)) = sh.drops.shift_remove_index(0) {
                    oldest_removed = Some(old_id);
                }
        }
        if let Some(old_id) = oldest_removed {
            self.bcast_shard(shard_id, &json!({ "type": "drop_remove", "id": old_id }), None);
        }
        self.bcast_shard(
            shard_id,
            &json!({
                "type": "drop_spawn",
                "id": id,
                "x": d.x,
                "y": d.y,
                "z": d.z,
                "vx": round_dec(vx, 100.0),
                "vy": round_dec(vy, 100.0),
                "vz": round_dec(vz, 100.0),
                "s": s,
                "delay": delay,
            }),
            None,
        );
    }

    #[expect(clippy::too_many_arguments, reason = "Matches JS Game.explode signature")]
    pub fn explode(&mut self, shard_id: u64, x: i32, y: i32, z: i32, radius: f64, is_tnt: bool, by_pid: Option<&str>) {
        if !self.shards.contains_key(&shard_id) {
            return;
        }
        if is_tnt {
            let cur = self.shards.get_mut(&shard_id).unwrap().core.get_block_ensured(x, y, z, &mut self.world.tables);
            if cur != B::TNT {
                return;
            }
            self.shards.get_mut(&shard_id).unwrap().core.set_block(x, y, z, B::AIR, 0, true, &mut self.world.tables, None);
        }
        let mut cells: Vec<[i32; 3]> = Vec::new();
        let mut drops_to_spawn: Vec<(f64, f64, f64, ItemStack)> = Vec::new();
        let r = radius.ceil() as i32;
        let r2 = radius * radius;
        let mut rng = rand::thread_rng();

        for dy in -r..=r {
            for dz in -r..=r {
                for dx in -r..=r {
                    if (dx == 0 && dy == 0 && dz == 0) || ((dx * dx + dy * dy + dz * dz) as f64) > r2 {
                        continue;
                    }
                    let (bx, by, bz) = (x + dx, y + dy, z + dz);
                    let id = self.shards.get_mut(&shard_id).unwrap().core.get_block_ensured(bx, by, bz, &mut self.world.tables);
                    if matches!(id, B::AIR | B::BEDROCK | B::OBSIDIAN | B::OBSIDIAN_CRYING) {
                        continue;
                    }
                    if id == B::TNT {
                        let sh = self.shards.get_mut(&shard_id).unwrap();
                        if sh.tnt_queue.len() <= 512 && !sh.tnt_queue.iter().any(|q| q.x == bx && q.y == by && q.z == bz) {
                            sh.tnt_queue.push(TntItem { x: bx, y: by, z: bz, t: 0.35 + rng.r#gen::<f64>() * 0.4 });
                        }
                        continue;
                    }
                    let abs_k = self.shards[&shard_id].core.abs_block_key(bx, by, bz);
                    if let Some(ent) = self.world.tables.block_entities.get(&abs_k) {
                        for st in ent.stacks().into_iter().flatten() {
                            drops_to_spawn.push((bx as f64 + 0.5, by as f64 + 0.4, bz as f64 + 0.5, st.clone()));
                        }
                    }
                    cells.push([dx, dy, dz]);
                    self.shards.get_mut(&shard_id).unwrap().core.set_block(bx, by, bz, B::AIR, 0, true, &mut self.world.tables, None);
                }
            }
        }
        for (dx, dy, dz, st) in drops_to_spawn {
            self.spawn_drop(shard_id, dx, dy, dz, st, None, 0.4);
        }
        self.bcast_shard(
            shard_id,
            &json!({ "type": "explosion", "x": x, "y": y, "z": z, "radius": (radius * 10.0).round() as i32, "cells": cells }),
            None,
        );
        self.schedule_world_save(5000);

        let (cx, cy, cz) = (x as f64 + 0.5, y as f64 + 0.5, z as f64 + 0.5);
        let pids = self.shards[&shard_id].player_ids.clone();
        for pid in pids {
            let Some((mode, dead, px, py, pz)) = self.players.get(&pid).map(|p| (p.mode.clone(), p.pl.dead, p.x, p.y, p.z)) else {
                continue;
            };
            if mode == "creative" || dead { continue; }
            let (pdx, pdy, pdz) = (px - cx, (py + 0.9) - cy, pz - cz);
            let dist = (pdx * pdx + pdy * pdy + pdz * pdz).sqrt().max(0.1);
            if dist >= radius + 3.5 { continue; }
            let k = 1.0 - dist / (radius + 3.5);
            let kx = (pdx / dist) * k * 14.0;
            let ky = ((pdy / dist) * k * 10.0 + 5.5).max(4.5);
            let kz = (pdz / dist) * k * 14.0;
            self.hurt_player(&pid, (k * 12.0).round(), "Blew up in an explosion.", kx, ky, kz, false);
        }

        let mut mob_events = MobEvents::default();
        if let Some(sh) = self.shards.get_mut(&shard_id) {
            for idx in 0..sh.mobs.mobs.len() {
                if sh.mobs.mobs[idx].dying { continue; }
                let (mx, my, mz) = (sh.mobs.mobs[idx].x, sh.mobs.mobs[idx].y, sh.mobs.mobs[idx].z);
                let (mdx, mdy, mdz) = (mx - cx, (my + 0.6) - cy, mz - cz);
                let md = (mdx * mdx + mdy * mdy + mdz * mdz).sqrt().max(0.1);
                if md < radius + 4.0 {
                    let mk = 1.0 - md / (radius + 4.0);
                    sh.mobs.damage_mob_by_idx(
                        idx,
                        (mk * 16.0).round(),
                        (mdx / md) * mk * 12.0,
                        6.5,
                        (mdz / md) * mk * 12.0,
                        by_pid,
                        false,
                        false,
                        &mut mob_events,
                    );
                }
            }
        }
        self.apply_mob_events(shard_id, mob_events);
    }

    #[expect(clippy::too_many_arguments, reason = "Matches JS ServerPlayer.hurt signature")]
    pub fn hurt_player(&mut self, pid: &str, dmg: f64, reason: &str, kx: f64, ky: f64, kz: f64, self_reported: bool) -> bool {
        let (landed, dead, pname) = {
            let Some(p) = self.players.get_mut(pid) else { return false };
            if p.pl.dead || !p.online {
                return false;
            }
            let landed = p.pl.hurt(dmg, reason, kx, ky, kz);
            if landed {
                p.ac.knockback_until_ms = now_ms() + 900;
                if !self_reported {
                    p.send_json(&json!({
                        "type": "damage",
                        "h": round_dec(p.pl.health, 100.0),
                        "kx": round_dec(kx, 100.0),
                        "ky": round_dec(ky, 100.0),
                        "kz": round_dec(kz, 100.0),
                        "r": reason,
                    }));
                }
                p.send_stats();
            }
            (landed, p.pl.dead, p.name.clone())
        };
        if landed && dead {
            let msg = if reason.is_empty() {
                format!("{}: died.", pname)
            } else {
                format!("{}: {}", pname, reason)
            };
            self.broadcast_chat(&msg, "sys", None);
        }
        landed
    }

    fn apply_mob_events(&mut self, shard_id: u64, events: MobEvents) {
        for (x, y, z, id, count) in events.drops {
            self.spawn_drop(shard_id, x, y, z, ItemStack { id, count, dur: None, enchant: None }, None, 0.4);
        }
        for (pid, xp) in events.xp_awards {
            if let Some(p) = self.players.get_mut(&pid) {
                p.pl.add_xp(xp);
            }
        }
        for (x, y, z, vx, vy, vz, exclude) in events.arrows {
            self.broadcast_near(
                shard_id,
                x,
                z,
                96.0,
                &json!({
                    "type": "arrow",
                    "x": round_dec(x, 100.0),
                    "y": round_dec(y, 100.0),
                    "z": round_dec(z, 100.0),
                    "vx": round_dec(vx, 100.0),
                    "vy": round_dec(vy, 100.0),
                    "vz": round_dec(vz, 100.0),
                }),
                exclude.as_deref(),
            );
        }
        for (pid, dmg, reason, kx, ky, kz) in events.player_hurts {
            self.hurt_player(&pid, dmg, &reason, kx, ky, kz, false);
        }
        for (x, y, z, r) in events.explosions {
            self.explode(shard_id, x, y, z, r, false, None);
        }
    }

    // ---- Tick Loops ----

    pub fn send_player_snapshots(&self) {
        if self.players.is_empty() {
            return;
        }
        for shard in self.shards.values() {
            if shard.player_ids.is_empty() {
                continue;
            }
            let mut list = Vec::with_capacity(shard.player_ids.len());
            for pid in &shard.player_ids {
                if let Some(p) = self.players.get(pid) {
                    list.push(json!({
                        "id": p.id,
                        "name": p.name,
                        "x": round_dec(p.x, 1000.0),
                        "y": round_dec(p.y, 1000.0),
                        "z": round_dec(p.z, 1000.0),
                        "yaw": round_dec(p.yaw, 1000.0),
                        "pitch": round_dec(p.pitch, 1000.0),
                        "crouch": p.crouch,
                        "swing": p.swing,
                        "ping": p.ping,
                        "mode": p.mode,
                        "op": self.is_op(&p.name),
                    }));
                }
            }
            if let Ok(raw) = serde_json::to_string(&json!({ "type": "players", "list": list })) {
                for pid in &shard.player_ids {
                    if let Some(p) = self.players.get(pid) {
                        p.send_text(raw.clone());
                    }
                }
            }
        }
    }

    pub fn tick(&mut self, dt: f64) {
        // Check debounced save
        if let Some(due) = self.save_pending_at
            && now_ms() >= due {
                self.save_pending_at = None;
                self.snapshot_online_players();
                if self.world.tables.dirty && self.world.save() {
                    println!(
                        "[Save] Auto-saved map ({} deltas, {} containers).",
                        self.world.tables.deltas.len(),
                        self.world.tables.block_entities.len()
                    );
                }
            }

        self.shard_accum += dt;
        if self.shard_accum >= SHARD_CHECK_SEC {
            self.shard_accum = 0.0;
            self.maintain_shards();
        }
        self.unload_accum += dt;
        let unload_now = self.unload_accum > 10.0;
        if unload_now { self.unload_accum = 0.0; }
        self.snap_accum += dt;
        let snap_now = self.snap_accum >= 0.1;
        if snap_now { self.snap_accum = 0.0; }
        self.crop_accum += dt;
        let crop_now = self.crop_accum >= 0.35;
        if crop_now { self.crop_accum = 0.0; }

        let mut furnaces_done = HashSet::new();
        let shard_ids: Vec<u64> = self.shards.keys().copied().collect();
        for sid in shard_ids {
            let active = self.shards.get(&sid).is_some_and(|s| !s.player_ids.is_empty() || sid == self.home_shard_id);
            if !active { continue; }
            self.tick_shard(sid, dt, unload_now, snap_now, crop_now, &mut furnaces_done);
        }
        // Unloaded furnaces still smelt
        if let Some(home) = self.shards.get_mut(&self.home_shard_id) {
            home.core.update_furnaces(dt, false, &mut furnaces_done, &mut self.world.tables, None);
        }
    }

    fn tick_shard(
        &mut self,
        shard_id: u64,
        dt: f64,
        unload_now: bool,
        snap_now: bool,
        crop_now: bool,
        furnaces_done: &mut HashSet<String>,
    ) {
        let pids = self.shards[&shard_id].player_ids.clone();
        let positions: Vec<(f64, f64, f64)> = pids
            .iter()
            .filter_map(|id| self.players.get(id).map(|p| (p.x, p.y, p.z)))
            .collect();

        // 1. Chunk loading & unloading
        {
            let sh = self.shards.get_mut(&shard_id).unwrap();
            let mut budget = 8;
            for &(px, py, pz) in &positions {
                let pcx = floor_div(px.floor() as i32, CHUNK_SIZE);
                let pcy = floor_div(py.floor() as i32, CHUNK_SIZE);
                let pcz = floor_div(pz.floor() as i32, CHUNK_SIZE);
                'outer: for r in 0..=LOAD_RADIUS_H {
                    for dx in -r..=r {
                        for dz in -r..=r {
                            if dx.abs().max(dz.abs()) != r { continue; }
                            for dy in -LOAD_RADIUS_V..=LOAD_RADIUS_V {
                                if budget <= 0 { break 'outer; }
                                let ck = (pcx + dx, pcy + dy, pcz + dz);
                                if !sh.core.chunks.contains_key(&ck) {
                                    sh.core.ensure_chunk(ck.0, ck.1, ck.2, &mut self.world.tables);
                                    budget -= 1;
                                }
                            }
                        }
                    }
                }
            }
            if unload_now {
                sh.core.unload_far(&positions);
                sh.core.relight_dirty();
            }
        }

        // 2. Redstone, furnaces, TNT, gravity, crops
        let mut change_log: Vec<(i32, i32, i32, u8, u8)> = Vec::new();
        let tnt_triggers: Vec<(i32, i32, i32)>;
        {
            let sh = self.shards.get_mut(&shard_id).unwrap();
            let mut rs = std::mem::take(&mut sh.core.redstone);
            {
                let mut adapter = CoreRedstoneAdapter {
                    world: &mut sh.core,
                    tables: &mut self.world.tables,
                    change_log: &mut change_log,
                    tnt_triggers: Vec::new(),
                };
                rs.update(&mut adapter, dt);
                tnt_triggers = adapter.tnt_triggers;
            }
            sh.core.redstone = rs;

            let burning = self.world.tables.block_entities.values().any(|e| e.is_furnace_burning());
            sh.core.update_furnaces(dt, true, furnaces_done, &mut self.world.tables, Some(&mut change_log));
            if burning {
                self.world.tables.dirty = true;
            }
            sh.core.process_gravity(&mut self.world.tables, Some(&mut change_log));
            if crop_now {
                let mut rng = rand::thread_rng();
                for &(px, py, pz) in &positions {
                    let (ipx, ipy, ipz) = (px.floor() as i32, py.floor() as i32, pz.floor() as i32);
                    for _ in 0..14 {
                        let wx = ipx + ((rng.r#gen::<f64>() - 0.5) * 28.0).floor() as i32;
                        let wz = ipz + ((rng.r#gen::<f64>() - 0.5) * 28.0).floor() as i32;
                        let wy = ipy + ((rng.r#gen::<f64>() - 0.5) * 14.0).floor() as i32;
                        let id = sh.core.get_block_ensured(wx, wy, wz, &mut self.world.tables);
                        if id == B::FARMLAND {
                            let mut near_w = false;
                            'wcheck: for dz in -4..=4 {
                                for dx in -4..=4 {
                                    if sh.core.get_block_ensured(wx + dx, wy, wz + dz, &mut self.world.tables) == B::WATER {
                                        near_w = true;
                                        break 'wcheck;
                                    }
                                }
                            }
                            if near_w && (sh.core.get_meta(wx, wy, wz) & 1) == 0 {
                                sh.core.set_block(wx, wy, wz, B::FARMLAND, 1, true, &mut self.world.tables, Some(&mut change_log));
                            }
                        } else if id == B::WHEAT {
                            let m = sh.core.get_meta(wx, wy, wz);
                            if m < 3 && rng.r#gen::<f64>() < 0.28 {
                                let wet = sh.core.get_block_ensured(wx, wy - 1, wz, &mut self.world.tables) == B::FARMLAND
                                    && (sh.core.get_meta(wx, wy - 1, wz) & 1) != 0;
                                if wet || rng.r#gen::<f64>() < 0.4 {
                                    sh.core.set_block(wx, wy, wz, B::WHEAT, m + 1, true, &mut self.world.tables, Some(&mut change_log));
                                }
                            }
                        }
                    }
                }
            }
        }
        if !change_log.is_empty() {
            let list: Vec<[i32; 5]> = change_log.iter().map(|&(x, y, z, id, m)| [x, y, z, id as i32, m as i32]).collect();
            self.bcast_shard(shard_id, &json!({ "type": "blocks", "list": list }), None);
            self.schedule_world_save(5000);
        }
        for (tx, ty, tz) in tnt_triggers {
            self.explode(shard_id, tx, ty, tz, 3.2, true, None);
        }
        // Chained TNT queue
        let mut ready_tnt = Vec::new();
        if let Some(sh) = self.shards.get_mut(&shard_id) {
            let mut idx = sh.tnt_queue.len();
            while idx > 0 {
                idx -= 1;
                sh.tnt_queue[idx].t -= dt;
                if sh.tnt_queue[idx].t <= 0.0 {
                    let q = sh.tnt_queue.remove(idx);
                    ready_tnt.push((q.x, q.y, q.z));
                }
            }
        }
        for (tx, ty, tz) in ready_tnt {
            self.explode(shard_id, tx, ty, tz, 3.2, true, None);
        }

        // 3. Mobs
        self.day_night.time = self.world.time;
        self.day_night.update(0.0);
        let mut mob_events = MobEvents::default();
        {
            let diff = self.config.difficulty.clone();
            let dn = self.day_night.clone();
            let pls: Vec<(String, &CorePlayer)> = pids
                .iter()
                .filter_map(|id| self.players.get(id).map(|p| (id.clone(), &p.pl)))
                .collect();
            if let Some(sh) = self.shards.get_mut(&shard_id) {
                if !pls.is_empty() {
                    sh.mobs.update(dt, &mut sh.core, &pls, &dn, &diff, &mut mob_events);
                } else {
                    sh.mobs.mobs.clear();
                }
            }
        }
        self.apply_mob_events(shard_id, mob_events);

        // 4. Player vitals
        let now = now_ms();
        let mut starve_hurts = Vec::new();
        for pid in &pids {
            if let Some(p) = self.players.get_mut(pid) {
                if p.pl.hurt_cooldown > 0.0 {
                    p.pl.hurt_cooldown -= dt;
                }
                if !p.pl.dead && p.mode == "survival" {
                    if p.pl.hunger >= 17.0 && p.pl.health < p.pl.max_health {
                        p.pl.regen_accum += dt;
                        if p.pl.regen_accum >= 2.2 {
                            p.pl.regen_accum = 0.0;
                            p.pl.health = (p.pl.health + 1.0).min(p.pl.max_health);
                            p.pl.hunger = (p.pl.hunger - 0.35).max(0.0);
                            p.pl.stats_dirty = true;
                        }
                    } else if p.pl.hunger <= 0.0 {
                        p.pl.starve_accum += dt;
                        if p.pl.starve_accum >= 3.5 {
                            p.pl.starve_accum = 0.0;
                            starve_hurts.push(pid.clone());
                        }
                    } else {
                        p.pl.regen_accum = 0.0;
                    }
                }
                if p.pl.stats_dirty && now - p.last_stats_ms > 200 {
                    p.send_stats();
                }
            }
        }
        for pid in starve_hurts {
            self.hurt_player(&pid, 1.0, "Starved to death.", 0.0, 0.0, 0.0, true);
        }

        // 5. Expired drops
        let mut expired = Vec::new();
        if let Some(sh) = self.shards.get_mut(&shard_id) {
            sh.drops.retain(|&id, d| {
                if now - d.born > DROP_LIFETIME_MS {
                    expired.push(id);
                    false
                } else {
                    true
                }
            });
        }
        for id in expired {
            self.bcast_shard(shard_id, &json!({ "type": "drop_remove", "id": id }), None);
        }

        // 6. Mob snapshots @ 10 Hz
        if snap_now
            && let Some(sh) = self.shards.get(&shard_id) {
                for pid in &pids {
                    let Some(p) = self.players.get(pid) else { continue };
                    let mut list = Vec::new();
                    for m in &sh.mobs.mobs {
                        if (m.x - p.x).abs() > 90.0 || (m.z - p.z).abs() > 90.0 { continue; }
                        let flags = (if m.hurt_timer > 0.0 { 1 } else { 0 })
                            | (if m.dying { 2 } else { 0 })
                            | (if m.baby { 4 } else { 0 })
                            | (if m.arctic { 8 } else { 0 })
                            | (if m.fire_timer > 0.0 { 16 } else { 0 });
                        list.push(json!([
                            m.id,
                            m.type_idx,
                            round_dec(m.x, 100.0),
                            round_dec(m.y, 100.0),
                            round_dec(m.z, 100.0),
                            round_dec(m.yaw, 100.0),
                            round_dec(m.head_yaw, 100.0),
                            round_dec(m.head_pitch, 100.0),
                            flags,
                            round_dec(m.fuse, 100.0),
                            m.wool_idx,
                        ]));
                    }
                    p.send_json(&json!({ "type": "mobs", "list": list }));
                }
            }
    }

    // ---- Packet Handling ----

    fn revert_block(&mut self, pid: &str, shard_id: u64, x: i32, y: i32, z: i32, reason: Option<&str>) {
        let (id, meta) = {
            let sh = self.shards.get_mut(&shard_id).unwrap();
            let id = sh.core.get_block_ensured(x, y, z, &mut self.world.tables);
            let meta = sh.core.get_meta(x, y, z);
            (id, meta)
        };
        if let Some(p) = self.players.get(pid) {
            p.send_json(&json!({
                "type": "block_revert",
                "x": x, "y": y, "z": z,
                "id": id, "meta": meta,
                "reason": reason,
            }));
        }
    }

    fn rubberband(&mut self, pid: &str, reason: &str) {
        if let Some(p) = self.players.get_mut(pid) {
            p.x = p.ac.last_safe_x;
            p.y = p.ac.last_safe_y;
            p.z = p.ac.last_safe_z;
            p.ac.last_move_ms = now_ms();
            p.ac.teleport_grace_until_ms = now_ms() + 700;
            p.send_json(&json!({ "type": "rubberband", "x": p.x, "y": p.y, "z": p.z, "reason": reason }));
        }
    }

    pub fn handle_move(&mut self, pid: &str, msg: &Value) {
        let Some(shard_id) = self.players.get(pid).map(|p| p.shard_id) else { return };
        let x = msg.get("x").and_then(|v| v.as_f64()).unwrap_or(f64::NAN);
        let y = msg.get("y").and_then(|v| v.as_f64()).unwrap_or(f64::NAN);
        let z = msg.get("z").and_then(|v| v.as_f64()).unwrap_or(f64::NAN);
        let yaw = msg.get("yaw").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let pitch = msg.get("pitch").and_then(|v| v.as_f64()).unwrap_or(0.0);

        if !x.is_finite() || !y.is_finite() || !z.is_finite() || !yaw.is_finite() || !pitch.is_finite()
            || x.abs() > ((1_i64 << 35) as f64) || z.abs() > ((1_i64 << 35) as f64) || y < -64.0 || y > 320.0
        {
            self.rubberband(pid, "Invalid coordinates detected.");
            self.flag_player(pid, "BAD_COORDS", 6.0, &format!("Invalid coordinates ({}, {}, {})", x, y, z));
            return;
        }

        let now = now_ms();
        let on_ground = msg.get("onGround").and_then(|v| v.as_bool()).unwrap_or(false);
        let in_water = msg.get("inWater").and_then(|v| v.as_bool()).unwrap_or(false);
        let on_ladder = msg.get("onLadder").and_then(|v| v.as_bool()).unwrap_or(false);
        let flying = msg.get("flying").and_then(|v| v.as_bool()).unwrap_or(false);

        let has_moved = self.players[pid].has_moved;
        if !has_moved {
            let p = self.players.get_mut(pid).unwrap();
            p.has_moved = true;
            p.ac.last_safe_x = x; p.ac.last_safe_y = y; p.ac.last_safe_z = z;
            p.ac.last_ground_y = y; p.ac.jump_start_y = y; p.ac.last_move_ms = now;
            Self::accept_move(p, msg, x, y, z, yaw, pitch);
            return;
        }

        if !self.config.anticheat.enabled {
            let p = self.players.get_mut(pid).unwrap();
            p.ac.last_move_ms = now;
            Self::accept_move(p, msg, x, y, z, yaw, pitch);
            return;
        }

        let on_slime = self.shards.get_mut(&shard_id).is_some_and(|sh| {
            sh.core.get_block_ensured(x.floor() as i32, (y - 0.35).floor() as i32, z.floor() as i32, &mut self.world.tables) == B::SLIME_BLOCK
        });

        let ac_cfg = self.config.anticheat.clone();
        let violation: Option<(&str, f64, String, String)> = {
            let p = self.players.get_mut(pid).unwrap();
            let dt = (((now - p.ac.last_move_ms) as f64) / 1000.0).clamp(0.02, 1.5);
            let (dx, dy, dz) = (x - p.x, y - p.y, z - p.z);
            let horiz_dist = dx.hypot(dz);
            let total_dist = (dx * dx + dy * dy + dz * dz).sqrt();
            let is_creative = p.mode == "creative";
            let in_grace = now < p.ac.teleport_grace_until_ms;
            let in_kb = now < p.ac.knockback_until_ms;
            if on_slime {
                p.ac.slime_bounce_until_ms = now + 2600;
            }
            let in_slime = now < p.ac.slime_bounce_until_ms;

            if !is_creative && flying {
                Some(("ILLEGAL_FLY", 5.0, "Flight is not allowed in Survival mode.".into(), "Client reported flying=true in survival".into()))
            } else {
                let max_tp = if is_creative { ac_cfg.max_teleport_dist * 1.8 } else { ac_cfg.max_teleport_dist };
                if !in_grace && total_dist > max_tp {
                    Some(("TELEPORT", 5.0, format!("Teleport/Blink detected ({:.1}m).", total_dist), format!("Moved {:.1}m in {:.0}ms", total_dist, dt * 1000.0)))
                } else {
                    let mut v = None;
                    if !in_grace {
                        let base_max = if is_creative { ac_cfg.max_creative_speed } else { ac_cfg.max_horiz_speed };
                        let max_speed = if in_kb { base_max * 1.65 } else { base_max };
                        let horiz_speed = horiz_dist / dt;
                        if horiz_speed > max_speed && horiz_dist > 0.45 {
                            p.ac.speed_burst += (horiz_speed - max_speed) * dt;
                            if p.ac.speed_burst > 1.8 || horiz_speed > max_speed * 1.9 {
                                p.ac.speed_burst = 0.0;
                                v = Some(("SPEED", 4.0, format!("Speed hack detected ({:.1} m/s).", horiz_speed), format!("Horiz speed {:.1} m/s (max {:.1})", horiz_speed, max_speed)));
                            }
                        } else {
                            p.ac.speed_burst = (p.ac.speed_burst - 2.4 * dt).max(0.0);
                        }
                    }
                    if v.is_none() {
                        if !is_creative && !in_grace {
                            let supported = on_ground || in_water || on_ladder;
                            if supported {
                                p.ac.air_hover_sec = 0.0;
                                p.ac.last_ground_y = y;
                                p.ac.jump_start_y = y;
                                p.ac.last_safe_x = x; p.ac.last_safe_y = y; p.ac.last_safe_z = z;
                            } else {
                                if dy >= -0.04 {
                                    p.ac.air_hover_sec += dt;
                                } else {
                                    p.ac.air_hover_sec = (p.ac.air_hover_sec - dt * 0.7).max(0.0);
                                }
                                let jump_gain = y - p.ac.jump_start_y;
                                let max_jump = if in_slime { 22.0 } else { ac_cfg.max_jump_gain };
                                if jump_gain > max_jump {
                                    p.ac.air_hover_sec = 0.0;
                                    v = Some(("HIGH_JUMP", 4.5, format!("Fly / High-Jump detected (+{:.1}m).", jump_gain), format!("Ascended +{:.2}m without ground/ladder/water", jump_gain)));
                                } else {
                                    let max_hover = if in_slime { 5.0 } else { ac_cfg.max_air_hover_sec };
                                    if p.ac.air_hover_sec > max_hover {
                                        let hs = p.ac.air_hover_sec;
                                        p.ac.air_hover_sec = 0.0;
                                        v = Some(("FLY_HOVER", 4.5, "Fly / Hover detected.".into(), format!("Airborne non-falling for {:.2}s", hs)));
                                    }
                                }
                            }
                        } else if on_ground || is_creative {
                            p.ac.last_safe_x = x; p.ac.last_safe_y = y; p.ac.last_safe_z = z;
                            p.ac.last_ground_y = y; p.ac.jump_start_y = y;
                            p.ac.air_hover_sec = 0.0;
                        }
                    }
                    if v.is_none() {
                        p.ac.last_move_ms = now;
                        Self::accept_move(p, msg, x, y, z, yaw, pitch);
                    }
                    v
                }
            }
        };

        if let Some((check, pts, rb_reason, detail)) = violation {
            self.rubberband(pid, &rb_reason);
            self.flag_player(pid, check, pts, &detail);
        }
    }

    fn accept_move(p: &mut PlayerSession, msg: &Value, x: f64, y: f64, z: f64, yaw: f64, pitch: f64) {
        p.x = x; p.y = y; p.z = z;
        p.yaw = yaw;
        p.pitch = pitch.clamp(-std::f64::consts::FRAC_PI_2, std::f64::consts::FRAC_PI_2);
        p.crouch = msg.get("crouch").and_then(|v| v.as_bool()).unwrap_or(false);
        let swing = msg.get("swing").and_then(|v| v.as_f64()).unwrap_or(0.0);
        p.swing = if swing.is_finite() { swing.clamp(0.0, 1.0) } else { 0.0 };
        p.held_id = msg.get("heldId").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
        p.on_ground = msg.get("onGround").and_then(|v| v.as_bool()).unwrap_or(false);

        p.pl.x = p.x; p.pl.y = p.y; p.pl.z = p.z;
        p.pl.yaw = p.yaw; p.pl.pitch = p.pitch;
        p.pl.crouching = p.crouch;
        p.pl.sprinting = msg.get("sprint").and_then(|v| v.as_bool()).unwrap_or(false);
        p.pl.blocking = msg.get("block").and_then(|v| v.as_bool()).unwrap_or(false);
        p.pl.on_ground = p.on_ground;
        if let Some(sel) = msg.get("sel").and_then(|v| v.as_u64())
            && sel < 9 {
                p.sel = sel as usize;
                p.pl.inventory.selected = p.sel;
            }
        if let Some(f) = msg.get("f").and_then(|v| v.as_f64())
            && f.is_finite() && f < p.pl.hunger {
                p.pl.hunger = f.max(0.0);
            }
        if let Some(s) = msg.get("s").and_then(|v| v.as_f64())
            && s.is_finite() && s < p.pl.saturation {
                p.pl.saturation = s.max(0.0);
            }
    }

    pub fn handle_game_packet(&mut self, pid: &str, msg: &Value) {
        let Some(mtype) = msg.get("type").and_then(|v| v.as_str()) else { return };
        if mtype.starts_with('_') {
            return;
        }
        match mtype {
            "inv_sync" => self.handle_inv_sync(pid, msg),
            "pickup" => self.handle_pickup(pid, msg),
            "drop_item" => self.handle_drop_item(pid, msg),
            "self_damage" => self.handle_self_damage(pid, msg),
            "respawn" => self.handle_respawn(pid, msg),
            "eat" => self.handle_eat(pid, msg),
            "craft" => self.handle_craft(pid, msg),
            "trade" => self.handle_trade(pid, msg),
            "enchant" => self.handle_enchant(pid, msg),
            "cook" => self.handle_cook(pid, msg),
            "container_open" => self.handle_container_open(pid, msg),
            "container_commit" => self.handle_container_commit(pid, msg),
            "block_break" => self.handle_block_break(pid, msg),
            "block_place" => self.handle_block_place(pid, msg),
            "bonemeal" => self.handle_bonemeal(pid, msg),
            "block_interact" => self.handle_block_interact(pid, msg),
            "mob_hit" => self.handle_mob_hit(pid, msg),
            "feed" => self.handle_feed(pid, msg),
            "shoot" => self.handle_shoot(pid, msg),
            "pvp_hit" => self.handle_pvp_hit(pid, msg),
            _ => {}
        }
    }

    fn handle_inv_sync(&mut self, pid: &str, msg: &Value) {
        let mut slots = Self::clean_array(msg.get("slots"), 36);
        let mut armor = Self::clean_array(msg.get("armor"), 5);
        let mut limbo = Self::clean_array(msg.get("limbo"), 12);
        for (i, armor_slot) in armor.iter_mut().enumerate().take(5) {
            if let Some(a) = armor_slot {
                let valid = ITEMS.get(a.id).is_some_and(|it| it.armor_slot == Some(i));
                if !valid {
                    if let Some(free) = slots.iter().position(|s| s.is_none()) {
                        slots[free] = armor_slot.take();
                    } else {
                        *armor_slot = None;
                    }
                }
            }
        }
        let changed = {
            let Some(p) = self.players.get_mut(pid) else { return };
            let mut ch = false;
            if p.is_survival() && !p.accept_next_sync {
                ch = p.ledger.clamp(&mut [&mut slots, &mut armor, &mut limbo]);
            }
            p.accept_next_sync = false;
            p.set_inventory(slots, armor, limbo);
            if ch {
                p.send_inventory();
            }
            ch
        };
        if changed {
            self.flag_player(pid, "INV_MISMATCH", 1.5, "Inventory contained items the server never granted (corrected)");
        }
        self.save_player_profile(pid);
    }

    fn handle_pickup(&mut self, pid: &str, msg: &Value) {
        let drop_id = msg.get("id").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
        let Some((shard_id, dead, px, py, pz)) = self.players.get(pid).map(|p| (p.shard_id, p.pl.dead, p.x, p.y, p.z)) else { return };
        let drop = self.shards.get(&shard_id).and_then(|s| s.drops.get(&drop_id).cloned());
        let Some(d) = drop else {
            self.players[pid].send_json(&json!({ "type": "drop_remove", "id": drop_id }));
            return;
        };
        if dead {
            self.players[pid].send_json(&json!({ "type": "drop_remove", "id": drop_id }));
            return;
        }
        let age = ((now_ms() - d.born) as f64) / 1000.0;
        let dy = py - d.gy;
        if age < d.delay - 0.25 || (px - d.x).hypot(pz - d.z) > 6.0 || !(-6.0..=6.0).contains(&dy) {
            self.players[pid].send_json(&json!({ "type": "pickup_fail", "id": d.id }));
            return;
        }
        self.shards.get_mut(&shard_id).unwrap().drops.shift_remove(&d.id);
        let p = self.players.get_mut(pid).unwrap();
        if p.is_survival() {
            p.ledger.add(&d.s);
        }
        p.send_json(&json!({ "type": "pickup_ok", "id": d.id, "s": d.s }));
        self.bcast_shard(shard_id, &json!({ "type": "drop_remove", "id": d.id, "by": pid }), Some(pid));
    }

    fn handle_drop_item(&mut self, pid: &str, msg: &Value) {
        let Some(mut s) = msg.get("s").and_then(Self::clean_stack) else { return };
        let Some((shard_id, dead, survival, px, py, pz, eye, dir)) = self.players.get(pid).map(|p| {
            (p.shard_id, p.pl.dead, p.is_survival(), p.x, p.y, p.z, p.eye(), p.look())
        }) else { return };
        if dead { return; }

        if let Some(src) = msg.get("src").and_then(|v| v.as_object()) {
            if let Some(sx) = src.get("x").and_then(|v| v.as_i64()) {
                let (sx, sy, sz) = (sx as i32, block_coord(src.get("y")), block_coord(src.get("z")));
                if !AntiCheat::check_reach(&self.config.anticheat, px, py, pz, sx, sy, sz).0 {
                    return;
                }
                let abs_k = self.shards[&shard_id].core.abs_block_key(sx, sy, sz);
                let Some(ent) = self.world.tables.block_entities.get_mut(&abs_k) else { return };
                let k = stack_key(&s);
                let mut left = s.count;
                let take = |slot: &mut Option<ItemStack>, left: &mut i32| {
                    if *left <= 0 { return; }
                    if let Some(st) = slot
                        && stack_key(st) == k {
                            let t = st.count.min(*left);
                            st.count -= t;
                            *left -= t;
                            if st.count <= 0 { *slot = None; }
                        }
                };
                match ent {
                    BlockEntity::Furnace { input, fuel, output, .. } => {
                        take(input, &mut left);
                        take(fuel, &mut left);
                        take(output, &mut left);
                    }
                    BlockEntity::Chest { slots } | BlockEntity::Barrel { slots } => {
                        for sl in slots.iter_mut() { take(sl, &mut left); }
                    }
                }
                if left == s.count { return; }
                s.count -= left;
                self.world.tables.dirty = true;
            }
        } else if survival {
            let p = self.players.get_mut(pid).unwrap();
            let Some(removed) = p.ledger.remove(&stack_key(&s), s.count, s.dur) else {
                p.send_inventory();
                return;
            };
            if let Some(&d) = removed.first() {
                s.dur = Some(d);
            }
        }
        self.spawn_drop(
            shard_id,
            eye.0 + dir.0 * 0.6,
            eye.1 - 0.3,
            eye.2 + dir.2 * 0.6,
            s,
            Some((dir.0 * 5.5, dir.1 * 5.0 + 2.2, dir.2 * 5.5)),
            1.5,
        );
    }

    fn handle_self_damage(&mut self, pid: &str, msg: &Value) {
        let Some(p) = self.players.get(pid) else { return };
        if !p.is_survival() || p.pl.dead { return; }
        let reason = msg.get("r").and_then(|v| v.as_str()).unwrap_or("");
        if !SELF_DAMAGE_REASONS.contains(&reason) { return; }
        let amt = msg.get("a").and_then(|v| v.as_f64()).unwrap_or(0.0);
        if !amt.is_finite() || amt <= 0.0 { return; }
        self.hurt_player(pid, amt.min(40.0), reason, 0.0, 0.0, 0.0, true);
    }

    fn handle_respawn(&mut self, pid: &str, msg: &Value) {
        let now = now_ms();
        let was_dead = {
            let Some(p) = self.players.get_mut(pid) else { return };
            if p.pl.dead {
                p.pl.dead = false;
                p.pl.health = p.pl.max_health;
                p.pl.hunger = p.pl.max_hunger;
                p.pl.saturation = 5.0;
                p.pl.stats_dirty = true;
                p.ac.grant_teleport_grace(p.x, p.y, p.z, 2500);
                p.send_stats();
                true
            } else {
                if now - p.last_respawn_ms < 10_000 { return; }
                p.last_respawn_ms = now;
                p.ac.grant_teleport_grace(p.x, p.y, p.z, 2500);
                false
            }
        };
        if was_dead
            && let Some(to) = msg.get("to").and_then(|v| v.as_object()) {
                let (Some(tx), Some(ty), Some(tz)) = (
                    to.get("x").and_then(|v| v.as_f64()).filter(|v| v.is_finite()),
                    to.get("y").and_then(|v| v.as_f64()).filter(|v| v.is_finite()),
                    to.get("z").and_then(|v| v.as_f64()).filter(|v| v.is_finite()),
                ) else { return };
                let ox = to.get("ox").and_then(|v| to_big_value(v).ok()).unwrap_or_default();
                let oz = to.get("oz").and_then(|v| to_big_value(v).ok()).unwrap_or_default();
                let xi = ox + BigInt::from(tx.floor() as i64);
                let zi = oz + BigInt::from(tz.floor() as i64);
                self.place_abs(pid, &xi, tx - tx.floor(), ty.clamp(-64.0, 320.0), &zi, tz - tz.floor(), None, true);
            }
    }

    fn handle_eat(&mut self, pid: &str, msg: &Value) {
        let id = msg.get("id").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
        let Some(it) = ITEMS.get(id) else { return };
        if it.food <= 0.0 { return; }
        let now = now_ms();
        let Some(p) = self.players.get_mut(pid) else { return };
        if p.pl.dead || now - p.last_eat_ms < 900 { return; }
        p.last_eat_ms = now;
        if p.is_survival() && p.ledger.remove(&format!("{}#", it.id), 1, None).is_none() {
            p.send_inventory();
            return;
        }
        p.pl.hunger = (p.pl.hunger + it.food).min(p.pl.max_hunger);
        p.pl.saturation = (p.pl.saturation + if it.sat > 0.0 { it.sat } else { 2.0 }).min(20.0);
        if it.heal > 0.0 {
            p.pl.health = (p.pl.health + it.heal).min(p.pl.max_health);
        }
        p.send_stats();
    }

    fn near_block(&mut self, shard_id: u64, px: f64, py: f64, pz: f64, ids: &[u8], r: i32) -> bool {
        let (bx, by, bz) = (px.floor() as i32, py.floor() as i32, pz.floor() as i32);
        let Some(sh) = self.shards.get_mut(&shard_id) else { return false };
        for dy in -r..=r {
            for dz in -r..=r {
                for dx in -r..=r {
                    if ids.contains(&sh.core.get_block_ensured(bx + dx, by + dy, bz + dz, &mut self.world.tables)) {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn handle_craft(&mut self, pid: &str, msg: &Value) {
        let r_idx = msg.get("r").and_then(|v| v.as_i64()).unwrap_or(-1);
        if r_idx < 0 || (r_idx as usize) >= CRAFTING.recipes.len() { return; }
        let r = &CRAFTING.recipes[r_idx as usize];
        let times = (msg.get("times").and_then(|v| v.as_i64()).unwrap_or(1) as i32).clamp(1, 64);
        let Some((shard_id, px, py, pz, survival)) = self.players.get(pid).map(|p| (p.shard_id, p.x, p.y, p.z, p.is_survival())) else { return };

        if r.needs_table && !self.near_block(shard_id, px, py, pz, &[B::CRAFTING_TABLE, B::WORKSTATION], 6) {
            self.flag_player(pid, "CRAFT", 2.0, &format!("Crafted #{} without a crafting table nearby", r.out_id));
            if let Some(p) = self.players.get(pid) { p.send_inventory(); }
            return;
        }
        if !survival { return; }

        let mut used: Vec<(i32, i32)> = Vec::new();
        if let Some(arr) = msg.get("used").and_then(|v| v.as_array()) {
            for u in arr.iter().take(16) {
                if let Some(ua) = u.as_array()
                    && ua.len() >= 2 {
                        let uid = ua[0].as_i64().unwrap_or(0) as i32;
                        let ucnt = ua[1].as_i64().unwrap_or(0) as i32;
                        if ITEMS.get(uid).is_some() && ucnt > 0 {
                            used.push((uid, ucnt));
                        }
                    }
            }
        }
        let mut pool = used.clone();
        for &(req, need) in &r.ingredients {
            let mut left = need * times;
            for u in &mut pool {
                if left <= 0 { break; }
                if u.1 > 0 && CRAFTING.matches_ingredient(u.0, req) {
                    let t = u.1.min(left);
                    u.1 -= t;
                    left -= t;
                }
            }
            if left > 0 {
                self.players[pid].send_inventory();
                return;
            }
        }
        if pool.iter().any(|u| u.1 > 0) {
            self.players[pid].send_inventory();
            return;
        }
        let p = self.players.get_mut(pid).unwrap();
        for &(id, cnt) in &used {
            if p.ledger.count(&format!("{}#", id)) < cnt {
                p.send_inventory();
                return;
            }
        }
        for &(id, cnt) in &used {
            p.ledger.remove(&format!("{}#", id), cnt, None);
        }
        p.ledger.add(&ItemStack { id: r.out_id, count: r.out_count * times, dur: None, enchant: None });
    }

    fn handle_trade(&mut self, pid: &str, msg: &Value) {
        let idx = msg.get("idx").and_then(|v| v.as_i64()).unwrap_or(-1);
        if idx < 0 || (idx as usize) >= TRADER_TRADES.len() { return; }
        let tr = &TRADER_TRADES[idx as usize];
        let Some((shard_id, px, pz, survival)) = self.players.get(pid).map(|p| (p.shard_id, p.x, p.z, p.is_survival())) else { return };
        let near = self.shards.get(&shard_id).is_some_and(|sh| {
            sh.mobs.mobs.iter().any(|m| m.kind == "trader" && !m.dying && (m.x - px).hypot(m.z - pz) < 7.0)
        });
        if !near {
            self.players[pid].send_inventory();
            return;
        }
        if !survival { return; }
        let p = self.players.get_mut(pid).unwrap();
        if p.ledger.remove(&format!("{}#", tr.give.0), tr.give.1, None).is_none() {
            p.send_inventory();
            return;
        }
        p.ledger.add(&ItemStack { id: tr.get.0, count: tr.get.1, dur: None, enchant: None });
    }

    fn handle_enchant(&mut self, pid: &str, msg: &Value) {
        let (x, y, z) = (block_coord(msg.get("x")), block_coord(msg.get("y")), block_coord(msg.get("z")));
        let Some((shard_id, px, py, pz, ench_seed)) = self.players.get(pid).map(|p| (p.shard_id, p.x, p.y, p.z, p.pl.ench_seed)) else { return };
        let blk = self.shards.get_mut(&shard_id).unwrap().core.get_block_ensured(x, y, z, &mut self.world.tables);
        if blk != B::ENCHANT_TABLE || !AntiCheat::check_reach(&self.config.anticheat, px, py, pz, x, y, z).0 {
            self.players[pid].send_inventory();
            return;
        }
        let item_id = msg.get("itemId").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
        let power = {
            let sh = &self.shards[&shard_id];
            compute_enchant_power(|bx, by, bz| sh.core.get_block(bx, by, bz), x, y, z)
        };
        let offers = compute_enchant_offers(item_id, false, power, ench_seed);
        let offer_idx = msg.get("offer").and_then(|v| v.as_i64()).unwrap_or(-1);
        let Some(off) = (offer_idx >= 0).then(|| offers.get(offer_idx as usize)).flatten() else {
            let p = self.players.get_mut(pid).unwrap();
            p.send_inventory();
            p.send_stats();
            return;
        };
        if off.list.is_empty() {
            let p = self.players.get_mut(pid).unwrap();
            p.send_inventory();
            p.send_stats();
            return;
        }
        let p = self.players.get_mut(pid).unwrap();
        if p.is_survival() {
            if p.pl.level < off.cost
                || p.ledger.count(&format!("{}#", IT::LAPIS)) < off.lapis
                || p.ledger.count(&format!("{}#", item_id)) < 1
            {
                p.send_inventory();
                p.send_stats();
                return;
            }
            let dur_hint = msg.get("dur").and_then(|v| v.as_f64()).filter(|d| d.is_finite()).map(|d| d as i32);
            let durs = p.ledger.remove(&format!("{}#", item_id), 1, dur_hint);
            p.ledger.remove(&format!("{}#", IT::LAPIS), off.lapis, None);
            p.ledger.add(&ItemStack {
                id: item_id,
                count: 1,
                dur: durs.and_then(|v| v.first().copied()),
                enchant: Some(make_enchant(&off.list)),
            });
            p.pl.level = (p.pl.level - off.lapis).max(0);
        }
        p.pl.ench_seed = rand::thread_rng().r#gen();
        p.send_stats();
    }

    fn handle_cook(&mut self, pid: &str, msg: &Value) {
        let (x, y, z) = (block_coord(msg.get("x")), block_coord(msg.get("y")), block_coord(msg.get("z")));
        let in_id = msg.get("id").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
        let Some(out_id) = campfire_cook(in_id) else {
            self.players[pid].send_inventory();
            return;
        };
        let Some((shard_id, px, py, pz, survival)) = self.players.get(pid).map(|p| (p.shard_id, p.x, p.y, p.z, p.is_survival())) else { return };
        let blk = self.shards.get_mut(&shard_id).unwrap().core.get_block_ensured(x, y, z, &mut self.world.tables);
        if blk != B::CAMPFIRE || !AntiCheat::check_reach(&self.config.anticheat, px, py, pz, x, y, z).0 {
            self.players[pid].send_inventory();
            return;
        }
        if !survival { return; }
        let p = self.players.get_mut(pid).unwrap();
        if p.ledger.remove(&format!("{}#", in_id), 1, None).is_none() {
            p.send_inventory();
            return;
        }
        p.ledger.add(&ItemStack { id: out_id, count: 1, dur: None, enchant: None });
    }

    fn handle_container_open(&mut self, pid: &str, msg: &Value) {
        let (x, y, z) = (block_coord(msg.get("x")), block_coord(msg.get("y")), block_coord(msg.get("z")));
        let Some((shard_id, px, py, pz)) = self.players.get(pid).map(|p| (p.shard_id, p.x, p.y, p.z)) else { return };
        let id = self.shards.get_mut(&shard_id).unwrap().core.get_block_ensured(x, y, z, &mut self.world.tables);
        if container_type(id).is_none() || !AntiCheat::check_reach(&self.config.anticheat, px, py, pz, x, y, z).0 {
            return;
        }
        let abs_k = self.shards[&shard_id].core.abs_block_key(x, y, z);
        let ent = self
            .world
            .tables
            .block_entities
            .entry(abs_k)
            .or_insert_with(|| {
                self.world.tables.dirty = true;
                Self::default_entity(id)
            })
            .clone();
        self.players[pid].send_json(&json!({ "type": "block_entity", "x": x, "y": y, "z": z, "ent": ent, "open": true }));
    }

    fn handle_container_commit(&mut self, pid: &str, msg: &Value) {
        let (x, y, z) = (block_coord(msg.get("x")), block_coord(msg.get("y")), block_coord(msg.get("z")));
        let Some((shard_id, px, py, pz)) = self.players.get(pid).map(|p| (p.shard_id, p.x, p.y, p.z)) else { return };
        let id = self.shards.get_mut(&shard_id).unwrap().core.get_block_ensured(x, y, z, &mut self.world.tables);
        let abs_k = self.shards[&shard_id].core.abs_block_key(x, y, z);
        let old_ent = self
            .world
            .tables
            .block_entities
            .get(&abs_k)
            .cloned()
            .or_else(|| container_type(id).map(|_| Self::default_entity(id)));
        let (_, dist, max_r) = AntiCheat::check_reach(&self.config.anticheat, px, py, pz, x, y, z);
        let new_ent = Self::clean_block_entity(msg.get("ent"), id);
        let (Some(old_ent), Some(mut new_ent)) = (old_ent, new_ent) else {
            self.handle_inv_sync(pid, msg);
            return;
        };
        if dist > max_r + 2.0 {
            self.handle_inv_sync(pid, msg);
            return;
        }

        let mut slots = Self::clean_array(msg.get("slots"), 36);
        let mut armor = Self::clean_array(msg.get("armor"), 5);
        let mut limbo = Self::clean_array(msg.get("limbo"), 12);
        let mut new_ent_stacks: Vec<Option<ItemStack>> = match &new_ent {
            BlockEntity::Furnace { input, fuel, output, .. } => vec![input.clone(), fuel.clone(), output.clone()],
            BlockEntity::Chest { slots: s } | BlockEntity::Barrel { slots: s } => s.clone(),
        };

        let changed = {
            let p = self.players.get_mut(pid).unwrap();
            let mut ch = false;
            if p.is_survival() && !p.accept_next_sync {
                let old_stacks: Vec<Option<ItemStack>> = old_ent.stacks().into_iter().map(|s| s.cloned()).collect();
                let mut pool = Ledger::from_stacks(&[&old_stacks]);
                for e in p.ledger.m.values() {
                    let md = Ledger::max_dur(e.proto_id);
                    if md > 0 {
                        for &d in &e.durs {
                            pool.add(&ItemStack { id: e.proto_id, count: 1, dur: Some(d), enchant: e.proto_enchant.clone() });
                        }
                    } else if e.count > 0 {
                        pool.add(&ItemStack { id: e.proto_id, count: e.count, dur: None, enchant: e.proto_enchant.clone() });
                    }
                }
                ch = pool.clamp(&mut [&mut slots, &mut armor, &mut limbo, &mut new_ent_stacks]);
            }
            p.accept_next_sync = false;
            p.set_inventory(slots, armor, limbo);
            ch
        };

        match &mut new_ent {
            BlockEntity::Furnace { input, fuel, output, burn_time, burn_max, progress } => {
                *input = new_ent_stacks.first().cloned().flatten();
                *fuel = new_ent_stacks.get(1).cloned().flatten();
                *output = new_ent_stacks.get(2).cloned().flatten();
                if let BlockEntity::Furnace { burn_time: obt, burn_max: obm, progress: opr, .. } = old_ent {
                    *burn_time = obt;
                    *burn_max = obm;
                    *progress = opr;
                }
            }
            BlockEntity::Chest { slots: s } | BlockEntity::Barrel { slots: s } => {
                *s = new_ent_stacks;
            }
        }
        self.world.tables.block_entities.insert(abs_k, new_ent.clone());
        self.world.tables.dirty = true;
        self.bcast_shard(shard_id, &json!({ "type": "block_entity", "x": x, "y": y, "z": z, "ent": new_ent }), Some(pid));
        if changed {
            self.flag_player(pid, "INV_MISMATCH", 1.5, &format!("Container transfer at ({},{},{}) created items (corrected)", x, y, z));
            if let Some(p) = self.players.get(pid) {
                p.send_inventory();
                p.send_json(&json!({ "type": "block_entity", "x": x, "y": y, "z": z, "ent": new_ent }));
            }
        }
        self.save_player_profile(pid);
        self.schedule_world_save(5000);
    }

    fn handle_block_break(&mut self, pid: &str, msg: &Value) {
        let (x, y, z) = (block_coord(msg.get("x")), block_coord(msg.get("y")), block_coord(msg.get("z")));
        let Some((shard_id, dead, creative, px, py, pz)) = self.players.get(pid).map(|p| {
            (p.shard_id, p.pl.dead, p.mode == "creative", p.x, p.y, p.z)
        }) else { return };
        if dead {
            self.revert_block(pid, shard_id, x, y, z, None);
            return;
        }
        let (id, meta) = {
            let sh = self.shards.get_mut(&shard_id).unwrap();
            let id = sh.core.get_block_ensured(x, y, z, &mut self.world.tables);
            let meta = sh.core.get_meta(x, y, z);
            (id, meta)
        };
        if msg.get("bucket").and_then(|v| v.as_bool()).unwrap_or(false) {
            self.handle_bucket_fill(pid, shard_id, x, y, z, id, meta);
            return;
        }
        if id == B::AIR { return; }
        let blk = BLOCKS.get(id);
        let tool_id = msg.get("toolId").and_then(|v| v.as_i64()).unwrap_or(0) as i32;

        if self.config.anticheat.enabled {
            let (ok, dist, max_r) = AntiCheat::check_reach(&self.config.anticheat, px, py, pz, x, y, z);
            if !ok {
                self.revert_block(pid, shard_id, x, y, z, Some(&format!("Reach too far ({:.1}m).", dist)));
                self.flag_player(pid, "REACH", 4.0, &format!("Block break reach {:.1}m > {}m", dist, max_r));
                return;
            }
            if !creative {
                if blk.hardness >= 9999.0 || blk.liquid {
                    self.revert_block(pid, shard_id, x, y, z, Some(&format!("Cannot break {}.", blk.name)));
                    self.flag_player(pid, "UNBREAKABLE", 6.0, &format!("Tried to break {} at ({},{},{})", blk.name, x, y, z));
                    return;
                }
                let now = now_ms();
                let max_bps = self.config.anticheat.max_blocks_per_sec.max(1);
                let bps = {
                    let p = self.players.get_mut(pid).unwrap();
                    p.ac.break_timestamps.retain(|&t| now - t < 1000);
                    p.ac.break_timestamps.push(now);
                    p.ac.break_timestamps.len()
                };
                if bps > max_bps {
                    self.revert_block(pid, shard_id, x, y, z, Some("Breaking blocks too fast (Nuker)."));
                    self.flag_player(pid, "NUKER", 5.0, &format!("Broke {} blocks/sec", bps));
                    return;
                }
                let held = self.players[pid].held_stack(tool_id);
                let hit = held.as_ref().and_then(|h| ITEMS.get(h.id));
                let mut speed = 1.0;
                if let Some(h_it) = hit {
                    if h_it.tool_type == Some(blk.tool) {
                        let base_s = if h_it.speed > 0.0 { h_it.speed } else { 2.0 };
                        speed = base_s * (1.0 + (ench_lvl(held.as_ref(), "Efficiency") as f64) * 0.32);
                    } else if blk.min_tier > 0 {
                        speed = 0.28;
                    }
                } else if blk.min_tier > 0 {
                    speed = 0.28;
                }
                let mine_ms = (blk.hardness / speed).max(0.06) * 1000.0;
                let min_ms = mine_ms * 0.55 - 80.0;
                let gap = {
                    let p = self.players.get_mut(pid).unwrap();
                    let g = (now - p.last_break_ms) as f64;
                    p.last_break_ms = now;
                    g
                };
                let reported_ms = msg.get("mineMs").and_then(|v| v.as_f64()).unwrap_or(0.0);
                if mine_ms > 250.0 && (gap < min_ms || reported_ms < min_ms) {
                    self.revert_block(pid, shard_id, x, y, z, Some(&format!("Fast-Break detected on {}.", blk.name)));
                    self.flag_player(pid, "FAST_BREAK", 4.5, &format!("Broke {} after {:.0}ms (min {}ms)", blk.name, gap.min(reported_ms), min_ms.round() as i64));
                    return;
                }
            }
        }

        let abs_k = self.shards[&shard_id].core.abs_block_key(x, y, z);
        let ent = self.world.tables.block_entities.get(&abs_k).cloned();
        self.shards.get_mut(&shard_id).unwrap().core.set_block(x, y, z, B::AIR, 0, true, &mut self.world.tables, None);
        self.bcast_shard(shard_id, &json!({ "type": "block_set", "x": x, "y": y, "z": z, "id": B::AIR, "meta": 0 }), Some(pid));

        if blk.shape == "door" {
            let py2 = if (meta & 8) != 0 { y - 1 } else { y + 1 };
            let other = self.shards.get_mut(&shard_id).unwrap().core.get_block_ensured(x, py2, z, &mut self.world.tables);
            if other == id {
                self.shards.get_mut(&shard_id).unwrap().core.set_block(x, py2, z, B::AIR, 0, true, &mut self.world.tables, None);
                self.bcast_shard(shard_id, &json!({ "type": "block_set", "x": x, "y": py2, "z": z, "id": B::AIR, "meta": 0 }), Some(pid));
            }
        }
        if let Some(ent) = ent {
            for s in ent.stacks().into_iter().flatten() {
                self.spawn_drop(shard_id, x as f64 + 0.5, y as f64 + 0.4, z as f64 + 0.5, s.clone(), None, 0.4);
            }
        }
        if !creative {
            let held = self.players[pid].held_stack(tool_id);
            let mut rng = rand::thread_rng();
            let res = compute_block_drops(blk, meta, held.as_ref(), || rng.r#gen());
            for d in res.drops {
                self.spawn_drop(shard_id, x as f64 + 0.5, y as f64 + 0.4, z as f64 + 0.5, d, None, 0.4);
            }
            if res.xp > 0
                && let Some(p) = self.players.get_mut(pid) {
                    p.pl.add_xp(res.xp);
                }
        }
        self.schedule_world_save(5000);
    }

    #[expect(clippy::too_many_arguments, reason = "Matches JS HANDLERS._bucketFill signature")]
    fn handle_bucket_fill(&mut self, pid: &str, shard_id: u64, x: i32, y: i32, z: i32, id: u8, meta: u8) {
        let Some((px, py, pz, survival)) = self.players.get(pid).map(|p| (p.x, p.y, p.z, p.is_survival())) else { return };
        if (id != B::WATER && id != B::LAVA) || (meta & 7) != 0 || !AntiCheat::check_reach(&self.config.anticheat, px, py, pz, x, y, z).0 {
            self.revert_block(pid, shard_id, x, y, z, None);
            if survival { self.players[pid].send_inventory(); }
            return;
        }
        if survival {
            let p = self.players.get_mut(pid).unwrap();
            if p.ledger.remove(&format!("{}#", IT::BUCKET), 1, None).is_none() {
                p.send_inventory();
                self.revert_block(pid, shard_id, x, y, z, None);
                return;
            }
            let filled = if id == B::WATER { IT::WATER_BUCKET } else { IT::LAVA_BUCKET };
            p.ledger.add(&ItemStack { id: filled, count: 1, dur: None, enchant: None });
        }
        self.shards.get_mut(&shard_id).unwrap().core.set_block(x, y, z, B::AIR, 0, true, &mut self.world.tables, None);
        self.bcast_shard(shard_id, &json!({ "type": "block_set", "x": x, "y": y, "z": z, "id": B::AIR, "meta": 0 }), Some(pid));
        self.schedule_world_save(5000);
    }

    fn handle_block_place(&mut self, pid: &str, msg: &Value) {
        let (x, y, z) = (block_coord(msg.get("x")), block_coord(msg.get("y")), block_coord(msg.get("z")));
        let id_i = msg.get("id").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
        let meta = (msg.get("meta").and_then(|v| v.as_i64()).unwrap_or(0) & 255) as u8;
        let cause = msg.get("cause").and_then(|v| v.as_str()).unwrap_or("");
        let Some((shard_id, dead, creative, px, py, pz)) = self.players.get(pid).map(|p| {
            (p.shard_id, p.pl.dead, p.mode == "creative", p.x, p.y, p.z)
        }) else { return };

        let Some(blk) = BLOCKS.get_opt(id_i) else {
            self.revert_block(pid, shard_id, x, y, z, None);
            return;
        };
        let id = blk.id;
        if dead || id == B::AIR {
            self.revert_block(pid, shard_id, x, y, z, None);
            return;
        }
        let (ok, dist, max_r) = AntiCheat::check_reach(&self.config.anticheat, px, py, pz, x, y, z);
        if !ok {
            self.revert_block(pid, shard_id, x, y, z, Some(&format!("Place reach too far ({:.1}m).", dist)));
            self.flag_player(pid, "REACH", 4.0, &format!("Block place reach {:.1}m > {}m", dist, max_r));
            return;
        }
        let cur = self.shards.get_mut(&shard_id).unwrap().core.get_block_ensured(x, y, z, &mut self.world.tables);
        let cur_b = BLOCKS.get(cur);

        let mut take: Option<i32> = None;
        let mut give: Option<i32> = None;
        let mut need_free = true;

        if cause == "hoe" {
            let tool_id = msg.get("toolId").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let hoe = self.players[pid].held_stack(tool_id);
            let is_hoe = hoe.as_ref().and_then(|h| ITEMS.get(h.id)).is_some_and(|i| i.tool_type == Some("hoe"));
            if id != B::FARMLAND || !matches!(cur, B::GRASS | B::DIRT | B::COARSE_DIRT | B::PODZOL) || (!creative && !is_hoe) {
                self.revert_block(pid, shard_id, x, y, z, None);
                return;
            }
            need_free = false;
        } else if cause == "bonemeal" {
            if id != B::WHEAT || cur != B::WHEAT {
                self.revert_block(pid, shard_id, x, y, z, None);
                return;
            }
            take = Some(IT::BONE_MEAL);
            need_free = false;
        } else if id == B::WATER {
            take = Some(IT::WATER_BUCKET);
            give = Some(IT::BUCKET);
        } else if id == B::LAVA {
            take = Some(IT::LAVA_BUCKET);
            give = Some(IT::BUCKET);
        } else if id == B::REDSTONE_WIRE {
            take = Some(IT::REDSTONE);
        } else if id == B::WHEAT {
            let below = self.shards.get_mut(&shard_id).unwrap().core.get_block_ensured(x, y - 1, z, &mut self.world.tables);
            if below != B::FARMLAND {
                self.revert_block(pid, shard_id, x, y, z, None);
                return;
            }
            take = Some(IT::WHEAT_SEEDS);
        } else if blk.shape == "door" && (meta & 8) != 0 {
            let below = self.shards.get_mut(&shard_id).unwrap().core.get_block_ensured(x, y - 1, z, &mut self.world.tables);
            if below != id {
                self.revert_block(pid, shard_id, x, y, z, None);
                return;
            }
        } else if id == B::FARMLAND {
            self.revert_block(pid, shard_id, x, y, z, None);
            return;
        } else {
            let it = ITEMS.get(id as i32);
            if it.is_none_or(|i| i.block_id != Some(id)) {
                self.revert_block(pid, shard_id, x, y, z, None);
                return;
            }
            take = Some(id as i32);
        }

        if need_free && cur != B::AIR && !cur_b.liquid && !cur_b.cross {
            self.revert_block(pid, shard_id, x, y, z, None);
            return;
        }
        if !creative
            && let Some(t_id) = take {
                let removed = self.players.get_mut(pid).unwrap().ledger.remove(&format!("{}#", t_id), 1, None);
                if removed.is_none() {
                    self.revert_block(pid, shard_id, x, y, z, Some("You do not have that item."));
                    self.flag_player(pid, "NO_ITEM", 1.5, &format!("Placed {} without owning it", blk.name));
                    if let Some(p) = self.players.get(pid) { p.send_inventory(); }
                    return;
                }
                if let Some(g_id) = give {
                    self.players.get_mut(pid).unwrap().ledger.add(&ItemStack { id: g_id, count: 1, dur: None, enchant: None });
                }
            }
        self.shards.get_mut(&shard_id).unwrap().core.set_block(x, y, z, id, meta, true, &mut self.world.tables, None);
        self.bcast_shard(shard_id, &json!({ "type": "block_set", "x": x, "y": y, "z": z, "id": id, "meta": meta }), Some(pid));
        self.schedule_world_save(5000);
    }

    fn handle_bonemeal(&mut self, pid: &str, msg: &Value) {
        let (x, y, z) = (block_coord(msg.get("x")), block_coord(msg.get("y")), block_coord(msg.get("z")));
        let Some((shard_id, px, py, pz, survival)) = self.players.get(pid).map(|p| (p.shard_id, p.x, p.y, p.z, p.is_survival())) else { return };
        let blk = self.shards.get_mut(&shard_id).unwrap().core.get_block_ensured(x, y, z, &mut self.world.tables);
        if blk != B::GRASS || !AntiCheat::check_reach(&self.config.anticheat, px, py, pz, x, y, z).0 {
            return;
        }
        if survival && self.players.get_mut(pid).unwrap().ledger.remove(&format!("{}#", IT::BONE_MEAL), 1, None).is_none() {
            self.players[pid].send_inventory();
            return;
        }
        let mut list: Vec<[i32; 5]> = Vec::new();
        if let Some(plants) = msg.get("plants").and_then(|v| v.as_array()) {
            let sh = self.shards.get_mut(&shard_id).unwrap();
            for pl in plants.iter().take(8) {
                let Some(arr) = pl.as_array() else { continue };
                if arr.len() < 4 { continue; }
                let px2 = arr[0].as_i64().unwrap_or(0) as i32;
                let pz2 = arr[2].as_i64().unwrap_or(0) as i32;
                let pid_b = arr[3].as_i64().unwrap_or(0) as u8;
                if (px2 - x).abs() > 2 || (pz2 - z).abs() > 2 || !matches!(pid_b, B::TALL_GRASS | B::POPPY | B::DANDELION | B::ORCHID) {
                    continue;
                }
                if sh.core.get_block_ensured(px2, y, pz2, &mut self.world.tables) != B::GRASS
                    || sh.core.get_block_ensured(px2, y + 1, pz2, &mut self.world.tables) != B::AIR
                {
                    continue;
                }
                sh.core.set_block(px2, y + 1, pz2, pid_b, 0, true, &mut self.world.tables, None);
                list.push([px2, y + 1, pz2, pid_b as i32, 0]);
            }
        }
        if !list.is_empty() {
            self.bcast_shard(shard_id, &json!({ "type": "blocks", "list": list }), Some(pid));
        }
    }

    fn handle_block_interact(&mut self, pid: &str, msg: &Value) {
        let (x, y, z) = (block_coord(msg.get("x")), block_coord(msg.get("y")), block_coord(msg.get("z")));
        let action = msg.get("action").and_then(|v| v.as_str()).unwrap_or("");
        let val = (msg.get("val").and_then(|v| v.as_i64()).unwrap_or(0) & 255) as u8;
        let Some((shard_id, dead, px, py, pz)) = self.players.get(pid).map(|p| (p.shard_id, p.pl.dead, p.x, p.y, p.z)) else { return };
        if dead { return; }
        let (ok, dist, max_r) = AntiCheat::check_reach(&self.config.anticheat, px, py, pz, x, y, z);
        let (id, meta) = {
            let sh = self.shards.get_mut(&shard_id).unwrap();
            let id = sh.core.get_block_ensured(x, y, z, &mut self.world.tables);
            let meta = sh.core.get_meta(x, y, z);
            (id, meta)
        };

        if action == "tnt" {
            if id != B::TNT { return; }
            if self.config.anticheat.enabled && dist > max_r + 24.0 {
                self.flag_player(pid, "REACH", 3.0, &format!("TNT detonation {:.1}m away", dist));
                return;
            }
            self.explode(shard_id, x, y, z, 3.2, true, Some(pid));
            return;
        }
        if !ok {
            self.flag_player(pid, "REACH", 3.0, &format!("Interact ({}) reach {:.1}m", action, dist));
            self.revert_block(pid, shard_id, x, y, z, None);
            return;
        }
        let shape = BLOCKS.get(id).shape;
        if matches!(action, "door" | "trapdoor" | "fence_gate") {
            if shape != action || id == B::IRON_DOOR || (val ^ meta) != 4 {
                self.revert_block(pid, shard_id, x, y, z, None);
                return;
            }
            self.shards.get_mut(&shard_id).unwrap().core.set_block(x, y, z, id, val, true, &mut self.world.tables, None);
            self.bcast_shard(shard_id, &json!({ "type": "block_set", "x": x, "y": y, "z": z, "id": id, "meta": val }), Some(pid));
            self.schedule_world_save(5000);
            return;
        }

        let mut change_log = Vec::new();
        let tnt_triggers;
        match action {
            "lever" => {
                if id != B::LEVER || (val ^ meta) != 1 {
                    self.revert_block(pid, shard_id, x, y, z, None);
                    return;
                }
                let sh = self.shards.get_mut(&shard_id).unwrap();
                sh.core.set_block(x, y, z, B::LEVER, val, true, &mut self.world.tables, Some(&mut change_log));
                let rs = std::mem::take(&mut sh.core.redstone);
                {
                    let mut ad = CoreRedstoneAdapter { world: &mut sh.core, tables: &mut self.world.tables, change_log: &mut change_log, tnt_triggers: Vec::new() };
                    rs.propagate_from(&mut ad, x, y, z, (val & 1) != 0);
                    tnt_triggers = ad.tnt_triggers;
                }
                sh.core.redstone = rs;
            }
            "button" => {
                if id != B::STONE_BUTTON {
                    self.revert_block(pid, shard_id, x, y, z, None);
                    return;
                }
                let sh = self.shards.get_mut(&shard_id).unwrap();
                let mut rs = std::mem::take(&mut sh.core.redstone);
                {
                    let mut ad = CoreRedstoneAdapter { world: &mut sh.core, tables: &mut self.world.tables, change_log: &mut change_log, tnt_triggers: Vec::new() };
                    rs.press_button(&mut ad, x, y, z);
                    tnt_triggers = ad.tnt_triggers;
                }
                sh.core.redstone = rs;
            }
            "plate" => {
                if id != B::PRESSURE_PLATE || (meta & 1) == (val & 1) { return; }
                if (val & 1) != 0 && (px - (x as f64 + 0.5)).hypot(pz - (z as f64 + 0.5)) > 2.5 { return; }
                let sh = self.shards.get_mut(&shard_id).unwrap();
                sh.core.set_block(x, y, z, B::PRESSURE_PLATE, val & 1, true, &mut self.world.tables, Some(&mut change_log));
                let rs = std::mem::take(&mut sh.core.redstone);
                {
                    let mut ad = CoreRedstoneAdapter { world: &mut sh.core, tables: &mut self.world.tables, change_log: &mut change_log, tnt_triggers: Vec::new() };
                    rs.propagate_from(&mut ad, x, y, z, (val & 1) != 0);
                    tnt_triggers = ad.tnt_triggers;
                }
                sh.core.redstone = rs;
            }
            _ => return,
        }
        if !change_log.is_empty() {
            let list: Vec<[i32; 5]> = change_log.iter().map(|&(cx, cy, cz, cid, cm)| [cx, cy, cz, cid as i32, cm as i32]).collect();
            self.bcast_shard(shard_id, &json!({ "type": "blocks", "list": list }), Some(pid));
            self.schedule_world_save(5000);
        }
        for (tx, ty, tz) in tnt_triggers {
            self.explode(shard_id, tx, ty, tz, 3.2, true, Some(pid));
        }
    }

    fn handle_mob_hit(&mut self, pid: &str, msg: &Value) {
        let now = now_ms();
        let mob_id = msg.get("id").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
        let Some((shard_id, dead, last_hit, eye, dir, sprinting, on_ground)) = self.players.get(pid).map(|p| {
            (p.shard_id, p.pl.dead, p.last_mob_hit_ms, p.eye(), p.look(), p.pl.sprinting, p.pl.on_ground)
        }) else { return };
        if dead || now - last_hit < 320 { return; }
        let mob_info = self.shards.get(&shard_id).and_then(|sh| {
            sh.mobs.mobs.iter().enumerate().find(|(_, m)| m.id == mob_id).map(|(idx, m)| {
                (idx, m.dying, m.hurt_cooldown, m.type_idx, m.x, m.y, m.z)
            })
        });
        let Some((mob_idx, dying, hurt_cd, type_idx, mx, my, mz)) = mob_info else { return };
        if dying || hurt_cd > 0.0 { return; }
        let def = &MOB_DEFS[type_idx];
        let (tx, ty, tz) = (mx - eye.0, (my + def.h * 0.55) - eye.1, mz - eye.2);
        let dist = (tx * tx + ty * ty + tz * tz).sqrt();
        if self.config.anticheat.enabled && dist > 4.8 {
            self.flag_player(pid, "MOB_REACH", 2.0, &format!("Hit {} from {:.1}m", def.name, dist));
            return;
        }
        if self.config.anticheat.enabled && dist > 1.5 && (tx * dir.0 + ty * dir.1 + tz * dir.2) / dist.max(1.0) < 0.3 {
            self.flag_player(pid, "MOB_ANGLE", 2.0, &format!("Hit {} outside look cone", def.name));
            return;
        }
        self.players.get_mut(pid).unwrap().last_mob_hit_ms = now;
        let w_id = msg.get("w").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
        let held = self.players[pid].held_stack(w_id);
        let crit = msg.get("crit").and_then(|v| v.as_bool()).unwrap_or(false) && !on_ground;
        let mut mob_events = MobEvents::default();
        self.shards.get_mut(&shard_id).unwrap().mobs.apply_melee(
            mob_idx,
            Some(pid),
            sprinting,
            held.as_ref(),
            dir,
            crit,
            &mut mob_events,
        );
        self.apply_mob_events(shard_id, mob_events);
    }

    fn handle_feed(&mut self, pid: &str, msg: &Value) {
        let id = msg.get("id").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
        let is_food = matches!(id, IT::WHEAT_ITEM | IT::APPLE | IT::CARROT | IT::SWEET_BERRIES) || id == B::BAMBOO as i32;
        let Some((shard_id, dead, survival, eye)) = self.players.get(pid).map(|p| (p.shard_id, p.pl.dead, p.is_survival(), p.eye())) else { return };
        if !is_food || dead { return; }
        if survival && self.players.get_mut(pid).unwrap().ledger.remove(&format!("{}#", id), 1, None).is_none() {
            self.players[pid].send_inventory();
            return;
        }
        let fed = self.shards.get_mut(&shard_id).unwrap().mobs.try_feed_animal(eye);
        if !fed && survival {
            self.players.get_mut(pid).unwrap().ledger.add(&ItemStack { id, count: 1, dur: None, enchant: None });
        }
    }

    fn handle_shoot(&mut self, pid: &str, msg: &Value) {
        let Some((shard_id, dead, survival, last_shot, eye)) = self.players.get(pid).map(|p| {
            (p.shard_id, p.pl.dead, p.is_survival(), p.last_shot_ms, p.eye())
        }) else { return };
        if dead { return; }
        let Some(bow) = self.players[pid].held_stack(IT::BOW) else { return };
        let now = now_ms();
        if now - last_shot < 180 { return; }
        self.players.get_mut(pid).unwrap().last_shot_ms = now;
        let parse_vec3 = |k: &str| -> Option<[f64; 3]> {
            let arr = msg.get(k)?.as_array()?;
            if arr.len() < 3 { return None; }
            let x = arr[0].as_f64()?;
            let y = arr[1].as_f64()?;
            let z = arr[2].as_f64()?;
            if x.is_finite() && y.is_finite() && z.is_finite() { Some([x, y, z]) } else { None }
        };
        let (Some(o), Some(mut v)) = (parse_vec3("o"), parse_vec3("v")) else { return };
        if ((o[0] - eye.0).powi(2) + (o[1] - eye.1).powi(2) + (o[2] - eye.2).powi(2)).sqrt() > 2.5 {
            return;
        }
        let spd = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        if spd > 32.0 {
            let k = 32.0 / spd;
            v[0] *= k; v[1] *= k; v[2] *= k;
        }
        if survival && ench_lvl(Some(&bow), "Infinity") == 0
            && self.players.get_mut(pid).unwrap().ledger.remove(&format!("{}#", IT::ARROW), 1, None).is_none() {
                self.players[pid].send_inventory();
                return;
            }
        let pow = ench_lvl(Some(&bow), "Power");
        let dmg = 8.0 * if pow > 0 { 1.0 + 0.25 * ((pow + 1) as f64) } else { 1.0 };
        let punch = ench_lvl(Some(&bow), "Punch");
        let flame = ench_lvl(Some(&bow), "Flame") > 0;
        let mut mob_events = MobEvents::default();
        self.shards.get_mut(&shard_id).unwrap().mobs.spawn_arrow(
            o[0], o[1], o[2], v[0], v[1], v[2],
            true, dmg, punch, flame, Some(pid.to_string()), &mut mob_events,
        );
        self.apply_mob_events(shard_id, mob_events);
    }

    fn handle_pvp_hit(&mut self, pid: &str, msg: &Value) {
        if !self.config.pvp { return; }
        let target_id = msg.get("targetId").and_then(|v| v.as_str()).unwrap_or("");
        if target_id.is_empty() || target_id == pid { return; }
        let Some((a_shard, a_dead, a_name, ax, ay, az, ayaw, apitch, a_on_ground, last_pvp)) = self.players.get(pid).map(|p| {
            (p.shard_id, p.pl.dead, p.name.clone(), p.x, p.y, p.z, p.yaw, p.pitch, p.pl.on_ground, p.ac.last_pvp_ms)
        }) else { return };
        let Some((t_shard, t_mode, t_dead, t_name, tx, ty, tz)) = self.players.get(target_id).map(|t| {
            (t.shard_id, t.mode.clone(), t.pl.dead, t.name.clone(), t.x, t.y, t.z)
        }) else { return };
        if a_dead || a_shard != t_shard || t_mode == "creative" || t_dead { return; }

        if self.config.anticheat.enabled {
            let now = now_ms();
            if now - last_pvp < 200 {
                self.flag_player(pid, "PVP_AUTOCLICK", 3.0, &format!("Hit interval {}ms", now - last_pvp));
                return;
            }
            self.players.get_mut(pid).unwrap().ac.last_pvp_ms = now;
            let dist = ((tx - ax).powi(2) + ((ty + 0.9) - (ay + 1.62)).powi(2) + (tz - az).powi(2)).sqrt();
            let max_r = if self.config.anticheat.max_pvp_reach > 0.0 { self.config.anticheat.max_pvp_reach } else { 4.8 };
            if dist > max_r {
                self.flag_player(pid, "PVP_REACH", 4.5, &format!("Hit {} from {:.2}m (max {}m)", t_name, dist, max_r));
                return;
            }
            let cp = apitch.cos();
            let (lx, ly, lz) = (-ayaw.sin() * cp, apitch.sin(), -ayaw.cos() * cp);
            let (vtx, vty, vtz) = (tx - ax, (ty + 1.0) - (ay + 1.62), tz - az);
            let tlen = (vtx * vtx + vty * vty + vtz * vtz).sqrt().max(1.0);
            let dot = (lx * vtx + ly * vty + lz * vtz) / tlen;
            if dist > 1.2 && dot < 0.32 {
                self.flag_player(pid, "PVP_ANGLE", 4.0, &format!("Hit {} outside look cone (dot={:.2})", t_name, dot));
                return;
            }
        }

        let wid = msg.get("weaponId").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
        let held = self.players[pid].held_stack(wid);
        let it = held.as_ref().and_then(|h| ITEMS.get(h.id));
        let mut dmg = it.map_or(1.0, |i| if i.dmg > 0.0 { i.dmg } else { 1.0 });
        dmg += (ench_lvl(held.as_ref(), "Sharpness") as f64) * 1.25;
        if msg.get("crit").and_then(|v| v.as_bool()).unwrap_or(false) && !a_on_ground {
            dmg *= 1.35;
        }
        let kb = 1.0 + (ench_lvl(held.as_ref(), "Knockback") as f64) * 0.9;
        let (dx, dz) = (tx - ax, tz - az);
        let d = dx.hypot(dz).max(1.0);
        self.hurt_player(target_id, dmg.round(), &format!("Slain by {}.", a_name), (dx / d) * 6.5 * kb, 5.0, (dz / d) * 6.5 * kb, false);
    }

    pub fn build_welcome_json(&self, pid: &str, account_created: bool, has_pos: bool) -> Value {
        let p = &self.players[pid];
        let sh = &self.shards[&p.shard_id];
        let existing_players: Vec<Value> = sh
            .player_ids
            .iter()
            .filter(|id| id.as_str() != pid)
            .filter_map(|id| self.players.get(id))
            .map(|o| {
                json!({
                    "id": o.id, "name": o.name,
                    "x": o.x, "y": o.y, "z": o.z,
                    "yaw": o.yaw, "pitch": o.pitch,
                    "crouch": o.crouch, "swing": o.swing, "ping": o.ping,
                    "mode": o.mode, "op": self.is_op(&o.name),
                })
            })
            .collect();
        let deltas: Vec<Value> = self
            .world
            .tables
            .deltas
            .values()
            .map(|(wx, wy, wz, id, meta)| json!([wx, wy, wz, id, meta]))
            .collect();
        let block_entities: Vec<Value> = self
            .world
            .tables
            .block_entities
            .iter()
            .map(|(k, ent)| json!([k, ent]))
            .collect();
        let drops: Vec<Value> = sh
            .drops
            .values()
            .map(|d| json!([d.id, d.x, d.y, d.z, d.s]))
            .collect();

        json!({
            "type": "welcome",
            "id": p.id,
            "name": p.name,
            "accountCreated": account_created,
            "serverName": self.config.server_name,
            "motd": self.config.motd,
            "maxPlayers": self.config.max_players,
            "isOp": self.is_op(&p.name),
            "seed": self.world.seed,
            "ox": sh.ox.to_string(),
            "oz": sh.oz.to_string(),
            "mode": p.mode,
            "difficulty": self.config.difficulty,
            "pvp": self.config.pvp,
            "day": self.world.day,
            "time": round_dec(self.world.time, 10000.0),
            "weather": self.world.weather,
            "player": {
                "x": if has_pos { Some(p.x) } else { None },
                "y": if has_pos { Some(p.y) } else { None },
                "z": if has_pos { Some(p.z) } else { None },
                "health": p.pl.health,
                "hunger": p.pl.hunger,
                "saturation": p.pl.saturation,
                "xp": p.pl.xp,
                "level": p.pl.level,
                "dead": p.pl.dead,
                "slots": p.pl.inventory.slots,
                "armor": p.pl.inventory.armor,
            },
            "players": existing_players,
            "deltas": deltas,
            "blockEntities": block_entities,
            "stats": p.stats_json(),
            "drops": drops,
            "mobTypes": MOB_TYPES,
        })
    }

    pub fn init_player_session(
        &mut self,
        name: String,
        ip: String,
        tx: UnboundedSender<WsOut>,
    ) -> (String, bool) {
        let id = format!("p{}", self.next_player_id);
        self.next_player_id += 1;
        let saved = self.world.saved_players.get(&name.to_lowercase()).cloned();
        let has_pos = saved
            .as_ref()
            .is_some_and(|s| s.x.is_some_and(|v| v.is_finite()) && s.y.is_some_and(|v| v.is_finite()) && s.z.is_some_and(|v| v.is_finite()));
        let (ix, iy, iz) = if let Some(s) = saved.as_ref().filter(|_| has_pos) {
            (s.x.unwrap(), s.y.unwrap(), s.z.unwrap())
        } else {
            (8.5, 80.0, 8.5)
        };
        let mode = saved
            .as_ref()
            .and_then(|s| s.mode.clone())
            .unwrap_or_else(|| self.config.mode.clone());
        let mut pl = CorePlayer::new(&mode, rand::thread_rng().r#gen());
        pl.x = ix; pl.y = iy; pl.z = iz;
        if let Some(s) = &saved {
            if let Some(h) = s.health.filter(|v| v.is_finite()) { pl.health = h.clamp(0.0, 20.0); }
            if let Some(h) = s.hunger.filter(|v| v.is_finite()) { pl.hunger = h.clamp(0.0, 20.0); }
            if let Some(h) = s.saturation.filter(|v| v.is_finite()) { pl.saturation = h.clamp(0.0, 20.0); }
            if let Some(xp) = s.xp { pl.xp = xp.max(0); }
            if let Some(lv) = s.level { pl.level = lv.max(0); }
            pl.dead = s.dead.unwrap_or(false) || pl.health <= 0.0;
        }
        let mut slots = vec![None; 36];
        let mut armor = vec![None; 5];
        if let Some(s) = &saved {
            for (i, sl) in s.slots.iter().take(36).enumerate() {
                if let Some(st) = sl {
                    slots[i] = Self::clean_stack(&serde_json::to_value(st).unwrap_or(Value::Null));
                }
            }
            for (i, sl) in s.armor.iter().take(5).enumerate() {
                if let Some(st) = sl {
                    armor[i] = Self::clean_stack(&serde_json::to_value(st).unwrap_or(Value::Null));
                }
            }
            if !s.limbo.is_empty() {
                let mut inv = Inventory::new();
                inv.slots = slots;
                for st in &s.limbo {
                    if let Some(c) = Self::clean_stack(&serde_json::to_value(st).unwrap_or(Value::Null)) {
                        inv.add_item(c.id, c.count, c.dur, c.enchant);
                    }
                }
                slots = inv.slots;
            }
        }
        let limbo = vec![None; 12];
        let ledger = Ledger::from_stacks(&[&slots, &armor, &limbo]);
        pl.inventory.slots = slots;
        pl.inventory.armor = armor;

        let home_id = self.home_shard_id;
        let (h_ox, h_oz) = (self.shards[&home_id].ox.clone(), self.shards[&home_id].oz.clone());
        let now = now_ms();
        let session = PlayerSession {
            id: id.clone(),
            name,
            ip,
            tx,
            shard_id: home_id,
            x: ix,
            y: iy,
            z: iz,
            yaw: 0.0,
            pitch: 0.0,
            crouch: false,
            swing: 0.0,
            held_id: 0,
            on_ground: true,
            has_moved: has_pos,
            mode,
            ping: 0,
            online: true,
            last_activity: now,
            sel: 0,
            pl,
            limbo,
            ledger,
            accept_next_sync: false,
            ac: PlayerAC::new(ix, iy, iz),
            frame_ox: h_ox,
            frame_oz: h_oz,
            frame_seq: 0,
            pending_frames: Vec::new(),
            last_stats_ms: 0,
            last_respawn_ms: 0,
            last_eat_ms: 0,
            last_break_ms: 0,
            last_mob_hit_ms: 0,
            last_shot_ms: 0,
        };
        self.players.insert(id.clone(), session);
        if has_pos {
            let s = saved.as_ref().unwrap();
            let ox = to_big_value(&s.ox).unwrap_or_default();
            let oz = to_big_value(&s.oz).unwrap_or_default();
            let xi = ox + BigInt::from(ix.floor() as i64);
            let zi = oz + BigInt::from(iz.floor() as i64);
            self.place_abs(&id, &xi, ix - ix.floor(), iy, &zi, iz - iz.floor(), None, false);
        } else {
            self.shards.get_mut(&home_id).unwrap().player_ids.push(id.clone());
        }
        (id, has_pos)
    }
}
