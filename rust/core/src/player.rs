use crate::inventory::Inventory;

#[derive(Clone, Debug)]
pub struct CorePlayer {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub vx: f64,
    pub vy: f64,
    pub vz: f64,
    pub yaw: f64,
    pub pitch: f64,
    pub on_ground: bool,
    pub crouching: bool,
    pub sprinting: bool,
    pub blocking: bool,
    pub mode: String,
    pub health: f64,
    pub max_health: f64,
    pub hunger: f64,
    pub max_hunger: f64,
    pub saturation: f64,
    pub xp: i32,
    pub level: i32,
    pub dead: bool,
    pub death_reason: String,
    pub hurt_cooldown: f64,
    pub regen_accum: f64,
    pub starve_accum: f64,
    pub stats_dirty: bool,
    pub ench_seed: u32,
    pub inventory: Inventory,
}

impl CorePlayer {
    pub fn new(mode: &str, ench_seed: u32) -> Self {
        Self {
            x: 8.5,
            y: 70.0,
            z: 8.5,
            vx: 0.0,
            vy: 0.0,
            vz: 0.0,
            yaw: 0.0,
            pitch: -0.15,
            on_ground: false,
            crouching: false,
            sprinting: false,
            blocking: false,
            mode: mode.to_string(),
            health: 20.0,
            max_health: 20.0,
            hunger: 20.0,
            max_hunger: 20.0,
            saturation: 5.0,
            xp: 0,
            level: 0,
            dead: false,
            death_reason: String::new(),
            hurt_cooldown: 0.0,
            regen_accum: 0.0,
            starve_accum: 0.0,
            stats_dirty: true,
            ench_seed,
            inventory: Inventory::new(),
        }
    }

    pub fn add_xp(&mut self, amt: i32) -> bool {
        self.xp += amt;
        let mut leveled = false;
        while self.xp >= 15 + self.level * 10 {
            self.xp -= 15 + self.level * 10;
            self.level += 1;
            leveled = true;
        }
        self.stats_dirty = true;
        leveled
    }

    /// Applies damage according to `Player.hurt` rules. Returns `true` if damage landed.
    pub fn hurt(&mut self, dmg: f64, reason: &str, kx: f64, ky: f64, kz: f64) -> bool {
        if self.mode == "creative" || self.dead || self.hurt_cooldown > 0.0 {
            return false;
        }
        let def = self.inventory.get_defense();
        let shield_mul = if self.blocking && self.inventory.armor.get(4).is_some_and(|s| s.is_some()) {
            0.35
        } else {
            1.0
        };
        let rs_low = reason.to_ascii_lowercase();
        let dmg_kind = if reason.contains("Fell") {
            "fall"
        } else if rs_low.contains("explosion") {
            "blast"
        } else if reason.contains("Shot") {
            "projectile"
        } else if rs_low.contains("lava") || rs_low.contains("magma") || rs_low.contains("burn") || rs_low.contains("fire") {
            "fire"
        } else {
            "generic"
        };
        let epf = self.inventory.get_enchant_protection(dmg_kind);
        let reduced = (dmg * (1.0 - (def * 0.032).min(0.80)) * shield_mul * (1.0 - (epf as f64) * 0.04)).max(0.5);
        self.health = (self.health - reduced).max(0.0);
        self.hurt_cooldown = 0.52;
        if kx != 0.0 || ky != 0.0 || kz != 0.0 {
            let km = if shield_mul < 1.0 { 0.45 } else { 1.0 };
            self.vx += kx * km;
            self.vy = self.vy.max(ky * km);
            self.vz += kz * km;
        }
        if def > 0.0 {
            self.inventory.damage_armor(1);
        }
        if self.health <= 0.0 {
            self.dead = true;
            self.death_reason = if reason.is_empty() {
                "Slain in the wilderness.".to_string()
            } else {
                reason.to_string()
            };
        }
        self.stats_dirty = true;
        true
    }
}
