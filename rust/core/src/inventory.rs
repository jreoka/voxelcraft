use rand::Rng;

use crate::enchants::{ItemEnchant, ItemStack, ench_lvl};
use crate::items::ITEMS;

#[derive(Clone, Debug)]
pub struct Inventory {
    pub slots: Vec<Option<ItemStack>>, // 36
    pub armor: Vec<Option<ItemStack>>, // 5
    pub selected: usize,
}

impl Default for Inventory {
    fn default() -> Self {
        Self::new()
    }
}

impl Inventory {
    pub fn new() -> Self {
        Self {
            slots: vec![None; 36],
            armor: vec![None; 5],
            selected: 0,
        }
    }

    pub fn get_held(&self) -> Option<&ItemStack> {
        self.slots.get(self.selected).and_then(|s| s.as_ref())
    }

    pub fn add_item(
        &mut self,
        id: i32,
        mut count: i32,
        dur: Option<i32>,
        enchant: Option<ItemEnchant>,
    ) -> i32 {
        let Some(def) = ITEMS.get(id) else {
            return count;
        };
        if count <= 0 {
            return count;
        }
        let max_s = def.stack.max(1);
        let dur = dur.or(if def.max_dur > 0 { Some(def.max_dur) } else { None });

        if max_s > 1 && enchant.is_none() {
            for slot in &mut self.slots {
                if count <= 0 {
                    break;
                }
                if let Some(s) = slot
                    && s.id == id && s.enchant.is_none() && s.count < max_s {
                        let add = (max_s - s.count).min(count);
                        s.count += add;
                        count -= add;
                    }
            }
        }
        for slot in &mut self.slots {
            if count <= 0 {
                break;
            }
            if slot.is_none() {
                let add = max_s.min(count);
                *slot = Some(ItemStack {
                    id,
                    count: add,
                    dur,
                    enchant: enchant.clone(),
                });
                count -= add;
            }
        }
        count
    }

    pub fn get_defense(&self) -> f64 {
        let mut def = 0.0;
        for a in self.armor.iter().take(5).flatten() {
            if let Some(it) = ITEMS.get(a.id) {
                def += it.defense;
            }
        }
        def
    }

    pub fn get_enchant_protection(&self, kind: &str) -> i32 {
        let mut epf = 0;
        for a in self.armor.iter().take(4).flatten() {
            epf += ench_lvl(Some(a), "Protection");
            match kind {
                "fire" => epf += ench_lvl(Some(a), "Fire Protection") * 2,
                "blast" => epf += ench_lvl(Some(a), "Blast Protection") * 2,
                "projectile" => epf += ench_lvl(Some(a), "Projectile Protection") * 2,
                "fall" => epf += ench_lvl(Some(a), "Feather Falling") * 3,
                _ => {}
            }
        }
        epf.min(20)
    }

    pub fn armor_ench_lvl(&self, name: &str) -> i32 {
        let mut best = 0;
        for a in self.armor.iter().take(4) {
            best = best.max(ench_lvl(a.as_ref(), name));
        }
        best
    }

    pub fn damage_armor(&mut self, amt: i32) {
        let mut rng = rand::thread_rng();
        for slot in self.armor.iter_mut().take(4) {
            let remove = if let Some(a) = slot {
                let ub = ench_lvl(Some(a), "Unbreaking");
                if let Some(dur) = &mut a.dur {
                    if ub > 0 && rng.r#gen::<f64>() < (ub as f64) / ((ub + 2) as f64) {
                        false
                    } else {
                        *dur -= amt;
                        *dur <= 0
                    }
                } else {
                    false
                }
            } else {
                false
            };
            if remove {
                *slot = None;
            }
        }
    }
}
