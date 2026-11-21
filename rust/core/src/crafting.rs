use std::collections::HashMap;
use std::sync::LazyLock;

use crate::blocks::B;
use crate::items::IT;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IngredientReq {
    Item(i32),
    Planks,
    Log,
}

#[derive(Clone, Debug)]
pub struct Recipe {
    pub out_id: i32,
    pub out_count: i32,
    pub ingredients: Vec<(IngredientReq, i32)>,
    pub needs_table: bool,
}

pub struct CraftingSystem {
    pub recipes: Vec<Recipe>,
    pub smelt_map: HashMap<i32, i32>,
}

impl CraftingSystem {
    fn new() -> Self {
        let mut sys = Self {
            recipes: Vec::new(),
            smelt_map: HashMap::new(),
        };
        sys.init_recipes();
        sys.init_smelting();
        sys
    }

    fn add(&mut self, out_id: i32, out_count: i32, ingredients: Vec<(IngredientReq, i32)>, needs_table: bool) {
        self.recipes.push(Recipe {
            out_id,
            out_count,
            ingredients,
            needs_table,
        });
    }

    fn init_recipes(&mut self) {
        use IngredientReq::{Item as I, Log, Planks};
        self.add(B::OAK_PLANKS as i32, 4, vec![(I(B::OAK_LOG as i32), 1)], false);
        self.add(B::BIRCH_PLANKS as i32, 4, vec![(I(B::BIRCH_LOG as i32), 1)], false);
        self.add(B::SPRUCE_PLANKS as i32, 4, vec![(I(B::SPRUCE_LOG as i32), 1)], false);
        self.add(B::JUNGLE_PLANKS as i32, 4, vec![(I(B::JUNGLE_LOG as i32), 1)], false);
        self.add(B::ACACIA_PLANKS as i32, 4, vec![(I(B::ACACIA_LOG as i32), 1)], false);
        self.add(B::DARK_OAK_PLANKS as i32, 4, vec![(I(B::DARK_OAK_LOG as i32), 1)], false);
        self.add(B::CHERRY_PLANKS as i32, 4, vec![(I(B::CHERRY_LOG as i32), 1)], false);
        self.add(B::MANGROVE_PLANKS as i32, 4, vec![(I(B::MANGROVE_LOG as i32), 1)], false);
        self.add(IT::STICK, 4, vec![(Planks, 2)], false);
        self.add(IT::STICK, 2, vec![(I(B::BAMBOO as i32), 2)], false);
        self.add(B::CRAFTING_TABLE as i32, 1, vec![(Planks, 4)], false);
        self.add(B::TORCH as i32, 4, vec![(I(IT::COAL), 1), (I(IT::STICK), 1)], false);
        self.add(B::TORCH as i32, 4, vec![(I(IT::CHARCOAL), 1), (I(IT::STICK), 1)], false);
        self.add(B::FURNACE as i32, 1, vec![(I(B::COBBLE as i32), 8)], true);
        self.add(B::CHEST as i32, 1, vec![(Planks, 8)], true);
        self.add(B::BARREL as i32, 1, vec![(Planks, 6), (I(B::OAK_SLAB as i32), 2)], true);
        self.add(B::WORKSTATION as i32, 1, vec![(I(IT::IRON_INGOT), 2), (Planks, 4)], true);
        self.add(B::ENCHANT_TABLE as i32, 1, vec![(I(B::OBSIDIAN as i32), 4), (I(IT::DIAMOND), 2), (I(IT::BOOK), 1)], true);
        self.add(B::CAMPFIRE as i32, 1, vec![(Log, 3), (I(IT::STICK), 3), (I(IT::COAL), 1)], true);
        self.add(B::JUKEBOX as i32, 1, vec![(Planks, 8), (I(IT::DIAMOND), 1)], true);

        // Building & Utility
        self.add(B::OAK_SLAB as i32, 6, vec![(Planks, 3)], true);
        self.add(B::STONE_SLAB as i32, 6, vec![(I(B::STONE_BRICKS as i32), 3)], true);
        self.add(B::OAK_STAIRS as i32, 4, vec![(Planks, 6)], true);
        self.add(B::COBBLE_STAIRS as i32, 4, vec![(I(B::COBBLE as i32), 6)], true);
        self.add(B::OAK_FENCE as i32, 3, vec![(Planks, 4), (I(IT::STICK), 2)], true);
        self.add(B::FENCE_GATE as i32, 1, vec![(Planks, 2), (I(IT::STICK), 4)], true);
        self.add(B::OAK_DOOR as i32, 3, vec![(I(B::OAK_PLANKS as i32), 6)], true);
        self.add(B::SPRUCE_DOOR as i32, 3, vec![(I(B::SPRUCE_PLANKS as i32), 6)], true);
        self.add(B::BIRCH_DOOR as i32, 3, vec![(I(B::BIRCH_PLANKS as i32), 6)], true);
        self.add(B::IRON_DOOR as i32, 3, vec![(I(IT::IRON_INGOT), 6)], true);
        self.add(B::TRAPDOOR as i32, 2, vec![(Planks, 6)], true);
        self.add(B::LADDER as i32, 3, vec![(I(IT::STICK), 7)], true);
        self.add(B::IRON_BARS as i32, 16, vec![(I(IT::IRON_INGOT), 6)], true);
        self.add(B::CARPET_RED as i32, 3, vec![(I(B::WOOL_RED as i32), 2)], false);
        self.add(B::STONE_BRICKS as i32, 4, vec![(I(B::STONE as i32), 4)], false);
        self.add(B::DEEPSLATE_BRICKS as i32, 4, vec![(I(B::DEEPSLATE as i32), 4)], false);
        self.add(B::BRICKS as i32, 1, vec![(I(IT::BRICK_ITEM), 4)], false);
        self.add(B::SANDSTONE as i32, 1, vec![(I(B::SAND as i32), 4)], false);
        self.add(B::WHITE_CONCRETE as i32, 4, vec![(I(B::SAND as i32), 2), (I(B::GRAVEL as i32), 2)], false);
        self.add(B::GRAY_CONCRETE as i32, 4, vec![(I(B::COBBLE as i32), 2), (I(B::GRAVEL as i32), 2)], false);
        self.add(B::QUARTZ_BLOCK as i32, 4, vec![(I(B::CALCITE as i32), 4)], false);
        self.add(B::GLASS_BLUE as i32, 4, vec![(I(B::GLASS as i32), 4), (I(IT::LAPIS), 1)], true);
        self.add(B::GLASS_RED as i32, 4, vec![(I(B::GLASS as i32), 4), (I(B::POPPY as i32), 1)], true);
        self.add(B::GLASS_GOLD as i32, 4, vec![(I(B::GLASS as i32), 4), (I(B::DANDELION as i32), 1)], true);
        self.add(B::WOOL_GREEN as i32, 1, vec![(I(B::WOOL_WHITE as i32), 1), (I(B::CACTUS as i32), 1)], false);
        self.add(B::WOOL_PURPLE as i32, 1, vec![(I(B::WOOL_WHITE as i32), 1), (I(IT::AMETHYST_SHARD), 1)], false);
        self.add(B::BED as i32, 1, vec![(I(B::WOOL_WHITE as i32), 3), (Planks, 3)], true);
        self.add(B::LANTERN as i32, 1, vec![(I(B::TORCH as i32), 1), (I(IT::IRON_INGOT), 2)], true);
        self.add(B::SEA_LANTERN as i32, 1, vec![(I(B::PRISMARINE as i32), 4), (I(B::GLOWSTONE as i32), 1)], true);
        self.add(B::JACK_O_LANTERN as i32, 1, vec![(I(B::PUMPKIN as i32), 1), (I(B::TORCH as i32), 1)], false);
        self.add(B::GLOWSTONE as i32, 1, vec![(I(B::TORCH as i32), 2), (I(B::GLASS as i32), 2)], true);
        self.add(B::SLIME_BLOCK as i32, 1, vec![(I(IT::SLIMEBALL), 9)], true);
        self.add(IT::SLIMEBALL, 9, vec![(I(B::SLIME_BLOCK as i32), 1)], false);
        self.add(B::LEVER as i32, 1, vec![(I(IT::STICK), 1), (I(B::COBBLE as i32), 1)], false);
        self.add(B::PRESSURE_PLATE as i32, 1, vec![(I(B::STONE as i32), 2)], false);
        self.add(B::STONE_BUTTON as i32, 1, vec![(I(B::STONE as i32), 1)], false);
        self.add(B::REDSTONE_LAMP as i32, 1, vec![(I(B::GLOWSTONE as i32), 1), (I(IT::REDSTONE), 4)], true);
        self.add(B::TNT as i32, 1, vec![(I(IT::GUNPOWDER), 5), (I(B::SAND as i32), 4)], true);
        self.add(B::IRON_BLOCK as i32, 1, vec![(I(IT::IRON_INGOT), 9)], true);
        self.add(B::COPPER_BLOCK as i32, 1, vec![(I(IT::COPPER_INGOT), 9)], true);
        self.add(B::GOLD_BLOCK as i32, 1, vec![(I(IT::GOLD_INGOT), 9)], true);
        self.add(B::DIAMOND_BLOCK as i32, 1, vec![(I(IT::DIAMOND), 9)], true);
        self.add(B::EMERALD_BLOCK as i32, 1, vec![(I(IT::EMERALD), 9)], true);
        self.add(B::NETHERITE_BLOCK as i32, 1, vec![(I(IT::NETHERITE_INGOT), 9)], true);
        self.add(B::HAY_BALE as i32, 1, vec![(I(IT::WHEAT_ITEM), 9)], true);
        self.add(B::BOOKSHELF as i32, 1, vec![(Planks, 6), (I(IT::BOOK), 3)], true);
        self.add(IT::PAPER, 3, vec![(I(B::REEDS as i32), 3)], true);
        self.add(IT::BOOK, 1, vec![(I(IT::PAPER), 3), (I(IT::LEATHER), 1)], false);
        self.add(IT::BONE_MEAL, 3, vec![(I(IT::BONE), 1)], false);

        // Food & Gear
        self.add(IT::BREAD, 1, vec![(I(IT::WHEAT_ITEM), 3)], true);
        self.add(IT::COOKIE, 8, vec![(I(IT::WHEAT_ITEM), 2), (I(IT::SWEET_BERRIES), 1)], true);
        self.add(IT::PUMPKIN_PIE, 1, vec![(I(B::PUMPKIN as i32), 1), (I(B::REEDS as i32), 1)], false);
        self.add(IT::MUSHROOM_STEW, 1, vec![(I(B::MUSHROOM_RED as i32), 1), (I(B::MUSHROOM_BROWN as i32), 1), (Planks, 2)], false);
        self.add(IT::GOLDEN_APPLE, 1, vec![(I(IT::APPLE), 1), (I(IT::GOLD_INGOT), 8)], true);
        self.add(IT::BUCKET, 1, vec![(I(IT::IRON_INGOT), 3)], true);
        self.add(IT::BOW, 1, vec![(I(IT::STICK), 3), (I(IT::STRING), 3)], true);
        self.add(IT::ARROW, 4, vec![(I(IT::FLINT), 1), (I(IT::STICK), 1), (I(IT::FEATHER), 1)], true);
        self.add(IT::SHIELD, 1, vec![(Planks, 6), (I(IT::IRON_INGOT), 1)], true);
        self.add(IT::NETHERITE_INGOT, 1, vec![(I(IT::ANCIENT_SCRAP), 4), (I(IT::GOLD_INGOT), 4)], true);

        let tool_mats: &[(IngredientReq, i32)] = &[
            (Planks, 170),
            (I(B::COBBLE as i32), 175),
            (I(IT::COPPER_INGOT), IT::COPPER_PICK),
            (I(IT::IRON_INGOT), 180),
            (I(IT::GOLD_INGOT), 185),
            (I(IT::DIAMOND), 190),
            (I(IT::EMERALD), IT::EMERALD_PICK),
            (I(IT::NETHERITE_INGOT), 195),
        ];
        for &(mat, base) in tool_mats {
            self.add(base, 1, vec![(mat, 3), (I(IT::STICK), 2)], true);
            self.add(base + 1, 1, vec![(mat, 3), (I(IT::STICK), 2)], true);
            self.add(base + 2, 1, vec![(mat, 1), (I(IT::STICK), 2)], true);
            self.add(base + 3, 1, vec![(mat, 2), (I(IT::STICK), 2)], true);
            self.add(base + 4, 1, vec![(mat, 2), (I(IT::STICK), 1)], true);
        }

        let armor_mats: &[(IngredientReq, i32)] = &[
            (I(IT::LEATHER), 200),
            (I(IT::COPPER_INGOT), IT::COPPER_HELM),
            (I(IT::IRON_INGOT), 204),
            (I(IT::GOLD_INGOT), 208),
            (I(IT::DIAMOND), 212),
            (I(IT::NETHERITE_INGOT), 216),
        ];
        for &(mat, base) in armor_mats {
            self.add(base, 1, vec![(mat, 5)], true);
            self.add(base + 1, 1, vec![(mat, 8)], true);
            self.add(base + 2, 1, vec![(mat, 7)], true);
            self.add(base + 3, 1, vec![(mat, 4)], true);
        }

        // Overhaul Elixirs, Traversal Gear & Boss Summon Recipes (appended after legacy indices)
        self.add(IT::POTION_HEAL, 2, vec![(I(IT::APPLE), 1), (I(IT::SWEET_BERRIES), 2), (I(B::MUSHROOM_RED as i32), 1)], false);
        self.add(IT::POTION_SPEED, 2, vec![(I(B::REEDS as i32), 2), (I(IT::FEATHER), 1), (I(IT::AMETHYST_SHARD), 1)], true);
        self.add(IT::POTION_STRENGTH, 2, vec![(I(IT::MAGMA_CREAM), 1), (I(IT::BONE), 2), (I(IT::AMETHYST_SHARD), 1)], true);
        self.add(IT::POTION_FIRE, 2, vec![(I(IT::MAGMA_CREAM), 1), (I(IT::SLIMEBALL), 1), (I(IT::COAL), 1)], true);
        self.add(IT::POTION_NIGHT, 2, vec![(I(B::TORCH as i32), 2), (I(IT::CARROT), 1), (I(IT::LAPIS), 1)], true);
        self.add(IT::GRAPPLE_HOOK, 1, vec![(I(IT::IRON_INGOT), 2), (I(IT::STRING), 3), (I(IT::STICK), 1)], true);
        self.add(IT::SPYGLASS, 1, vec![(I(IT::COPPER_INGOT), 2), (I(IT::AMETHYST_SHARD), 1)], false);
        self.add(IT::RECALL_CRYSTAL, 1, vec![(I(IT::AMETHYST_SHARD), 2), (I(IT::GOLD_INGOT), 2), (I(IT::LAPIS), 2)], true);
        self.add(IT::GLIDER, 1, vec![(I(IT::LEATHER), 4), (I(IT::FEATHER), 6), (I(IT::DIAMOND), 2)], true);
        self.add(IT::GLIDER, 1, vec![(I(IT::COLOSSUS_HEART), 1), (I(IT::LEATHER), 2), (I(IT::FEATHER), 4)], true);
        self.add(IT::BOSS_SIGIL, 1, vec![(I(B::OBSIDIAN as i32), 4), (I(IT::AMETHYST_SHARD), 2), (I(IT::GOLD_INGOT), 2), (I(IT::DIAMOND), 1)], true);
        self.add(IT::HEART_CRYSTAL, 1, vec![(I(IT::COLOSSUS_HEART), 1), (I(IT::GOLDEN_APPLE), 1), (I(IT::DIAMOND), 2)], true);
        self.add(IT::NETHERITE_INGOT, 1, vec![(I(IT::COLOSSUS_HEART), 1), (I(IT::GOLD_INGOT), 2), (I(IT::DIAMOND), 1)], true);
    }

    fn init_smelting(&mut self) {
        let pairs: &[(i32, i32)] = &[
            (IT::RAW_IRON, IT::IRON_INGOT),
            (IT::RAW_COPPER, IT::COPPER_INGOT),
            (IT::RAW_GOLD, IT::GOLD_INGOT),
            (B::IRON_ORE as i32, IT::IRON_INGOT),
            (B::COPPER_ORE as i32, IT::COPPER_INGOT),
            (B::GOLD_ORE as i32, IT::GOLD_INGOT),
            (B::SAND as i32, B::GLASS as i32),
            (B::RED_SAND as i32, B::GLASS as i32),
            (B::COBBLE as i32, B::STONE as i32),
            (B::STONE_BRICKS as i32, B::CRACKED_BRICKS as i32),
            (IT::CLAY_BALL, IT::BRICK_ITEM),
            (B::OAK_LOG as i32, IT::CHARCOAL),
            (B::BIRCH_LOG as i32, IT::CHARCOAL),
            (B::SPRUCE_LOG as i32, IT::CHARCOAL),
            (B::JUNGLE_LOG as i32, IT::CHARCOAL),
            (B::ACACIA_LOG as i32, IT::CHARCOAL),
            (B::DARK_OAK_LOG as i32, IT::CHARCOAL),
            (B::CHERRY_LOG as i32, IT::CHARCOAL),
            (B::MANGROVE_LOG as i32, IT::CHARCOAL),
            (IT::RAW_BEEF, IT::STEAK),
            (IT::RAW_PORK, IT::COOKED_PORK),
            (IT::RAW_MUTTON, IT::COOKED_MUTTON),
            (IT::RAW_CHICKEN, IT::COOKED_CHICKEN),
            (IT::RAW_FISH, IT::COOKED_FISH),
            (IT::POTATO, IT::BAKED_POTATO),
            (B::ANCIENT_DEBRIS as i32, IT::ANCIENT_SCRAP),
        ];
        for &(k, v) in pairs {
            self.smelt_map.insert(k, v);
        }
    }

    pub fn matches_ingredient(&self, item_id: i32, req: IngredientReq) -> bool {
        match req {
            IngredientReq::Planks => {
                (B::OAK_PLANKS as i32..=B::JUNGLE_PLANKS as i32).contains(&item_id)
                    || item_id == B::CHERRY_PLANKS as i32
                    || item_id == B::MANGROVE_PLANKS as i32
                    || item_id == B::ACACIA_PLANKS as i32
                    || item_id == B::DARK_OAK_PLANKS as i32
            }
            IngredientReq::Log => {
                (B::OAK_LOG as i32..=B::DARK_OAK_LOG as i32).contains(&item_id)
                    || item_id == B::CHERRY_LOG as i32
                    || item_id == B::MANGROVE_LOG as i32
            }
            IngredientReq::Item(id) => item_id == id,
        }
    }
}

pub static CRAFTING: LazyLock<CraftingSystem> = LazyLock::new(CraftingSystem::new);
