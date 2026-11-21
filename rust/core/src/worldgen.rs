use std::sync::Arc;

use indexmap::IndexMap;
use num_bigint::BigInt;
use num_traits::Zero;

use crate::blocks::B;
use crate::chunk::Chunk;
use crate::enchants::ItemStack;
use crate::items::{IT, ITEMS};
use crate::math::{
    AbsAxis, CHUNK_SIZE, NoiseEngine, NoiseFrame, ORIGIN_STEP_BIG, SEA_LEVEL, SeededRng, clamp,
    floor_div, floor_mod, hash_mix_y, hash2i, hash2w, lerp, smoothstep,
};
use crate::world::BlockEntity;

#[derive(Clone, Copy, Debug)]
pub struct BiomeDef {
    pub id: u8,
    pub name: &'static str,
    pub cat: &'static str,
    pub top: u8,
    pub sub: u8,
    pub tree_rate: f64,
    pub tree_type: &'static str,
    pub flowers: f64,
    pub no_hostiles: bool,
}

pub const BIOMES: [BiomeDef; 37] = [
    BiomeDef { id: 0, name: "Plains", cat: "temperate", top: B::GRASS, sub: B::DIRT, tree_rate: 0.0025, tree_type: "oak", flowers: 0.04, no_hostiles: false },
    BiomeDef { id: 1, name: "Forest", cat: "temperate", top: B::GRASS, sub: B::DIRT, tree_rate: 0.022, tree_type: "oak", flowers: 0.02, no_hostiles: false },
    BiomeDef { id: 2, name: "Dense Forest", cat: "temperate", top: B::GRASS, sub: B::DIRT, tree_rate: 0.036, tree_type: "darkoak", flowers: 0.015, no_hostiles: false },
    BiomeDef { id: 3, name: "Birch Forest", cat: "temperate", top: B::GRASS, sub: B::DIRT, tree_rate: 0.021, tree_type: "birch", flowers: 0.03, no_hostiles: false },
    BiomeDef { id: 4, name: "Flower Fields", cat: "temperate", top: B::GRASS, sub: B::DIRT, tree_rate: 0.004, tree_type: "birch", flowers: 0.19, no_hostiles: false },
    BiomeDef { id: 5, name: "Swamp", cat: "temperate", top: B::GRASS, sub: B::MUD, tree_rate: 0.014, tree_type: "swamp", flowers: 0.02, no_hostiles: false },
    BiomeDef { id: 6, name: "Meadow", cat: "temperate", top: B::GRASS, sub: B::DIRT, tree_rate: 0.005, tree_type: "oak", flowers: 0.09, no_hostiles: false },
    BiomeDef { id: 7, name: "Snowy Plains", cat: "cold", top: B::SNOW, sub: B::DIRT, tree_rate: 0.002, tree_type: "spruce", flowers: 0.0, no_hostiles: false },
    BiomeDef { id: 8, name: "Snowy Forest", cat: "cold", top: B::SNOW, sub: B::DIRT, tree_rate: 0.022, tree_type: "spruce", flowers: 0.0, no_hostiles: false },
    BiomeDef { id: 9, name: "Frozen Lakes", cat: "cold", top: B::SNOW, sub: B::PACKED_ICE, tree_rate: 0.003, tree_type: "spruce", flowers: 0.0, no_hostiles: false },
    BiomeDef { id: 10, name: "Ice Mountains", cat: "cold", top: B::SNOW, sub: B::PACKED_ICE, tree_rate: 0.004, tree_type: "spruce", flowers: 0.0, no_hostiles: false },
    BiomeDef { id: 11, name: "Tundra", cat: "cold", top: B::GRASS, sub: B::DIRT, tree_rate: 0.009, tree_type: "spruce", flowers: 0.005, no_hostiles: false },
    BiomeDef { id: 12, name: "Desert", cat: "warm", top: B::SAND, sub: B::SANDSTONE, tree_rate: 0.003, tree_type: "cactus", flowers: 0.0, no_hostiles: false },
    BiomeDef { id: 13, name: "Savanna", cat: "warm", top: B::GRASS, sub: B::DIRT, tree_rate: 0.009, tree_type: "acacia", flowers: 0.01, no_hostiles: false },
    BiomeDef { id: 14, name: "Tropical Jungle", cat: "warm", top: B::GRASS, sub: B::DIRT, tree_rate: 0.034, tree_type: "jungle", flowers: 0.04, no_hostiles: false },
    BiomeDef { id: 15, name: "Badlands", cat: "warm", top: B::RED_SAND, sub: B::TERRACOTTA, tree_rate: 0.002, tree_type: "cactus", flowers: 0.0, no_hostiles: false },
    BiomeDef { id: 16, name: "Sparse Desert", cat: "warm", top: B::SAND, sub: B::SANDSTONE, tree_rate: 0.0012, tree_type: "cactus", flowers: 0.0, no_hostiles: false },
    BiomeDef { id: 17, name: "Mountain Peaks", cat: "mountain", top: B::STONE, sub: B::STONE, tree_rate: 0.002, tree_type: "spruce", flowers: 0.005, no_hostiles: false },
    BiomeDef { id: 18, name: "Rocky Mountains", cat: "mountain", top: B::STONE, sub: B::STONE, tree_rate: 0.005, tree_type: "spruce", flowers: 0.01, no_hostiles: false },
    BiomeDef { id: 19, name: "Alpine Meadows", cat: "mountain", top: B::GRASS, sub: B::DIRT, tree_rate: 0.008, tree_type: "spruce", flowers: 0.08, no_hostiles: false },
    BiomeDef { id: 20, name: "Snow-Capped Mountains", cat: "mountain", top: B::SNOW, sub: B::STONE, tree_rate: 0.003, tree_type: "spruce", flowers: 0.0, no_hostiles: false },
    BiomeDef { id: 21, name: "Deep Ocean", cat: "aquatic", top: B::GRAVEL, sub: B::STONE, tree_rate: 0.0, tree_type: "none", flowers: 0.0, no_hostiles: false },
    BiomeDef { id: 22, name: "Ocean", cat: "aquatic", top: B::SAND, sub: B::SANDSTONE, tree_rate: 0.0, tree_type: "none", flowers: 0.0, no_hostiles: false },
    BiomeDef { id: 23, name: "Coral Reef", cat: "aquatic", top: B::SAND, sub: B::SANDSTONE, tree_rate: 0.0, tree_type: "none", flowers: 0.0, no_hostiles: false },
    BiomeDef { id: 24, name: "Warm Ocean", cat: "aquatic", top: B::SAND, sub: B::SANDSTONE, tree_rate: 0.0, tree_type: "none", flowers: 0.0, no_hostiles: false },
    BiomeDef { id: 25, name: "Cold Ocean", cat: "aquatic", top: B::GRAVEL, sub: B::STONE, tree_rate: 0.0, tree_type: "none", flowers: 0.0, no_hostiles: false },
    BiomeDef { id: 26, name: "River", cat: "aquatic", top: B::SAND, sub: B::CLAY, tree_rate: 0.0, tree_type: "none", flowers: 0.0, no_hostiles: false },
    BiomeDef { id: 27, name: "Cherry Blossom Grove", cat: "temperate", top: B::GRASS, sub: B::DIRT, tree_rate: 0.019, tree_type: "cherry", flowers: 0.14, no_hostiles: false },
    BiomeDef { id: 28, name: "Volcanic Wastes", cat: "volcanic", top: B::BASALT, sub: B::BASALT, tree_rate: 0.005, tree_type: "basalt_spire", flowers: 0.0, no_hostiles: false },
    BiomeDef { id: 29, name: "Mangrove Swamp", cat: "temperate", top: B::MUD, sub: B::MUD, tree_rate: 0.024, tree_type: "mangrove", flowers: 0.02, no_hostiles: false },
    BiomeDef { id: 30, name: "Bamboo Jungle", cat: "warm", top: B::GRASS, sub: B::DIRT, tree_rate: 0.038, tree_type: "bamboo", flowers: 0.03, no_hostiles: false },
    BiomeDef { id: 31, name: "Redwood Giant Taiga", cat: "cold", top: B::PODZOL, sub: B::COARSE_DIRT, tree_rate: 0.016, tree_type: "redwood", flowers: 0.01, no_hostiles: false },
    BiomeDef { id: 32, name: "Mushroom Island", cat: "exotic", top: B::MYCELIUM, sub: B::DIRT, tree_rate: 0.016, tree_type: "giant_shroom", flowers: 0.0, no_hostiles: true },
    BiomeDef { id: 33, name: "Autumn Woods", cat: "temperate", top: B::GRASS, sub: B::DIRT, tree_rate: 0.024, tree_type: "autumn", flowers: 0.03, no_hostiles: false },
    BiomeDef { id: 34, name: "Crystal Highlands", cat: "mountain", top: B::CALCITE, sub: B::STONE, tree_rate: 0.009, tree_type: "crystal_spire", flowers: 0.01, no_hostiles: false },
    BiomeDef { id: 35, name: "Desert Oasis", cat: "warm", top: B::GRASS, sub: B::SAND, tree_rate: 0.018, tree_type: "palm", flowers: 0.06, no_hostiles: false },
    BiomeDef { id: 36, name: "Shattered Cliffs", cat: "mountain", top: B::GRASS, sub: B::STONE, tree_rate: 0.008, tree_type: "spruce", flowers: 0.02, no_hostiles: false },
];

#[derive(Clone, Copy, Debug)]
pub struct Climate {
    pub cont: f64,
    pub temp: f64,
    pub moist: f64,
    pub eros: f64,
    pub ridge: f64,
    pub river_val: f64,
    pub weird: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct ColumnSample {
    pub height: i32,
    pub biome: &'static BiomeDef,
    pub climate: Climate,
}

#[derive(Clone)]
pub struct OriginHashes {
    pub x1: AbsAxis,
    pub z1: AbsAxis,
    pub x4: AbsAxis,
    pub z4: AbsAxis,
    pub xc: AbsAxis,
    pub zc: AbsAxis,
}

impl Default for OriginHashes {
    fn default() -> Self {
        Self::new()
    }
}

impl OriginHashes {
    pub fn new() -> Self {
        let mut s = Self {
            x1: AbsAxis::new(BigInt::zero()),
            z1: AbsAxis::new(BigInt::zero()),
            x4: AbsAxis::new(BigInt::zero()),
            z4: AbsAxis::new(BigInt::zero()),
            xc: AbsAxis::new(BigInt::zero()),
            zc: AbsAxis::new(BigInt::zero()),
        };
        s.set(&BigInt::zero(), &BigInt::zero());
        s
    }

    pub fn set(&mut self, ox: &BigInt, oz: &BigInt) {
        self.x1 = AbsAxis::new(ox.clone());
        self.z1 = AbsAxis::new(oz.clone());
        self.x4 = AbsAxis::new(ox / 4);
        self.z4 = AbsAxis::new(oz / 4);
        self.xc = AbsAxis::new(ox / 160);
        self.zc = AbsAxis::new(oz / 160);
    }

    fn h(ax: &mut AbsAxis, az: &mut AbsAxis, lx: i64, lz: i64, seed: u32) -> u32 {
        if let (Some(an), Some(bn)) = (ax.num, az.num) {
            return hash2i(an + lx, bn + lz, seed);
        }
        ax.words(lx);
        az.words(lz);
        let ext = if ax.w_ext != 0 || az.w_ext != 0 {
            ((ax.w_ext ^ 0x5bd1_e995).wrapping_mul(31) ^ az.w_ext) | 1
        } else {
            0
        };
        hash2w(ax.w_lo, ax.w_hi, az.w_lo, az.w_hi, seed, ext)
    }

    pub fn h2(&mut self, lx: i32, lz: i32, seed: u32) -> u32 {
        Self::h(&mut self.x1, &mut self.z1, lx as i64, lz as i64, seed)
    }

    pub fn h3(&mut self, lx: i32, y: i32, lz: i32, seed: u32) -> u32 {
        hash_mix_y(self.h2(lx, lz, seed), y)
    }

    pub fn h2q(&mut self, lx: i32, lz: i32, seed: u32) -> u32 {
        Self::h(
            &mut self.x4,
            &mut self.z4,
            floor_div(lx, 4) as i64,
            floor_div(lz, 4) as i64,
            seed,
        )
    }

    pub fn h3q(&mut self, lx: i32, y: i32, lz: i32, seed: u32) -> u32 {
        hash_mix_y(self.h2q(lx, lz, seed), y >> 2)
    }

    pub fn h_cell(&mut self, cx: i32, cz: i32, seed: u32) -> u32 {
        Self::h(&mut self.xc, &mut self.zc, cx as i64, cz as i64, seed)
    }
}

#[derive(Clone)]
pub struct ColumnChunkData {
    pub heights: Arc<[i32; 256]>,
    pub biome_ids: Arc<[u8; 256]>,
    pub ug_biomes: Arc<[u8; 256]>,
    pub min_h: i32,
    pub max_h: i32,
}

struct CellStructure {
    kind: &'static str,
    wx: i32,
    wy: i32,
    wz: i32,
    seed: u32,
    biome: &'static BiomeDef,
}

struct UndergroundStructure {
    kind: &'static str,
    wx: i32,
    wy: i32,
    wz: i32,
    seed: u32,
}

#[derive(Clone)]
pub struct WorldGenerator {
    pub seed: u32,
    pub ox: BigInt,
    pub oz: BigInt,
    n_cont: NoiseEngine,
    n_temp: NoiseEngine,
    n_moist: NoiseEngine,
    n_eros: NoiseEngine,
    n_ridge: NoiseEngine,
    n_river: NoiseEngine,
    n_weird: NoiseEngine,
    n_detail: NoiseEngine,
    n_cliff: NoiseEngine,
    n_spag1: NoiseEngine,
    n_spag2: NoiseEngine,
    n_cheese: NoiseEngine,
    n_region: NoiseEngine,
    n_ore: NoiseEngine,
    pub frame: NoiseFrame,
    pub abs: OriginHashes,
    col_cache: IndexMap<(i32, i32), ColumnChunkData>,
}

impl WorldGenerator {
    pub fn new(seed: u32) -> Self {
        let mut g = Self {
            seed,
            ox: BigInt::zero(),
            oz: BigInt::zero(),
            n_cont: NoiseEngine::new(seed ^ 0x11a5c3),
            n_temp: NoiseEngine::new(seed ^ 0x29b7e1),
            n_moist: NoiseEngine::new(seed ^ 0x47d2f9),
            n_eros: NoiseEngine::new(seed ^ 0x63f8a5),
            n_ridge: NoiseEngine::new(seed ^ 0x85c1b7),
            n_river: NoiseEngine::new(seed ^ 0xa3e9d1),
            n_weird: NoiseEngine::new(seed ^ 0xc7e391),
            n_detail: NoiseEngine::new(seed ^ 0x55aa33),
            n_cliff: NoiseEngine::new(seed ^ 0x99cc77),
            n_spag1: NoiseEngine::new(seed ^ 0x314159),
            n_spag2: NoiseEngine::new(seed ^ 0x265358),
            n_cheese: NoiseEngine::new(seed ^ 0x979323),
            n_region: NoiseEngine::new(seed ^ 0x846264),
            n_ore: NoiseEngine::new(seed ^ 0x338327),
            frame: NoiseFrame::new(),
            abs: OriginHashes::new(),
            col_cache: IndexMap::new(),
        };
        g.set_origin(BigInt::zero(), BigInt::zero());
        g
    }

    pub fn set_origin(&mut self, ox: BigInt, oz: BigInt) {
        debug_assert!((&ox % &*ORIGIN_STEP_BIG).is_zero());
        debug_assert!((&oz % &*ORIGIN_STEP_BIG).is_zero());
        self.ox = ox;
        self.oz = oz;
        self.abs.set(&self.ox, &self.oz);
        self.frame.set_origin(self.abs.x1.clone(), self.abs.z1.clone());
        self.col_cache.clear();
    }

    pub fn sample_climate(&mut self, lx: i32, lz: i32) -> Climate {
        self.frame.set(lx as f64, lz as f64);
        let f = &self.frame;
        let wx = f.xq;
        let wz = f.zq;
        let warp_x = self.n_eros.s2(f, wx, wz, 0.0018, 0.0, 0.0) * 38.0;
        let warp_z = self.n_eros.s2(f, wx, wz, 0.0018, 73.0, -41.0) * 38.0;
        let x = wx + warp_x;
        let z = wz + warp_z;
        let cont = self.n_cont.fbm2_f(f, x, z, 0.0014, 4);
        let temp = self.n_temp.fbm2_f(f, x, z, 0.0012, 3);
        let moist = self.n_moist.fbm2_f(f, x, z, 0.0013, 3);
        let eros = self.n_eros.fbm2_f(f, x, z, 0.0022, 3);
        let ridge = self.n_ridge.ridged2_f(f, wx, wz, 0.0032, 4);
        let river_val = self.n_river.fbm2_f(f, wx, wz, 0.0019, 3).abs();
        let weird = self.n_weird.fbm2_f(f, x, z, 0.0017, 3);
        Climate {
            cont,
            temp,
            moist,
            eros,
            ridge,
            river_val,
            weird,
        }
    }

    pub fn get_biome_from_climate(c: Climate, height: i32) -> &'static BiomeDef {
        let Climate { cont, temp, moist, eros, ridge, river_val, weird } = c;
        if cont < -0.24 && cont > -0.36 && weird > 0.46 { return &BIOMES[32]; }
        if cont < -0.34 { return &BIOMES[21]; }
        if cont < -0.14 {
            if temp < -0.28 { return &BIOMES[25]; }
            if temp > 0.32 && moist > 0.08 { return &BIOMES[23]; }
            if temp > 0.25 { return &BIOMES[24]; }
            return &BIOMES[22];
        }
        if river_val < 0.042 && cont > -0.10 && height <= SEA_LEVEL + 2 { return &BIOMES[26]; }
        if temp > 0.22 && weird > 0.38 && cont > 0.04 { return &BIOMES[28]; }
        if height > 78 || (cont > 0.24 && ridge > 0.56 && eros < 0.06) {
            if weird < -0.28 && height > 68 { return &BIOMES[34]; }
            if weird > 0.22 && eros < -0.12 { return &BIOMES[36]; }
            if temp < -0.18 || height > 92 { return &BIOMES[20]; }
            if ridge > 0.70 { return &BIOMES[17]; }
            if moist > 0.12 && height < 82 { return &BIOMES[19]; }
            return &BIOMES[18];
        }
        if temp < -0.22 {
            if ridge > 0.55 && cont > 0.1 { return &BIOMES[10]; }
            if height <= SEA_LEVEL + 3 && moist > 0.15 { return &BIOMES[9]; }
            if weird > 0.12 && moist > 0.04 { return &BIOMES[31]; }
            if moist > 0.18 { return &BIOMES[8]; }
            if temp > -0.40 && moist < -0.05 { return &BIOMES[11]; }
            return &BIOMES[7];
        }
        if temp > 0.22 {
            if moist > 0.22 { return if weird > 0.08 { &BIOMES[30] } else { &BIOMES[14] }; }
            if moist > -0.06 { return &BIOMES[13]; }
            if weird < -0.34 && height <= SEA_LEVEL + 7 { return &BIOMES[35]; }
            if eros < -0.12 && temp > 0.34 { return &BIOMES[15]; }
            if moist < -0.35 { return &BIOMES[16]; }
            return &BIOMES[12];
        }
        if height <= SEA_LEVEL + 4 && moist > 0.24 && eros > 0.04 {
            return if weird > 0.0 { &BIOMES[29] } else { &BIOMES[5] };
        }
        if weird > 0.26 && moist > -0.05 { return &BIOMES[27]; }
        if weird < -0.24 && moist > 0.04 { return &BIOMES[33]; }
        if moist > 0.34 { return &BIOMES[2]; }
        if moist > 0.14 { return if eros > 0.08 { &BIOMES[3] } else { &BIOMES[1] }; }
        if moist < -0.22 && eros > 0.12 { return &BIOMES[4]; }
        if eros < -0.05 { return &BIOMES[6]; }
        &BIOMES[0]
    }

    pub fn sample_column(&mut self, lx: i32, lz: i32) -> ColumnSample {
        let c = self.sample_climate(lx, lz);
        let f = &self.frame;
        let wx = f.xq;
        let wz = f.zq;
        let Climate { cont, temp, moist, eros, ridge, river_val, weird } = c;
        let sea = SEA_LEVEL as f64;

        let mut base_h = if cont < -0.34 {
            22.0 + (cont + 0.7) * 28.0
        } else if cont < -0.14 {
            let t = smoothstep((cont + 0.34) / 0.20);
            let mut bh = lerp(31.0, sea - 2.0, t);
            if cont > -0.36 && weird > 0.46 {
                let m_boost = smoothstep(clamp((weird - 0.46) / 0.18, 0.0, 1.0));
                bh = lerp(bh, sea + 9.0, m_boost);
            }
            bh
        } else if cont < -0.04 {
            let t = smoothstep((cont + 0.14) / 0.10);
            lerp(sea - 2.0, sea + 5.0, t)
        } else {
            let inland = ((cont + 0.04) / 0.65).min(1.0);
            sea + 5.0 + inland * 20.0
        };

        let mtn_factor = clamp((cont + 0.05) * 1.4, 0.0, 1.0) * clamp((0.35 - eros) * 1.4, 0.0, 1.0);
        let peaks = clamp(ridge, 0.0, 1.0).powf(1.65) * 48.0 * mtn_factor;
        let hill_amp = lerp(12.0, 3.5, clamp((eros + 0.6) / 1.2, 0.0, 1.0)) * clamp((cont + 0.25) * 2.0, 0.15, 1.0);
        let hills = self.n_detail.fbm2_f(f, wx, wz, 0.009, 4) * hill_amp;

        base_h += peaks + hills;
        if temp > 0.32 && moist < -0.08 && eros < -0.05 && cont > 0.02 {
            let mesa = clamp((self.n_cliff.s2(f, wx, wz, 0.006, 0.0, 0.0) + 0.2) * 2.5, 0.0, 1.0);
            base_h += (mesa * 4.0).floor() * 4.5;
        }
        if weird > 0.22 && eros < -0.12 && cont > 0.18 {
            let spike = (self.n_cliff.ridged2_f(f, wx, wz, 0.012, 3) - 0.45).max(0.0);
            base_h += spike * 26.0;
        }
        if temp > 0.22 && moist <= -0.06 && weird < -0.34 && cont > -0.05 {
            let ob = smoothstep(clamp((-0.34 - weird) / 0.22, 0.0, 1.0));
            base_h = lerp(base_h, sea - 1.5, ob * 0.72);
        }
        if cont > -0.12 && river_val < 0.075 {
            let rt = 1.0 - smoothstep(river_val / 0.075);
            let river_bed = sea - 5.0;
            base_h = lerp(base_h, base_h.min(river_bed), rt * 0.92);
        }
        let canyon = self.n_cliff.s2(f, wx, wz, 0.0028, 140.0, -90.0).abs();
        if cont > 0.12 && canyon < 0.032 {
            let ct = 1.0 - smoothstep(canyon / 0.032);
            base_h -= ct * 18.0;
        }

        let height = base_h.floor() as i32;
        let biome = Self::get_biome_from_climate(c, height);
        ColumnSample { height, biome, climate: c }
    }

    pub fn sample_cave(&mut self, lx: i32, wy: i32, lz: i32, surf_h: i32) -> u8 {
        if wy >= surf_h - 2 {
            return 0;
        }
        self.frame.set(lx as f64, lz as f64);
        let f = &self.frame;
        let wx = f.xq;
        let wz = f.zq;
        let yf = wy as f64;
        let s1 = self.n_spag1.s3(f, wx, yf, wz, 0.028, 0.042, 0.0, 0.0, 0.0);
        let s2 = self.n_spag2.s3(f, wx, yf, wz, 0.028, 0.042, 41.0, -19.0, 73.0);
        if s1 * s1 + s2 * s2 < 0.022 {
            return 1;
        }
        if wy < (surf_h - 8).min(42) && self.n_cheese.s3(f, wx, yf, wz, 0.020, 0.028, 0.0, 0.0, 0.0) > 0.56 {
            return 1;
        }
        if wy < surf_h - 3 && wy > -128 {
            let r1 = self.n_spag1.s2(f, wx, wz, 0.011, 200.0, -150.0);
            let r2 = self.n_spag2.s2(f, wx, wz, 0.008, -80.0, 110.0);
            if r1.abs() < 0.026 && r2 > 0.42 {
                return 1;
            }
        }
        if wy < 30 {
            let g = self.n_region.s3(f, wx, yf, wz, 0.045, 0.045, 12.0, 0.0, -12.0);
            if g > 0.80 { return 1; }
            if g > 0.75 { return 3; }
            if g > 0.71 { return 2; }
        }
        0
    }

    pub fn get_underground_biome(&mut self, lx: i32, lz: i32) -> u8 {
        self.frame.set(lx as f64, lz as f64);
        let v = self.n_region.s2(&self.frame, self.frame.xq, self.frame.zq, 0.006, 0.0, 0.0);
        if v > 0.48 { 1 } else if v < -0.52 { 2 } else if v > 0.15 && v < 0.28 { 3 } else { 0 }
    }

    pub fn pick_ore(&mut self, lx: i32, wy: i32, lz: i32, biome_id: u8) -> u8 {
        self.frame.set(lx as f64, lz as f64);
        let n = self.n_ore.s3(&self.frame, self.frame.xq, wy as f64, self.frame.zq, 0.14, 0.14, 0.0, 0.0, 0.0);
        if n < 0.58 {
            return 0;
        }
        let h = self.abs.h3(lx, wy, lz, self.seed ^ 0x777);
        let r = ((h & 1023) as f64) / 1024.0;
        if wy <= 12 && n > 0.75 && r < 0.08 { return B::ANCIENT_DEBRIS; }
        if wy <= 18 && n > 0.70 && r < 0.24 { return B::DIAMOND_ORE; }
        if wy <= 22 && n > 0.67 && r < 0.34 { return B::REDSTONE_ORE; }
        if wy <= 30 && n > 0.68 && r < 0.26 { return B::GOLD_ORE; }
        if wy <= 32 && n > 0.69 && r < 0.22 { return B::LAPIS_ORE; }
        if matches!(biome_id, 17 | 18 | 20 | 34 | 36) && wy <= 48 && n > 0.73 && r < 0.28 { return B::EMERALD_ORE; }
        if wy <= 54 && n > 0.62 && r < 0.46 { return B::IRON_ORE; }
        if (-32..=64).contains(&wy) && n > 0.63 && r < 0.38 { return B::COPPER_ORE; }
        if (-64..=78).contains(&wy) && n > 0.59 && r < 0.60 { return B::COAL_ORE; }
        0
    }

    pub fn get_column_chunk(&mut self, cx: i32, cz: i32) -> ColumnChunkData {
        if let Some(d) = self.col_cache.get(&(cx, cz)) {
            return d.clone();
        }
        if self.col_cache.len() > 600 {
            self.col_cache.shift_remove_index(0);
        }
        let ox = cx * CHUNK_SIZE;
        let oz = cz * CHUNK_SIZE;
        let mut heights = [0i32; 256];
        let mut biome_ids = [0u8; 256];
        let mut ug_biomes = [0u8; 256];
        let mut min_h = i32::MAX;
        let mut max_h = i32::MIN;
        for lz in 0..CHUNK_SIZE {
            let wz = oz + lz;
            for lx in 0..CHUNK_SIZE {
                let wx = ox + lx;
                let col = self.sample_column(wx, wz);
                let c_idx = ((lz as usize) << 4) | (lx as usize);
                heights[c_idx] = col.height;
                biome_ids[c_idx] = col.biome.id;
                ug_biomes[c_idx] = self.get_underground_biome(wx, wz);
                if col.height < min_h { min_h = col.height; }
                if col.height > max_h { max_h = col.height; }
            }
        }
        let data = ColumnChunkData {
            heights: Arc::new(heights),
            biome_ids: Arc::new(biome_ids),
            ug_biomes: Arc::new(ug_biomes),
            min_h,
            max_h,
        };
        self.col_cache.insert((cx, cz), data.clone());
        data
    }

    fn get_cell_structure(&mut self, cell_x: i32, cell_z: i32) -> Option<CellStructure> {
        const CELL: i32 = 10;
        let h = self.abs.h_cell(cell_x, cell_z, self.seed ^ 0x51a7c3);
        if (h % 100) > 68 {
            return None;
        }
        let mut rng = SeededRng::new(h);
        let local_cx = 2 + (rng.next() * ((CELL - 4) as f64)).floor() as i32;
        let local_cz = 2 + (rng.next() * ((CELL - 4) as f64)).floor() as i32;
        let wx = (cell_x * CELL + local_cx) * CHUNK_SIZE + 8;
        let wz = (cell_z * CELL + local_cz) * CHUNK_SIZE + 8;
        let col = self.sample_column(wx, wz);
        let b = col.biome;

        let kind = if b.cat == "aquatic" {
            if col.height < SEA_LEVEL - 4 { "shipwreck" } else { return None; }
        } else if matches!(b.id, 12 | 15 | 16) {
            if rng.next() < 0.55 { "temple" } else { "ruined_portal" }
        } else if b.id == 14 {
            if rng.next() < 0.5 { "jungle_temple" } else { "watchtower" }
        } else if matches!(b.id, 0 | 3 | 6 | 13 | 7) {
            let r = rng.next();
            if r < 0.36 { "village" } else if r < 0.62 { "watchtower" } else if r < 0.80 { "ruined_portal" } else { "campsite" }
        } else {
            let r = rng.next();
            if r < 0.38 { "watchtower" } else if r < 0.68 { "cabin" } else { "ruined_portal" }
        };
        Some(CellStructure { kind, wx, wy: col.height, wz, seed: h, biome: b })
    }

    fn get_underground_structure(&mut self, cell_x: i32, cell_z: i32) -> Option<UndergroundStructure> {
        const CELL: i32 = 10;
        let h = self.abs.h_cell(cell_x, cell_z, self.seed ^ 0x993311);
        if (h % 100) > 55 {
            return None;
        }
        let mut rng = SeededRng::new(h);
        let wx = (cell_x * CELL + 2 + (rng.next() * 6.0).floor() as i32) * CHUNK_SIZE + 8;
        let wz = (cell_z * CELL + 2 + (rng.next() * 6.0).floor() as i32) * CHUNK_SIZE + 8;
        let wy = 14 + (rng.next() * 14.0).floor() as i32;
        let kind = if rng.next() < 0.55 { "mineshaft" } else { "dungeon" };
        Some(UndergroundStructure { kind, wx, wy, wz, seed: h })
    }

    pub fn generate_loot(seed: u32, tier: i32) -> BlockEntity {
        let mut rng = SeededRng::new(seed);
        let mut slots: Vec<Option<ItemStack>> = vec![None; 27];
        let pool: &[(i32, i32, i32)] = if tier >= 2 {
            &[
                (IT::DIAMOND, 1, 3), (IT::IRON_INGOT, 2, 7), (IT::GOLD_INGOT, 2, 6),
                (IT::EMERALD, 1, 3), (IT::GOLDEN_APPLE, 1, 2), (IT::IRON_SWORD, 1, 1),
                (IT::IRON_PICK, 1, 1), (IT::COAL, 4, 14), (B::TORCH as i32, 4, 12), (IT::BREAD, 2, 6),
                (IT::REDSTONE, 4, 10), (IT::AMETHYST_SHARD, 2, 5), (IT::ARROW, 6, 16),
            ]
        } else {
            &[
                (IT::BREAD, 2, 6), (IT::APPLE, 2, 5), (IT::COAL, 3, 10), (IT::IRON_INGOT, 1, 4),
                (B::OAK_PLANKS as i32, 6, 18), (B::TORCH as i32, 4, 10), (IT::STICK, 4, 12), (IT::LEATHER, 2, 5),
                (IT::STONE_PICK, 1, 1), (IT::STEAK, 1, 4), (IT::ARROW, 4, 10), (IT::STRING, 2, 6),
            ]
        };
        let items = rng.int(4, 8);
        for _ in 0..items {
            let &(id, lo, hi) = rng.pick(pool);
            let slot_idx = rng.int(0, 26) as usize;
            let count = rng.int(lo, hi);
            let dur = ITEMS.get(id).and_then(|it| if it.max_dur > 0 { Some(it.max_dur) } else { None });
            slots[slot_idx] = Some(ItemStack { id, count, dur, enchant: None });
        }
        BlockEntity::Chest { slots }
    }

    /// Generates a chunk's blocks and returns any newly generated loot chests `(wx, wy, wz, entity)`.
    pub fn generate_chunk_blocks(&mut self, chunk: &mut Chunk) -> Vec<(i32, i32, i32, BlockEntity)> {
        let mut generated_chests = Vec::new();
        let (cx, cy, cz) = (chunk.cx, chunk.cy, chunk.cz);
        let (ox, oy, oz) = (cx * CHUNK_SIZE, cy * CHUNK_SIZE, cz * CHUNK_SIZE);
        let (min_y, max_y) = (oy, oy + CHUNK_SIZE - 1);
        let col_data = self.get_column_chunk(cx, cz);
        let heights = col_data.heights.clone();
        let biome_ids = col_data.biome_ids.clone();
        let ug_biomes = col_data.ug_biomes.clone();
        chunk.biome_map = Some(biome_ids.clone());
        chunk.height_map = Some(heights.clone());

        if min_y > col_data.max_h + 28 && min_y > SEA_LEVEL {
            return generated_chests;
        }

        // 1. Base terrain, biomes, oceans, 3D caves & ores
        for lz in 0..CHUNK_SIZE {
            let wz = oz + lz;
            for lx in 0..CHUNK_SIZE {
                let wx = ox + lx;
                let c_idx = ((lz as usize) << 4) | (lx as usize);
                let surf_h = heights[c_idx];
                let biome = &BIOMES[biome_ids[c_idx] as usize];
                let ug_biome = ug_biomes[c_idx];
                let near_water = surf_h <= SEA_LEVEL + 1 && biome.cat != "cold" && biome.id != 5;

                if min_y > surf_h && min_y > SEA_LEVEL {
                    continue;
                }

                for ly in 0..CHUNK_SIZE {
                    let y = oy + ly;
                    if y <= surf_h {
                        let cave = self.sample_cave(wx, y, wz, surf_h);
                        if cave == 1 {
                            let lava_band = y <= 9
                                && floor_mod(y, 64) <= 3
                                && (y >= -4 || (self.abs.h2q(wx, wz, self.seed ^ 0x1a7a) & 3) == 0);
                            if lava_band {
                                chunk.set_block(lx, ly, lz, B::LAVA, 0);
                            } else if (-32..=18).contains(&y) && ug_biome == 3 {
                                chunk.set_block(lx, ly, lz, B::WATER, 0);
                            } else {
                                chunk.set_block(lx, ly, lz, B::AIR, 0);
                            }
                            continue;
                        }
                        if cave == 2 { chunk.set_block(lx, ly, lz, B::CALCITE, 0); continue; }
                        if cave == 3 { chunk.set_block(lx, ly, lz, B::AMETHYST, 0); continue; }

                        if y == surf_h {
                            let b = if biome.id == 15 {
                                match floor_mod(y >> 1, 3) {
                                    0 => B::RED_SAND,
                                    1 => B::RED_TERRACOTTA,
                                    _ => B::YEL_TERRACOTTA,
                                }
                            } else if biome.id == 28 {
                                let vh = self.abs.h2(wx, wz, self.seed ^ 0x5511) % 20;
                                if vh == 0 { B::LAVA } else if vh <= 3 { B::MAGMA_BLOCK } else if vh <= 9 { B::ASH_BLOCK } else { B::BASALT }
                            } else if biome.id == 34 {
                                let ch = self.abs.h2(wx, wz, self.seed ^ 0x9123) % 16;
                                if ch == 0 { B::AMETHYST } else if ch <= 8 { B::CALCITE } else { B::STONE }
                            } else if biome.id == 29 {
                                let mh = self.abs.h2(wx, wz, self.seed ^ 0x3377) % 10;
                                if mh < 3 { B::MOSS } else if mh < 6 { B::MUD } else { B::GRASS }
                            } else if near_water && biome.top == B::GRASS && biome.id != 35 {
                                B::SAND
                            } else if surf_h > 88 && biome.cat == "mountain" && biome.id != 34 {
                                B::SNOW
                            } else {
                                biome.top
                            };
                            chunk.set_block(lx, ly, lz, b, 0);
                        } else if y >= surf_h - 3 {
                            let b = if biome.id == 15 {
                                match floor_mod(y >> 1, 3) {
                                    0 => B::TERRACOTTA,
                                    1 => B::RED_TERRACOTTA,
                                    _ => B::YEL_TERRACOTTA,
                                }
                            } else if near_water && biome.sub == B::DIRT {
                                B::SAND
                            } else {
                                biome.sub
                            };
                            chunk.set_block(lx, ly, lz, b, 0);
                        } else {
                            let ore = self.pick_ore(wx, y, wz, biome.id);
                            if ore != 0 {
                                chunk.set_block(lx, ly, lz, ore, 0);
                            } else if y < -64 && (self.abs.h3q(wx, y, wz, self.seed ^ 0x6a5a) & 3) == 0 {
                                chunk.set_block(lx, ly, lz, B::BASALT, 0);
                            } else if y < 20 {
                                chunk.set_block(lx, ly, lz, B::DEEPSLATE, 0);
                            } else {
                                chunk.set_block(lx, ly, lz, B::STONE, 0);
                            }
                        }
                    } else if y <= SEA_LEVEL {
                        if biome.id == 28 && y < SEA_LEVEL {
                            chunk.set_block(lx, ly, lz, B::LAVA, 0);
                        } else if y == SEA_LEVEL && matches!(biome.id, 9 | 7 | 25) {
                            chunk.set_block(lx, ly, lz, B::ICE, 0);
                        } else {
                            chunk.set_block(lx, ly, lz, B::WATER, 0);
                        }
                    }
                }

                // Cave floor & ceiling decorations
                if ug_biome != 0 && min_y < surf_h - 4 {
                    let top_ly = (CHUNK_SIZE - 2).min(surf_h - 5 - oy);
                    for ly in 1..=top_ly {
                        let y = oy + ly;
                        if y > 42 { break; }
                        if chunk.get_block(lx, ly, lz) != B::AIR { continue; }
                        let below = chunk.get_block(lx, ly - 1, lz);
                        let above = chunk.get_block(lx, ly + 1, lz);
                        let is_rock = |b: u8| b == B::STONE || b == B::DEEPSLATE || b == B::BASALT;
                        let h = self.abs.h3(wx, y, wz, self.seed ^ 0x4455);
                        if ug_biome == 1 {
                            if (is_rock(above) && (h & 7) == 0) || (is_rock(below) && (h & 7) == 1) {
                                chunk.set_block(lx, ly, lz, B::DRIPSTONE, 0);
                            }
                        } else if ug_biome == 2 && is_rock(below) {
                            chunk.set_block(lx, ly - 1, lz, B::MYCELIUM, 0);
                            if (h & 15) == 0 {
                                chunk.set_block(lx, ly, lz, B::GLOW_SHROOM_BLK, 0);
                            } else if (h & 7) == 1 {
                                chunk.set_block(lx, ly, lz, if (h & 1) != 0 { B::MUSHROOM_RED } else { B::MUSHROOM_BROWN }, 0);
                            }
                        } else if ug_biome == 3 {
                            if is_rock(below) {
                                chunk.set_block(lx, ly - 1, lz, B::MOSS, 0);
                                if (h & 7) == 0 {
                                    chunk.set_block(lx, ly, lz, B::TALL_GRASS, 0);
                                }
                            }
                            if is_rock(above) && (h & 15) == 0 {
                                chunk.set_block(lx, ly, lz, B::GLOWSTONE, 0);
                            }
                        }
                    }
                }
            }
        }

        // 2. Surface Flora, Aquatic Plants, Coral Reefs, Reeds
        if max_y >= col_data.min_h && min_y <= col_data.max_h + 4 {
            for lz in 0..CHUNK_SIZE {
                let wz = oz + lz;
                for lx in 0..CHUNK_SIZE {
                    let wx = ox + lx;
                    let c_idx = ((lz as usize) << 4) | (lx as usize);
                    let surf_h = heights[c_idx];
                    if surf_h + 4 < min_y || surf_h > max_y { continue; }
                    let biome = &BIOMES[biome_ids[c_idx] as usize];
                    let near_water = surf_h <= SEA_LEVEL + 1 && biome.cat != "cold" && biome.id != 5;
                    let get_surf = |ch: &Chunk, wy: i32| -> i16 {
                        let ly = wy - oy;
                        if (0..CHUNK_SIZE).contains(&ly) { ch.get_block(lx, ly, lz) as i16 } else { -1 }
                    };
                    let top_g = get_surf(chunk, surf_h);
                    let top_blk = if top_g != -1 {
                        top_g as u8
                    } else if near_water && biome.top == B::GRASS && biome.id != 35 {
                        B::SAND
                    } else {
                        biome.top
                    };
                    let r = (self.abs.h2(wx, wz, self.seed ^ 0x223344) as f64) / 4294967296.0;
                    let set_surf = |ch: &mut Chunk, wy: i32, id: u8, meta: u8| {
                        let ly = wy - oy;
                        if (0..CHUNK_SIZE).contains(&ly) { ch.set_block(lx, ly, lz, id, meta); }
                    };

                    if surf_h < SEA_LEVEL - 1 {
                        if biome.id == 23 && r < 0.08 && surf_h < SEA_LEVEL - 3 {
                            let c_blk = if r < 0.027 { B::CORAL_PINK } else if r < 0.054 { B::CORAL_BLUE } else { B::CORAL_YELLOW };
                            let ch_h = 1 + (self.abs.h2(wx, wz, self.seed) % 3) as i32;
                            for y in 1..=ch_h {
                                if surf_h + y < SEA_LEVEL {
                                    set_surf(chunk, surf_h + y, c_blk, 0);
                                }
                            }
                        } else if r < 0.14 {
                            let g = get_surf(chunk, surf_h + 1);
                            if g == B::WATER as i16 || g == -1 {
                                set_surf(chunk, surf_h + 1, B::SEAGRASS, 0);
                            }
                        }
                        continue;
                    }

                    let above_surf = get_surf(chunk, surf_h + 1);
                    if above_surf != B::AIR as i16 && above_surf != -1 {
                        continue;
                    }

                    if surf_h == SEA_LEVEL && (top_blk == B::SAND || top_blk == B::GRASS) && r < 0.045 {
                        let rh = 2 + (self.abs.h2(wx, wz, self.seed) % 2) as i32;
                        for y in 1..=rh {
                            set_surf(chunk, surf_h + y, B::REEDS, 0);
                        }
                        continue;
                    }

                    if top_blk == B::GRASS || top_blk == B::PODZOL || top_blk == B::MOSS {
                        if matches!(biome.id, 33 | 0 | 6) && r < 0.0045 {
                            let m = (self.abs.h2(wx, wz, self.seed) & 3) as u8;
                            set_surf(chunk, surf_h + 1, B::PUMPKIN, m);
                        } else if matches!(biome.id, 14 | 30 | 35) && r < 0.006 {
                            set_surf(chunk, surf_h + 1, B::MELON, 0);
                        } else if biome.id == 31 && r < 0.03 {
                            let m = if (self.abs.h2(wx, wz, self.seed) & 1) != 0 { B::MUSHROOM_RED } else { B::MUSHROOM_BROWN };
                            set_surf(chunk, surf_h + 1, m, 0);
                        } else if r < biome.flowers {
                            let fr = self.abs.h2(wx, wz, self.seed ^ 0x99) % 3;
                            let fb = if fr == 0 { B::POPPY } else if fr == 1 { B::DANDELION } else { B::ORCHID };
                            set_surf(chunk, surf_h + 1, fb, 0);
                        } else if r < biome.flowers + 0.14 {
                            set_surf(chunk, surf_h + 1, B::TALL_GRASS, 0);
                        }
                    } else if top_blk == B::MYCELIUM && r < 0.055 {
                        let m = if (self.abs.h2(wx, wz, self.seed) & 1) != 0 { B::MUSHROOM_RED } else { B::MUSHROOM_BROWN };
                        set_surf(chunk, surf_h + 1, m, 0);
                    } else if (top_blk == B::SAND || top_blk == B::RED_SAND) && r < 0.008 {
                        set_surf(chunk, surf_h + 1, B::DEAD_BUSH, 0);
                    }
                }
            }
        }

        // 3. Trees & Landmarks
        if max_y >= col_data.min_h - 4 && min_y <= col_data.max_h + 24 {
            let set_tree = |ch: &mut Chunk, wx: i32, wy: i32, wz: i32, id: u8, overwrite: bool| {
                let (lx, ly, lz) = (wx - ox, wy - oy, wz - oz);
                if !(0..CHUNK_SIZE).contains(&lx) || !(0..CHUNK_SIZE).contains(&ly) || !(0..CHUNK_SIZE).contains(&lz) {
                    return;
                }
                let cur = ch.get_block(lx, ly, lz);
                if overwrite || matches!(cur, B::AIR | B::TALL_GRASS | B::POPPY | B::DANDELION | B::ORCHID) {
                    ch.set_block(lx, ly, lz, id, 0);
                }
            };

            for wz in (oz - 4)..(oz + CHUNK_SIZE + 4) {
                for wx in (ox - 4)..(ox + CHUNK_SIZE + 4) {
                    if floor_mod(wx, 3) != 1 || floor_mod(wz, 3) != 1 { continue; }
                    let h = self.abs.h2(wx, wz, self.seed ^ 0x7a3b19);
                    let chance = ((h & 65535) as f64) / 65536.0;
                    let col = self.sample_column(wx, wz);
                    if col.height <= SEA_LEVEL { continue; }
                    if col.height + 24 < min_y || col.height > max_y { continue; }
                    let biome = col.biome;
                    if chance > biome.tree_rate * 8.5 { continue; }

                    let mut rng = SeededRng::new(h);
                    let y0 = col.height + 1;
                    match biome.tree_type {
                        "cactus" => {
                            let ch_h = rng.int(2, 4);
                            for y in 0..ch_h { set_tree(chunk, wx, y0 + y, wz, B::CACTUS, true); }
                        }
                        "bamboo" => {
                            if rng.next() < 0.72 {
                                let bh = rng.int(6, 13);
                                for y in 0..bh { set_tree(chunk, wx, y0 + y, wz, B::BAMBOO, true); }
                            } else {
                                let th = rng.int(7, 11);
                                for y in 0..th { set_tree(chunk, wx, y0 + y, wz, B::JUNGLE_LOG, true); }
                                for dy in -1..=1 {
                                    let rad = if dy == 1 { 1 } else { 2 };
                                    for dz in -rad..=rad {
                                        for dx in -rad..=rad {
                                            if dx * dx + dz * dz <= rad * rad + 1 {
                                                set_tree(chunk, wx + dx, y0 + th + dy, wz + dz, B::JUNGLE_LEAVES, false);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        "cherry" => {
                            let th = rng.int(5, 7);
                            let lean_x = if rng.next() < 0.5 { 1 } else { -1 };
                            for y in 0..th {
                                set_tree(chunk, wx + if y >= th - 2 { lean_x } else { 0 }, y0 + y, wz, B::CHERRY_LOG, true);
                            }
                            let cxt = wx + lean_x;
                            for dy in -1..=2 {
                                let rad = if dy == 2 { 1 } else if dy == -1 { 2 } else { 3 };
                                for dz in -rad..=rad {
                                    for dx in -rad..=rad {
                                        if dx * dx + dz * dz <= rad * rad + 1 && rng.next() > 0.14 {
                                            set_tree(chunk, cxt + dx, y0 + th + dy, wz + dz, B::CHERRY_LEAVES, false);
                                        }
                                    }
                                }
                            }
                        }
                        "mangrove" => {
                            let th = rng.int(5, 8);
                            for (rx, rz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                                set_tree(chunk, wx + rx, y0, wz + rz, B::MANGROVE_LOG, true);
                                set_tree(chunk, wx + rx, y0 + 1, wz + rz, B::MANGROVE_LOG, true);
                            }
                            for y in 1..th { set_tree(chunk, wx, y0 + y, wz, B::MANGROVE_LOG, true); }
                            for dy in -1..=1 {
                                let rad: i32 = if dy == 1 { 1 } else { 2 };
                                for dz in -rad..=rad {
                                    for dx in -rad..=rad {
                                        if dx.abs() + dz.abs() <= rad + 1 {
                                            set_tree(chunk, wx + dx, y0 + th + dy, wz + dz, B::MANGROVE_LEAVES, false);
                                        }
                                    }
                                }
                            }
                            for y in 1..(th - 1) {
                                set_tree(chunk, wx + 2, y0 + y, wz, B::VINES, false);
                                set_tree(chunk, wx, y0 + y, wz - 2, B::VINES, false);
                            }
                        }
                        "redwood" => {
                            let th = rng.int(13, 18);
                            for y in 0..th {
                                set_tree(chunk, wx, y0 + y, wz, B::SPRUCE_LOG, true);
                                set_tree(chunk, wx + 1, y0 + y, wz, B::SPRUCE_LOG, true);
                                set_tree(chunk, wx, y0 + y, wz + 1, B::SPRUCE_LOG, true);
                                set_tree(chunk, wx + 1, y0 + y, wz + 1, B::SPRUCE_LOG, true);
                            }
                            for layer in (th - 6)..=(th + 2) {
                                let rad = if layer > th { 1 } else if layer >= th - 1 { 2 } else { 3 };
                                for dz in -rad..=(rad + 1) {
                                    for dx in -rad..=(rad + 1) {
                                        let fx = dx as f64 - 0.5;
                                        let fz = dz as f64 - 0.5;
                                        let fr = rad as f64 + 0.6;
                                        if fx * fx + fz * fz <= fr * fr {
                                            set_tree(chunk, wx + dx, y0 + layer, wz + dz, B::SPRUCE_LEAVES, false);
                                        }
                                    }
                                }
                            }
                        }
                        "giant_shroom" => {
                            let th = rng.int(5, 8);
                            let red = rng.next() < 0.62;
                            let cap_blk = if red { B::WOOL_RED } else { B::GLOW_SHROOM_BLK };
                            for y in 0..th { set_tree(chunk, wx, y0 + y, wz, B::QUARTZ_BLOCK, true); }
                            for dz in -2_i32..=2 {
                                for dx in -2_i32..=2 {
                                    if dx.abs() == 2 && dz.abs() == 2 { continue; }
                                    let is_spot = red && ((dx.abs() == 1 && dz.abs() == 1) || (dx == 0 && dz == 0));
                                    set_tree(chunk, wx + dx, y0 + th, wz + dz, if is_spot { B::WOOL_WHITE } else { cap_blk }, true);
                                    if dx.abs() == 2 || dz.abs() == 2 {
                                        set_tree(chunk, wx + dx, y0 + th - 1, wz + dz, cap_blk, true);
                                    }
                                }
                            }
                        }
                        "basalt_spire" => {
                            let sh = rng.int(4, 10);
                            for y in 0..sh {
                                set_tree(chunk, wx, y0 + y, wz, if y == sh - 1 { B::MAGMA_BLOCK } else { B::BASALT }, true);
                                if y < sh - 3 {
                                    set_tree(chunk, wx + 1, y0 + y, wz, B::BASALT, true);
                                    set_tree(chunk, wx, y0 + y, wz + 1, B::BASALT, true);
                                }
                            }
                        }
                        "crystal_spire" => {
                            let sh = rng.int(4, 8);
                            for y in 0..sh {
                                set_tree(chunk, wx, y0 + y, wz, B::AMETHYST, true);
                                if y < sh - 2 {
                                    set_tree(chunk, wx + 1, y0 + y, wz, if y % 2 == 0 { B::AMETHYST } else { B::CALCITE }, true);
                                    set_tree(chunk, wx - 1, y0 + y, wz, B::CALCITE, true);
                                }
                            }
                        }
                        "palm" => {
                            let th = rng.int(5, 8);
                            let lean_x = if rng.next() < 0.5 { 1 } else { -1 };
                            for y in 0..th {
                                let off = (((y as f64) / (th as f64)) * 2.0).floor() as i32 * lean_x;
                                set_tree(chunk, wx + off, y0 + y, wz, B::JUNGLE_LOG, true);
                            }
                            let cxt = wx + lean_x * 2;
                            set_tree(chunk, cxt, y0 + th, wz, B::JUNGLE_LEAVES, false);
                            for (dx, dz) in [(1,0),(-1,0),(0,1),(0,-1),(1,1),(-1,-1),(1,-1),(-1,1)] {
                                set_tree(chunk, cxt + dx, y0 + th, wz + dz, B::JUNGLE_LEAVES, false);
                                set_tree(chunk, cxt + dx * 2, y0 + th - 1, wz + dz * 2, B::JUNGLE_LEAVES, false);
                            }
                        }
                        "spruce" => {
                            let th = rng.int(6, 9);
                            for y in 0..th { set_tree(chunk, wx, y0 + y, wz, B::SPRUCE_LOG, true); }
                            for layer in 2..=(th + 1) {
                                let rad: i32 = if layer == th + 1 { 0 } else if layer >= th - 1 { 1 } else if layer % 2 == 0 { 2 } else { 1 };
                                for dz in -rad..=rad {
                                    for dx in -rad..=rad {
                                        if dx.abs() + dz.abs() <= rad + 1 {
                                            set_tree(chunk, wx + dx, y0 + layer, wz + dz, B::SPRUCE_LEAVES, false);
                                        }
                                    }
                                }
                            }
                        }
                        "jungle" => {
                            let th = rng.int(8, 13);
                            for y in 0..th { set_tree(chunk, wx, y0 + y, wz, B::JUNGLE_LOG, true); }
                            for dy in -2..=1 {
                                let rad = if dy == 1 { 1 } else { 3 };
                                for dz in -rad..=rad {
                                    for dx in -rad..=rad {
                                        if dx * dx + dz * dz <= rad * rad + 1 {
                                            set_tree(chunk, wx + dx, y0 + th + dy, wz + dz, B::JUNGLE_LEAVES, false);
                                        }
                                    }
                                }
                            }
                            for y in 2..(th - 1) { set_tree(chunk, wx + 2, y0 + y, wz, B::VINES, false); }
                        }
                        "acacia" => {
                            let th = rng.int(4, 6);
                            let lean_x = if rng.next() < 0.5 { 1 } else { -1 };
                            for y in 0..th {
                                set_tree(chunk, wx + if y > 2 { lean_x } else { 0 }, y0 + y, wz, B::ACACIA_LOG, true);
                            }
                            let cxt = wx + lean_x;
                            for dz in -2_i32..=2 {
                                for dx in -2_i32..=2 {
                                    if dx.abs() + dz.abs() <= 3 {
                                        set_tree(chunk, cxt + dx, y0 + th, wz + dz, B::ACACIA_LEAVES, false);
                                    }
                                }
                            }
                            for dz in -1..=1 {
                                for dx in -1..=1 {
                                    set_tree(chunk, cxt + dx, y0 + th + 1, wz + dz, B::ACACIA_LEAVES, false);
                                }
                            }
                        }
                        other => {
                            let is_birch = other == "birch";
                            let is_dark = other == "darkoak";
                            let log_blk = if is_birch { B::BIRCH_LOG } else if is_dark { B::DARK_OAK_LOG } else { B::OAK_LOG };
                            let leaf_blk = if is_birch { B::BIRCH_LEAVES } else { B::OAK_LEAVES };
                            let th = rng.int(4, 6);
                            for y in 0..th { set_tree(chunk, wx, y0 + y, wz, log_blk, true); }
                            for dy in -2..=1 {
                                let rad: i32 = if dy <= 0 { 2 } else { 1 };
                                for dz in -rad..=rad {
                                    for dx in -rad..=rad {
                                        if dx.abs() == rad && dz.abs() == rad && (dy == 1 || rng.next() < 0.45) {
                                            continue;
                                        }
                                        set_tree(chunk, wx + dx, y0 + th + dy, wz + dz, leaf_blk, false);
                                    }
                                }
                            }
                            if other == "swamp" {
                                for y in 1..(th - 1) {
                                    set_tree(chunk, wx + 2, y0 + y, wz, B::VINES, false);
                                    set_tree(chunk, wx - 2, y0 + y, wz, B::VINES, false);
                                }
                            }
                        }
                    }
                }
            }
        }

        // 4. Procedural Structures
        self.apply_structures_to_chunk(chunk, &mut generated_chests);
        generated_chests
    }

    fn apply_structures_to_chunk(&mut self, chunk: &mut Chunk, chests: &mut Vec<(i32, i32, i32, BlockEntity)>) {
        const CELL: i32 = 10;
        let (cx, cy, cz) = (chunk.cx, chunk.cy, chunk.cz);
        let min_x = cx * CHUNK_SIZE;
        let max_x = min_x + CHUNK_SIZE - 1;
        let min_y = cy * CHUNK_SIZE;
        let max_y = min_y + CHUNK_SIZE - 1;
        let min_z = cz * CHUNK_SIZE;
        let max_z = min_z + CHUNK_SIZE - 1;
        let base_cell_x = floor_div(cx, CELL);
        let base_cell_z = floor_div(cz, CELL);

        let set_b = |ch: &mut Chunk, wx: i32, wy: i32, wz: i32, id: u8, meta: u8| {
            if wx < min_x || wx > max_x || wy < min_y || wy > max_y || wz < min_z || wz > max_z {
                return;
            }
            ch.set_block(wx - min_x, wy - min_y, wz - min_z, id, meta);
        };
        let set_chest = |ch: &mut Chunk, chests: &mut Vec<(i32, i32, i32, BlockEntity)>, wx: i32, wy: i32, wz: i32, seed: u32, tier: i32| {
            if wx < min_x || wx > max_x || wy < min_y || wy > max_y || wz < min_z || wz > max_z {
                return;
            }
            ch.set_block(wx - min_x, wy - min_y, wz - min_z, B::CHEST, 0);
            chests.push((wx, wy, wz, Self::generate_loot(seed, tier)));
        };
        let fill = |ch: &mut Chunk, x0: i32, y0: i32, z0: i32, x1: i32, y1: i32, z1: i32, id: u8, meta: u8| {
            if x1 < min_x || x0 > max_x || y1 < min_y || y0 > max_y || z1 < min_z || z0 > max_z {
                return;
            }
            let sx = x0.max(min_x);
            let ex = x1.min(max_x);
            let sz = z0.max(min_z);
            let ez = z1.min(max_z);
            let sy = y0.max(min_y);
            let ey = y1.min(max_y);
            for y in sy..=ey {
                for z in sz..=ez {
                    for x in sx..=ex {
                        ch.set_block(x - min_x, y - min_y, z - min_z, id, meta);
                    }
                }
            }
        };

        let build_house = |ch: &mut Chunk, chests: &mut Vec<(i32, i32, i32, BlockEntity)>, hx: i32, hy: i32, hz: i32, w: i32, d: i32, h: i32, wall: u8, trim: u8, roof: u8, seed: u32| {
            fill(ch, hx, hy - 5, hz, hx + w, hy, hz + d, B::COBBLE, 0);
            fill(ch, hx + 1, hy + 1, hz + 1, hx + w - 1, hy + h, hz + d - 1, B::AIR, 0);
            fill(ch, hx + 1, hy, hz + 1, hx + w - 1, hy, hz + d - 1, B::OAK_PLANKS, 0);
            for y in 1..=h {
                for x in 0..=w {
                    let is_corner = x == 0 || x == w;
                    let b = if is_corner { trim } else { wall };
                    set_b(ch, hx + x, hy + y, hz, b, 0);
                    set_b(ch, hx + x, hy + y, hz + d, b, 0);
                }
                for z in 1..d {
                    set_b(ch, hx, hy + y, hz + z, wall, 0);
                    set_b(ch, hx + w, hy + y, hz + z, wall, 0);
                }
            }
            let mid_x = hx + (w >> 1);
            let door_blk = if wall == B::SPRUCE_PLANKS { B::SPRUCE_DOOR } else if wall == B::BIRCH_PLANKS { B::BIRCH_DOOR } else { B::OAK_DOOR };
            set_b(ch, mid_x, hy + 1, hz, door_blk, 2);
            set_b(ch, mid_x, hy + 2, hz, door_blk, 2 | 8);
            set_b(ch, hx, hy + 2, hz + (d >> 1), B::GLASS, 0);
            set_b(ch, hx + w, hy + 2, hz + (d >> 1), B::GLASS, 0);
            set_b(ch, mid_x, hy + 2, hz + d, B::GLASS, 0);
            let steps = ((w as f64) / 2.0).ceil() as i32;
            for step in 0..=steps {
                fill(ch, hx - 1 + step, hy + h + 1 + step, hz - 1, hx + w + 1 - step, hy + h + 1 + step, hz + d + 1, roof, 0);
            }
            set_b(ch, hx + 1, hy + 1, hz + d - 1, B::CRAFTING_TABLE, 0);
            set_b(ch, hx + 2, hy + 1, hz + d - 1, B::FURNACE, 0);
            set_chest(ch, chests, hx + w - 1, hy + 1, hz + d - 1, seed ^ 0x111, 1);
            set_b(ch, hx + 1, hy + 3, hz + 1, B::TORCH, 0);
            set_b(ch, hx + w - 1, hy + 3, hz + d - 1, B::TORCH, 0);
        };

        for dz in -1..=1 {
            for dx in -1..=1 {
                if let Some(st) = self.get_cell_structure(base_cell_x + dx, base_cell_z + dz)
                    && (st.wx - (min_x + 8)).abs() <= 36 && (st.wz - (min_z + 8)).abs() <= 36 {
                        let (wx, wy, wz, seed, biome) = (st.wx, st.wy, st.wz, st.seed, st.biome);
                        let is_snow = biome.cat == "cold";
                        let is_cherry = biome.id == 27;
                        let plank = if is_snow { B::SPRUCE_PLANKS } else if is_cherry { B::CHERRY_PLANKS } else { B::OAK_PLANKS };
                        let log = if is_snow { B::SPRUCE_LOG } else if is_cherry { B::CHERRY_LOG } else { B::OAK_LOG };

                        match st.kind {
                            "village" => {
                                fill(chunk, wx - 2, wy - 4, wz - 2, wx + 2, wy, wz + 2, B::COBBLE, 0);
                                fill(chunk, wx - 1, wy - 2, wz - 1, wx + 1, wy, wz + 1, B::WATER, 0);
                                for (px, pz) in [(-2, -2), (2, -2), (-2, 2), (2, 2)] {
                                    set_b(chunk, wx + px, wy + 1, wz + pz, B::OAK_FENCE, 0);
                                    set_b(chunk, wx + px, wy + 2, wz + pz, B::OAK_FENCE, 0);
                                }
                                fill(chunk, wx - 2, wy + 3, wz - 2, wx + 2, wy + 3, wz + 2, plank, 0);
                                set_b(chunk, wx, wy + 4, wz, B::LANTERN, 0);
                                fill(chunk, wx - 14, wy, wz - 1, wx + 14, wy, wz + 1, B::GRAVEL, 0);
                                fill(chunk, wx - 1, wy, wz - 14, wx + 1, wy, wz + 14, B::GRAVEL, 0);
                                let offsets = [(-13, -10), (6, -10), (-12, 5)];
                                for (i, &(oxx, ozz)) in offsets.iter().enumerate() {
                                    let hx = wx + oxx;
                                    let hz = wz + ozz;
                                    let hy = self.sample_column(hx + 3, hz + 3).height;
                                    if hy > SEA_LEVEL {
                                        build_house(chunk, chests, hx, hy, hz, 6, 6, 4, plank, log, B::COBBLE, seed.wrapping_add((i as u32) * 97));
                                    }
                                }
                                let (fx, fz) = (wx + 5, wz + 5);
                                let fy = self.sample_column(fx + 3, fz + 3).height;
                                if fy > SEA_LEVEL {
                                    fill(chunk, fx, fy - 2, fz, fx + 6, fy, fz + 6, B::DIRT, 0);
                                    fill(chunk, fx + 1, fy, fz + 1, fx + 5, fy, fz + 5, B::FARMLAND, 1);
                                    fill(chunk, fx + 3, fy, fz + 1, fx + 3, fy, fz + 5, B::WATER, 0);
                                    for x in (fx + 1)..=(fx + 5) {
                                        if x == fx + 3 { continue; }
                                        for z in (fz + 1)..=(fz + 5) {
                                            set_b(chunk, x, fy + 1, z, B::WHEAT, 3);
                                        }
                                    }
                                    set_b(chunk, fx, fy + 1, fz, B::HAY_BALE, 0);
                                    set_b(chunk, fx + 6, fy + 1, fz, B::TORCH, 0);
                                }
                            }
                            "watchtower" => {
                                let h = 13 + (seed % 5) as i32;
                                fill(chunk, wx - 3, wy - 5, wz - 3, wx + 3, wy, wz + 3, B::STONE_BRICKS, 0);
                                fill(chunk, wx - 2, wy + 1, wz - 2, wx + 2, wy + h, wz + 2, B::AIR, 0);
                                for y in 1..=h {
                                    let blk = if y % 4 == 0 { B::MOSSY_BRICKS } else { B::STONE_BRICKS };
                                    for i in -3..=3 {
                                        set_b(chunk, wx + i, wy + y, wz - 3, blk, 0);
                                        set_b(chunk, wx + i, wy + y, wz + 3, blk, 0);
                                        set_b(chunk, wx - 3, wy + y, wz + i, blk, 0);
                                        set_b(chunk, wx + 3, wy + y, wz + i, blk, 0);
                                    }
                                    set_b(chunk, wx, wy + y, wz + 2, B::LADDER, 0);
                                }
                                set_b(chunk, wx, wy + 1, wz - 3, B::OAK_DOOR, 2);
                                set_b(chunk, wx, wy + 2, wz - 3, B::OAK_DOOR, 2 | 8);
                                fill(chunk, wx - 4, wy + h, wz - 4, wx + 4, wy + h, wz + 4, B::STONE_BRICKS, 0);
                                fill(chunk, wx - 2, wy + h, wz - 2, wx + 2, wy + h, wz + 2, plank, 0);
                                set_b(chunk, wx, wy + h, wz + 2, B::LADDER, 0);
                                for i in (-4..=4).step_by(2) {
                                    set_b(chunk, wx + i, wy + h + 1, wz - 4, B::STONE_BRICKS, 0);
                                    set_b(chunk, wx + i, wy + h + 1, wz + 4, B::STONE_BRICKS, 0);
                                    set_b(chunk, wx - 4, wy + h + 1, wz + i, B::STONE_BRICKS, 0);
                                    set_b(chunk, wx + 4, wy + h + 1, wz + i, B::STONE_BRICKS, 0);
                                }
                                set_b(chunk, wx - 3, wy + h + 1, wz - 3, B::LANTERN, 0);
                                set_b(chunk, wx + 3, wy + h + 1, wz + 3, B::LANTERN, 0);
                                set_chest(chunk, chests, wx + 2, wy + h + 1, wz, seed ^ 0x333, 2);
                            }
                            "temple" | "jungle_temple" => {
                                let mat = if st.kind == "temple" { B::SANDSTONE } else { B::MOSSY_BRICKS };
                                let trim = if st.kind == "temple" { B::YEL_TERRACOTTA } else { B::STONE_BRICKS };
                                for s in 0..6 {
                                    let r = 7 - s;
                                    fill(chunk, wx - r, wy + s, wz - r, wx + r, wy + s, wz + r, if s == 2 { trim } else { mat }, 0);
                                }
                                fill(chunk, wx - 7, wy - 4, wz - 7, wx + 7, wy - 1, wz + 7, mat, 0);
                                fill(chunk, wx - 3, wy + 1, wz - 3, wx + 3, wy + 4, wz + 3, B::AIR, 0);
                                set_b(chunk, wx, wy + 1, wz - 7, B::AIR, 0);
                                set_b(chunk, wx, wy + 2, wz - 7, B::AIR, 0);
                                fill(chunk, wx, wy + 1, wz - 6, wx, wy + 2, wz - 3, B::AIR, 0);
                                set_b(chunk, wx - 2, wy + 1, wz - 2, B::TORCH, 0);
                                set_b(chunk, wx + 2, wy + 1, wz + 2, B::TORCH, 0);
                                set_b(chunk, wx, wy + 1, wz, B::GOLD_BLOCK, 0);
                                set_chest(chunk, chests, wx + 2, wy + 1, wz, seed ^ 0x555, 2);
                                set_chest(chunk, chests, wx - 2, wy + 1, wz, seed ^ 0x666, 2);
                            }
                            "ruined_portal" => {
                                fill(chunk, wx - 3, wy, wz - 3, wx + 3, wy, wz + 3, B::COBBLE, 0);
                                fill(chunk, wx - 1, wy, wz - 1, wx + 1, wy, wz + 1, B::LAVA, 0);
                                for y in 1..=5 {
                                    set_b(chunk, wx - 2, wy + y, wz, if y == 3 { B::OBSIDIAN_CRYING } else { B::OBSIDIAN }, 0);
                                    if y < 4 {
                                        set_b(chunk, wx + 2, wy + y, wz, if y == 2 { B::OBSIDIAN_CRYING } else { B::OBSIDIAN }, 0);
                                    }
                                }
                                set_b(chunk, wx - 1, wy + 5, wz, B::OBSIDIAN, 0);
                                set_b(chunk, wx, wy + 5, wz, B::GOLD_BLOCK, 0);
                                set_chest(chunk, chests, wx + 3, wy + 1, wz + 2, seed ^ 0x777, 2);
                            }
                            "shipwreck" => {
                                let sy = wy + 1;
                                fill(chunk, wx - 5, sy, wz - 2, wx + 5, sy + 2, wz + 2, B::OAK_PLANKS, 0);
                                fill(chunk, wx - 4, sy + 1, wz - 1, wx + 4, sy + 2, wz + 1, B::WATER, 0);
                                for y in 1..=7 { set_b(chunk, wx, sy + y, wz, B::SPRUCE_LOG, 0); }
                                fill(chunk, wx, sy + 4, wz - 2, wx, sy + 6, wz + 2, B::WOOL_WHITE, 0);
                                set_b(chunk, wx - 3, sy + 1, wz, B::BARREL, 0);
                                set_b(chunk, wx + 1, sy + 1, wz, B::SEA_LANTERN, 0);
                                set_chest(chunk, chests, wx + 3, sy + 1, wz, seed ^ 0x888, 2);
                            }
                            "cabin" => {
                                build_house(chunk, chests, wx - 3, wy, wz - 3, 6, 5, 4, log, log, plank, seed);
                            }
                            "campsite" => {
                                set_b(chunk, wx, wy + 1, wz, B::CAMPFIRE, 0);
                                set_b(chunk, wx + 2, wy + 1, wz, B::CRAFTING_TABLE, 0);
                                set_chest(chunk, chests, wx - 2, wy + 1, wz, seed ^ 0x999, 1);
                                set_b(chunk, wx, wy + 1, wz + 2, B::OAK_LOG, 0);
                                set_b(chunk, wx + 1, wy + 1, wz + 2, B::OAK_LOG, 0);
                                fill(chunk, wx - 2, wy + 1, wz - 3, wx + 2, wy + 2, wz - 3, B::WOOL_RED, 0);
                                set_b(chunk, wx + 2, wy + 1, wz + 2, B::LANTERN, 0);
                            }
                            _ => {}
                        }
                    }

                if let Some(ust) = self.get_underground_structure(base_cell_x + dx, base_cell_z + dz)
                    && (ust.wx - (min_x + 8)).abs() <= 24 && (ust.wz - (min_z + 8)).abs() <= 24 {
                        let (wx, wy, wz, seed) = (ust.wx, ust.wy, ust.wz, ust.seed);
                        if ust.kind == "mineshaft" {
                            fill(chunk, wx - 12, wy, wz - 1, wx + 12, wy + 3, wz + 1, B::AIR, 0);
                            fill(chunk, wx - 1, wy, wz - 12, wx + 1, wy + 3, wz + 12, B::AIR, 0);
                            fill(chunk, wx - 12, wy - 1, wz - 1, wx + 12, wy - 1, wz + 1, B::OAK_PLANKS, 0);
                            fill(chunk, wx - 1, wy - 1, wz - 12, wx + 1, wy - 1, wz + 12, B::OAK_PLANKS, 0);
                            for k in (-10..=10).step_by(5) {
                                set_b(chunk, wx + k, wy, wz - 1, B::OAK_FENCE, 0);
                                set_b(chunk, wx + k, wy + 1, wz - 1, B::OAK_FENCE, 0);
                                set_b(chunk, wx + k, wy, wz + 1, B::OAK_FENCE, 0);
                                set_b(chunk, wx + k, wy + 1, wz + 1, B::OAK_FENCE, 0);
                                fill(chunk, wx + k, wy + 2, wz - 1, wx + k, wy + 2, wz + 1, B::OAK_PLANKS, 0);
                                set_b(chunk, wx + k, wy + 1, wz, B::TORCH, 0);
                                set_b(chunk, wx + k + 2, wy + 2, wz - 1, B::COBWEB, 0);
                            }
                            set_chest(chunk, chests, wx + 10, wy, wz, seed ^ 0xabc, 2);
                            set_chest(chunk, chests, wx, wy, wz - 10, seed ^ 0xdef, 2);
                        } else {
                            fill(chunk, wx - 4, wy - 1, wz - 4, wx + 4, wy + 5, wz + 4, B::MOSSY_BRICKS, 0);
                            fill(chunk, wx - 3, wy, wz - 3, wx + 3, wy + 4, wz + 3, B::AIR, 0);
                            set_b(chunk, wx - 3, wy + 2, wz - 3, B::LANTERN, 0);
                            set_b(chunk, wx + 3, wy + 2, wz + 3, B::LANTERN, 0);
                            set_b(chunk, wx, wy + 1, wz - 4, B::IRON_BARS, 0);
                            set_b(chunk, wx, wy + 2, wz - 4, B::IRON_BARS, 0);
                            set_b(chunk, wx, wy, wz, B::DIAMOND_ORE, 0);
                            set_chest(chunk, chests, wx + 2, wy, wz, seed ^ 0x444, 2);
                            set_chest(chunk, chests, wx - 2, wy, wz, seed ^ 0x555, 2);
                        }
                    }
            }
        }
    }
}
