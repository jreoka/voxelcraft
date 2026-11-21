use std::f64::consts::PI;

use rand::Rng;

use crate::blocks::{B, BLOCKS};
use crate::day_night::DayNightSystem;
use crate::enchants::{ItemStack, ench_lvl};
use crate::items::{IT, ITEMS, WOOL_PALETTE};
use crate::math::{CHUNK_SIZE, SEA_LEVEL, clamp, floor_div};
use crate::player::CorePlayer;
use crate::world::CoreWorld;

#[derive(Clone, Copy, Debug)]
pub struct MobDef {
    pub name: &'static str,
    pub hp: f64,
    pub spd: f64,
    pub hostile: bool,
    pub dmg: f64,
    pub burns_day: bool,
    pub ranged: bool,
    pub climber: bool,
    pub creeper: bool,
    pub hopper: bool,
    pub aquatic: bool,
    pub trader: bool,
    pub drops: &'static [(i32, i32)],
    pub xp: i32,
    pub h: f64,
}

pub const MOB_DEFS: &[MobDef] = &[
    MobDef { name: "cow", hp: 10.0, spd: 2.1, hostile: false, dmg: 0.0, burns_day: false, ranged: false, climber: false, creeper: false, hopper: false, aquatic: false, trader: false, drops: &[(IT::RAW_BEEF, 2), (IT::LEATHER, 1)], xp: 3, h: 1.3 },
    MobDef { name: "pig", hp: 10.0, spd: 2.3, hostile: false, dmg: 0.0, burns_day: false, ranged: false, climber: false, creeper: false, hopper: false, aquatic: false, trader: false, drops: &[(IT::RAW_PORK, 2)], xp: 3, h: 1.0 },
    MobDef { name: "sheep", hp: 8.0, spd: 2.2, hostile: false, dmg: 0.0, burns_day: false, ranged: false, climber: false, creeper: false, hopper: false, aquatic: false, trader: false, drops: &[(B::WOOL_WHITE as i32, 1), (IT::RAW_MUTTON, 2)], xp: 3, h: 1.2 },
    MobDef { name: "chicken", hp: 4.0, spd: 2.6, hostile: false, dmg: 0.0, burns_day: false, ranged: false, climber: false, creeper: false, hopper: false, aquatic: false, trader: false, drops: &[(IT::RAW_CHICKEN, 1), (IT::FEATHER, 2)], xp: 2, h: 0.8 },
    MobDef { name: "wolf", hp: 12.0, spd: 3.4, hostile: false, dmg: 0.0, burns_day: false, ranged: false, climber: false, creeper: false, hopper: false, aquatic: false, trader: false, drops: &[(IT::BONE, 1)], xp: 4, h: 0.9 },
    MobDef { name: "rabbit", hp: 4.0, spd: 3.6, hostile: false, dmg: 0.0, burns_day: false, ranged: false, climber: false, creeper: false, hopper: false, aquatic: false, trader: false, drops: &[(IT::LEATHER, 1)], xp: 2, h: 0.7 },
    MobDef { name: "deer", hp: 12.0, spd: 3.5, hostile: false, dmg: 0.0, burns_day: false, ranged: false, climber: false, creeper: false, hopper: false, aquatic: false, trader: false, drops: &[(IT::RAW_BEEF, 2), (IT::LEATHER, 2)], xp: 4, h: 1.4 },
    MobDef { name: "fox", hp: 10.0, spd: 3.8, hostile: false, dmg: 0.0, burns_day: false, ranged: false, climber: false, creeper: false, hopper: false, aquatic: false, trader: false, drops: &[(IT::SWEET_BERRIES, 2), (IT::LEATHER, 1)], xp: 4, h: 0.75 },
    MobDef { name: "panda", hp: 20.0, spd: 1.9, hostile: false, dmg: 0.0, burns_day: false, ranged: false, climber: false, creeper: false, hopper: false, aquatic: false, trader: false, drops: &[(B::BAMBOO as i32, 3)], xp: 5, h: 1.25 },
    MobDef { name: "trader", hp: 24.0, spd: 2.0, hostile: false, dmg: 0.0, burns_day: false, ranged: false, climber: false, creeper: false, hopper: false, aquatic: false, trader: true, drops: &[(IT::EMERALD, 2)], xp: 5, h: 1.85 },
    MobDef { name: "fish", hp: 3.0, spd: 2.4, hostile: false, dmg: 0.0, burns_day: false, ranged: false, climber: false, creeper: false, hopper: false, aquatic: true, trader: false, drops: &[(IT::RAW_FISH, 1)], xp: 2, h: 0.5 },
    MobDef { name: "zombie", hp: 20.0, spd: 2.7, hostile: true, dmg: 3.0, burns_day: true, ranged: false, climber: false, creeper: false, hopper: false, aquatic: false, trader: false, drops: &[(IT::COAL, 1), (IT::RAW_IRON, 1), (IT::POTATO, 1)], xp: 6, h: 1.8 },
    MobDef { name: "skeleton", hp: 18.0, spd: 2.6, hostile: true, dmg: 3.0, burns_day: true, ranged: true, climber: false, creeper: false, hopper: false, aquatic: false, trader: false, drops: &[(IT::BONE, 2), (IT::ARROW, 3)], xp: 7, h: 1.8 },
    MobDef { name: "spider", hp: 16.0, spd: 3.6, hostile: true, dmg: 3.0, burns_day: false, ranged: false, climber: true, creeper: false, hopper: false, aquatic: false, trader: false, drops: &[(IT::STRING, 2)], xp: 6, h: 0.9 },
    MobDef { name: "creeper", hp: 18.0, spd: 2.6, hostile: true, dmg: 0.0, burns_day: false, ranged: false, climber: false, creeper: true, hopper: false, aquatic: false, trader: false, drops: &[(IT::GUNPOWDER, 2)], xp: 8, h: 1.7 },
    MobDef { name: "slime", hp: 14.0, spd: 2.4, hostile: true, dmg: 2.0, burns_day: false, ranged: false, climber: false, creeper: false, hopper: true, aquatic: false, trader: false, drops: &[(IT::SLIMEBALL, 2)], xp: 5, h: 1.0 },
    MobDef { name: "magma", hp: 22.0, spd: 2.6, hostile: true, dmg: 4.0, burns_day: false, ranged: false, climber: false, creeper: false, hopper: true, aquatic: false, trader: false, drops: &[(IT::MAGMA_CREAM, 2)], xp: 9, h: 1.0 },
    MobDef { name: "stalker", hp: 28.0, spd: 3.4, hostile: true, dmg: 5.0, burns_day: false, ranged: false, climber: false, creeper: false, hopper: false, aquatic: false, trader: false, drops: &[(IT::AMETHYST_SHARD, 2), (IT::DIAMOND, 1)], xp: 14, h: 1.9 },
    MobDef { name: "golem", hp: 55.0, spd: 2.2, hostile: true, dmg: 7.0, burns_day: false, ranged: false, climber: false, creeper: false, hopper: false, aquatic: false, trader: false, drops: &[(IT::IRON_INGOT, 4), (IT::EMERALD, 1)], xp: 22, h: 2.5 },
    MobDef { name: "colossus", hp: 260.0, spd: 2.8, hostile: true, dmg: 8.0, burns_day: false, ranged: false, climber: false, creeper: false, hopper: false, aquatic: false, trader: false, drops: &[(IT::COLOSSUS_HEART, 1), (IT::DIAMOND, 4), (IT::ANCIENT_SCRAP, 2), (IT::GOLDEN_APPLE, 2)], xp: 120, h: 3.4 },
];

pub const MOB_TYPES: &[&str] = &[
    "cow", "pig", "sheep", "chicken", "wolf", "rabbit", "deer", "fox", "panda", "trader",
    "fish", "zombie", "skeleton", "spider", "creeper", "slime", "magma", "stalker", "golem", "colossus",
];

pub fn mob_def_by_name(name: &str) -> Option<(usize, &'static MobDef)> {
    MOB_DEFS.iter().enumerate().find(|(_, d)| d.name == name)
}

#[derive(Clone, Debug)]
pub struct Mob {
    pub id: i32,
    pub type_idx: usize,
    pub kind: &'static str,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub vx: f64,
    pub vy: f64,
    pub vz: f64,
    pub yaw: f64,
    pub target_yaw: f64,
    pub head_yaw: f64,
    pub head_pitch: f64,
    pub hp: f64,
    pub max_hp: f64,
    pub hostile: bool,
    pub state_wander: bool,
    pub state_timer: f64,
    pub walk_phase: f64,
    pub hurt_timer: f64,
    pub hurt_cooldown: f64,
    pub on_ground: bool,
    pub attack_cd: f64,
    pub fuse: f64,
    pub flee_timer: f64,
    pub love_timer: f64,
    pub baby: bool,
    pub arctic: bool,
    pub dying: bool,
    pub death_timer: f64,
    pub fire_timer: f64,
    pub fire_tick: f64,
    pub looting_lvl: i32,
    pub wool_idx: i32,
    pub sight_cd: f64,
    pub sees_player: bool,
    pub sight_memory: f64,
}

#[derive(Clone, Debug)]
pub struct Projectile {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub vx: f64,
    pub vy: f64,
    pub vz: f64,
    pub owner_is_player: bool,
    pub shooter_id: Option<String>,
    pub dmg: f64,
    pub punch: i32,
    pub flame: bool,
    pub life: f64,
    pub stuck: bool,
    pub bx: i32,
    pub by: i32,
    pub bz: i32,
}

pub type ArrowSpawnEvent = (f64, f64, f64, f64, f64, f64, Option<String>);

/// Side effects emitted during `MobManager` operations for the server `Game` layer to apply.
#[derive(Default)]
pub struct MobEvents {
    pub drops: Vec<(f64, f64, f64, i32, i32)>,
    pub xp_awards: Vec<(String, i32)>,
    pub explosions: Vec<(i32, i32, i32, f64)>,
    pub arrows: Vec<ArrowSpawnEvent>,
    pub player_hurts: Vec<(String, f64, String, f64, f64, f64)>,
}

pub struct MobManager {
    pub mobs: Vec<Mob>,
    pub projectiles: Vec<Projectile>,
    pub next_id: i32,
    pub spawn_timer: f64,
}

impl Default for MobManager {
    fn default() -> Self {
        Self::new()
    }
}

impl MobManager {
    pub fn new() -> Self {
        Self {
            mobs: Vec::new(),
            projectiles: Vec::new(),
            next_id: 1,
            spawn_timer: 1.5,
        }
    }

    pub fn spawn_mob(&mut self, kind: &str, x: f64, y: f64, z: f64, baby: bool, arctic: bool) -> Option<usize> {
        let (type_idx, def) = mob_def_by_name(kind)?;
        let mut rng = rand::thread_rng();
        let yaw = rng.r#gen::<f64>() * PI * 2.0;
        let wool_idx = rng.gen_range(0..WOOL_PALETTE.len() as i32);
        let id = self.next_id;
        self.next_id += 1;
        self.mobs.push(Mob {
            id,
            type_idx,
            kind: def.name,
            x,
            y,
            z,
            vx: 0.0,
            vy: 0.0,
            vz: 0.0,
            yaw,
            target_yaw: yaw,
            head_yaw: 0.0,
            head_pitch: 0.0,
            hp: def.hp,
            max_hp: def.hp,
            hostile: def.hostile,
            state_wander: true,
            state_timer: 2.0 + rng.r#gen::<f64>() * 4.0,
            walk_phase: rng.r#gen::<f64>() * 6.0,
            hurt_timer: 0.0,
            hurt_cooldown: 0.0,
            on_ground: true,
            attack_cd: 0.0,
            fuse: 0.0,
            flee_timer: 0.0,
            love_timer: 0.0,
            baby,
            arctic,
            dying: false,
            death_timer: 0.0,
            fire_timer: 0.0,
            fire_tick: 0.0,
            looting_lvl: 0,
            wool_idx,
            sight_cd: 0.0,
            sees_player: false,
            sight_memory: 0.0,
        });
        Some(self.mobs.len() - 1)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn spawn_arrow(
        &mut self,
        x: f64,
        y: f64,
        z: f64,
        vx: f64,
        vy: f64,
        vz: f64,
        owner_is_player: bool,
        dmg: f64,
        punch: i32,
        flame: bool,
        shooter_id: Option<String>,
        events: &mut MobEvents,
    ) {
        self.projectiles.push(Projectile {
            x,
            y,
            z,
            vx,
            vy,
            vz,
            owner_is_player,
            shooter_id: shooter_id.clone(),
            dmg,
            punch,
            flame,
            life: 4.5,
            stuck: false,
            bx: 0,
            by: 0,
            bz: 0,
        });
        events.arrows.push((x, y, z, vx, vy, vz, shooter_id));
    }

    pub fn shift_local(&mut self, dx: f64, dz: f64) {
        for m in &mut self.mobs {
            m.x -= dx;
            m.z -= dz;
        }
        for p in &mut self.projectiles {
            p.x -= dx;
            p.z -= dz;
        }
    }

    pub fn clear_all(&mut self) {
        self.mobs.clear();
        self.projectiles.clear();
    }

    #[allow(clippy::too_many_arguments)]
    pub fn apply_melee(
        &mut self,
        mob_idx: usize,
        attacker_id: Option<&str>,
        sprinting: bool,
        held: Option<&ItemStack>,
        dir: (f64, f64, f64),
        crit: bool,
        events: &mut MobEvents,
    ) -> bool {
        let Some(m) = self.mobs.get_mut(mob_idx) else { return false };
        if m.dying || m.hurt_cooldown > 0.0 {
            return false;
        }
        let it = held.and_then(|h| ITEMS.get(h.id));
        let mut dmg = it.map_or(1.0, |i| if i.dmg > 0.0 { i.dmg } else { 1.0 });
        let def = &MOB_DEFS[m.type_idx];
        dmg += (ench_lvl(held, "Sharpness") as f64) * 1.25;
        if def.burns_day {
            dmg += (ench_lvl(held, "Smite") as f64) * 2.5;
        }
        if crit {
            dmg = ((dmg * 1.5) * 100.0).round() / 100.0;
        }
        let kb_mul = 1.0 + (ench_lvl(held, "Knockback") as f64) * 0.9;
        m.looting_lvl = ench_lvl(held, "Looting");
        let fa = ench_lvl(held, "Fire Aspect");
        if fa > 0 {
            m.fire_timer = m.fire_timer.max((fa as f64) * 4.0);
        }
        Self::damage_mob_inner(
            m,
            dmg,
            dir.0 * 6.0 * kb_mul,
            4.8 + (kb_mul - 1.0) * 1.5,
            dir.2 * 6.0 * kb_mul,
            attacker_id,
            sprinting,
            false,
            events,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn damage_mob_by_idx(
        &mut self,
        mob_idx: usize,
        dmg: f64,
        kx: f64,
        ky: f64,
        kz: f64,
        attacker_id: Option<&str>,
        sprinting: bool,
        bypass_cd: bool,
        events: &mut MobEvents,
    ) -> bool {
        let Some(m) = self.mobs.get_mut(mob_idx) else { return false };
        Self::damage_mob_inner(m, dmg, kx, ky, kz, attacker_id, sprinting, bypass_cd, events)
    }

    #[allow(clippy::too_many_arguments)]
    fn damage_mob_inner(
        m: &mut Mob,
        dmg: f64,
        kx: f64,
        ky: f64,
        kz: f64,
        attacker_id: Option<&str>,
        sprinting: bool,
        bypass_cd: bool,
        events: &mut MobEvents,
    ) -> bool {
        if m.dying || (!bypass_cd && m.hurt_cooldown > 0.0) {
            return false;
        }
        m.hp -= dmg;
        m.hurt_timer = 0.30;
        if !bypass_cd {
            m.hurt_cooldown = 0.50;
        }
        let sprint_bonus = if sprinting { 1.45 } else { 1.0 };
        let up_ky = if m.on_ground { ky } else { (ky * 0.4).min(2.0) };
        m.vx += kx * sprint_bonus;
        m.vy = m.vy.max(up_ky);
        m.vz += kz * sprint_bonus;
        if !m.hostile {
            m.flee_timer = 5.0;
        }
        if m.hp <= 0.0 {
            m.dying = true;
            m.death_timer = 0.0;
            let def = &MOB_DEFS[m.type_idx];
            let loot = m.looting_lvl;
            let mut rng = rand::thread_rng();
            for &(id, cnt) in def.drops {
                let extra = if loot > 0 { rng.gen_range(0..=loot) } else { 0 };
                events.drops.push((m.x, m.y + 0.5, m.z, id, cnt + extra));
            }
            if let Some(pid) = attacker_id {
                events.xp_awards.push((pid.to_string(), def.xp));
            }
        }
        true
    }

    pub fn try_feed_animal(&mut self, eye: (f64, f64, f64)) -> bool {
        let mut fed_idx = None;
        for (i, m) in self.mobs.iter_mut().enumerate() {
            if m.hostile || m.baby || m.love_timer > 0.0 || m.kind == "trader" {
                continue;
            }
            let dist = ((m.x - eye.0).powi(2) + (m.y + 0.7 - eye.1).powi(2) + (m.z - eye.2).powi(2)).sqrt();
            if dist > 4.0 {
                continue;
            }
            m.love_timer = 25.0;
            fed_idx = Some(i);
            break;
        }
        let Some(idx) = fed_idx else { return false };
        let (kind, mx, my, mz) = {
            let m = &self.mobs[idx];
            (m.kind, m.x, m.y, m.z)
        };
        let mut mate_idx = None;
        for (j, o) in self.mobs.iter().enumerate() {
            if j != idx && o.kind == kind && o.love_timer > 0.0 && (o.x - mx).hypot(o.z - mz) < 8.0 {
                mate_idx = Some(j);
                break;
            }
        }
        if let Some(mj) = mate_idx {
            let (ox, oz) = (self.mobs[mj].x, self.mobs[mj].z);
            self.mobs[idx].love_timer = 0.0;
            self.mobs[mj].love_timer = 0.0;
            self.spawn_mob(kind, (mx + ox) * 0.5, my, (mz + oz) * 0.5, true, false);
        }
        true
    }

    fn melee_clear(world: &CoreWorld, ax: f64, ay: f64, az: f64, bx: f64, by: f64, bz: f64) -> bool {
        let (dx, dy, dz) = (bx - ax, by - ay, bz - az);
        let len = (dx * dx + dy * dy + dz * dz).sqrt();
        let steps = ((len / 0.05).ceil() as i32).max(2);
        for i in 1..steps {
            let t = (i as f64) / (steps as f64);
            let (x, y, z) = (ax + dx * t, ay + dy * t, az + dz * t);
            let (fx, fy, fz) = (x.floor() as i32, y.floor() as i32, z.floor() as i32);
            let id = world.get_block(fx, fy, fz);
            if id == 0 { continue; }
            let Some(b) = BLOCKS.blocks[id as usize].as_ref() else { continue };
            if !b.solid || b.shape == "fence" || b.shape == "ladder" { continue; }
            let meta = world.get_meta(fx, fy, fz);
            let (lx, ly, lz) = (x - (fx as f64), y - (fy as f64), z - (fz as f64));
            match b.shape {
                "door" => {
                    if (meta & 4) != 0 { continue; }
                    let facing = meta & 3;
                    let inside = match facing {
                        0 => lz >= 0.70,
                        1 => lx >= 0.70,
                        2 => lz <= 0.30,
                        _ => lx <= 0.30,
                    };
                    if inside { return false; }
                    continue;
                }
                "fence_gate" => { if (meta & 4) != 0 { continue; } return false; }
                "trapdoor" => { if (meta & 4) != 0 || ly < 0.75 { continue; } return false; }
                "slab" => {
                    let top = (meta & 8) != 0;
                    if if top { ly >= 0.5 } else { ly < 0.5 } { return false; }
                    continue;
                }
                "enchant_table" => { if ly < 0.76 { return false; } continue; }
                "campfire" => { if ly < 0.4 { return false; } continue; }
                _ => return false,
            }
        }
        true
    }

    pub fn update(
        &mut self,
        dt: f64,
        world: &mut CoreWorld,
        players: &[(String, &CorePlayer)],
        day_night: &DayNightSystem,
        difficulty: &str,
        events: &mut MobEvents,
    ) {
        if players.is_empty() {
            self.mobs.clear();
            return;
        }
        let mut rng = rand::thread_rng();
        self.spawn_timer -= dt;
        if self.spawn_timer <= 0.0 {
            self.spawn_timer = 2.6;
            let max_hostile = match difficulty {
                "peaceful" => 0,
                "easy" => 7,
                "hard" => 16,
                _ => 11,
            };
            for (_, pl) in players {
                let (mut passive, mut hostile) = (0, 0);
                for m in &self.mobs {
                    if (m.x - pl.x).abs() > 64.0 || (m.z - pl.z).abs() > 64.0 { continue; }
                    if m.hostile { hostile += 1; } else { passive += 1; }
                }
                if passive < 14 {
                    self.try_spawn(world, pl, false, day_night);
                }
                if hostile < max_hostile {
                    self.try_spawn(world, pl, true, day_night);
                }
            }
        }

        let dmg_scale = match difficulty {
            "easy" => 0.7,
            "hard" => 1.5,
            _ => 1.0,
        };

        let mut i = self.mobs.len();
        while i > 0 {
            i -= 1;
            let def = &MOB_DEFS[self.mobs[i].type_idx];
            let (mx, my, mz, hostile) = (self.mobs[i].x, self.mobs[i].y, self.mobs[i].z, self.mobs[i].hostile);

            let mut best_p = 0usize;
            let mut best_d = f64::INFINITY;
            let mut target_p: Option<(usize, f64)> = None;
            for (pi, (_, pl)) in players.iter().enumerate() {
                let d = ((pl.x - mx).powi(2) + (pl.y - my).powi(2) + (pl.z - mz).powi(2)).sqrt();
                if d < best_d { best_d = d; best_p = pi; }
                if !pl.dead && pl.mode == "survival" && target_p.is_none_or(|(_, td)| d < td) {
                    target_p = Some((pi, d));
                }
            }
            if hostile
                && let Some((ti, td)) = target_p
                    && td < 24.0 {
                        best_p = ti;
                        best_d = td;
                    }
            let (pid, pl) = &players[best_p];
            let (dx, dy, dz) = (pl.x - mx, pl.y - my, pl.z - mz);
            let dist = best_d;

            if dist > 74.0 || (hostile && difficulty == "peaceful") {
                self.mobs.remove(i);
                continue;
            }
            let ck = (
                floor_div(mx.floor() as i32, CHUNK_SIZE),
                floor_div(my.floor() as i32, CHUNK_SIZE),
                floor_div(mz.floor() as i32, CHUNK_SIZE),
            );
            if !world.chunks.contains_key(&ck) {
                continue;
            }

            if self.mobs[i].dying {
                let m = &mut self.mobs[i];
                m.death_timer += dt;
                m.vy = (m.vy - 24.0 * dt).max(-24.0);
                let damp = 0.08_f64.powf(dt);
                m.vx *= damp;
                m.vz *= damp;
                let ny = m.y + m.vy * dt;
                if BLOCKS.get(world.get_block(m.x.floor() as i32, ny.floor() as i32, m.z.floor() as i32)).solid {
                    if m.vy < 0.0 { m.y = ny.floor() + 1.0; }
                    m.vy = 0.0;
                } else {
                    m.y = ny;
                }
                if m.death_timer >= 0.42 {
                    self.mobs.remove(i);
                }
                continue;
            }

            let mut spawn_arrow_req = None;
            let mut explode_creeper = false;
            {
                let m = &mut self.mobs[i];
                if m.hurt_timer > 0.0 { m.hurt_timer -= dt; }
                if m.hurt_cooldown > 0.0 { m.hurt_cooldown -= dt; }
                if m.attack_cd > 0.0 { m.attack_cd -= dt; }
                if m.fire_timer > 0.0 {
                    m.fire_timer -= dt;
                    m.fire_tick += dt;
                    if m.fire_tick >= 1.0 {
                        m.fire_tick = 0.0;
                        Self::damage_mob_inner(m, 1.0, 0.0, 0.5, 0.0, Some(pid.as_str()), pl.sprinting, true, events);
                        if m.dying { continue; }
                    }
                }
                if m.flee_timer > 0.0 { m.flee_timer -= dt; }
                if m.love_timer > 0.0 { m.love_timer -= dt; }

                if def.burns_day && !day_night.is_night {
                    let (sky, _) = world.get_light_at(m.x.floor() as i32, (m.y + 1.0).floor() as i32, m.z.floor() as i32);
                    if sky >= 13 && rng.r#gen::<f64>() < dt * 1.4 {
                        Self::damage_mob_inner(m, 2.0, 0.0, 1.0, 0.0, Some(pid.as_str()), pl.sprinting, true, events);
                        if m.dying { continue; }
                    }
                }

                let creeper_ok = if def.creeper {
                    m.sight_cd -= dt;
                    if m.sight_cd <= 0.0 {
                        m.sight_cd = 0.2 + rng.r#gen::<f64>() * 0.1;
                        let ey = m.y + def.h * 0.85;
                        m.sees_player = Self::melee_clear(world, m.x, ey, m.z, pl.x, pl.y + 1.5, pl.z)
                            || Self::melee_clear(world, m.x, ey, m.z, pl.x, pl.y + 0.4, pl.z);
                        if m.sees_player { m.sight_memory = 2.5; }
                    }
                    if !m.sees_player {
                        m.sight_memory -= dt;
                        if m.sight_memory <= 0.0 {
                            m.fuse = (m.fuse - dt * 1.5).max(0.0);
                            false
                        } else {
                            true
                        }
                    } else {
                        true
                    }
                } else {
                    true
                };

                let mut move_spd = 0.0;
                if def.aquatic {
                    m.state_timer -= dt;
                    if m.state_timer <= 0.0 {
                        m.state_timer = 2.0 + rng.r#gen::<f64>() * 3.0;
                        m.target_yaw += (rng.r#gen::<f64>() - 0.5) * 2.2;
                    }
                    move_spd = def.spd;
                    let in_w = world.get_block(m.x.floor() as i32, (m.y + 0.3).floor() as i32, m.z.floor() as i32) == B::WATER;
                    m.vy = if in_w { m.walk_phase.sin() * 0.4 } else { -6.0 };
                } else if m.hostile && dist < 24.0 && !pl.dead && pl.mode == "survival" && creeper_ok {
                    m.target_yaw = dx.atan2(dz);
                    if def.creeper {
                        if dist < 3.4 && m.sees_player {
                            m.fuse += dt;
                            if m.fuse >= 1.45 {
                                explode_creeper = true;
                            }
                        } else {
                            m.fuse = (m.fuse - dt * 1.5).max(0.0);
                            move_spd = def.spd;
                        }
                    } else if def.ranged {
                        move_spd = if dist > 9.0 { def.spd } else if dist < 5.0 { -def.spd * 0.6 } else { 0.0 };
                        if dist < 17.0 && m.attack_cd <= 0.0 {
                            m.attack_cd = 2.1;
                            let inv = 1.0 / dist.max(1.0);
                            spawn_arrow_req = Some((m.x, m.y + 1.4, m.z, dx * inv * 19.0, dy * inv * 19.0 + 2.4, dz * inv * 19.0, 3.0 * dmg_scale));
                        }
                    } else {
                        move_spd = if dist > 1.35 { def.spd } else { 0.0 };
                        if dist < 1.85 && m.attack_cd <= 0.0 && Self::melee_clear(world, m.x, m.y + def.h * 0.72, m.z, pl.x, pl.y + 1.0, pl.z) {
                            m.attack_cd = 1.05;
                            let hd = dx.hypot(dz).max(1.0);
                            events.player_hurts.push((
                                pid.clone(),
                                def.dmg * dmg_scale,
                                format!("Slain by a {}.", m.kind),
                                (dx / hd) * 6.5,
                                4.4,
                                (dz / hd) * 6.5,
                            ));
                            let th = pl.inventory.armor_ench_lvl("Thorns");
                            if th > 0 && rng.r#gen::<f64>() < (th as f64) * 0.15 {
                                let tdmg = (1 + rng.gen_range(0..4)) as f64;
                                Self::damage_mob_inner(m, tdmg, -(dx / hd) * 3.0, 2.5, -(dz / hd) * 3.0, Some(pid.as_str()), pl.sprinting, false, events);
                                if m.dying { continue; }
                            }
                        }
                    }
                } else {
                    let held = pl.inventory.get_held();
                    let holding_food = !m.hostile && !def.trader && held.is_some_and(|h| {
                        matches!(h.id, IT::WHEAT_ITEM | IT::APPLE | IT::CARROT) || h.id == B::BAMBOO as i32
                    });
                    if m.flee_timer > 0.0 {
                        m.target_yaw = (-dx).atan2(-dz);
                        move_spd = def.spd * 1.55;
                    } else if def.trader && dist < 8.0 {
                        m.target_yaw = dx.atan2(dz);
                        move_spd = 0.0;
                    } else if holding_food && dist < 11.0 && dist > 2.2 {
                        m.target_yaw = dx.atan2(dz);
                        move_spd = def.spd * 1.05;
                    } else {
                        m.state_timer -= dt;
                        if m.state_timer <= 0.0 {
                            m.state_wander = rng.r#gen::<f64>() < 0.55;
                            m.state_timer = 2.0 + rng.r#gen::<f64>() * 4.5;
                            m.target_yaw = m.yaw + (rng.r#gen::<f64>() - 0.5) * 2.6;
                        }
                        move_spd = if m.state_wander { def.spd * 0.62 } else { 0.0 };
                    }
                }

                if !explode_creeper {
                    let mut d_yaw = m.target_yaw - m.yaw;
                    while d_yaw > PI { d_yaw -= PI * 2.0; }
                    while d_yaw < -PI { d_yaw += PI * 2.0; }
                    m.yaw += d_yaw * (7.5 * dt).min(1.0);

                    let (mut tgt_hy, mut tgt_hp) = (0.0, 0.0);
                    if dist < 9.5 && !def.aquatic {
                        let mut look_y = dx.atan2(dz) - m.yaw;
                        while look_y > PI { look_y -= PI * 2.0; }
                        while look_y < -PI { look_y += PI * 2.0; }
                        if look_y.abs() < 1.15 {
                            tgt_hy = clamp(look_y, -0.7, 0.7);
                            tgt_hp = clamp(-((pl.y + 1.5) - (m.y + def.h * 0.8)).atan2(dx.hypot(dz).max(1.0)), -0.4, 0.4);
                        }
                    }
                    m.head_yaw += (tgt_hy - m.head_yaw) * (8.0 * dt).min(1.0);
                    m.head_pitch += (tgt_hp - m.head_pitch) * (8.0 * dt).min(1.0);

                    let tx = m.yaw.sin() * move_spd;
                    let tz = m.yaw.cos() * move_spd;
                    m.vx += (tx - m.vx) * (7.5 * dt).min(1.0);
                    m.vz += (tz - m.vz) * (7.5 * dt).min(1.0);

                    if !def.aquatic {
                        let in_w = world.get_block(m.x.floor() as i32, (m.y + 0.2).floor() as i32, m.z.floor() as i32) == B::WATER;
                        if in_w {
                            m.vy += (2.8 - m.vy) * (6.0 * dt).min(1.0);
                        } else {
                            m.vy = (m.vy - 24.0 * dt).max(-32.0);
                        }
                    }

                    let mr = if m.kind == "golem" { 0.48 } else if m.kind == "spider" { 0.45 } else if m.baby { 0.22 } else { 0.34 };
                    let mh = (def.h * if m.baby { 0.6 } else { 1.0 }).max(0.5);
                    let mut hit_wall = false;
                    let nx = m.x + m.vx * dt;
                    if !world.aabb_collides(nx, m.y + 0.05, m.z, mr, mh - 0.1) { m.x = nx; } else { m.vx = 0.0; hit_wall = true; }
                    let nz = m.z + m.vz * dt;
                    if !world.aabb_collides(m.x, m.y + 0.05, nz, mr, mh - 0.1) { m.z = nz; } else { m.vz = 0.0; hit_wall = true; }
                    if hit_wall {
                        let fx = m.x + m.yaw.sin() * (mr + 0.35);
                        let fz = m.z + m.yaw.cos() * (mr + 0.35);
                        let clear_above = !BLOCKS.get(world.get_block(fx.floor() as i32, (m.y + 1.3).floor() as i32, fz.floor() as i32)).solid;
                        if (clear_above && m.on_ground) || (def.climber && m.vy.abs() <= 0.15) {
                            m.vy = 7.8;
                        } else {
                            m.target_yaw += 1.6;
                        }
                    }
                    if def.hopper && m.on_ground && move_spd > 0.5 && rng.r#gen::<f64>() < dt * 2.5 {
                        m.vy = 6.8;
                    }
                    let ny = m.y + m.vy * dt;
                    if world.aabb_collides(m.x, ny, m.z, mr, mh) {
                        if m.vy < 0.0 {
                            m.y = ny.floor() + 1.0;
                            m.on_ground = true;
                        } else {
                            m.on_ground = false;
                        }
                        m.vy = 0.0;
                    } else {
                        m.y = ny;
                        m.on_ground = false;
                    }
                    let actual_spd = m.vx.hypot(m.vz);
                    if actual_spd > 0.15 {
                        m.walk_phase += dt * (actual_spd * 2.7);
                    }
                }
            }

            if explode_creeper {
                let m = self.mobs.remove(i);
                events.explosions.push((m.x.floor() as i32, m.y.floor() as i32, m.z.floor() as i32, 3.0));
                continue;
            }
            if let Some((ax, ay, az, avx, avy, avz, admg)) = spawn_arrow_req {
                self.spawn_arrow(ax, ay, az, avx, avy, avz, false, admg, 0, false, None, events);
            }
        }

        // Update projectiles
        let mut pi = self.projectiles.len();
        while pi > 0 {
            pi -= 1;
            self.projectiles[pi].life -= dt;
            if self.projectiles[pi].life <= 0.0 {
                self.projectiles.remove(pi);
                continue;
            }
            if self.projectiles[pi].stuck {
                let (bx, by, bz) = (self.projectiles[pi].bx, self.projectiles[pi].by, self.projectiles[pi].bz);
                if !BLOCKS.get(world.get_block(bx, by, bz)).solid {
                    let pr = &mut self.projectiles[pi];
                    pr.stuck = false;
                    pr.vy = -1.5;
                    pr.vx = 0.0;
                    pr.vz = 0.0;
                }
                continue;
            }
            let (px, py, pz, vx, _vy, vz, owner_is_player, dmg, punch, flame, shooter_id) = {
                let pr = &mut self.projectiles[pi];
                pr.vy -= 18.0 * dt;
                pr.x += pr.vx * dt;
                pr.y += pr.vy * dt;
                pr.z += pr.vz * dt;
                (pr.x, pr.y, pr.z, pr.vx, pr.vy, pr.vz, pr.owner_is_player, pr.dmg, pr.punch, pr.flame, pr.shooter_id.clone())
            };
            let (bx, by, bz) = (px.floor() as i32, py.floor() as i32, pz.floor() as i32);
            if BLOCKS.get(world.get_block(bx, by, bz)).solid {
                let pr = &mut self.projectiles[pi];
                pr.stuck = true;
                pr.bx = bx;
                pr.by = by;
                pr.bz = bz;
                pr.life = pr.life.min(6.0);
                continue;
            }
            if owner_is_player {
                let mut hit = false;
                for m in &mut self.mobs {
                    if m.dying { continue; }
                    let def = &MOB_DEFS[m.type_idx];
                    let d = ((m.x - px).powi(2) + (m.y + def.h * 0.5 - py).powi(2) + (m.z - pz).powi(2)).sqrt();
                    if d < 0.95 {
                        let pm = 1.0 + (punch as f64) * 0.8;
                        if flame { m.fire_timer = m.fire_timer.max(5.0); }
                        Self::damage_mob_inner(m, dmg, vx * 0.25 * pm, 3.8 + (punch as f64), vz * 0.25 * pm, shooter_id.as_deref(), false, false, events);
                        hit = true;
                        break;
                    }
                }
                if hit {
                    self.projectiles.remove(pi);
                }
            } else {
                let mut hit = false;
                for (vid, vpl) in players {
                    if vpl.dead { continue; }
                    let d = ((vpl.x - px).powi(2) + (vpl.y + 0.9 - py).powi(2) + (vpl.z - pz).powi(2)).sqrt();
                    if d < 0.75 {
                        let spd = vx.hypot(vz).max(1.0);
                        events.player_hurts.push((
                            vid.clone(),
                            dmg,
                            "Shot by a Skeleton.".to_string(),
                            (vx / spd) * 5.2,
                            3.6,
                            (vz / spd) * 5.2,
                        ));
                        hit = true;
                        break;
                    }
                }
                if hit {
                    self.projectiles.remove(pi);
                }
            }
        }
    }

    fn try_spawn(&mut self, world: &mut CoreWorld, pl: &CorePlayer, hostile: bool, day_night: &DayNightSystem) {
        let mut rng = rand::thread_rng();
        let ang = rng.r#gen::<f64>() * PI * 2.0;
        let rad = 18.0 + rng.r#gen::<f64>() * 28.0;
        let wx = (pl.x + ang.cos() * rad).floor() as i32;
        let wz = (pl.z + ang.sin() * rad).floor() as i32;
        let pcy = floor_div(pl.y.floor() as i32, CHUNK_SIZE);
        if !world.chunks.contains_key(&(floor_div(wx, CHUNK_SIZE), pcy, floor_div(wz, CHUNK_SIZE))) {
            return;
        }
        let biome = world.get_biome_at(wx, wz);
        let wy = world.get_surface_height(wx, wz);

        if hostile {
            if biome.no_hostiles { return; }
            if biome.id == 28 && wy > SEA_LEVEL {
                self.spawn_mob("magma", wx as f64 + 0.5, wy as f64, wz as f64 + 0.5, false, false);
                return;
            }
            if pl.y < (SEA_LEVEL - 6) as f64 || (!day_night.is_night && rng.r#gen::<f64>() < 0.65) {
                let mut found_y = None;
                let start_y = 40.min((pl.y + 10.0).floor() as i32);
                let end_y = (pl.y - 16.0).floor() as i32;
                for y in (end_y..=start_y).rev() {
                    if world.get_block(wx, y, wz) == B::AIR
                        && world.get_block(wx, y + 1, wz) == B::AIR
                        && BLOCKS.get(world.get_block(wx, y - 1, wz)).solid
                    {
                        let (sky, blk) = world.get_light_at(wx, y, wz);
                        if sky <= 4 && blk <= 7 {
                            found_y = Some(y);
                            break;
                        }
                    }
                }
                let Some(fy) = found_y else { return };
                let r = rng.r#gen::<f64>();
                let kind = if fy < 22 && r < 0.14 { "golem" }
                    else if fy < 28 && r < 0.36 { "stalker" }
                    else if r < 0.56 { "zombie" }
                    else if r < 0.75 { "skeleton" }
                    else if r < 0.88 { "spider" }
                    else { "creeper" };
                self.spawn_mob(kind, wx as f64 + 0.5, fy as f64, wz as f64 + 0.5, false, false);
                return;
            }
            if !day_night.is_night { return; }
            let (_, blk) = world.get_light_at(wx, wy, wz);
            if blk > 7 || world.get_block(wx, wy - 1, wz) == B::WATER { return; }
            let pool: &[&str] = if biome.id == 5 || biome.id == 29 {
                &["slime", "zombie", "spider"]
            } else {
                &["zombie", "skeleton", "spider", "creeper"]
            };
            let kind = pool[rng.gen_range(0..pool.len())];
            self.spawn_mob(kind, wx as f64 + 0.5, wy as f64, wz as f64 + 0.5, false, false);
        } else {
            if biome.cat == "aquatic" || world.get_block(wx, SEA_LEVEL - 1, wz) == B::WATER {
                if world.get_block(wx, SEA_LEVEL - 2, wz) == B::WATER {
                    self.spawn_mob("fish", wx as f64 + 0.5, (SEA_LEVEL - 2) as f64, wz as f64 + 0.5, false, false);
                }
                return;
            }
            if wy <= SEA_LEVEL { return; }
            if rng.r#gen::<f64>() < 0.04 && !self.mobs.iter().any(|m| m.kind == "trader") {
                self.spawn_mob("trader", wx as f64 + 0.5, wy as f64, wz as f64 + 0.5, false, false);
                return;
            }
            if biome.id == 30 {
                let k = if rng.r#gen::<f64>() < 0.65 { "panda" } else { "chicken" };
                self.spawn_mob(k, wx as f64 + 0.5, wy as f64, wz as f64 + 0.5, false, false);
                return;
            }
            if biome.id == 31 || biome.id == 33 {
                let k = if rng.r#gen::<f64>() < 0.45 { "fox" } else { "deer" };
                self.spawn_mob(k, wx as f64 + 0.5, wy as f64, wz as f64 + 0.5, false, false);
                return;
            }
            if biome.cat == "cold" {
                let r = rng.r#gen::<f64>();
                let k = if r < 0.35 { "fox" } else if r < 0.6 { "wolf" } else { "rabbit" };
                self.spawn_mob(k, wx as f64 + 0.5, wy as f64, wz as f64 + 0.5, false, true);
                return;
            }
            let pool: &[&str] = if biome.cat == "warm" {
                &["rabbit", "chicken", "cow"]
            } else {
                &["cow", "pig", "sheep", "chicken", "deer", "wolf", "rabbit", "fox"]
            };
            let k = pool[rng.gen_range(0..pool.len())];
            self.spawn_mob(k, wx as f64 + 0.5, wy as f64, wz as f64 + 0.5, false, false);
        }
    }
}
