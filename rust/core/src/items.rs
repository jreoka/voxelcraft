use std::collections::HashMap;
use std::sync::LazyLock;

use crate::blocks::{B, BLOCKS};

#[allow(non_snake_case)]
pub mod IT {
    pub const STICK: i32 = 120;
    pub const COAL: i32 = 121;
    pub const RAW_IRON: i32 = 122;
    pub const IRON_INGOT: i32 = 123;
    pub const RAW_COPPER: i32 = 124;
    pub const COPPER_INGOT: i32 = 125;
    pub const RAW_GOLD: i32 = 126;
    pub const GOLD_INGOT: i32 = 127;
    pub const REDSTONE: i32 = 128;
    pub const LAPIS: i32 = 129;
    pub const DIAMOND: i32 = 130;
    pub const EMERALD: i32 = 131;
    pub const ANCIENT_SCRAP: i32 = 132;
    pub const NETHERITE_INGOT: i32 = 133;
    pub const FLINT: i32 = 134;
    pub const STRING: i32 = 135;
    pub const BONE: i32 = 136;
    pub const GUNPOWDER: i32 = 137;
    pub const SLIMEBALL: i32 = 138;
    pub const LEATHER: i32 = 139;
    pub const FEATHER: i32 = 140;
    pub const CLAY_BALL: i32 = 141;
    pub const BRICK_ITEM: i32 = 142;
    pub const WHEAT_ITEM: i32 = 143;
    pub const AMETHYST_SHARD: i32 = 144;
    pub const PAPER: i32 = 145;
    pub const BOOK: i32 = 146;
    // Food
    pub const APPLE: i32 = 150;
    pub const RAW_BEEF: i32 = 151;
    pub const STEAK: i32 = 152;
    pub const RAW_PORK: i32 = 153;
    pub const COOKED_PORK: i32 = 154;
    pub const RAW_MUTTON: i32 = 155;
    pub const COOKED_MUTTON: i32 = 156;
    pub const RAW_CHICKEN: i32 = 157;
    pub const COOKED_CHICKEN: i32 = 158;
    pub const RAW_FISH: i32 = 159;
    pub const COOKED_FISH: i32 = 160;
    pub const BREAD: i32 = 161;
    pub const MUSHROOM_STEW: i32 = 162;
    pub const GOLDEN_APPLE: i32 = 163;
    // Tools
    pub const WOOD_PICK: i32 = 170;
    pub const WOOD_AXE: i32 = 171;
    pub const WOOD_SHOVEL: i32 = 172;
    pub const WOOD_HOE: i32 = 173;
    pub const WOOD_SWORD: i32 = 174;
    pub const STONE_PICK: i32 = 175;
    pub const STONE_AXE: i32 = 176;
    pub const STONE_SHOVEL: i32 = 177;
    pub const STONE_HOE: i32 = 178;
    pub const STONE_SWORD: i32 = 179;
    pub const IRON_PICK: i32 = 180;
    pub const IRON_AXE: i32 = 181;
    pub const IRON_SHOVEL: i32 = 182;
    pub const IRON_HOE: i32 = 183;
    pub const IRON_SWORD: i32 = 184;
    pub const GOLD_PICK: i32 = 185;
    pub const GOLD_AXE: i32 = 186;
    pub const GOLD_SHOVEL: i32 = 187;
    pub const GOLD_HOE: i32 = 188;
    pub const GOLD_SWORD: i32 = 189;
    pub const DIAMOND_PICK: i32 = 190;
    pub const DIAMOND_AXE: i32 = 191;
    pub const DIAMOND_SHOVEL: i32 = 192;
    pub const DIAMOND_HOE: i32 = 193;
    pub const DIAMOND_SWORD: i32 = 194;
    pub const NETHERITE_PICK: i32 = 195;
    pub const NETHERITE_AXE: i32 = 196;
    pub const NETHERITE_SHOVEL: i32 = 197;
    pub const NETHERITE_HOE: i32 = 198;
    pub const NETHERITE_SWORD: i32 = 199;
    // Armor & Utility
    pub const LEATHER_HELM: i32 = 200;
    pub const LEATHER_CHEST: i32 = 201;
    pub const LEATHER_LEGS: i32 = 202;
    pub const LEATHER_BOOTS: i32 = 203;
    pub const IRON_HELM: i32 = 204;
    pub const IRON_CHEST: i32 = 205;
    pub const IRON_LEGS: i32 = 206;
    pub const IRON_BOOTS: i32 = 207;
    pub const GOLD_HELM: i32 = 208;
    pub const GOLD_CHEST: i32 = 209;
    pub const GOLD_LEGS: i32 = 210;
    pub const GOLD_BOOTS: i32 = 211;
    pub const DIAMOND_HELM: i32 = 212;
    pub const DIAMOND_CHEST: i32 = 213;
    pub const DIAMOND_LEGS: i32 = 214;
    pub const DIAMOND_BOOTS: i32 = 215;
    pub const NETHERITE_HELM: i32 = 216;
    pub const NETHERITE_CHEST: i32 = 217;
    pub const NETHERITE_LEGS: i32 = 218;
    pub const NETHERITE_BOOTS: i32 = 219;
    pub const SHIELD: i32 = 220;
    pub const BOW: i32 = 221;
    pub const ARROW: i32 = 222;
    pub const BUCKET: i32 = 223;
    pub const WATER_BUCKET: i32 = 224;
    pub const LAVA_BUCKET: i32 = 225;
    // Expanded Items
    pub const WHEAT_SEEDS: i32 = 260;
    pub const CARROT: i32 = 261;
    pub const POTATO: i32 = 262;
    pub const BAKED_POTATO: i32 = 263;
    pub const MELON_SLICE: i32 = 264;
    pub const PUMPKIN_PIE: i32 = 265;
    pub const BONE_MEAL: i32 = 266;
    pub const CHARCOAL: i32 = 267;
    pub const MAGMA_CREAM: i32 = 268;
    pub const SWEET_BERRIES: i32 = 269;
    pub const COOKIE: i32 = 270;
    pub const COPPER_PICK: i32 = 271;
    pub const COPPER_AXE: i32 = 272;
    pub const COPPER_SHOVEL: i32 = 273;
    pub const COPPER_HOE: i32 = 274;
    pub const COPPER_SWORD: i32 = 275;
    pub const EMERALD_PICK: i32 = 276;
    pub const EMERALD_AXE: i32 = 277;
    pub const EMERALD_SHOVEL: i32 = 278;
    pub const EMERALD_HOE: i32 = 279;
    pub const EMERALD_SWORD: i32 = 280;
    pub const COPPER_HELM: i32 = 281;
    pub const COPPER_CHEST: i32 = 282;
    pub const COPPER_LEGS: i32 = 283;
    pub const COPPER_BOOTS: i32 = 284;
}

#[derive(Clone, Debug)]
pub struct ItemDef {
    pub id: i32,
    pub name: String,
    pub block_id: Option<u8>,
    pub stack: i32,
    pub category: &'static str,
    pub fuel: f64,
    pub food: f64,
    pub sat: f64,
    pub heal: f64,
    pub tool_type: Option<&'static str>,
    pub tier: i32,
    pub speed: f64,
    pub max_dur: i32,
    pub dmg: f64,
    pub armor_slot: Option<usize>,
    pub defense: f64,
}

impl ItemDef {
    fn base(id: i32, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            block_id: None,
            stack: 64,
            category: "material",
            fuel: 0.0,
            food: 0.0,
            sat: 0.0,
            heal: 0.0,
            tool_type: None,
            tier: 0,
            speed: 0.0,
            max_dur: 0,
            dmg: 0.0,
            armor_slot: None,
            defense: 0.0,
        }
    }
}

pub struct ItemRegistry {
    pub items: HashMap<i32, ItemDef>,
}

impl ItemRegistry {
    fn new() -> Self {
        let mut items = HashMap::new();
        for (id, slot) in BLOCKS.blocks.iter().enumerate().skip(1) {
            if let Some(b) = slot {
                if b.liquid || b.crop {
                    continue;
                }
                let mut it = ItemDef::base(id as i32, b.name);
                it.block_id = Some(id as u8);
                it.category = "block";
                items.insert(id as i32, it);
            }
        }
        let mut add = |id: i32, name: &str, f: Box<dyn FnOnce(&mut ItemDef)>| {
            let mut it = ItemDef::base(id, name);
            f(&mut it);
            items.insert(id, it);
        };

        add(IT::STICK, "Stick", Box::new(|i| { i.fuel = 5.0; }));
        add(IT::COAL, "Coal", Box::new(|i| { i.fuel = 40.0; }));
        add(IT::RAW_IRON, "Raw Iron", Box::new(|_| {}));
        add(IT::IRON_INGOT, "Iron Ingot", Box::new(|_| {}));
        add(IT::RAW_COPPER, "Raw Copper", Box::new(|_| {}));
        add(IT::COPPER_INGOT, "Copper Ingot", Box::new(|_| {}));
        add(IT::RAW_GOLD, "Raw Gold", Box::new(|_| {}));
        add(IT::GOLD_INGOT, "Gold Ingot", Box::new(|_| {}));
        add(IT::REDSTONE, "Redstone Dust", Box::new(|i| { i.block_id = Some(B::REDSTONE_WIRE); }));
        add(IT::LAPIS, "Lapis Lazuli", Box::new(|_| {}));
        add(IT::DIAMOND, "Diamond", Box::new(|_| {}));
        add(IT::EMERALD, "Emerald", Box::new(|_| {}));
        add(IT::ANCIENT_SCRAP, "Ancient Scrap", Box::new(|_| {}));
        add(IT::NETHERITE_INGOT, "Netherite Ingot", Box::new(|_| {}));
        add(IT::FLINT, "Flint", Box::new(|_| {}));
        add(IT::STRING, "String", Box::new(|_| {}));
        add(IT::BONE, "Bone", Box::new(|_| {}));
        add(IT::GUNPOWDER, "Gunpowder", Box::new(|_| {}));
        add(IT::SLIMEBALL, "Slimeball", Box::new(|_| {}));
        add(IT::LEATHER, "Leather", Box::new(|_| {}));
        add(IT::FEATHER, "Feather", Box::new(|_| {}));
        add(IT::CLAY_BALL, "Clay Ball", Box::new(|_| {}));
        add(IT::BRICK_ITEM, "Brick", Box::new(|_| {}));
        add(IT::WHEAT_ITEM, "Wheat", Box::new(|_| {}));
        add(IT::AMETHYST_SHARD, "Amethyst Shard", Box::new(|_| {}));
        add(IT::PAPER, "Paper", Box::new(|_| {}));
        add(IT::BOOK, "Book", Box::new(|_| {}));

        // Food
        add(IT::APPLE, "Apple", Box::new(|i| { i.category = "food"; i.food = 4.0; i.sat = 2.4; }));
        add(IT::RAW_BEEF, "Raw Beef", Box::new(|i| { i.category = "food"; i.food = 3.0; i.sat = 1.8; }));
        add(IT::STEAK, "Cooked Steak", Box::new(|i| { i.category = "food"; i.food = 8.0; i.sat = 10.0; }));
        add(IT::RAW_PORK, "Raw Porkchop", Box::new(|i| { i.category = "food"; i.food = 3.0; i.sat = 1.8; }));
        add(IT::COOKED_PORK, "Cooked Porkchop", Box::new(|i| { i.category = "food"; i.food = 8.0; i.sat = 10.0; }));
        add(IT::RAW_MUTTON, "Raw Mutton", Box::new(|i| { i.category = "food"; i.food = 2.0; i.sat = 1.2; }));
        add(IT::COOKED_MUTTON, "Cooked Mutton", Box::new(|i| { i.category = "food"; i.food = 6.0; i.sat = 8.0; }));
        add(IT::RAW_CHICKEN, "Raw Chicken", Box::new(|i| { i.category = "food"; i.food = 2.0; i.sat = 1.2; }));
        add(IT::COOKED_CHICKEN, "Roast Chicken", Box::new(|i| { i.category = "food"; i.food = 6.0; i.sat = 7.2; }));
        add(IT::RAW_FISH, "Raw Fish", Box::new(|i| { i.category = "food"; i.food = 2.0; i.sat = 1.0; }));
        add(IT::COOKED_FISH, "Cooked Fish", Box::new(|i| { i.category = "food"; i.food = 6.0; i.sat = 7.0; }));
        add(IT::BREAD, "Bread", Box::new(|i| { i.category = "food"; i.food = 5.0; i.sat = 6.0; }));
        add(IT::MUSHROOM_STEW, "Mushroom Stew", Box::new(|i| { i.category = "food"; i.food = 7.0; i.sat = 8.0; i.stack = 16; }));
        add(IT::GOLDEN_APPLE, "Golden Apple", Box::new(|i| { i.category = "food"; i.food = 6.0; i.sat = 12.0; i.heal = 8.0; }));

        let tool_tiers: &[(&str, i32, i32, f64, i32, f64)] = &[
            ("Wooden", 170, 1, 2.2, 65, 4.0),
            ("Stone", 175, 2, 4.0, 140, 5.0),
            ("Iron", 180, 3, 6.2, 260, 6.0),
            ("Golden", 185, 2, 9.5, 55, 5.0),
            ("Diamond", 190, 4, 8.5, 1200, 8.0),
            ("Netherite", 195, 5, 10.5, 2200, 10.0),
            ("Copper", IT::COPPER_PICK, 2, 5.0, 190, 5.5),
            ("Emerald", IT::EMERALD_PICK, 4, 9.2, 980, 8.5),
        ];
        for &(name, base, tier, speed, dur, dmg) in tool_tiers {
            add(base, &format!("{} Pickaxe", name), Box::new(move |i| {
                i.category = "tool"; i.tool_type = Some("pickaxe"); i.tier = tier; i.speed = speed; i.max_dur = dur; i.dmg = dmg - 2.0; i.stack = 1;
            }));
            add(base + 1, &format!("{} Axe", name), Box::new(move |i| {
                i.category = "tool"; i.tool_type = Some("axe"); i.tier = tier; i.speed = speed; i.max_dur = dur; i.dmg = dmg + 1.0; i.stack = 1;
            }));
            add(base + 2, &format!("{} Shovel", name), Box::new(move |i| {
                i.category = "tool"; i.tool_type = Some("shovel"); i.tier = tier; i.speed = speed; i.max_dur = dur; i.dmg = dmg - 2.5; i.stack = 1;
            }));
            add(base + 3, &format!("{} Hoe", name), Box::new(move |i| {
                i.category = "tool"; i.tool_type = Some("hoe"); i.tier = tier; i.speed = speed; i.max_dur = dur; i.dmg = 2.0; i.stack = 1;
            }));
            add(base + 4, &format!("{} Sword", name), Box::new(move |i| {
                i.category = "weapon"; i.tool_type = Some("sword"); i.tier = tier; i.speed = 1.5; i.max_dur = dur; i.dmg = dmg + 2.0; i.stack = 1;
            }));
        }

        let armor_tiers: &[(&str, i32, [f64; 4], i32)] = &[
            ("Leather", 200, [1.0, 3.0, 2.0, 1.0], 80),
            ("Iron", 204, [2.0, 6.0, 5.0, 2.0], 240),
            ("Golden", 208, [2.0, 5.0, 3.0, 1.0], 110),
            ("Diamond", 212, [3.0, 8.0, 6.0, 3.0], 520),
            ("Netherite", 216, [4.0, 9.0, 7.0, 4.0], 850),
            ("Copper", IT::COPPER_HELM, [2.0, 5.0, 4.0, 1.0], 165),
        ];
        let labels = ["Helmet", "Chestplate", "Leggings", "Boots"];
        for &(name, base, defs, dur) in armor_tiers {
            for idx in 0..4usize {
                let def_v = defs[idx];
                add(base + idx as i32, &format!("{} {}", name, labels[idx]), Box::new(move |i| {
                    i.category = "armor"; i.armor_slot = Some(idx); i.defense = def_v; i.max_dur = dur; i.stack = 1;
                }));
            }
        }

        add(IT::SHIELD, "Shield", Box::new(|i| { i.category = "armor"; i.armor_slot = Some(4); i.defense = 2.0; i.max_dur = 340; i.stack = 1; }));
        add(IT::BOW, "Bow", Box::new(|i| { i.category = "weapon"; i.tool_type = Some("bow"); i.max_dur = 380; i.dmg = 7.0; i.stack = 1; }));
        add(IT::ARROW, "Arrow", Box::new(|i| { i.category = "ammo"; }));
        add(IT::BUCKET, "Bucket", Box::new(|i| { i.category = "tool"; i.stack = 16; }));
        add(IT::WATER_BUCKET, "Water Bucket", Box::new(|i| { i.category = "tool"; i.stack = 1; }));
        add(IT::LAVA_BUCKET, "Lava Bucket", Box::new(|i| { i.category = "tool"; i.stack = 1; i.fuel = 200.0; }));

        add(IT::WHEAT_SEEDS, "Wheat Seeds", Box::new(|i| { i.category = "seed"; }));
        add(IT::CARROT, "Carrot", Box::new(|i| { i.category = "food"; i.food = 3.0; i.sat = 3.6; }));
        add(IT::POTATO, "Potato", Box::new(|i| { i.category = "food"; i.food = 2.0; i.sat = 1.2; }));
        add(IT::BAKED_POTATO, "Baked Potato", Box::new(|i| { i.category = "food"; i.food = 6.0; i.sat = 7.2; }));
        add(IT::MELON_SLICE, "Melon Slice", Box::new(|i| { i.category = "food"; i.food = 2.0; i.sat = 1.6; }));
        add(IT::PUMPKIN_PIE, "Pumpkin Pie", Box::new(|i| { i.category = "food"; i.food = 8.0; i.sat = 6.0; }));
        add(IT::BONE_MEAL, "Bone Meal", Box::new(|_| {}));
        add(IT::CHARCOAL, "Charcoal", Box::new(|i| { i.fuel = 40.0; }));
        add(IT::MAGMA_CREAM, "Magma Cream", Box::new(|i| { i.fuel = 60.0; }));
        add(IT::SWEET_BERRIES, "Sweet Berries", Box::new(|i| { i.category = "food"; i.food = 2.0; i.sat = 1.4; }));
        add(IT::COOKIE, "Cookie", Box::new(|i| { i.category = "food"; i.food = 3.0; i.sat = 2.0; }));

        Self { items }
    }

    pub fn get(&self, id: i32) -> Option<&ItemDef> {
        self.items.get(&id)
    }

    pub fn get_fuel_time(&self, id: i32) -> f64 {
        let Some(it) = self.items.get(&id) else {
            return 0.0;
        };
        if it.fuel > 0.0 {
            return it.fuel;
        }
        if (B::OAK_LOG as i32..=B::DARK_OAK_LOG as i32).contains(&id)
            || id == B::CHERRY_LOG as i32
            || id == B::MANGROVE_LOG as i32
        {
            return 15.0;
        }
        if (B::OAK_PLANKS as i32..=B::JUNGLE_PLANKS as i32).contains(&id)
            || id == B::CHERRY_PLANKS as i32
            || id == B::MANGROVE_PLANKS as i32
            || id == B::ACACIA_PLANKS as i32
            || id == B::DARK_OAK_PLANKS as i32
        {
            return 15.0;
        }
        if id == B::OAK_FENCE as i32
            || id == B::FENCE_GATE as i32
            || id == B::OAK_SLAB as i32
            || id == B::OAK_STAIRS as i32
            || id == B::BAMBOO as i32
            || id == B::BARREL as i32
        {
            return 12.0;
        }
        0.0
    }
}

pub static ITEMS: LazyLock<ItemRegistry> = LazyLock::new(ItemRegistry::new);

pub struct TraderTrade {
    pub give: (i32, i32),
    pub get: (i32, i32),
}

pub const TRADER_TRADES: &[TraderTrade] = &[
    TraderTrade { give: (IT::WHEAT_ITEM, 12), get: (IT::EMERALD, 1) },
    TraderTrade { give: (IT::COAL, 14), get: (IT::EMERALD, 1) },
    TraderTrade { give: (IT::EMERALD, 2), get: (IT::GOLDEN_APPLE, 1) },
    TraderTrade { give: (IT::EMERALD, 3), get: (IT::DIAMOND, 1) },
    TraderTrade { give: (IT::EMERALD, 1), get: (B::SLIME_BLOCK as i32, 2) },
    TraderTrade { give: (IT::EMERALD, 2), get: (B::SEA_LANTERN as i32, 4) },
    TraderTrade { give: (IT::EMERALD, 2), get: (B::CHERRY_PLANKS as i32, 16) },
    TraderTrade { give: (IT::EMERALD, 1), get: (IT::ARROW, 16) },
];

pub fn campfire_cook(id: i32) -> Option<i32> {
    match id {
        IT::RAW_BEEF => Some(IT::STEAK),
        IT::RAW_PORK => Some(IT::COOKED_PORK),
        IT::RAW_MUTTON => Some(IT::COOKED_MUTTON),
        IT::RAW_CHICKEN => Some(IT::COOKED_CHICKEN),
        IT::RAW_FISH => Some(IT::COOKED_FISH),
        IT::POTATO => Some(IT::BAKED_POTATO),
        _ => None,
    }
}

pub const WOOL_PALETTE: [[f64; 3]; 4] = [
    [0.94, 0.94, 0.92],
    [0.82, 0.80, 0.78],
    [0.45, 0.34, 0.26],
    [0.24, 0.24, 0.28],
];
