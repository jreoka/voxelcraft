//! World "life": random ticks (crops, saplings, cane/cactus, grass), sapling trees and leaf decay.
//! Twin of `randomTickBlock`, `growTreeAt` and the World leaf-decay queue in index.html; keep in step.

use std::collections::HashSet;

use crate::blocks::{B, BLOCKS};
use crate::world::{BlockChange, CoreWorld, SharedWorldTables};

/// Random-tick samples per crop tick and player, over a 32x16x32 box (matches the client).
pub const RANDOM_TICK_SAMPLES: usize = 40;

const NEIGHBORS6: [(i32, i32, i32); 6] = [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)];

pub fn is_log(id: u8) -> bool {
    matches!(
        id,
        B::OAK_LOG | B::BIRCH_LOG | B::SPRUCE_LOG | B::JUNGLE_LOG | B::ACACIA_LOG | B::DARK_OAK_LOG | B::CHERRY_LOG | B::MANGROVE_LOG
    )
}

pub fn is_crop(id: u8) -> bool {
    matches!(id, B::WHEAT | B::CARROTS | B::POTATOES)
}

pub fn is_sapling(id: u8) -> bool {
    sapling_tree(id).is_some()
}

pub fn is_sapling_soil(id: u8) -> bool {
    matches!(id, B::GRASS | B::DIRT | B::PODZOL | B::COARSE_DIRT | B::MOSS | B::MUD | B::FARMLAND | B::MYCELIUM)
}

/// Which sapling a leaf block drops.
pub fn leaf_sapling(id: u8) -> Option<u8> {
    Some(match id {
        B::OAK_LEAVES => B::OAK_SAPLING,
        B::BIRCH_LEAVES => B::BIRCH_SAPLING,
        B::SPRUCE_LEAVES => B::SPRUCE_SAPLING,
        B::JUNGLE_LEAVES => B::JUNGLE_SAPLING,
        B::ACACIA_LEAVES => B::ACACIA_SAPLING,
        B::CHERRY_LEAVES => B::CHERRY_SAPLING,
        B::MANGROVE_LEAVES => B::MANGROVE_SAPLING,
        _ => return None,
    })
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TreeKind {
    Oak,
    Birch,
    Spruce,
    Jungle,
    Acacia,
    Cherry,
    Mangrove,
}

pub fn sapling_tree(id: u8) -> Option<TreeKind> {
    Some(match id {
        B::OAK_SAPLING => TreeKind::Oak,
        B::BIRCH_SAPLING => TreeKind::Birch,
        B::SPRUCE_SAPLING => TreeKind::Spruce,
        B::JUNGLE_SAPLING => TreeKind::Jungle,
        B::ACACIA_SAPLING => TreeKind::Acacia,
        B::CHERRY_SAPLING => TreeKind::Cherry,
        B::MANGROVE_SAPLING => TreeKind::Mangrove,
        _ => return None,
    })
}

fn tree_wood(kind: TreeKind) -> (u8, u8) {
    match kind {
        TreeKind::Oak => (B::OAK_LOG, B::OAK_LEAVES),
        TreeKind::Birch => (B::BIRCH_LOG, B::BIRCH_LEAVES),
        TreeKind::Spruce => (B::SPRUCE_LOG, B::SPRUCE_LEAVES),
        TreeKind::Jungle => (B::JUNGLE_LOG, B::JUNGLE_LEAVES),
        TreeKind::Acacia => (B::ACACIA_LOG, B::ACACIA_LEAVES),
        TreeKind::Cherry => (B::CHERRY_LOG, B::CHERRY_LEAVES),
        TreeKind::Mangrove => (B::MANGROVE_LOG, B::MANGROVE_LEAVES),
    }
}

/// Grow a sapling at (x,y,z) into a full tree. Places nothing and returns false if the trunk is blocked.
/// Leaves are natural (meta 0) so the canopy decays once the tree is felled.
#[expect(clippy::too_many_arguments, reason = "Mirrors the JS twin: world, tables, position, kind/id, rng and change log")]
pub fn grow_tree_at<R: FnMut() -> f64>(
    w: &mut CoreWorld,
    t: &mut SharedWorldTables,
    x: i32,
    y: i32,
    z: i32,
    kind: TreeKind,
    rnd: &mut R,
    log: &mut Vec<BlockChange>,
) -> bool {
    let (log_id, leaf_id) = tree_wood(kind);
    let ri = |lo: i32, hi: i32, r: &mut R| lo + (r() * (hi - lo + 1) as f64).floor() as i32;
    let mut logs: Vec<(i32, i32, i32)> = Vec::new();
    let mut leaves: Vec<(i32, i32, i32)> = Vec::new();
    let disc = |leaves: &mut Vec<(i32, i32, i32)>, r: &mut R, cx: i32, cy: i32, cz: i32, rad: f64, cut: bool| {
        let rr = rad.ceil() as i32;
        for dz in -rr..=rr {
            for dx in -rr..=rr {
                if ((dx * dx + dz * dz) as f64) > rad * rad + 0.6 {
                    continue;
                }
                if cut && dx.abs() == rr && dz.abs() == rr && r() < 0.6 {
                    continue;
                }
                leaves.push((cx + dx, cy, cz + dz));
            }
        }
    };
    let h: i32;
    match kind {
        TreeKind::Oak | TreeKind::Birch => {
            h = if kind == TreeKind::Oak { ri(4, 6, rnd) } else { ri(5, 7, rnd) };
            for i in 0..h { logs.push((x, y + i, z)); }
            disc(&mut leaves, rnd, x, y + h - 3, z, 2.0, true);
            disc(&mut leaves, rnd, x, y + h - 2, z, 2.0, true);
            disc(&mut leaves, rnd, x, y + h - 1, z, 1.0, false);
            leaves.extend_from_slice(&[(x, y + h, z), (x + 1, y + h, z), (x - 1, y + h, z), (x, y + h, z + 1), (x, y + h, z - 1)]);
        }
        TreeKind::Spruce => {
            h = ri(6, 9, rnd);
            for i in 0..h { logs.push((x, y + i, z)); }
            leaves.push((x, y + h, z));
            leaves.push((x, y + h + 1, z));
            for k in 1..=(h - 2) {
                let r = if k % 2 == 1 { (1 + k / 3).min(3) } else { (k / 3).max(1) };
                disc(&mut leaves, rnd, x, y + h - k, z, r as f64, false);
            }
        }
        TreeKind::Jungle => {
            h = ri(7, 11, rnd);
            for i in 0..h { logs.push((x, y + i, z)); }
            disc(&mut leaves, rnd, x, y + h - 2, z, 3.0, true);
            disc(&mut leaves, rnd, x, y + h - 1, z, 2.6, true);
            disc(&mut leaves, rnd, x, y + h, z, 1.6, false);
        }
        TreeKind::Acacia => {
            h = ri(5, 6, rnd);
            let dirs = [(1, 0), (-1, 0), (0, 1), (0, -1)];
            let (bx, bz) = dirs[ri(0, 3, rnd) as usize];
            let (mut tx, mut tz) = (x, z);
            for i in 0..h {
                if i >= h - 3 && i < h - 1 {
                    tx += bx;
                    tz += bz;
                }
                logs.push((tx, y + i, tz));
            }
            let top = y + h - 1;
            for dz in -3..=3_i32 {
                for dx in -3..=3_i32 {
                    if dx.abs() + dz.abs() <= 4 { leaves.push((tx + dx, top, tz + dz)); }
                }
            }
            for dz in -2..=2_i32 {
                for dx in -2..=2_i32 {
                    if dx.abs() + dz.abs() <= 2 { leaves.push((tx + dx, top + 1, tz + dz)); }
                }
            }
        }
        TreeKind::Cherry => {
            h = ri(5, 7, rnd);
            let lean = if rnd() < 0.5 { 1 } else { -1 };
            for i in 0..h { logs.push((x + if i >= h - 2 { lean } else { 0 }, y + i, z)); }
            let (cx, ty) = (x + lean, y + h - 1);
            disc(&mut leaves, rnd, cx, ty - 1, z, 2.0, true);
            disc(&mut leaves, rnd, cx, ty, z, 3.0, true);
            disc(&mut leaves, rnd, cx, ty + 1, z, 3.0, true);
            disc(&mut leaves, rnd, cx, ty + 2, z, 1.0, false);
        }
        TreeKind::Mangrove => {
            h = ri(5, 7, rnd);
            for i in 0..h { logs.push((x, y + 1 + i, z)); }
            logs.push((x, y, z));
            for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                if rnd() < 0.75 { logs.push((x + dx, y, z + dz)); }
            }
            let ty = y + h;
            disc(&mut leaves, rnd, x, ty - 1, z, 2.6, true);
            disc(&mut leaves, rnd, x, ty, z, 2.6, true);
            disc(&mut leaves, rnd, x, ty + 1, z, 1.6, false);
        }
    }

    let passable = |id: u8| {
        if id == B::AIR || is_sapling(id) {
            return true;
        }
        let b = BLOCKS.get(id);
        (b.cross && !b.liquid && !is_crop(id)) || b.is_leaf
    };
    for &(lx, ly, lz) in &logs {
        if !passable(w.get_block_ensured(lx, ly, lz, t)) {
            return false;
        }
    }
    if kind != TreeKind::Acacia && !passable(w.get_block_ensured(x, y + h, z, t)) {
        return false;
    }

    let below = w.get_block_ensured(x, y - 1, z, t);
    if matches!(below, B::GRASS | B::FARMLAND | B::MYCELIUM) {
        w.set_block(x, y - 1, z, B::DIRT, 0, true, t, Some(log));
    }
    for &(lx, ly, lz) in &logs {
        w.set_block(lx, ly, lz, log_id, 0, true, t, Some(log));
    }
    for &(lx, ly, lz) in &leaves {
        let cur = w.get_block_ensured(lx, ly, lz, t);
        let cb = BLOCKS.get(cur);
        if cur == B::AIR || (cb.cross && !cb.liquid && !is_crop(cur) && !is_sapling(cur)) {
            w.set_block(lx, ly, lz, leaf_id, 0, true, t, Some(log));
        }
    }
    true
}

/// One random tick at (x,y,z). Returns true when a sapling grew into a tree.
#[expect(clippy::too_many_arguments, reason = "Mirrors the JS twin: world, tables, position, kind/id, rng and change log")]
pub fn random_tick_block<R: FnMut() -> f64>(
    w: &mut CoreWorld,
    t: &mut SharedWorldTables,
    x: i32,
    y: i32,
    z: i32,
    id: u8,
    rnd: &mut R,
    log: &mut Vec<BlockChange>,
) -> bool {
    if id == B::FARMLAND {
        if (w.get_meta(x, y, z) & 1) != 0 {
            return false;
        }
        for dz in -4..=4 {
            for dx in -4..=4 {
                if w.get_block_ensured(x + dx, y, z + dz, t) == B::WATER {
                    w.set_block(x, y, z, B::FARMLAND, 1, true, t, Some(log));
                    return false;
                }
            }
        }
    } else if is_crop(id) {
        let m = w.get_meta(x, y, z) & 3;
        if m >= 3 || rnd() >= 0.33 {
            return false;
        }
        let wet = w.get_block_ensured(x, y - 1, z, t) == B::FARMLAND && (w.get_meta(x, y - 1, z) & 1) != 0;
        if wet || rnd() < 0.4 {
            w.set_block(x, y, z, id, m + 1, true, t, Some(log));
        }
    } else if let Some(kind) = sapling_tree(id) {
        if rnd() >= 0.25 {
            return false;
        }
        let (sky, blk) = w.get_light_at(x, y, z);
        if sky.max(blk) < 9 {
            return false;
        }
        return grow_tree_at(w, t, x, y, z, kind, rnd, log);
    } else if id == B::REEDS || id == B::CACTUS {
        if rnd() >= 0.2 || w.get_block_ensured(x, y + 1, z, t) != B::AIR {
            return false;
        }
        let mut base = y;
        while base > y - 3 && w.get_block_ensured(x, base - 1, z, t) == id {
            base -= 1;
        }
        if y - base + 1 >= 3 {
            return false;
        }
        if id == B::REEDS {
            let wet = NEIGHBORS6.iter().any(|&(dx, _, dz)| w.get_block_ensured(x + dx, base - 1, z + dz, t) == B::WATER);
            if !wet {
                return false;
            }
        }
        w.set_block(x, y + 1, z, id, 0, true, t, Some(log));
    } else if id == B::DIRT {
        let above = BLOCKS.get(w.get_block_ensured(x, y + 1, z, t));
        if (above.solid && !above.transparent) || above.liquid {
            return false;
        }
        if w.get_light_at(x, y + 1, z).0 < 9 {
            return false;
        }
        for dy in -3..=1 {
            for dz in -1..=1 {
                for dx in -1..=1 {
                    if w.get_block_ensured(x + dx, y + dy, z + dz, t) == B::GRASS && rnd() < 0.6 {
                        w.set_block(x, y, z, B::GRASS, 0, true, t, Some(log));
                        return false;
                    }
                }
            }
        }
    } else if id == B::GRASS {
        let above = BLOCKS.get(w.get_block_ensured(x, y + 1, z, t));
        if (above.solid && !above.transparent && above.shape == "cube") || above.liquid {
            w.set_block(x, y, z, B::DIRT, 0, true, t, Some(log));
        }
    }
    false
}

/// Leaves near a removed log are re-checked after a short random delay; orphaned leaves decay.
#[derive(Default)]
pub struct LeafDecay {
    pub queue: Vec<(i32, i32, i32, f64)>,
    set: HashSet<(i32, i32, i32)>,
}

impl LeafDecay {
    pub fn clear(&mut self) {
        self.queue.clear();
        self.set.clear();
    }

    pub fn shift_local(&mut self, dx: i32, dz: i32) {
        self.set.clear();
        for e in &mut self.queue {
            e.0 -= dx;
            e.2 -= dz;
            self.set.insert((e.0, e.1, e.2));
        }
    }

    /// Queue every natural (meta 0) leaf within `r` of (x,y,z). Only looks at loaded chunks.
    #[expect(clippy::too_many_arguments, reason = "Position, radius and delay window, as in the JS World")]
    pub fn schedule_around(&mut self, w: &CoreWorld, x: i32, y: i32, z: i32, r: i32, delay_min: f64, delay_spread: f64) {
        if self.queue.len() > 4000 {
            return;
        }
        for dy in -r..=r {
            for dz in -r..=r {
                for dx in -r..=r {
                    let (lx, ly, lz) = (x + dx, y + dy, z + dz);
                    if !BLOCKS.get(w.get_block(lx, ly, lz)).is_leaf || w.get_meta(lx, ly, lz) != 0 {
                        continue;
                    }
                    if self.set.insert((lx, ly, lz)) {
                        self.queue.push((lx, ly, lz, delay_min + rand::random::<f64>() * delay_spread));
                    }
                }
            }
        }
    }

    /// Advance timers; returns the (x, y, z, leaf id) of every leaf that decayed this step.
    pub fn update(
        &mut self,
        w: &mut CoreWorld,
        t: &mut SharedWorldTables,
        dt: f64,
        log: &mut Vec<BlockChange>,
    ) -> Vec<(i32, i32, i32, u8)> {
        let mut out = Vec::new();
        if self.queue.is_empty() {
            return out;
        }
        let mut budget = 24;
        let mut i = self.queue.len();
        while i > 0 && budget > 0 {
            i -= 1;
            self.queue[i].3 -= dt;
            if self.queue[i].3 > 0.0 {
                continue;
            }
            let (x, y, z, _) = self.queue.swap_remove(i);
            self.set.remove(&(x, y, z));
            budget -= 1;
            let id = w.get_block(x, y, z);
            if !BLOCKS.get(id).is_leaf || w.get_meta(x, y, z) != 0 || leaf_has_log(w, x, y, z, 6) {
                continue;
            }
            w.set_block(x, y, z, B::AIR, 0, true, t, Some(log));
            out.push((x, y, z, id));
            self.schedule_around(w, x, y, z, 1, 0.4, 2.5);
        }
        out
    }
}

/// Breadth-first search through connected leaves for a log within `max_dist` steps.
pub fn leaf_has_log(w: &CoreWorld, x: i32, y: i32, z: i32, max_dist: i32) -> bool {
    let mut seen: HashSet<(i32, i32, i32)> = HashSet::new();
    seen.insert((x, y, z));
    let mut frontier = vec![(x, y, z)];
    for _ in 0..max_dist {
        if frontier.is_empty() {
            break;
        }
        let mut next = Vec::new();
        for &(cx, cy, cz) in &frontier {
            for &(ox, oy, oz) in &NEIGHBORS6 {
                let n = (cx + ox, cy + oy, cz + oz);
                if !seen.insert(n) {
                    continue;
                }
                let id = w.get_block(n.0, n.1, n.2);
                if is_log(id) {
                    return true;
                }
                if BLOCKS.get(id).is_leaf {
                    next.push(n);
                }
            }
        }
        frontier = next;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use num_bigint::BigInt;
    use num_traits::Zero;

    fn count(w: &CoreWorld, x: i32, y: i32, z: i32) -> (usize, usize) {
        let (mut logs, mut leaves) = (0, 0);
        for dy in -1..20 {
            for dz in -6..=6 {
                for dx in -6..=6 {
                    let id = w.get_block(x + dx, y + dy, z + dz);
                    if is_log(id) { logs += 1; }
                    if BLOCKS.get(id).is_leaf { leaves += 1; }
                }
            }
        }
        (logs, leaves)
    }

    #[test]
    fn every_sapling_grows_and_its_canopy_decays_when_felled() {
        let saplings = [B::OAK_SAPLING, B::BIRCH_SAPLING, B::SPRUCE_SAPLING, B::JUNGLE_SAPLING, B::ACACIA_SAPLING, B::CHERRY_SAPLING, B::MANGROVE_SAPLING];
        for (i, &sap) in saplings.iter().enumerate() {
            let mut w = CoreWorld::new("growth-test", BigInt::zero(), BigInt::zero());
            let mut t = SharedWorldTables::default();
            let (x, y, z) = (i as i32 * 20, 200, 0);
            let mut log = Vec::new();
            w.set_block(x, y - 1, z, B::DIRT, 0, true, &mut t, None);
            w.set_block(x, y, z, sap, 0, true, &mut t, None);
            assert!(w.has_block_support(x, y, z, sap, 0, &mut t));
            let mut seed = 0.37_f64;
            let mut rnd = || { seed = (seed * 9301.0 + 49297.0) % 233280.0 / 233280.0; seed };
            assert!(grow_tree_at(&mut w, &mut t, x, y, z, sapling_tree(sap).unwrap(), &mut rnd, &mut log), "sapling {sap}");
            let (logs, leaves) = count(&w, x, y, z);
            assert!(logs >= 4 && leaves >= 20, "sapling {sap}: {logs} logs, {leaves} leaves");

            // Fell the tree, then let the decay queue run
            for dy in -1..20 {
                for dz in -6..=6 {
                    for dx in -6..=6 {
                        if is_log(w.get_block(x + dx, y + dy, z + dz)) {
                            w.set_block(x + dx, y + dy, z + dz, B::AIR, 0, true, &mut t, None);
                        }
                    }
                }
            }
            for _ in 0..300 {
                let mut ld = std::mem::take(&mut w.leaf_decay);
                ld.update(&mut w, &mut t, 0.1, &mut log);
                w.leaf_decay = ld;
            }
            assert_eq!(count(&w, x, y, z).1, 0, "sapling {sap}: leaves left after decay");
        }
    }

    #[test]
    fn placed_leaves_and_attached_leaves_persist() {
        let mut w = CoreWorld::new("growth-test", BigInt::zero(), BigInt::zero());
        let mut t = SharedWorldTables::default();
        let mut log = Vec::new();
        w.set_block(0, 200, 0, B::OAK_LOG, 0, true, &mut t, None);
        w.set_block(0, 201, 0, B::OAK_LEAVES, 0, true, &mut t, None);
        w.set_block(3, 201, 0, B::OAK_LEAVES, 4, true, &mut t, None);
        w.set_block(2, 200, 0, B::OAK_LOG, 0, true, &mut t, None);
        w.set_block(2, 200, 0, B::AIR, 0, true, &mut t, None);
        for _ in 0..200 {
            let mut ld = std::mem::take(&mut w.leaf_decay);
            ld.update(&mut w, &mut t, 0.1, &mut log);
            w.leaf_decay = ld;
        }
        assert_eq!(w.get_block(0, 201, 0), B::OAK_LEAVES);
        assert_eq!(w.get_block(3, 201, 0), B::OAK_LEAVES);
    }

    #[test]
    fn crops_grow_to_maturity_and_drop_produce() {
        let mut w = CoreWorld::new("growth-test", BigInt::zero(), BigInt::zero());
        let mut t = SharedWorldTables::default();
        let mut log = Vec::new();
        w.set_block(0, 199, 0, B::FARMLAND, 1, true, &mut t, None);
        w.set_block(0, 200, 0, B::CARROTS, 0, true, &mut t, None);
        let mut rnd = || 0.1;
        for _ in 0..10 {
            random_tick_block(&mut w, &mut t, 0, 200, 0, B::CARROTS, &mut rnd, &mut log);
        }
        assert_eq!(w.get_meta(0, 200, 0) & 3, 3);
        let res = crate::enchants::compute_block_drops(BLOCKS.get(B::CARROTS), 3, None, || 0.5);
        assert_eq!(res.drops[0].id, crate::items::IT::CARROT);
        assert!(res.drops[0].count >= 2);
        assert!(crate::items::ITEMS.get(B::CARROTS as i32).is_none(), "crop blocks are not items");
        assert!(crate::items::ITEMS.get(B::OAK_SAPLING as i32).is_some());
    }
}
