use std::collections::{HashMap, HashSet, VecDeque};

use indexmap::IndexMap;
use num_bigint::BigInt;
use num_traits::Zero;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::blocks::{B, BLOCKS};
use crate::chunk::Chunk;
use crate::crafting::CRAFTING;
use crate::enchants::ItemStack;
use crate::growth::{LeafDecay, is_crop, is_log, is_sapling, is_sapling_soil};
use crate::items::{IT, ITEMS};
use crate::lighting::compute_local_light;
use crate::math::{
    CHUNK_SIZE, NUM_SAFE, SEA_LEVEL, abs_int_str, abs_json, big_to_num, floor_div, floor_mod,
    hash_string,
};
use crate::redstone::{RedstoneSystem, RedstoneWorld};
use crate::worldgen::{BIOMES, BiomeDef, WorldGenerator};

pub type BlockChange = (i32, i32, i32, u8, u8);

#[expect(clippy::large_enum_variant, reason = "BlockEntity matches persisted JSON tagged enum directly")]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum BlockEntity {
    #[serde(rename = "chest")]
    Chest { slots: Vec<Option<ItemStack>> },
    #[serde(rename = "barrel")]
    Barrel { slots: Vec<Option<ItemStack>> },
    #[serde(rename = "furnace")]
    Furnace {
        input: Option<ItemStack>,
        fuel: Option<ItemStack>,
        output: Option<ItemStack>,
        #[serde(rename = "burnTime", default)]
        burn_time: f64,
        #[serde(rename = "burnMax", default = "default_burn_max")]
        burn_max: f64,
        #[serde(default)]
        progress: f64,
    },
}

fn default_burn_max() -> f64 {
    1.0
}

impl BlockEntity {
    pub fn stacks(&self) -> Vec<Option<&ItemStack>> {
        match self {
            BlockEntity::Chest { slots } | BlockEntity::Barrel { slots } => {
                slots.iter().map(|s| s.as_ref()).collect()
            }
            BlockEntity::Furnace { input, fuel, output, .. } => {
                vec![input.as_ref(), fuel.as_ref(), output.as_ref()]
            }
        }
    }

    pub fn is_furnace_burning(&self) -> bool {
        matches!(self, BlockEntity::Furnace { burn_time, .. } if *burn_time > 0.0)
    }
}

pub fn container_type(block_id: u8) -> Option<&'static str> {
    match block_id {
        B::CHEST => Some("chest"),
        B::BARREL => Some("barrel"),
        B::FURNACE | B::FURNACE_LIT => Some("furnace"),
        _ => None,
    }
}

/// Shared persisted world tables across all shards.
#[derive(Default)]
pub struct SharedWorldTables {
    /// Absolute "wx,wy,wz" -> [wx_json, wy, wz_json, id, meta]
    pub deltas: IndexMap<String, (Value, i32, Value, u8, u8)>,
    /// Absolute "wx,wy,wz" -> BlockEntity
    pub block_entities: IndexMap<String, BlockEntity>,
    /// Absolute chunk key "cx,cy,cz" -> (idx -> (id, meta))
    pub modifications: HashMap<String, HashMap<usize, (u8, u8)>>,
    pub dirty: bool,
}

pub struct CoreWorld {
    pub seed_str: String,
    pub seed: u32,
    pub ox: BigInt,
    pub oz: BigInt,
    pub ox_n: Option<i64>,
    pub oz_n: Option<i64>,
    pub ocx: BigInt,
    pub ocz: BigInt,
    pub ocx_n: Option<i64>,
    pub ocz_n: Option<i64>,
    pub generator: WorldGenerator,
    pub chunks: HashMap<(i32, i32, i32), Chunk>,
    pub redstone: RedstoneSystem,
    pub gravity_queue: VecDeque<(i32, i32, i32)>,
    pub leaf_decay: LeafDecay,
}

pub struct RebaseResult {
    pub dx: i64,
    pub dz: i64,
    pub far: bool,
}

impl CoreWorld {
    pub fn new(seed_str: &str, ox: BigInt, oz: BigInt) -> Self {
        let seed = hash_string(seed_str);
        let mut w = Self {
            seed_str: seed_str.to_string(),
            seed,
            ox: BigInt::zero(),
            oz: BigInt::zero(),
            ox_n: Some(0),
            oz_n: Some(0),
            ocx: BigInt::zero(),
            ocz: BigInt::zero(),
            ocx_n: Some(0),
            ocz_n: Some(0),
            generator: WorldGenerator::new(seed),
            chunks: HashMap::new(),
            redstone: RedstoneSystem::new(),
            gravity_queue: VecDeque::new(),
            leaf_decay: LeafDecay::default(),
        };
        w.apply_origin(ox, oz);
        w
    }

    fn apply_origin(&mut self, ox: BigInt, oz: BigInt) {
        self.ox_n = big_to_num(&ox);
        self.oz_n = big_to_num(&oz);
        self.ocx = &ox / CHUNK_SIZE;
        self.ocz = &oz / CHUNK_SIZE;
        self.ocx_n = big_to_num(&self.ocx);
        self.ocz_n = big_to_num(&self.ocz);
        self.ox = ox.clone();
        self.oz = oz.clone();
        self.generator.set_origin(ox, oz);
    }

    pub fn set_origin(&mut self, ox: BigInt, oz: BigInt) -> RebaseResult {
        let dx_b = &ox - &self.ox;
        let dz_b = &oz - &self.oz;
        if dx_b.is_zero() && dz_b.is_zero() {
            return RebaseResult { dx: 0, dz: 0, far: false };
        }
        let dx_n = big_to_num(&dx_b);
        let dz_n = big_to_num(&dz_b);
        let far = match (dx_n, dz_n) {
            (Some(dx), Some(dz)) => dx.abs() > (1_i64 << 40) || dz.abs() > (1_i64 << 40),
            _ => true,
        };
        self.apply_origin(ox, oz);
        if far {
            self.chunks.clear();
            self.redstone.timed_buttons.clear();
            self.gravity_queue.clear();
            self.leaf_decay.clear();
            return RebaseResult { dx: 0, dz: 0, far: true };
        }
        let dx = dx_n.unwrap();
        let dz = dz_n.unwrap();
        self.shift_local(dx as i32, dz as i32);
        RebaseResult { dx, dz, far: false }
    }

    fn shift_local(&mut self, dx: i32, dz: i32) {
        let dcx = dx / CHUNK_SIZE;
        let dcz = dz / CHUNK_SIZE;
        let old: Vec<Chunk> = self.chunks.drain().map(|(_, c)| c).collect();
        for mut ch in old {
            ch.cx -= dcx;
            ch.cz -= dcz;
            ch.key = format!("{},{},{}", ch.cx, ch.cy, ch.cz);
            self.chunks.insert((ch.cx, ch.cy, ch.cz), ch);
        }
        self.redstone.shift_local(dx, dz);
        for g in &mut self.gravity_queue {
            g.0 -= dx;
            g.2 -= dz;
        }
        self.leaf_decay.shift_local(dx, dz);
    }

    pub fn abs_chunk_key(&self, cx: i32, cy: i32, cz: i32) -> String {
        format!(
            "{},{},{}",
            abs_int_str(&self.ocx, self.ocx_n, cx as i64),
            cy,
            abs_int_str(&self.ocz, self.ocz_n, cz as i64)
        )
    }

    pub fn abs_block_key(&self, wx: i32, wy: i32, wz: i32) -> String {
        format!(
            "{},{},{}",
            abs_int_str(&self.ox, self.ox_n, wx as i64),
            wy,
            abs_int_str(&self.oz, self.oz_n, wz as i64)
        )
    }

    pub fn abs_x(&self, lx: i32) -> String {
        abs_int_str(&self.ox, self.ox_n, lx as i64)
    }

    pub fn abs_z(&self, lz: i32) -> String {
        abs_int_str(&self.oz, self.oz_n, lz as i64)
    }

    pub fn key_to_local(&self, k: &str) -> Option<(i32, i32, i32)> {
        let mut parts = k.split(',');
        let xs = parts.next()?;
        let ys = parts.next()?;
        let zs = parts.next()?;
        if parts.next().is_some() {
            return None;
        }
        let x = Self::abs_to_local(xs, &self.ox, self.ox_n)?;
        let z = Self::abs_to_local(zs, &self.oz, self.oz_n)?;
        let y = ys.parse::<i32>().ok()?;
        Some((x as i32, y, z as i32))
    }

    fn abs_to_local(s: &str, o: &BigInt, o_n: Option<i64>) -> Option<i64> {
        if s.len() < 16
            && let (Some(on), Ok(v)) = (o_n, s.parse::<i64>()) {
                let l = v - on;
                return if l.abs() < NUM_SAFE { Some(l) } else { None };
            }
        let b = s.parse::<BigInt>().ok()?;
        big_to_num(&(b - o))
    }

    pub fn ensure_chunk(&mut self, cx: i32, cy: i32, cz: i32, tables: &mut SharedWorldTables) {
        if self.chunks.contains_key(&(cx, cy, cz)) {
            return;
        }
        let mut ch = Chunk::new(cx, cy, cz);
        let chests = self.generator.generate_chunk_blocks(&mut ch);
        for (wx, wy, wz, ent) in chests {
            let key = self.abs_block_key(wx, wy, wz);
            let mkey = self.abs_chunk_key(floor_div(wx, CHUNK_SIZE), floor_div(wy, CHUNK_SIZE), floor_div(wz, CHUNK_SIZE));
            let midx = Chunk::idx(
                floor_mod(wx, CHUNK_SIZE) as usize,
                floor_mod(wy, CHUNK_SIZE) as usize,
                floor_mod(wz, CHUNK_SIZE) as usize,
            );
            let has_mod = tables.modifications.get(&mkey).is_some_and(|m| m.contains_key(&midx));
            if !tables.block_entities.contains_key(&key) && !has_mod {
                tables.block_entities.insert(key, ent);
            }
        }
        let mk = self.abs_chunk_key(cx, cy, cz);
        if let Some(mods) = tables.modifications.get(&mk) {
            for (&idx, &(id, meta)) in mods {
                let lx = (idx & 15) as i32;
                let lz = ((idx >> 4) & 15) as i32;
                let ly = ((idx >> 8) & 15) as i32;
                ch.set_block(lx, ly, lz, id, meta);
            }
        }
        let (above_bottom, above_empty) = self.above_light_info(cx, cy, cz);
        let heights = ch.height_map.clone().unwrap_or_else(|| self.generator.get_column_chunk(cx, cz).heights);
        compute_local_light(&mut ch, above_bottom.as_ref(), above_empty, &heights);
        ch.light_dirty = false;
        self.chunks.insert((cx, cy, cz), ch);
    }

    fn above_light_info(&self, cx: i32, cy: i32, cz: i32) -> (Option<[u8; 256]>, Option<bool>) {
        let Some(ab) = self.chunks.get(&(cx, cy + 1, cz)) else {
            return (None, None);
        };
        let empty = Some(ab.is_empty);
        if ab.light_dirty {
            return (None, empty);
        }
        let mut slice = [0u8; 256];
        slice.copy_from_slice(&ab.light[0..256]);
        (Some(slice), empty)
    }

    pub fn ensure_chunk_at(&mut self, x: i32, y: i32, z: i32, tables: &mut SharedWorldTables) {
        self.ensure_chunk(floor_div(x, CHUNK_SIZE), floor_div(y, CHUNK_SIZE), floor_div(z, CHUNK_SIZE), tables);
    }

    pub fn get_block(&self, wx: i32, wy: i32, wz: i32) -> u8 {
        let cx = floor_div(wx, CHUNK_SIZE);
        let cy = floor_div(wy, CHUNK_SIZE);
        let cz = floor_div(wz, CHUNK_SIZE);
        let Some(c) = self.chunks.get(&(cx, cy, cz)) else { return 0 };
        if c.is_empty { return 0; }
        c.blocks[Chunk::idx(
            floor_mod(wx, CHUNK_SIZE) as usize,
            floor_mod(wy, CHUNK_SIZE) as usize,
            floor_mod(wz, CHUNK_SIZE) as usize,
        )]
    }

    pub fn get_block_ensured(&mut self, wx: i32, wy: i32, wz: i32, tables: &mut SharedWorldTables) -> u8 {
        self.ensure_chunk_at(wx, wy, wz, tables);
        self.get_block(wx, wy, wz)
    }

    pub fn get_meta(&self, wx: i32, wy: i32, wz: i32) -> u8 {
        let cx = floor_div(wx, CHUNK_SIZE);
        let cy = floor_div(wy, CHUNK_SIZE);
        let cz = floor_div(wz, CHUNK_SIZE);
        let Some(c) = self.chunks.get(&(cx, cy, cz)) else { return 0 };
        if c.is_empty { return 0; }
        c.meta[Chunk::idx(
            floor_mod(wx, CHUNK_SIZE) as usize,
            floor_mod(wy, CHUNK_SIZE) as usize,
            floor_mod(wz, CHUNK_SIZE) as usize,
        )]
    }

    pub fn get_meta_ensured(&mut self, wx: i32, wy: i32, wz: i32, tables: &mut SharedWorldTables) -> u8 {
        self.ensure_chunk_at(wx, wy, wz, tables);
        self.get_meta(wx, wy, wz)
    }

    pub fn has_block_support(
        &mut self,
        wx: i32,
        wy: i32,
        wz: i32,
        id: u8,
        meta: u8,
        tables: &mut SharedWorldTables,
    ) -> bool {
        let is_full = |sid: u8| {
            let sb = BLOCKS.get(sid);
            sb.solid && !sb.liquid && sb.shape == "cube"
        };
        let is_floor = |sid: u8| {
            let sb = BLOCKS.get(sid);
            sb.solid && !sb.liquid && matches!(sb.shape, "cube" | "slab" | "stairs")
        };
        let is_top = |sid: u8| {
            let sb = BLOCKS.get(sid);
            sb.solid && !sb.liquid && matches!(sb.shape, "cube" | "slab" | "stairs" | "fence")
        };
        let wall_sup = |w: &mut Self, t: &mut SharedWorldTables, attach: u8| -> bool {
            match attach {
                1 => is_full(w.get_block_ensured(wx, wy, wz - 1, t)),
                2 => is_full(w.get_block_ensured(wx, wy, wz + 1, t)),
                3 => is_full(w.get_block_ensured(wx - 1, wy, wz, t)),
                4 => is_full(w.get_block_ensured(wx + 1, wy, wz, t)),
                _ => false,
            }
        };
        if id == B::AIR {
            return true;
        }
        let blk = BLOCKS.get(id);
        if id == B::TORCH {
            let attach = meta & 7;
            return if attach == 0 {
                is_top(self.get_block_ensured(wx, wy - 1, wz, tables))
            } else {
                wall_sup(self, tables, attach)
            };
        }
        if id == B::LANTERN {
            return if (meta & 1) != 0 {
                is_top(self.get_block_ensured(wx, wy + 1, wz, tables))
            } else {
                is_top(self.get_block_ensured(wx, wy - 1, wz, tables))
            };
        }
        if id == B::LEVER || id == B::STONE_BUTTON {
            let attach = (meta >> 1) & 7;
            return match attach {
                0 => is_floor(self.get_block_ensured(wx, wy - 1, wz, tables)),
                5 => is_floor(self.get_block_ensured(wx, wy + 1, wz, tables)),
                _ => wall_sup(self, tables, attach),
            };
        }
        if id == B::LADDER {
            return match meta & 3 {
                0 => is_full(self.get_block_ensured(wx, wy, wz + 1, tables)),
                1 => is_full(self.get_block_ensured(wx + 1, wy, wz, tables)),
                2 => is_full(self.get_block_ensured(wx, wy, wz - 1, tables)),
                _ => is_full(self.get_block_ensured(wx - 1, wy, wz, tables)),
            };
        }
        if matches!(id, B::REDSTONE_WIRE | B::PRESSURE_PLATE | B::CAMPFIRE | B::BED) {
            return is_floor(self.get_block_ensured(wx, wy - 1, wz, tables));
        }
        if id == B::CARPET_RED {
            let below = self.get_block_ensured(wx, wy - 1, wz, tables);
            return below != B::AIR && !BLOCKS.get(below).liquid;
        }
        if blk.shape == "door" {
            return if (meta & 8) != 0 {
                self.get_block_ensured(wx, wy - 1, wz, tables) == id
            } else {
                is_floor(self.get_block_ensured(wx, wy - 1, wz, tables))
            };
        }
        if is_crop(id) {
            return self.get_block_ensured(wx, wy - 1, wz, tables) == B::FARMLAND;
        }
        if is_sapling(id) {
            return is_sapling_soil(self.get_block_ensured(wx, wy - 1, wz, tables));
        }
        if id == B::REEDS {
            let below = self.get_block_ensured(wx, wy - 1, wz, tables);
            return below == B::REEDS || is_floor(below);
        }
        if id == B::BAMBOO {
            let below = self.get_block_ensured(wx, wy - 1, wz, tables);
            return below == B::BAMBOO || is_floor(below);
        }
        if id == B::CACTUS {
            let below = self.get_block_ensured(wx, wy - 1, wz, tables);
            return matches!(below, B::CACTUS | B::SAND | B::RED_SAND);
        }
        if matches!(
            id,
            B::TALL_GRASS
                | B::POPPY
                | B::DANDELION
                | B::ORCHID
                | B::MUSHROOM_RED
                | B::MUSHROOM_BROWN
                | B::DEAD_BUSH
                | B::SEAGRASS
        ) {
            return is_floor(self.get_block_ensured(wx, wy - 1, wz, tables));
        }
        true
    }

    pub fn get_light_at(&mut self, wx: i32, wy: i32, wz: i32) -> (u8, u8) {
        let cx = floor_div(wx, CHUNK_SIZE);
        let cy = floor_div(wy, CHUNK_SIZE);
        let cz = floor_div(wz, CHUNK_SIZE);
        if let Some(c) = self.chunks.get(&(cx, cy, cz)) {
            let lb = c.light[Chunk::idx(
                floor_mod(wx, CHUNK_SIZE) as usize,
                floor_mod(wy, CHUNK_SIZE) as usize,
                floor_mod(wz, CHUNK_SIZE) as usize,
            )];
            return (lb >> 4, lb & 0x0f);
        }
        let col = self.generator.get_column_chunk(cx, cz);
        let surf_h = col.heights[((floor_mod(wz, CHUNK_SIZE) as usize) << 4) | (floor_mod(wx, CHUNK_SIZE) as usize)];
        (if wy > surf_h { 15 } else { 0 }, 0)
    }

    pub fn get_biome_at(&mut self, wx: i32, wz: i32) -> &'static BiomeDef {
        let cx = floor_div(wx, CHUNK_SIZE);
        let cz = floor_div(wz, CHUNK_SIZE);
        let col = self.generator.get_column_chunk(cx, cz);
        let id = col.biome_ids[((floor_mod(wz, CHUNK_SIZE) as usize) << 4) | (floor_mod(wx, CHUNK_SIZE) as usize)];
        &BIOMES[id as usize]
    }

    pub fn get_surface_height(&mut self, wx: i32, wz: i32) -> i32 {
        let cx = floor_div(wx, CHUNK_SIZE);
        let cz = floor_div(wz, CHUNK_SIZE);
        let col = self.generator.get_column_chunk(cx, cz);
        let lx = floor_mod(wx, CHUNK_SIZE) as usize;
        let lz = floor_mod(wz, CHUNK_SIZE) as usize;
        let base_h = col.heights[(lz << 4) | lx];
        let start_cy = floor_div(base_h + 32, CHUNK_SIZE);
        let end_cy = floor_div(base_h - 32, CHUNK_SIZE);
        for cy in (end_cy..=start_cy).rev() {
            if let Some(c) = self.chunks.get(&(cx, cy, cz)) {
                if c.is_empty { continue; }
                for ly in (0..CHUNK_SIZE as usize).rev() {
                    if BLOCKS.get(c.blocks[Chunk::idx(lx, ly, lz)]).solid {
                        return cy * CHUNK_SIZE + (ly as i32) + 1;
                    }
                }
            }
        }
        (SEA_LEVEL + 1).max(base_h + 1)
    }

    #[expect(clippy::too_many_arguments, reason = "Matches JS World.setBlock plus shared tables and change log")]
    pub fn set_block(
        &mut self,
        wx: i32,
        wy: i32,
        wz: i32,
        id: u8,
        meta: u8,
        record_delta: bool,
        tables: &mut SharedWorldTables,
        change_log: Option<&mut Vec<BlockChange>>,
    ) {
        self.ensure_chunk_at(wx, wy, wz, tables);
        let cx = floor_div(wx, CHUNK_SIZE);
        let cy = floor_div(wy, CHUNK_SIZE);
        let cz = floor_div(wz, CHUNK_SIZE);
        let lx = floor_mod(wx, CHUNK_SIZE);
        let ly = floor_mod(wy, CHUNK_SIZE);
        let lz = floor_mod(wz, CHUNK_SIZE);

        if record_delta {
            let mk = self.abs_chunk_key(cx, cy, cz);
            tables
                .modifications
                .entry(mk)
                .or_default()
                .insert(Chunk::idx(lx as usize, ly as usize, lz as usize), (id, meta));
        }

        let prev_id = if let Some(c) = self.chunks.get_mut(&(cx, cy, cz)) {
            let pid = c.get_block(lx, ly, lz);
            c.set_block(lx, ly, lz, id, meta);
            c.light_dirty = true;
            c.border_light_dirty = true;
            c.dirty = true;
            pid
        } else {
            return;
        };

        let prev_blk = BLOCKS.get(prev_id);
        let new_blk = BLOCKS.get(id);
        let opacity_changed = prev_blk.transparent != new_blk.transparent;
        let light_changed = prev_blk.light != new_blk.light || opacity_changed;
        let rad = if prev_blk.light > 0 || new_blk.light > 0 { 14 } else { 1 };
        let mut mark_n = |ncx: i32, ncy: i32, ncz: i32| {
            if let Some(nc) = self.chunks.get_mut(&(ncx, ncy, ncz)) {
                nc.dirty = true;
                if light_changed {
                    nc.light_dirty = true;
                    nc.border_light_dirty = true;
                }
            }
        };
        if lx <= rad { mark_n(cx - 1, cy, cz); }
        if lx >= CHUNK_SIZE - 1 - rad { mark_n(cx + 1, cy, cz); }
        if ly <= rad { mark_n(cx, cy - 1, cz); }
        if ly >= CHUNK_SIZE - 1 - rad { mark_n(cx, cy + 1, cz); }
        if lz <= rad { mark_n(cx, cy, cz - 1); }
        if lz >= CHUNK_SIZE - 1 - rad { mark_n(cx, cy, cz + 1); }

        if opacity_changed {
            for dcy in ((cy - 10)..cy).rev() {
                if let Some(below) = self.chunks.get_mut(&(cx, dcy, cz)) {
                    below.light_dirty = true;
                    below.border_light_dirty = true;
                    below.dirty = true;
                } else {
                    break;
                }
            }
        }

        let abs_key = self.abs_block_key(wx, wy, wz);
        if id == B::AIR && matches!(prev_id, B::CHEST | B::BARREL | B::FURNACE | B::FURNACE_LIT) {
            tables.block_entities.shift_remove(&abs_key);
        }

        if record_delta {
            if is_log(prev_id) && !is_log(id) {
                let mut ld = std::mem::take(&mut self.leaf_decay);
                ld.schedule_around(self, wx, wy, wz, 4, 1.2, 7.0);
                self.leaf_decay = ld;
            }
            if id == B::AIR || new_blk.liquid {
                let above = self.get_block(wx, wy + 1, wz);
                if BLOCKS.get(above).gravity {
                    self.gravity_queue.push_back((wx, wy + 1, wz));
                }
            } else if new_blk.gravity {
                self.gravity_queue.push_back((wx, wy, wz));
            }
            let ax = abs_json(&self.abs_x(wx));
            let az = abs_json(&self.abs_z(wz));
            let dkey = format!("{},{},{}", self.abs_x(wx), wy, self.abs_z(wz));
            tables.deltas.insert(dkey.clone(), (ax, wy, az, id, meta));
            if container_type(id).is_none() {
                tables.block_entities.shift_remove(&dkey);
            }
            tables.dirty = true;
            if let Some(log) = change_log {
                log.push((wx, wy, wz, id, meta));
            }
        }
    }

    pub fn relight_dirty(&mut self) {
        let dirty_keys: Vec<(i32, i32, i32)> = self
            .chunks
            .iter()
            .filter(|(_, c)| c.light_dirty)
            .map(|(&k, _)| k)
            .collect();
        for (cx, cy, cz) in dirty_keys {
            let (above_bottom, above_empty) = self.above_light_info(cx, cy, cz);
            let heights = self
                .chunks
                .get(&(cx, cy, cz))
                .and_then(|c| c.height_map.clone())
                .unwrap_or_else(|| self.generator.get_column_chunk(cx, cz).heights);
            if let Some(ch) = self.chunks.get_mut(&(cx, cy, cz)) {
                compute_local_light(ch, above_bottom.as_ref(), above_empty, &heights);
                ch.light_dirty = false;
            }
        }
    }

    pub fn process_gravity(
        &mut self,
        tables: &mut SharedWorldTables,
        mut change_log: Option<&mut Vec<BlockChange>>,
    ) {
        let mut n = 0;
        while let Some((x, y, z)) = self.gravity_queue.pop_front() {
            if n >= 256 {
                break;
            }
            n += 1;
            let id = self.get_block_ensured(x, y, z, tables);
            if !BLOCKS.get(id).gravity {
                continue;
            }
            let mut ty = y;
            while ty > y - 64 {
                let below = BLOCKS.get(self.get_block_ensured(x, ty - 1, z, tables));
                if below.id == 0 || below.liquid || below.cross {
                    ty -= 1;
                } else {
                    break;
                }
            }
            if ty != y {
                self.set_block(x, y, z, 0, 0, true, tables, change_log.as_deref_mut());
                self.set_block(x, ty, z, id, 0, true, tables, change_log.as_deref_mut());
            }
        }
    }

    pub fn unload_far(&mut self, player_positions: &[(f64, f64, f64)]) {
        const UNLOAD_H: i32 = 6;
        const UNLOAD_V: i32 = 4;
        self.chunks.retain(|&(cx, cy, cz), _| {
            for &(px, py, pz) in player_positions {
                let pcx = floor_div(px.floor() as i32, CHUNK_SIZE);
                let pcy = floor_div(py.floor() as i32, CHUNK_SIZE);
                let pcz = floor_div(pz.floor() as i32, CHUNK_SIZE);
                if (cx - pcx).abs() <= UNLOAD_H && (cz - pcz).abs() <= UNLOAD_H && (cy - pcy).abs() <= UNLOAD_V {
                    return true;
                }
            }
            false
        });
    }

    pub fn update_furnaces(
        &mut self,
        dt: f64,
        only_loaded: bool,
        done: &mut HashSet<String>,
        tables: &mut SharedWorldTables,
        mut change_log: Option<&mut Vec<BlockChange>>,
    ) {
        let keys: Vec<String> = tables.block_entities.keys().cloned().collect();
        let mut block_updates: Vec<(i32, i32, i32, u8, u8)> = Vec::new();
        for k in keys {
            if done.contains(&k) {
                continue;
            }
            let is_furnace = matches!(tables.block_entities.get(&k), Some(BlockEntity::Furnace { .. }));
            if !is_furnace {
                continue;
            }
            let loc = self.key_to_local(&k);
            if only_loaded {
                let Some((lx, ly, lz)) = loc else { continue };
                let ck = (floor_div(lx, CHUNK_SIZE), floor_div(ly, CHUNK_SIZE), floor_div(lz, CHUNK_SIZE));
                if !self.chunks.contains_key(&ck) {
                    continue;
                }
            }
            done.insert(k.clone());
            let (wx, wy, wz) = loc.unwrap_or((0, -100_000, 0));
            let Some(BlockEntity::Furnace { input, fuel, output, burn_time, burn_max, progress }) =
                tables.block_entities.get_mut(&k)
            else {
                continue;
            };
            let smelt_out = input.as_ref().and_then(|s| CRAFTING.smelt_map.get(&s.id).copied());
            let can_output = smelt_out.is_some_and(|so| match output.as_ref() {
                None => true,
                Some(o) => o.id == so && o.count < 64,
            });
            if *burn_time > 0.0 {
                *burn_time = (*burn_time - dt).max(0.0);
            }
            if *burn_time <= 0.0 && can_output
                && let Some(fl) = fuel.as_mut() {
                    let ft = ITEMS.get_fuel_time(fl.id);
                    if ft > 0.0 {
                        *burn_time = ft;
                        *burn_max = ft;
                        fl.count -= 1;
                        if fl.count <= 0 {
                            let fid = fl.id;
                            *fuel = if fid == IT::LAVA_BUCKET {
                                Some(ItemStack { id: IT::BUCKET, count: 1, dur: None, enchant: None })
                            } else {
                                None
                            };
                        }
                    }
                }
            let is_lit = *burn_time > 0.0;
            if loc.is_some() {
                let cur_blk = self.get_block(wx, wy, wz);
                if is_lit && cur_blk == B::FURNACE {
                    block_updates.push((wx, wy, wz, B::FURNACE_LIT, self.get_meta(wx, wy, wz)));
                } else if !is_lit && cur_blk == B::FURNACE_LIT {
                    block_updates.push((wx, wy, wz, B::FURNACE, self.get_meta(wx, wy, wz)));
                }
            }
            if is_lit && can_output {
                *progress += dt;
                if *progress >= 4.0 {
                    *progress = 0.0;
                    if let Some(inp) = input.as_mut() {
                        inp.count -= 1;
                        if inp.count <= 0 {
                            *input = None;
                        }
                    }
                    let so = smelt_out.unwrap();
                    match output.as_mut() {
                        None => *output = Some(ItemStack { id: so, count: 1, dur: None, enchant: None }),
                        Some(o) => o.count += 1,
                    }
                }
            } else {
                *progress = 0.0;
            }
        }
        for (wx, wy, wz, id, meta) in block_updates {
            self.set_block(wx, wy, wz, id, meta, true, tables, change_log.as_deref_mut());
        }
    }

    pub fn aabb_collides(&self, x: f64, y: f64, z: f64, hw: f64, h: f64) -> bool {
        let min_x = (x - hw).floor() as i32;
        let max_x = (x + hw).floor() as i32;
        let min_y = (y - 0.5).floor() as i32;
        let max_y = (y + h - 0.001).floor() as i32;
        let min_z = (z - hw).floor() as i32;
        let max_z = (z + hw).floor() as i32;
        let (px0, px1, py0, py1, pz0, pz1) = (x - hw, x + hw, y, y + h, z - hw, z + hw);

        for by in min_y..=max_y {
            for bz in min_z..=max_z {
                for bx in min_x..=max_x {
                    let id = self.get_block(bx, by, bz);
                    if id == 0 { continue; }
                    let Some(b) = BLOCKS.blocks[id as usize].as_ref() else { continue };
                    if !b.solid { continue; }
                    let meta = self.get_meta(bx, by, bz);
                    let byf = by as f64;
                    if b.shape == "fence" || b.shape == "fence_gate" {
                        if b.shape == "fence_gate" && (meta & 4) != 0 { continue; }
                        if py0 < byf + 1.5 && py1 > byf { return true; }
                        continue;
                    }
                    if by < py0.floor() as i32 { continue; }
                    if b.shape == "door" {
                        if (meta & 4) != 0 { continue; }
                        let facing = meta & 3;
                        let (mut dx0, mut dx1, mut dz0, mut dz1) = (bx as f64, (bx + 1) as f64, bz as f64, (bz + 1) as f64);
                        match facing {
                            0 => dz0 = (bz as f64) + 0.78,
                            1 => dx0 = (bx as f64) + 0.78,
                            2 => dz1 = (bz as f64) + 0.22,
                            _ => dx1 = (bx as f64) + 0.22,
                        }
                        if px0 < dx1 && px1 > dx0 && py0 < byf + 1.0 && py1 > byf && pz0 < dz1 && pz1 > dz0 {
                            return true;
                        }
                        continue;
                    }
                    if b.shape == "trapdoor" {
                        if (meta & 4) != 0 { continue; }
                        if py0 < byf + 1.0 && py1 > byf + 0.80 { return true; }
                        continue;
                    }
                    if b.shape == "slab" {
                        let top = (meta & 8) != 0;
                        let (b_min_y, b_max_y) = if top { (byf + 0.5, byf + 1.0) } else { (byf, byf + 0.5) };
                        if py0 < b_max_y && py1 > b_min_y { return true; }
                        continue;
                    }
                    if b.shape == "enchant_table" {
                        if py0 < byf + 0.76 && py1 > byf { return true; }
                        continue;
                    }
                    if b.shape == "campfire" {
                        if py0 < byf + 0.38 && py1 > byf { return true; }
                        continue;
                    }
                    return true;
                }
            }
        }
        false
    }
}

/// Adapter for running RedstoneSystem operations against CoreWorld + SharedWorldTables + TNT queue.
pub struct CoreRedstoneAdapter<'a> {
    pub world: &'a mut CoreWorld,
    pub tables: &'a mut SharedWorldTables,
    pub change_log: &'a mut Vec<(i32, i32, i32, u8, u8)>,
    pub tnt_triggers: Vec<(i32, i32, i32)>,
}

impl<'a> RedstoneWorld for CoreRedstoneAdapter<'a> {
    fn rs_get_block(&mut self, x: i32, y: i32, z: i32) -> u8 {
        self.world.get_block_ensured(x, y, z, self.tables)
    }
    fn rs_get_meta(&mut self, x: i32, y: i32, z: i32) -> u8 {
        self.world.get_meta_ensured(x, y, z, self.tables)
    }
    fn rs_set_block(&mut self, x: i32, y: i32, z: i32, id: u8, meta: u8) {
        self.world.set_block(x, y, z, id, meta, true, self.tables, Some(self.change_log));
    }
    fn rs_detonate_tnt(&mut self, x: i32, y: i32, z: i32) {
        self.tnt_triggers.push((x, y, z));
    }
}
