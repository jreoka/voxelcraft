use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::blocks::{B, BlockDef};
use crate::items::{IT, ITEMS, ItemDef};
use crate::math::{RawSeededRng, clamp, js_math_round};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct EnchantEntry {
    pub name: String,
    pub lvl: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ItemEnchant {
    pub name: String,
    pub lvl: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list: Option<Vec<EnchantEntry>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ItemStack {
    pub id: i32,
    pub count: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dur: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enchant: Option<ItemEnchant>,
}

pub struct EnchantDef {
    pub name: &'static str,
    pub max: i32,
    pub w: f64,
    pub kinds: &'static [&'static str],
    pub group: Option<&'static str>,
}

pub const ENCHANTS: &[EnchantDef] = &[
    EnchantDef { name: "Sharpness", max: 5, w: 10.0, kinds: &["sword", "axe"], group: Some("dmg") },
    EnchantDef { name: "Smite", max: 5, w: 5.0, kinds: &["sword", "axe"], group: Some("dmg") },
    EnchantDef { name: "Knockback", max: 2, w: 5.0, kinds: &["sword"], group: None },
    EnchantDef { name: "Fire Aspect", max: 2, w: 2.0, kinds: &["sword"], group: None },
    EnchantDef { name: "Looting", max: 3, w: 2.0, kinds: &["sword"], group: None },
    EnchantDef { name: "Efficiency", max: 5, w: 10.0, kinds: &["pickaxe", "axe", "shovel", "hoe"], group: None },
    EnchantDef { name: "Fortune", max: 3, w: 2.0, kinds: &["pickaxe", "axe", "shovel", "hoe"], group: Some("drop") },
    EnchantDef { name: "Silk Touch", max: 1, w: 1.0, kinds: &["pickaxe", "axe", "shovel", "hoe"], group: Some("drop") },
    EnchantDef { name: "Unbreaking", max: 3, w: 5.0, kinds: &["sword", "axe", "pickaxe", "shovel", "hoe", "bow", "armor", "shield"], group: None },
    EnchantDef { name: "Protection", max: 4, w: 10.0, kinds: &["armor"], group: Some("prot") },
    EnchantDef { name: "Fire Protection", max: 4, w: 5.0, kinds: &["armor"], group: Some("prot") },
    EnchantDef { name: "Blast Protection", max: 4, w: 2.0, kinds: &["armor"], group: Some("prot") },
    EnchantDef { name: "Projectile Protection", max: 4, w: 5.0, kinds: &["armor"], group: Some("prot") },
    EnchantDef { name: "Feather Falling", max: 4, w: 5.0, kinds: &["boots"], group: None },
    EnchantDef { name: "Respiration", max: 3, w: 2.0, kinds: &["helmet"], group: None },
    EnchantDef { name: "Thorns", max: 3, w: 1.0, kinds: &["chestplate"], group: None },
    EnchantDef { name: "Power", max: 5, w: 10.0, kinds: &["bow"], group: None },
    EnchantDef { name: "Punch", max: 2, w: 2.0, kinds: &["bow"], group: None },
    EnchantDef { name: "Flame", max: 1, w: 2.0, kinds: &["bow"], group: None },
    EnchantDef { name: "Infinity", max: 1, w: 1.0, kinds: &["bow"], group: None },
];

pub fn is_valid_enchant_name(name: &str) -> bool {
    ENCHANTS.iter().any(|e| e.name == name)
}

const ARMOR_KIND: [&str; 4] = ["helmet", "chestplate", "leggings", "boots"];

pub fn ench_kinds_of(it: &ItemDef) -> Vec<&'static str> {
    if let Some(tt) = it.tool_type
        && matches!(tt, "sword" | "axe" | "pickaxe" | "shovel" | "hoe" | "bow") {
            return vec![tt];
        }
    if it.category == "armor"
        && let Some(slot) = it.armor_slot {
            if slot == 4 {
                return vec!["shield"];
            }
            if slot < 4 {
                return vec!["armor", ARMOR_KIND[slot]];
            }
        }
    Vec::new()
}

pub fn ench_list(stack: Option<&ItemStack>) -> Vec<EnchantEntry> {
    let Some(e) = stack.and_then(|s| s.enchant.as_ref()) else {
        return Vec::new();
    };
    if let Some(list) = &e.list {
        list.clone()
    } else {
        vec![EnchantEntry {
            name: e.name.clone(),
            lvl: e.lvl,
        }]
    }
}

pub fn ench_lvl(stack: Option<&ItemStack>, name: &str) -> i32 {
    let Some(e) = stack.and_then(|s| s.enchant.as_ref()) else {
        return 0;
    };
    if let Some(list) = &e.list {
        for x in list {
            if x.name == name {
                return x.lvl;
            }
        }
        0
    } else if e.name == name {
        e.lvl
    } else {
        0
    }
}

pub fn make_enchant(list: &[EnchantEntry]) -> ItemEnchant {
    ItemEnchant {
        name: list[0].name.clone(),
        lvl: list[0].lvl,
        list: Some(list.to_vec()),
    }
}

pub fn roll_enchants(it: &ItemDef, cost: i32, rng: &mut RawSeededRng) -> Vec<EnchantEntry> {
    let kinds = ench_kinds_of(it);
    let pool: Vec<&EnchantDef> = ENCHANTS
        .iter()
        .filter(|d| d.kinds.iter().any(|k| kinds.contains(k)))
        .collect();
    if pool.is_empty() {
        return Vec::new();
    }
    let mut result: Vec<EnchantEntry> = Vec::new();
    let mut used_groups: HashSet<&'static str> = HashSet::new();

    let pick = |result: &[EnchantEntry], used_groups: &mut HashSet<&'static str>, rng: &mut RawSeededRng| -> Option<EnchantEntry> {
        let cand: Vec<&EnchantDef> = pool
            .iter()
            .copied()
            .filter(|d| {
                !result.iter().any(|r| r.name == d.name)
                    && !d.group.is_some_and(|g| used_groups.contains(g))
            })
            .collect();
        if cand.is_empty() {
            return None;
        }
        let tot: f64 = cand.iter().map(|d| d.w).sum();
        let mut r = rng.next() * tot;
        for d in cand {
            r -= d.w;
            if r <= 0.0 {
                let frac = clamp(((cost as f64) + rng.next() * 6.0 - 3.0) / 30.0, 0.0, 1.0);
                let lvl = clamp(
                    js_math_round(1.0 + frac * ((d.max - 1) as f64) + (rng.next() - 0.5) * 0.8),
                    1.0,
                    d.max as f64,
                ) as i32;
                if let Some(g) = d.group {
                    used_groups.insert(g);
                }
                return Some(EnchantEntry {
                    name: d.name.to_string(),
                    lvl,
                });
            }
        }
        None
    };

    let Some(first) = pick(&result, &mut used_groups, rng) else {
        return Vec::new();
    };
    result.push(first);
    let mut chance = ((cost + 1) as f64) / 50.0;
    while rng.next() < chance {
        let Some(nx) = pick(&result, &mut used_groups, rng) else {
            break;
        };
        result.push(nx);
        chance *= 0.5;
    }
    result
}

pub fn compute_enchant_power<F: Fn(i32, i32, i32) -> u8>(get_block: F, x: i32, y: i32, z: i32) -> i32 {
    let mut power = 0;
    for dz in -2_i32..=2 {
        for dx in -2_i32..=2 {
            if dx.abs() != 2 && dz.abs() != 2 {
                continue;
            }
            for dy in 0..=1 {
                if get_block(x + dx, y + dy, z + dz) != B::BOOKSHELF {
                    continue;
                }
                let gx = if dx.abs() == 2 { x + dx.signum() } else { x + dx };
                let gz = if dz.abs() == 2 { z + dz.signum() } else { z + dz };
                let gap = get_block(gx, y + dy, gz);
                if gap == B::AIR {
                    power += 1;
                }
            }
        }
    }
    power.min(15)
}

#[derive(Clone, Debug)]
pub struct EnchantOffer {
    pub cost: i32,
    pub lapis: i32,
    pub list: Vec<EnchantEntry>,
}

pub fn compute_enchant_offers(item_id: i32, has_enchant: bool, power: i32, seed: u32) -> Vec<EnchantOffer> {
    let Some(it) = ITEMS.get(item_id) else {
        return Vec::new();
    };
    if ench_kinds_of(it).is_empty() || has_enchant {
        return Vec::new();
    }
    // JS: `seed ^ (itemSt.id * 2654435761)` -> bitwise XOR truncates both operands to i32 then ToUint32
    let mul_i32 = (((item_id as f64) * 2654435761.0) as i64) as u32;
    let mut rng = RawSeededRng::new(seed ^ mul_i32);
    let base = 1
        + (rng.next() * 8.0).floor() as i32
        + (power / 2)
        + (rng.next() * ((power + 1) as f64)).floor() as i32;
    let costs = [
        1.max(base / 3),
        (base * 2) / 3 + 1,
        base.max(power * 2),
    ];
    costs
        .iter()
        .enumerate()
        .map(|(i, &cost)| {
            let c = cost.min(30);
            let offer_seed = seed.wrapping_add((i as u32) * 7919).wrapping_add(item_id as u32);
            let mut orng = RawSeededRng::new(offer_seed);
            EnchantOffer {
                cost: c,
                lapis: (i as i32) + 1,
                list: roll_enchants(it, c, &mut orng),
            }
        })
        .collect()
}

pub struct BlockDropResult {
    pub drops: Vec<ItemStack>,
    pub xp: i32,
}

pub fn compute_block_drops<R: FnMut() -> f64>(
    blk: &BlockDef,
    meta: u8,
    held: Option<&ItemStack>,
    mut rnd: R,
) -> BlockDropResult {
    let mut out = BlockDropResult {
        drops: Vec::new(),
        xp: 0,
    };
    let held_it = held.and_then(|h| ITEMS.get(h.id));
    let tool_tier = match held_it {
        Some(hit) if hit.tool_type == Some(blk.tool) => hit.tier,
        _ => 0,
    };
    if !(blk.min_tier == 0 || tool_tier >= blk.min_tier) {
        return out;
    }
    let fortune_lvl = ench_lvl(held, "Fortune");
    let silk = ench_lvl(held, "Silk Touch") > 0;
    let silk_ok = silk
        && blk.shape == "cube"
        && !blk.liquid
        && ITEMS.get(blk.id as i32).is_some()
        && blk.drop != blk.id as i32;

    let mut add = |id: i32, count: i32| {
        if count > 0 {
            out.drops.push(ItemStack {
                id,
                count,
                dur: None,
                enchant: None,
            });
        }
    };

    if silk_ok {
        add(blk.id as i32, 1);
    } else if blk.is_leaf {
        let r = rnd();
        if r < 0.11 {
            add(IT::APPLE, 1);
        } else if r < 0.17 {
            add(IT::SWEET_BERRIES, 1);
        } else if r < 0.30 {
            add(IT::STICK, 1);
        }
    } else if blk.id == B::TALL_GRASS {
        if rnd() < 0.28 {
            add(IT::WHEAT_SEEDS, 1);
        }
    } else if blk.id == B::WHEAT {
        if (meta & 3) >= 2 {
            let bonus = if fortune_lvl > 0 && rnd() < 0.5 { 1 } else { 0 };
            add(IT::WHEAT_ITEM, 1 + bonus);
            add(IT::WHEAT_SEEDS, 1 + (rnd() * 2.0).floor() as i32);
        } else {
            add(IT::WHEAT_SEEDS, 1);
        }
    } else if blk.id == B::GRAVEL && rnd() < 0.22 {
        add(IT::FLINT, 1);
    } else if blk.drop > 0 {
        let mut cnt = blk.drop_count.max(1);
        if fortune_lvl > 0 && blk.id >= B::COAL_ORE && blk.id <= B::EMERALD_ORE {
            cnt += (rnd() * ((fortune_lvl + 1) as f64)).floor() as i32;
        }
        add(blk.drop, cnt);
    }

    if !silk_ok && blk.id >= B::COAL_ORE && blk.id <= B::ANCIENT_DEBRIS {
        out.xp = 4;
    }
    out
}
