use std::sync::Arc;

use crate::math::{CHUNK_SIZE, CHUNK_VOL};

#[derive(Clone)]
pub struct Chunk {
    pub cx: i32,
    pub cy: i32,
    pub cz: i32,
    pub key: String,
    pub blocks: [u8; CHUNK_VOL],
    pub meta: [u8; CHUNK_VOL],
    pub light: [u8; CHUNK_VOL],
    pub height_map: Option<Arc<[i32; 256]>>,
    pub biome_map: Option<Arc<[u8; 256]>>,
    pub non_air_count: usize,
    pub is_empty: bool,
    pub dirty: bool,
    pub light_dirty: bool,
    pub border_light_dirty: bool,
}

impl Chunk {
    pub fn new(cx: i32, cy: i32, cz: i32) -> Self {
        Self {
            cx,
            cy,
            cz,
            key: format!("{},{},{}", cx, cy, cz),
            blocks: [0; CHUNK_VOL],
            meta: [0; CHUNK_VOL],
            light: [0; CHUNK_VOL],
            height_map: None,
            biome_map: None,
            non_air_count: 0,
            is_empty: true,
            dirty: true,
            light_dirty: true,
            border_light_dirty: true,
        }
    }

    #[inline]
    pub fn idx(x: usize, y: usize, z: usize) -> usize {
        (y << 8) | (z << 4) | x
    }

    #[inline]
    pub fn get_block(&self, x: i32, y: i32, z: i32) -> u8 {
        if !(0..CHUNK_SIZE).contains(&y) || !(0..CHUNK_SIZE).contains(&x) || !(0..CHUNK_SIZE).contains(&z) {
            return 0;
        }
        self.blocks[Self::idx(x as usize, y as usize, z as usize)]
    }

    #[inline]
    pub fn set_block(&mut self, x: i32, y: i32, z: i32, id: u8, meta: u8) {
        if !(0..CHUNK_SIZE).contains(&y) || !(0..CHUNK_SIZE).contains(&x) || !(0..CHUNK_SIZE).contains(&z) {
            return;
        }
        let i = Self::idx(x as usize, y as usize, z as usize);
        let prev = self.blocks[i];
        if prev == 0 && id != 0 {
            self.non_air_count += 1;
        } else if prev != 0 && id == 0 {
            self.non_air_count -= 1;
        }
        self.is_empty = self.non_air_count == 0;
        self.blocks[i] = id;
        self.meta[i] = meta;
    }

    #[inline]
    pub fn get_meta(&self, x: i32, y: i32, z: i32) -> u8 {
        if !(0..CHUNK_SIZE).contains(&y) || !(0..CHUNK_SIZE).contains(&x) || !(0..CHUNK_SIZE).contains(&z) {
            return 0;
        }
        self.meta[Self::idx(x as usize, y as usize, z as usize)]
    }
}
