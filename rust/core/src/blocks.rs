use std::sync::LazyLock;

use crate::items::IT;

#[allow(non_snake_case)]
pub mod B {
    pub const AIR: u8 = 0;
    pub const GRASS: u8 = 1;
    pub const DIRT: u8 = 2;
    pub const STONE: u8 = 3;
    pub const COBBLE: u8 = 4;
    pub const DEEPSLATE: u8 = 5;
    pub const GRAVEL: u8 = 6;
    pub const SAND: u8 = 7;
    pub const RED_SAND: u8 = 8;
    pub const SANDSTONE: u8 = 9;
    pub const CLAY: u8 = 10;
    pub const SNOW: u8 = 11;
    pub const ICE: u8 = 12;
    pub const PACKED_ICE: u8 = 13;
    pub const BEDROCK: u8 = 14;
    pub const MUD: u8 = 15;
    pub const MOSS: u8 = 16;
    pub const MYCELIUM: u8 = 17;
    pub const TERRACOTTA: u8 = 18;
    pub const RED_TERRACOTTA: u8 = 19;
    pub const YEL_TERRACOTTA: u8 = 20;
    pub const DRIPSTONE: u8 = 21;
    pub const CALCITE: u8 = 22;
    pub const AMETHYST: u8 = 23;
    pub const GLOW_SHROOM_BLK: u8 = 24;
    pub const COAL_ORE: u8 = 25;
    pub const IRON_ORE: u8 = 26;
    pub const COPPER_ORE: u8 = 27;
    pub const GOLD_ORE: u8 = 28;
    pub const REDSTONE_ORE: u8 = 29;
    pub const LAPIS_ORE: u8 = 30;
    pub const DIAMOND_ORE: u8 = 31;
    pub const EMERALD_ORE: u8 = 32;
    pub const ANCIENT_DEBRIS: u8 = 33;
    pub const OAK_LOG: u8 = 34;
    pub const BIRCH_LOG: u8 = 35;
    pub const SPRUCE_LOG: u8 = 36;
    pub const JUNGLE_LOG: u8 = 37;
    pub const ACACIA_LOG: u8 = 38;
    pub const DARK_OAK_LOG: u8 = 39;
    pub const OAK_LEAVES: u8 = 40;
    pub const BIRCH_LEAVES: u8 = 41;
    pub const SPRUCE_LEAVES: u8 = 42;
    pub const JUNGLE_LEAVES: u8 = 43;
    pub const ACACIA_LEAVES: u8 = 44;
    pub const OAK_PLANKS: u8 = 45;
    pub const BIRCH_PLANKS: u8 = 46;
    pub const SPRUCE_PLANKS: u8 = 47;
    pub const JUNGLE_PLANKS: u8 = 48;
    pub const OAK_SLAB: u8 = 49;
    pub const STONE_SLAB: u8 = 50;
    pub const OAK_STAIRS: u8 = 51;
    pub const COBBLE_STAIRS: u8 = 52;
    pub const OAK_FENCE: u8 = 53;
    pub const OAK_DOOR: u8 = 54;
    pub const TRAPDOOR: u8 = 55;
    pub const LADDER: u8 = 56;
    pub const BRICKS: u8 = 57;
    pub const GLASS: u8 = 58;
    pub const STONE_BRICKS: u8 = 59;
    pub const MOSSY_BRICKS: u8 = 60;
    pub const WHITE_CONCRETE: u8 = 61;
    pub const GRAY_CONCRETE: u8 = 62;
    pub const IRON_BLOCK: u8 = 63;
    pub const GOLD_BLOCK: u8 = 64;
    pub const DIAMOND_BLOCK: u8 = 65;
    pub const WOOL_WHITE: u8 = 66;
    pub const WOOL_RED: u8 = 67;
    pub const WOOL_BLUE: u8 = 68;
    pub const WOOL_YELLOW: u8 = 69;
    pub const BOOKSHELF: u8 = 70;
    pub const GLOWSTONE: u8 = 71;
    pub const OBSIDIAN: u8 = 72;
    pub const HAY_BALE: u8 = 73;
    pub const CORAL_PINK: u8 = 74;
    pub const CORAL_BLUE: u8 = 75;
    pub const CORAL_YELLOW: u8 = 76;
    pub const CRAFTING_TABLE: u8 = 77;
    pub const FURNACE: u8 = 78;
    pub const FURNACE_LIT: u8 = 79;
    pub const CHEST: u8 = 80;
    pub const TORCH: u8 = 81;
    pub const LANTERN: u8 = 82;
    pub const BED: u8 = 83;
    pub const WORKSTATION: u8 = 84;
    pub const REDSTONE_WIRE: u8 = 85;
    pub const LEVER: u8 = 86;
    pub const REDSTONE_LAMP: u8 = 87;
    pub const REDSTONE_LAMP_ON: u8 = 88;
    pub const TNT: u8 = 89;
    pub const WATER: u8 = 90;
    pub const LAVA: u8 = 91;
    pub const TALL_GRASS: u8 = 92;
    pub const POPPY: u8 = 93;
    pub const DANDELION: u8 = 94;
    pub const ORCHID: u8 = 95;
    pub const MUSHROOM_RED: u8 = 96;
    pub const MUSHROOM_BROWN: u8 = 97;
    pub const CACTUS: u8 = 98;
    pub const REEDS: u8 = 99;
    pub const SEAGRASS: u8 = 100;
    pub const DEAD_BUSH: u8 = 101;
    pub const VINES: u8 = 102;
    pub const WHEAT: u8 = 103;
    pub const CHERRY_LOG: u8 = 104;
    pub const CHERRY_LEAVES: u8 = 105;
    pub const CHERRY_PLANKS: u8 = 106;
    pub const MANGROVE_LOG: u8 = 107;
    pub const MANGROVE_LEAVES: u8 = 108;
    pub const MANGROVE_PLANKS: u8 = 109;
    pub const BAMBOO: u8 = 110;
    pub const ACACIA_PLANKS: u8 = 111;
    pub const DARK_OAK_PLANKS: u8 = 112;
    pub const BASALT: u8 = 113;
    pub const MAGMA_BLOCK: u8 = 114;
    pub const ASH_BLOCK: u8 = 115;
    pub const PODZOL: u8 = 116;
    pub const COARSE_DIRT: u8 = 117;
    pub const PRISMARINE: u8 = 118;
    pub const SEA_LANTERN: u8 = 119;
    pub const FARMLAND: u8 = 226;
    pub const PUMPKIN: u8 = 227;
    pub const JACK_O_LANTERN: u8 = 228;
    pub const MELON: u8 = 229;
    pub const OBSIDIAN_CRYING: u8 = 230;
    pub const COPPER_BLOCK: u8 = 231;
    pub const EMERALD_BLOCK: u8 = 232;
    pub const NETHERITE_BLOCK: u8 = 233;
    pub const QUARTZ_BLOCK: u8 = 234;
    pub const DEEPSLATE_BRICKS: u8 = 235;
    pub const CRACKED_BRICKS: u8 = 236;
    pub const GLASS_BLUE: u8 = 237;
    pub const GLASS_RED: u8 = 238;
    pub const GLASS_GOLD: u8 = 239;
    pub const WOOL_GREEN: u8 = 240;
    pub const WOOL_PURPLE: u8 = 241;
    pub const COBWEB: u8 = 242;
    pub const SPRUCE_DOOR: u8 = 243;
    pub const BIRCH_DOOR: u8 = 244;
    pub const IRON_DOOR: u8 = 245;
    pub const PRESSURE_PLATE: u8 = 246;
    pub const STONE_BUTTON: u8 = 247;
    pub const CAMPFIRE: u8 = 248;
    pub const SLIME_BLOCK: u8 = 249;
    pub const BARREL: u8 = 250;
    pub const ENCHANT_TABLE: u8 = 251;
    pub const FENCE_GATE: u8 = 252;
    pub const CARPET_RED: u8 = 253;
    pub const JUKEBOX: u8 = 254;
    pub const IRON_BARS: u8 = 255;
}

#[derive(Clone, Debug)]
pub struct BlockDef {
    pub id: u8,
    pub name: &'static str,
    pub hardness: f64,
    pub tool: &'static str,
    pub min_tier: i32,
    pub drop: i32,
    pub drop_count: i32,
    pub transparent: bool,
    pub solid: bool,
    pub liquid: bool,
    pub cross: bool,
    pub is_leaf: bool,
    pub shape: &'static str,
    pub light: u8,
    pub climbable: bool,
    pub interactable: bool,
    pub gravity: bool,
    pub bouncy: bool,
    pub slows: bool,
    pub hot: bool,
    pub prickly: bool,
}

impl BlockDef {
    fn base(id: u8, name: &'static str) -> Self {
        Self {
            id,
            name,
            hardness: 1.5,
            tool: "none",
            min_tier: 0,
            drop: id as i32,
            drop_count: 1,
            transparent: false,
            solid: true,
            liquid: false,
            cross: false,
            is_leaf: false,
            shape: "cube",
            light: 0,
            climbable: false,
            interactable: false,
            gravity: false,
            bouncy: false,
            slows: false,
            hot: false,
            prickly: false,
        }
    }
}

pub struct BlockRegistry {
    pub blocks: [Option<BlockDef>; 256],
}

impl BlockRegistry {
    fn new() -> Self {
        let mut blocks: [Option<BlockDef>; 256] = std::array::from_fn(|_| None);
        let mut reg = |id: u8, name: &'static str, f: fn(&mut BlockDef)| {
            let mut b = BlockDef::base(id, name);
            f(&mut b);
            blocks[id as usize] = Some(b);
        };

        reg(B::AIR, "Air", |b| { b.hardness = 0.0; b.solid = false; b.transparent = true; b.drop = 0; });
        reg(B::GRASS, "Grass Block", |b| { b.hardness = 0.7; b.tool = "shovel"; b.drop = B::DIRT as i32; });
        reg(B::DIRT, "Dirt", |b| { b.hardness = 0.6; b.tool = "shovel"; });
        reg(B::STONE, "Stone", |b| { b.hardness = 2.2; b.tool = "pickaxe"; b.min_tier = 1; b.drop = B::COBBLE as i32; });
        reg(B::COBBLE, "Cobblestone", |b| { b.hardness = 2.4; b.tool = "pickaxe"; b.min_tier = 1; });
        reg(B::DEEPSLATE, "Deepslate", |b| { b.hardness = 3.4; b.tool = "pickaxe"; b.min_tier = 1; });
        reg(B::GRAVEL, "Gravel", |b| { b.hardness = 0.75; b.tool = "shovel"; b.gravity = true; });
        reg(B::SAND, "Sand", |b| { b.hardness = 0.65; b.tool = "shovel"; b.gravity = true; });
        reg(B::RED_SAND, "Red Sand", |b| { b.hardness = 0.65; b.tool = "shovel"; b.gravity = true; });
        reg(B::SANDSTONE, "Sandstone", |b| { b.hardness = 1.4; b.tool = "pickaxe"; b.min_tier = 1; });
        reg(B::CLAY, "Clay", |b| { b.hardness = 0.75; b.tool = "shovel"; b.drop = IT::CLAY_BALL; b.drop_count = 4; });
        reg(B::SNOW, "Snow Block", |b| { b.hardness = 0.4; b.tool = "shovel"; });
        reg(B::ICE, "Ice", |b| { b.hardness = 0.7; b.tool = "pickaxe"; b.transparent = true; });
        reg(B::PACKED_ICE, "Packed Ice", |b| { b.hardness = 1.0; b.tool = "pickaxe"; });
        reg(B::BEDROCK, "Bedrock", |b| { b.hardness = 999999.0; b.drop = 0; });
        reg(B::MUD, "Mud", |b| { b.hardness = 0.6; b.tool = "shovel"; });
        reg(B::MOSS, "Moss Block", |b| { b.hardness = 0.35; });
        reg(B::MYCELIUM, "Mycelium", |b| { b.hardness = 0.7; b.tool = "shovel"; b.drop = B::DIRT as i32; });
        reg(B::TERRACOTTA, "Terracotta", |b| { b.hardness = 1.8; b.tool = "pickaxe"; b.min_tier = 1; });
        reg(B::RED_TERRACOTTA, "Red Terracotta", |b| { b.hardness = 1.8; b.tool = "pickaxe"; b.min_tier = 1; });
        reg(B::YEL_TERRACOTTA, "Ochre Terracotta", |b| { b.hardness = 1.8; b.tool = "pickaxe"; b.min_tier = 1; });
        reg(B::DRIPSTONE, "Dripstone", |b| { b.hardness = 1.6; b.tool = "pickaxe"; b.min_tier = 1; });
        reg(B::CALCITE, "Calcite", |b| { b.hardness = 1.2; b.tool = "pickaxe"; b.min_tier = 1; });
        reg(B::AMETHYST, "Amethyst Crystal", |b| { b.hardness = 1.6; b.tool = "pickaxe"; b.min_tier = 1; b.drop = IT::AMETHYST_SHARD; b.drop_count = 3; b.light = 8; });
        reg(B::GLOW_SHROOM_BLK, "Glowing Shroom", |b| { b.hardness = 0.6; b.light = 13; });

        // Ores
        reg(B::COAL_ORE, "Coal Ore", |b| { b.hardness = 3.0; b.tool = "pickaxe"; b.min_tier = 1; b.drop = IT::COAL; b.drop_count = 1; });
        reg(B::IRON_ORE, "Iron Ore", |b| { b.hardness = 3.5; b.tool = "pickaxe"; b.min_tier = 2; b.drop = IT::RAW_IRON; });
        reg(B::COPPER_ORE, "Copper Ore", |b| { b.hardness = 3.2; b.tool = "pickaxe"; b.min_tier = 2; b.drop = IT::RAW_COPPER; b.drop_count = 2; });
        reg(B::GOLD_ORE, "Gold Ore", |b| { b.hardness = 3.8; b.tool = "pickaxe"; b.min_tier = 3; b.drop = IT::RAW_GOLD; });
        reg(B::REDSTONE_ORE, "Redstone Ore", |b| { b.hardness = 3.6; b.tool = "pickaxe"; b.min_tier = 3; b.drop = IT::REDSTONE; b.drop_count = 4; b.light = 5; });
        reg(B::LAPIS_ORE, "Lapis Ore", |b| { b.hardness = 3.5; b.tool = "pickaxe"; b.min_tier = 2; b.drop = IT::LAPIS; b.drop_count = 4; });
        reg(B::DIAMOND_ORE, "Diamond Ore", |b| { b.hardness = 4.5; b.tool = "pickaxe"; b.min_tier = 3; b.drop = IT::DIAMOND; });
        reg(B::EMERALD_ORE, "Emerald Ore", |b| { b.hardness = 4.5; b.tool = "pickaxe"; b.min_tier = 3; b.drop = IT::EMERALD; });
        reg(B::ANCIENT_DEBRIS, "Ancient Debris", |b| { b.hardness = 7.0; b.tool = "pickaxe"; b.min_tier = 4; b.drop = IT::ANCIENT_SCRAP; });

        // Logs, Leaves, Planks
        reg(B::OAK_LOG, "Oak Log", |b| { b.hardness = 2.0; b.tool = "axe"; });
        reg(B::BIRCH_LOG, "Birch Log", |b| { b.hardness = 2.0; b.tool = "axe"; });
        reg(B::SPRUCE_LOG, "Spruce Log", |b| { b.hardness = 2.0; b.tool = "axe"; });
        reg(B::JUNGLE_LOG, "Jungle Log", |b| { b.hardness = 2.0; b.tool = "axe"; });
        reg(B::ACACIA_LOG, "Acacia Log", |b| { b.hardness = 2.0; b.tool = "axe"; });
        reg(B::DARK_OAK_LOG, "Dark Oak Log", |b| { b.hardness = 2.2; b.tool = "axe"; });
        reg(B::CHERRY_LOG, "Cherry Log", |b| { b.hardness = 2.0; b.tool = "axe"; });
        reg(B::MANGROVE_LOG, "Mangrove Log", |b| { b.hardness = 2.0; b.tool = "axe"; });

        reg(B::OAK_LEAVES, "Oak Leaves", |b| { b.hardness = 0.3; b.transparent = true; b.is_leaf = true; b.drop = 0; });
        reg(B::BIRCH_LEAVES, "Birch Leaves", |b| { b.hardness = 0.3; b.transparent = true; b.is_leaf = true; b.drop = 0; });
        reg(B::SPRUCE_LEAVES, "Spruce Leaves", |b| { b.hardness = 0.3; b.transparent = true; b.is_leaf = true; b.drop = 0; });
        reg(B::JUNGLE_LEAVES, "Jungle Leaves", |b| { b.hardness = 0.3; b.transparent = true; b.is_leaf = true; b.drop = 0; });
        reg(B::ACACIA_LEAVES, "Acacia Leaves", |b| { b.hardness = 0.3; b.transparent = true; b.is_leaf = true; b.drop = 0; });
        reg(B::CHERRY_LEAVES, "Cherry Blossom Leaves", |b| { b.hardness = 0.25; b.transparent = true; b.is_leaf = true; b.drop = 0; });
        reg(B::MANGROVE_LEAVES, "Mangrove Leaves", |b| { b.hardness = 0.3; b.transparent = true; b.is_leaf = true; b.drop = 0; });

        reg(B::OAK_PLANKS, "Oak Planks", |b| { b.hardness = 1.8; b.tool = "axe"; });
        reg(B::BIRCH_PLANKS, "Birch Planks", |b| { b.hardness = 1.8; b.tool = "axe"; });
        reg(B::SPRUCE_PLANKS, "Spruce Planks", |b| { b.hardness = 1.8; b.tool = "axe"; });
        reg(B::JUNGLE_PLANKS, "Jungle Planks", |b| { b.hardness = 1.8; b.tool = "axe"; });
        reg(B::CHERRY_PLANKS, "Cherry Planks", |b| { b.hardness = 1.8; b.tool = "axe"; });
        reg(B::MANGROVE_PLANKS, "Mangrove Planks", |b| { b.hardness = 1.8; b.tool = "axe"; });
        reg(B::ACACIA_PLANKS, "Acacia Planks", |b| { b.hardness = 1.8; b.tool = "axe"; });
        reg(B::DARK_OAK_PLANKS, "Dark Oak Planks", |b| { b.hardness = 1.8; b.tool = "axe"; });

        // Shaped building blocks
        reg(B::OAK_SLAB, "Oak Slab", |b| { b.hardness = 1.6; b.tool = "axe"; b.transparent = true; b.shape = "slab"; });
        reg(B::STONE_SLAB, "Stone Slab", |b| { b.hardness = 2.2; b.tool = "pickaxe"; b.min_tier = 1; b.transparent = true; b.shape = "slab"; });
        reg(B::OAK_STAIRS, "Oak Stairs", |b| { b.hardness = 1.8; b.tool = "axe"; b.transparent = true; b.shape = "stairs"; });
        reg(B::COBBLE_STAIRS, "Cobble Stairs", |b| { b.hardness = 2.2; b.tool = "pickaxe"; b.min_tier = 1; b.transparent = true; b.shape = "stairs"; });
        reg(B::OAK_FENCE, "Oak Fence", |b| { b.hardness = 1.8; b.tool = "axe"; b.transparent = true; b.shape = "fence"; });
        reg(B::FENCE_GATE, "Oak Fence Gate", |b| { b.hardness = 1.8; b.tool = "axe"; b.transparent = true; b.shape = "fence_gate"; b.interactable = true; });
        reg(B::OAK_DOOR, "Oak Door", |b| { b.hardness = 2.0; b.tool = "axe"; b.transparent = true; b.shape = "door"; b.interactable = true; });
        reg(B::SPRUCE_DOOR, "Spruce Door", |b| { b.hardness = 2.0; b.tool = "axe"; b.transparent = true; b.shape = "door"; b.interactable = true; });
        reg(B::BIRCH_DOOR, "Birch Door", |b| { b.hardness = 2.0; b.tool = "axe"; b.transparent = true; b.shape = "door"; b.interactable = true; });
        reg(B::IRON_DOOR, "Iron Door", |b| { b.hardness = 4.0; b.tool = "pickaxe"; b.min_tier = 1; b.transparent = true; b.shape = "door"; b.interactable = true; });
        reg(B::TRAPDOOR, "Oak Trapdoor", |b| { b.hardness = 1.8; b.tool = "axe"; b.transparent = true; b.shape = "trapdoor"; b.interactable = true; });
        reg(B::LADDER, "Ladder", |b| { b.hardness = 0.6; b.tool = "axe"; b.solid = false; b.transparent = true; b.shape = "ladder"; b.climbable = true; });
        reg(B::IRON_BARS, "Iron Bars", |b| { b.hardness = 3.5; b.tool = "pickaxe"; b.min_tier = 1; b.transparent = true; b.shape = "bars"; });
        reg(B::CARPET_RED, "Crimson Carpet", |b| { b.hardness = 0.2; b.solid = false; b.transparent = true; b.shape = "carpet"; });

        // Building & Decorative
        reg(B::BRICKS, "Bricks", |b| { b.hardness = 2.6; b.tool = "pickaxe"; b.min_tier = 1; });
        reg(B::GLASS, "Glass", |b| { b.hardness = 0.45; b.transparent = true; b.drop = 0; });
        reg(B::GLASS_BLUE, "Sapphire Glass", |b| { b.hardness = 0.45; b.transparent = true; b.drop = 0; });
        reg(B::GLASS_RED, "Ruby Glass", |b| { b.hardness = 0.45; b.transparent = true; b.drop = 0; });
        reg(B::GLASS_GOLD, "Amber Glass", |b| { b.hardness = 0.45; b.transparent = true; b.drop = 0; });
        reg(B::STONE_BRICKS, "Stone Bricks", |b| { b.hardness = 2.5; b.tool = "pickaxe"; b.min_tier = 1; });
        reg(B::MOSSY_BRICKS, "Mossy Stone Bricks", |b| { b.hardness = 2.5; b.tool = "pickaxe"; b.min_tier = 1; });
        reg(B::CRACKED_BRICKS, "Cracked Stone Bricks", |b| { b.hardness = 2.4; b.tool = "pickaxe"; b.min_tier = 1; });
        reg(B::DEEPSLATE_BRICKS, "Deepslate Bricks", |b| { b.hardness = 3.6; b.tool = "pickaxe"; b.min_tier = 1; });
        reg(B::WHITE_CONCRETE, "White Concrete", |b| { b.hardness = 2.4; b.tool = "pickaxe"; b.min_tier = 1; });
        reg(B::GRAY_CONCRETE, "Slate Concrete", |b| { b.hardness = 2.4; b.tool = "pickaxe"; b.min_tier = 1; });
        reg(B::QUARTZ_BLOCK, "Quartz Block", |b| { b.hardness = 2.2; b.tool = "pickaxe"; b.min_tier = 1; });
        reg(B::IRON_BLOCK, "Iron Block", |b| { b.hardness = 4.5; b.tool = "pickaxe"; b.min_tier = 2; });
        reg(B::COPPER_BLOCK, "Copper Block", |b| { b.hardness = 4.0; b.tool = "pickaxe"; b.min_tier = 2; });
        reg(B::GOLD_BLOCK, "Gold Block", |b| { b.hardness = 3.5; b.tool = "pickaxe"; b.min_tier = 3; });
        reg(B::DIAMOND_BLOCK, "Diamond Block", |b| { b.hardness = 5.5; b.tool = "pickaxe"; b.min_tier = 3; });
        reg(B::EMERALD_BLOCK, "Emerald Block", |b| { b.hardness = 5.0; b.tool = "pickaxe"; b.min_tier = 3; });
        reg(B::NETHERITE_BLOCK, "Netherite Block", |b| { b.hardness = 10.0; b.tool = "pickaxe"; b.min_tier = 4; });
        reg(B::WOOL_WHITE, "White Wool", |b| { b.hardness = 0.7; });
        reg(B::WOOL_RED, "Crimson Wool", |b| { b.hardness = 0.7; });
        reg(B::WOOL_BLUE, "Cobalt Wool", |b| { b.hardness = 0.7; });
        reg(B::WOOL_YELLOW, "Gold Wool", |b| { b.hardness = 0.7; });
        reg(B::WOOL_GREEN, "Emerald Wool", |b| { b.hardness = 0.7; });
        reg(B::WOOL_PURPLE, "Amethyst Wool", |b| { b.hardness = 0.7; });
        reg(B::BOOKSHELF, "Bookshelf", |b| { b.hardness = 1.5; b.tool = "axe"; });
        reg(B::GLOWSTONE, "Glowstone", |b| { b.hardness = 0.6; b.light = 15; });
        reg(B::OBSIDIAN, "Obsidian", |b| { b.hardness = 12.0; b.tool = "pickaxe"; b.min_tier = 4; });
        reg(B::OBSIDIAN_CRYING, "Crying Obsidian", |b| { b.hardness = 12.0; b.tool = "pickaxe"; b.min_tier = 4; b.light = 10; });
        reg(B::HAY_BALE, "Hay Bale", |b| { b.hardness = 0.6; });
        reg(B::CORAL_PINK, "Pink Coral", |b| { b.hardness = 0.8; });
        reg(B::CORAL_BLUE, "Blue Coral", |b| { b.hardness = 0.8; });
        reg(B::CORAL_YELLOW, "Yellow Coral", |b| { b.hardness = 0.8; });
        reg(B::BASALT, "Basalt", |b| { b.hardness = 2.6; b.tool = "pickaxe"; b.min_tier = 1; });
        reg(B::MAGMA_BLOCK, "Magma Block", |b| { b.hardness = 2.0; b.tool = "pickaxe"; b.min_tier = 1; b.light = 9; b.hot = true; });
        reg(B::ASH_BLOCK, "Volcanic Ash", |b| { b.hardness = 0.55; b.tool = "shovel"; });
        reg(B::PODZOL, "Podzol", |b| { b.hardness = 0.7; b.tool = "shovel"; b.drop = B::DIRT as i32; });
        reg(B::COARSE_DIRT, "Coarse Dirt", |b| { b.hardness = 0.65; b.tool = "shovel"; });
        reg(B::PRISMARINE, "Prismarine", |b| { b.hardness = 2.6; b.tool = "pickaxe"; b.min_tier = 1; });
        reg(B::SEA_LANTERN, "Sea Lantern", |b| { b.hardness = 0.8; b.light = 15; });
        reg(B::FARMLAND, "Farmland", |b| { b.hardness = 0.6; b.tool = "shovel"; b.drop = B::DIRT as i32; });
        reg(B::PUMPKIN, "Pumpkin", |b| { b.hardness = 1.0; b.tool = "axe"; });
        reg(B::JACK_O_LANTERN, "Jack o'Lantern", |b| { b.hardness = 1.0; b.tool = "axe"; b.light = 15; });
        reg(B::MELON, "Melon", |b| { b.hardness = 1.0; b.tool = "axe"; b.drop = IT::MELON_SLICE; b.drop_count = 4; });
        reg(B::SLIME_BLOCK, "Slime Block", |b| { b.hardness = 0.15; b.transparent = true; b.bouncy = true; });
        reg(B::COBWEB, "Cobweb", |b| { b.hardness = 1.6; b.tool = "sword"; b.solid = false; b.transparent = true; b.cross = true; b.shape = "cross"; b.drop = IT::STRING; b.slows = true; });

        // Functional Blocks
        reg(B::CRAFTING_TABLE, "Crafting Table", |b| { b.hardness = 2.0; b.tool = "axe"; b.interactable = true; });
        reg(B::FURNACE, "Furnace", |b| { b.hardness = 3.0; b.tool = "pickaxe"; b.min_tier = 1; b.interactable = true; });
        reg(B::FURNACE_LIT, "Lit Furnace", |b| { b.hardness = 3.0; b.tool = "pickaxe"; b.min_tier = 1; b.drop = B::FURNACE as i32; b.light = 13; b.interactable = true; });
        reg(B::CHEST, "Chest", |b| { b.hardness = 2.2; b.tool = "axe"; b.interactable = true; });
        reg(B::BARREL, "Barrel", |b| { b.hardness = 2.2; b.tool = "axe"; b.interactable = true; });
        reg(B::ENCHANT_TABLE, "Enchanting Table", |b| { b.hardness = 5.0; b.tool = "pickaxe"; b.min_tier = 2; b.transparent = true; b.shape = "enchant_table"; b.light = 7; b.interactable = true; });
        reg(B::JUKEBOX, "Jukebox", |b| { b.hardness = 2.0; b.tool = "axe"; b.interactable = true; });
        reg(B::CAMPFIRE, "Campfire", |b| { b.hardness = 1.4; b.tool = "axe"; b.transparent = true; b.shape = "campfire"; b.light = 15; b.interactable = true; });
        reg(B::TORCH, "Torch", |b| { b.hardness = 0.05; b.solid = false; b.transparent = true; b.shape = "torch"; b.light = 14; });
        reg(B::LANTERN, "Lantern", |b| { b.hardness = 1.5; b.tool = "pickaxe"; b.transparent = true; b.shape = "torch"; b.light = 15; });
        reg(B::BED, "Bed", |b| { b.hardness = 0.8; b.transparent = true; b.shape = "slab"; b.interactable = true; });
        reg(B::WORKSTATION, "Smithing Bench", |b| { b.hardness = 3.0; b.tool = "pickaxe"; b.interactable = true; });
        reg(B::REDSTONE_WIRE, "Redstone Dust", |b| { b.hardness = 0.05; b.solid = false; b.transparent = true; b.shape = "redstone"; b.drop = IT::REDSTONE; });
        reg(B::LEVER, "Lever", |b| { b.hardness = 0.4; b.solid = false; b.transparent = true; b.shape = "lever"; b.interactable = true; });
        reg(B::PRESSURE_PLATE, "Pressure Plate", |b| { b.hardness = 0.5; b.solid = false; b.transparent = true; b.shape = "pressure_plate"; });
        reg(B::STONE_BUTTON, "Stone Button", |b| { b.hardness = 0.4; b.solid = false; b.transparent = true; b.shape = "button"; b.interactable = true; });
        reg(B::REDSTONE_LAMP, "Redstone Lamp", |b| { b.hardness = 0.8; });
        reg(B::REDSTONE_LAMP_ON, "Lit Redstone Lamp", |b| { b.hardness = 0.8; b.drop = B::REDSTONE_LAMP as i32; b.light = 15; });
        reg(B::TNT, "TNT", |b| { b.hardness = 0.2; b.interactable = true; });

        // Fluids
        reg(B::WATER, "Water", |b| { b.hardness = 999.0; b.solid = false; b.transparent = true; b.liquid = true; b.drop = 0; });
        reg(B::LAVA, "Lava", |b| { b.hardness = 999.0; b.solid = false; b.transparent = true; b.liquid = true; b.light = 15; b.drop = 0; });

        // Vegetation
        reg(B::TALL_GRASS, "Tall Grass", |b| { b.hardness = 0.05; b.solid = false; b.transparent = true; b.cross = true; b.shape = "cross"; b.drop = 0; });
        reg(B::POPPY, "Poppy", |b| { b.hardness = 0.05; b.solid = false; b.transparent = true; b.cross = true; b.shape = "cross"; });
        reg(B::DANDELION, "Dandelion", |b| { b.hardness = 0.05; b.solid = false; b.transparent = true; b.cross = true; b.shape = "cross"; });
        reg(B::ORCHID, "Blue Orchid", |b| { b.hardness = 0.05; b.solid = false; b.transparent = true; b.cross = true; b.shape = "cross"; });
        reg(B::MUSHROOM_RED, "Red Mushroom", |b| { b.hardness = 0.05; b.solid = false; b.transparent = true; b.cross = true; b.shape = "cross"; });
        reg(B::MUSHROOM_BROWN, "Brown Mushroom", |b| { b.hardness = 0.05; b.solid = false; b.transparent = true; b.cross = true; b.shape = "cross"; });
        reg(B::CACTUS, "Cactus", |b| { b.hardness = 0.5; b.prickly = true; });
        reg(B::REEDS, "Sugar Cane", |b| { b.hardness = 0.05; b.solid = false; b.transparent = true; b.cross = true; b.shape = "cross"; });
        reg(B::BAMBOO, "Bamboo", |b| { b.hardness = 0.4; b.tool = "axe"; b.transparent = true; b.shape = "bamboo"; });
        reg(B::SEAGRASS, "Seagrass", |b| { b.hardness = 0.05; b.solid = false; b.transparent = true; b.cross = true; b.shape = "cross"; b.drop = 0; });
        reg(B::DEAD_BUSH, "Dead Bush", |b| { b.hardness = 0.05; b.solid = false; b.transparent = true; b.cross = true; b.shape = "cross"; b.drop = IT::STICK; });
        reg(B::VINES, "Vines", |b| { b.hardness = 0.2; b.solid = false; b.transparent = true; b.cross = true; b.shape = "cross"; b.climbable = true; b.drop = 0; });
        reg(B::WHEAT, "Wheat Crop", |b| { b.hardness = 0.1; b.solid = false; b.transparent = true; b.cross = true; b.shape = "cross"; b.drop = IT::WHEAT_ITEM; });

        Self { blocks }
    }

    pub fn get(&self, id: u8) -> &BlockDef {
        self.blocks[id as usize]
            .as_ref()
            .unwrap_or_else(|| self.blocks[0].as_ref().unwrap())
    }

    pub fn get_opt(&self, id: i32) -> Option<&BlockDef> {
        if (0..256).contains(&id) {
            self.blocks[id as usize].as_ref()
        } else {
            None
        }
    }
}

pub static BLOCKS: LazyLock<BlockRegistry> = LazyLock::new(BlockRegistry::new);
