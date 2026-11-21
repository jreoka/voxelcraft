use crate::blocks::BLOCKS;
use crate::chunk::Chunk;
use crate::math::CHUNK_SIZE;

#[inline]
fn is_light_passable(id: u8) -> bool {
    if id == 0 {
        return true;
    }
    BLOCKS.blocks[id as usize].as_ref().is_none_or(|b| b.transparent)
}

fn run_sky_bfs(chunk: &mut Chunk, queue: &mut Vec<(i32, i32, i32)>) {
    let mut head = 0;
    const DIRS: [(i32, i32, i32); 6] = [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)];
    while head < queue.len() {
        let (x, y, z) = queue[head];
        head += 1;
        let cur = chunk.light[Chunk::idx(x as usize, y as usize, z as usize)] >> 4;
        if cur <= 1 {
            continue;
        }
        for (d, &(dx, dy, dz)) in DIRS.iter().enumerate() {
            let (nx, ny, nz) = (x + dx, y + dy, z + dz);
            if !(0..CHUNK_SIZE).contains(&nx) || !(0..CHUNK_SIZE).contains(&ny) || !(0..CHUNK_SIZE).contains(&nz) {
                continue;
            }
            let ni = Chunk::idx(nx as usize, ny as usize, nz as usize);
            let nid = chunk.blocks[ni];
            if !is_light_passable(nid) {
                continue;
            }
            let next_sky = if d == 3 && cur == 15 && nid == 0 { 15 } else { cur - 1 };
            let nsl = chunk.light[ni] >> 4;
            if nsl < next_sky {
                chunk.light[ni] = (chunk.light[ni] & 0x0f) | (next_sky << 4);
                queue.push((nx, ny, nz));
            }
        }
    }
}

fn run_blk_bfs(chunk: &mut Chunk, queue: &mut Vec<(i32, i32, i32)>) {
    let mut head = 0;
    const DIRS: [(i32, i32, i32); 6] = [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)];
    while head < queue.len() {
        let (x, y, z) = queue[head];
        head += 1;
        let cur = chunk.light[Chunk::idx(x as usize, y as usize, z as usize)] & 0x0f;
        if cur <= 1 {
            continue;
        }
        for &(dx, dy, dz) in &DIRS {
            let (nx, ny, nz) = (x + dx, y + dy, z + dz);
            if !(0..CHUNK_SIZE).contains(&nx) || !(0..CHUNK_SIZE).contains(&ny) || !(0..CHUNK_SIZE).contains(&nz) {
                continue;
            }
            let ni = Chunk::idx(nx as usize, ny as usize, nz as usize);
            if !is_light_passable(chunk.blocks[ni]) {
                continue;
            }
            let nbl = chunk.light[ni] & 0x0f;
            if nbl < cur - 1 {
                chunk.light[ni] = (chunk.light[ni] & 0xf0) | (cur - 1);
                queue.push((nx, ny, nz));
            }
        }
    }
}

/// Computes local skylight + block light for a single chunk (matching JS `LightingSystem.computeLocalLight`).
pub fn compute_local_light(
    chunk: &mut Chunk,
    above_light_bottom: Option<&[u8; 256]>,
    above_is_empty: Option<bool>,
    heights: &[i32; 256],
) {
    let oy = chunk.cy * CHUNK_SIZE;
    let top_wy = oy + CHUNK_SIZE - 1;

    if chunk.is_empty {
        let mut all_sky = true;
        if above_is_empty == Some(false) {
            all_sky = false;
        } else {
            for &h in heights.iter() {
                if h >= oy {
                    all_sky = false;
                    break;
                }
            }
        }
        if all_sky {
            chunk.light.fill(0xf0);
            chunk.light_dirty = false;
            chunk.border_light_dirty = true;
            return;
        }
    }

    chunk.light.fill(0);
    let mut blk_q: Vec<(i32, i32, i32)> = Vec::new();

    for z in 0..CHUNK_SIZE {
        for x in 0..CHUNK_SIZE {
            let col_idx = ((z as usize) << 4) | (x as usize);
            let mut sky: u8 = if let Some(ab) = above_light_bottom {
                ab[col_idx] >> 4
            } else if top_wy > heights[col_idx] {
                15
            } else {
                0
            };
            for y in (0..CHUNK_SIZE).rev() {
                let i = Chunk::idx(x as usize, y as usize, z as usize);
                let id = chunk.blocks[i];
                if id == 0 {
                    chunk.light[i] = sky << 4;
                } else {
                    let b = BLOCKS.blocks[id as usize].as_ref();
                    if let Some(bd) = b {
                        if !bd.transparent && bd.shape == "cube" {
                            sky = 0;
                        } else if bd.liquid || bd.is_leaf {
                            sky = sky.saturating_sub(1);
                        }
                    }
                    let bl = b.map_or(0, |bd| bd.light);
                    chunk.light[i] = (sky << 4) | bl;
                    if bl > 0 {
                        blk_q.push((x, y, z));
                    }
                }
            }
        }
    }

    let mut sky_q: Vec<(i32, i32, i32)> = Vec::new();
    for y in 0..CHUNK_SIZE {
        let y_off = (y as usize) << 8;
        for z in 0..CHUNK_SIZE {
            let yz_off = y_off | ((z as usize) << 4);
            for x in 0..CHUNK_SIZE {
                let i = yz_off | (x as usize);
                let sl = chunk.light[i] >> 4;
                if sl < 2 {
                    continue;
                }
                let need = sl - 1;
                if (x < 15 && (chunk.light[i + 1] >> 4) < need && is_light_passable(chunk.blocks[i + 1]))
                    || (x > 0 && (chunk.light[i - 1] >> 4) < need && is_light_passable(chunk.blocks[i - 1]))
                    || (z < 15 && (chunk.light[i + 16] >> 4) < need && is_light_passable(chunk.blocks[i + 16]))
                    || (z > 0 && (chunk.light[i - 16] >> 4) < need && is_light_passable(chunk.blocks[i - 16]))
                    || (y < 15 && (chunk.light[i + 256] >> 4) < need && is_light_passable(chunk.blocks[i + 256]))
                    || (y > 0 && (chunk.light[i - 256] >> 4) < need && is_light_passable(chunk.blocks[i - 256]))
                {
                    sky_q.push((x, y, z));
                }
            }
        }
    }

    if !sky_q.is_empty() {
        run_sky_bfs(chunk, &mut sky_q);
    }
    if !blk_q.is_empty() {
        run_blk_bfs(chunk, &mut blk_q);
    }
    chunk.light_dirty = false;
    chunk.border_light_dirty = true;
}
