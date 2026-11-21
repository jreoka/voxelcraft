use std::collections::{HashSet, VecDeque};

use indexmap::IndexSet;

use crate::blocks::{B, BLOCKS};

#[derive(Clone, Debug)]
pub struct TimedButton {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub timer: f64,
}

#[derive(Clone, Default)]
pub struct RedstoneSystem {
    pub timed_buttons: Vec<TimedButton>,
}

pub trait RedstoneWorld {
    fn rs_get_block(&mut self, x: i32, y: i32, z: i32) -> u8;
    fn rs_get_meta(&mut self, x: i32, y: i32, z: i32) -> u8;
    fn rs_set_block(&mut self, x: i32, y: i32, z: i32, id: u8, meta: u8);
    fn rs_detonate_tnt(&mut self, x: i32, y: i32, z: i32);
}

impl RedstoneSystem {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_power_source(id: u8, meta: u8) -> bool {
        (id == B::LEVER || id == B::PRESSURE_PLATE || id == B::STONE_BUTTON) && (meta & 1) != 0
    }

    pub fn shift_local(&mut self, dx: i32, dz: i32) {
        for tb in &mut self.timed_buttons {
            tb.x -= dx;
            tb.z -= dz;
        }
    }

    pub fn press_button<W: RedstoneWorld>(&mut self, world: &mut W, wx: i32, wy: i32, wz: i32) {
        let m = world.rs_get_meta(wx, wy, wz);
        if (m & 1) == 0 {
            world.rs_set_block(wx, wy, wz, B::STONE_BUTTON, m | 1);
            self.timed_buttons.push(TimedButton { x: wx, y: wy, z: wz, timer: 1.5 });
            self.propagate_from(world, wx, wy, wz, true);
        }
    }

    pub fn update<W: RedstoneWorld>(&mut self, world: &mut W, dt: f64) {
        let mut i = self.timed_buttons.len();
        while i > 0 {
            i -= 1;
            self.timed_buttons[i].timer -= dt;
            if self.timed_buttons[i].timer <= 0.0 {
                let tb = self.timed_buttons.remove(i);
                if world.rs_get_block(tb.x, tb.y, tb.z) == B::STONE_BUTTON {
                    let m = world.rs_get_meta(tb.x, tb.y, tb.z) & !1;
                    world.rs_set_block(tb.x, tb.y, tb.z, B::STONE_BUTTON, m);
                    self.propagate_from(world, tb.x, tb.y, tb.z, false);
                }
            }
        }
    }

    pub fn propagate_from<W: RedstoneWorld>(&self, world: &mut W, sx: i32, sy: i32, sz: i32, active: bool) {
        const DIRS: [(i32, i32, i32); 6] = [(1,0,0), (-1,0,0), (0,1,0), (0,-1,0), (0,0,1), (0,0,-1)];
        const STEP_DIRS: [(i32, i32, i32); 8] = [
            (1,1,0), (-1,1,0), (0,1,1), (0,1,-1),
            (1,-1,0), (-1,-1,0), (0,-1,1), (0,-1,-1),
        ];
        let mut visited: HashSet<(i32, i32, i32)> = HashSet::new();
        visited.insert((sx, sy, sz));
        let mut queue: VecDeque<(i32, i32, i32, i32)> = VecDeque::new();
        queue.push_back((sx, sy, sz, 14));
        let mut touched: IndexSet<(i32, i32, i32)> = IndexSet::new();

        while let Some((x, y, z, pwr)) = queue.pop_front() {
            let cur_id = world.rs_get_block(x, y, z);
            for &(dx, dy, dz) in &DIRS {
                let (nx, ny, nz) = (x + dx, y + dy, z + dz);
                let nid = world.rs_get_block(nx, ny, nz);
                if nid == B::REDSTONE_WIRE && !visited.contains(&(nx, ny, nz)) && pwr > 1 {
                    visited.insert((nx, ny, nz));
                    world.rs_set_block(nx, ny, nz, B::REDSTONE_WIRE, if active { 1 } else { 0 });
                    queue.push_back((nx, ny, nz, pwr - 1));
                } else if nid != B::AIR {
                    touched.insert((nx, ny, nz));
                }
            }
            if (cur_id == B::REDSTONE_WIRE || (x == sx && y == sy && z == sz)) && pwr > 1 {
                for &(dx, dy, dz) in &STEP_DIRS {
                    let (nx, ny, nz) = (x + dx, y + dy, z + dz);
                    if !visited.contains(&(nx, ny, nz)) && world.rs_get_block(nx, ny, nz) == B::REDSTONE_WIRE {
                        visited.insert((nx, ny, nz));
                        world.rs_set_block(nx, ny, nz, B::REDSTONE_WIRE, if active { 1 } else { 0 });
                        queue.push_back((nx, ny, nz, pwr - 1));
                    }
                }
            }
        }

        for (rx, ry, rz) in touched {
            let id = world.rs_get_block(rx, ry, rz);
            let blk = BLOCKS.get(id);
            if id == B::REDSTONE_LAMP && active {
                world.rs_set_block(rx, ry, rz, B::REDSTONE_LAMP_ON, 0);
            } else if id == B::REDSTONE_LAMP_ON && !active {
                world.rs_set_block(rx, ry, rz, B::REDSTONE_LAMP, 0);
            } else if id == B::TNT && active {
                world.rs_detonate_tnt(rx, ry, rz);
            } else if blk.shape == "door" {
                let m = world.rs_get_meta(rx, ry, rz);
                let is_top = (m & 8) != 0;
                let bot_y = if is_top { ry - 1 } else { ry };
                let bot_m = world.rs_get_meta(rx, bot_y, rz);
                let target_bit = if active { 4 } else { 0 };
                if (bot_m & 4) != target_bit {
                    world.rs_set_block(rx, bot_y, rz, id, (bot_m & !4) | target_bit);
                    if world.rs_get_block(rx, bot_y + 1, rz) == id {
                        let top_m = world.rs_get_meta(rx, bot_y + 1, rz);
                        world.rs_set_block(rx, bot_y + 1, rz, id, (top_m & !4) | target_bit);
                    }
                }
            } else if blk.shape == "trapdoor" || blk.shape == "fence_gate" {
                let m = world.rs_get_meta(rx, ry, rz);
                let target_bit = if active { 4 } else { 0 };
                if (m & 4) != target_bit {
                    world.rs_set_block(rx, ry, rz, id, (m & !4) | target_bit);
                }
            }
        }
    }
}
