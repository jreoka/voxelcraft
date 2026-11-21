use indexmap::IndexMap;
use voxelcraft_core::enchants::{ItemEnchant, ItemStack};
use voxelcraft_core::items::ITEMS;

pub fn ench_sig(e: Option<&ItemEnchant>) -> String {
    let Some(e) = e else {
        return String::new();
    };
    let mut pairs: Vec<String> = match &e.list {
        Some(list) => list.iter().map(|x| format!("{}:{}", x.name, x.lvl)).collect(),
        None => vec![format!("{}:{}", e.name, e.lvl)],
    };
    pairs.sort();
    pairs.join(",")
}

pub fn stack_key(s: &ItemStack) -> String {
    format!("{}#{}", s.id, ench_sig(s.enchant.as_ref()))
}

#[derive(Clone, Debug)]
pub struct LedgerEntry {
    pub count: i32,
    pub durs: Vec<i32>,
    pub proto_id: i32,
    pub proto_enchant: Option<ItemEnchant>,
}

#[derive(Clone, Debug, Default)]
pub struct Ledger {
    pub m: IndexMap<String, LedgerEntry>,
}

impl Ledger {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_stacks(lists: &[&[Option<ItemStack>]]) -> Self {
        let mut l = Self::new();
        for arr in lists {
            for s in arr.iter().flatten() {
                l.add(s);
            }
        }
        l
    }

    pub fn max_dur(id: i32) -> i32 {
        ITEMS.get(id).map_or(0, |it| it.max_dur)
    }

    pub fn add(&mut self, s: &ItemStack) {
        if s.count <= 0 {
            return;
        }
        let k = stack_key(s);
        let entry = self.m.entry(k).or_insert_with(|| LedgerEntry {
            count: 0,
            durs: Vec::new(),
            proto_id: s.id,
            proto_enchant: s.enchant.clone(),
        });
        entry.count += s.count;
        let md = Self::max_dur(s.id);
        if md > 0 {
            let d = s.dur.map_or(md, |v| v.min(md));
            for _ in 0..s.count {
                entry.durs.push(d);
            }
        }
    }

    pub fn count(&self, k: &str) -> i32 {
        self.m.get(k).map_or(0, |e| e.count)
    }

    pub fn remove(&mut self, k: &str, n: i32, dur_hint: Option<i32>) -> Option<Vec<i32>> {
        let e = self.m.get_mut(k)?;
        if e.count < n {
            return None;
        }
        e.count -= n;
        let mut removed = Vec::new();
        if !e.durs.is_empty() {
            for _ in 0..n {
                if e.durs.is_empty() {
                    break;
                }
                let mut best = 0usize;
                for j in 1..e.durs.len() {
                    let d = e.durs[j];
                    let bd = e.durs[best];
                    if let Some(hint) = dur_hint {
                        let score = |v: i32| -> i64 {
                            if v >= hint {
                                (v - hint) as i64
                            } else {
                                1_000_000 + (hint - v) as i64
                            }
                        };
                        if score(d) < score(bd) {
                            best = j;
                        }
                    } else if d > bd {
                        best = j;
                    }
                }
                removed.push(e.durs.remove(best));
            }
        }
        if e.count <= 0 {
            self.m.shift_remove(k);
        }
        Some(removed)
    }

    pub fn clamp(&self, arrays: &mut [&mut [Option<ItemStack>]]) -> bool {
        struct Budget {
            count: i32,
            durs: Vec<i32>,
        }
        let mut budget: IndexMap<String, Budget> = IndexMap::new();
        for (k, e) in &self.m {
            let mut durs = e.durs.clone();
            durs.sort_by(|a, b| b.cmp(a));
            budget.insert(k.clone(), Budget { count: e.count, durs });
        }
        let mut changed = false;
        // Track (array_idx, slot_idx) for durable items per key
        let mut dur_units: IndexMap<String, Vec<(usize, usize)>> = IndexMap::new();

        for (ai, arr) in arrays.iter_mut().enumerate() {
            for (si, slot) in arr.iter_mut().enumerate() {
                let Some(s) = slot else { continue };
                let k = stack_key(s);
                let avail = budget.get(&k).map_or(0, |b| b.count);
                if avail <= 0 {
                    *slot = None;
                    changed = true;
                    continue;
                }
                if s.count > avail {
                    s.count = avail;
                    changed = true;
                }
                if let Some(b) = budget.get_mut(&k) {
                    b.count -= s.count;
                    if !b.durs.is_empty() {
                        dur_units.entry(k).or_default().push((ai, si));
                    }
                }
            }
        }

        for (k, mut locs) in dur_units {
            let durs = &budget.get(&k).unwrap().durs;
            locs.sort_by(|&(a1, s1), &(a2, s2)| {
                let d1 = arrays[a1][s1].as_ref().and_then(|s| s.dur).unwrap_or(0);
                let d2 = arrays[a2][s2].as_ref().and_then(|s| s.dur).unwrap_or(0);
                d2.cmp(&d1)
            });
            for (j, (ai, si)) in locs.into_iter().enumerate() {
                let cap = durs.get(j).copied().unwrap_or(0);
                if let Some(s) = arrays[ai][si].as_mut()
                    && s.dur.is_none_or(|d| d > cap) {
                        s.dur = Some(cap);
                        changed = true;
                    }
            }
        }
        changed
    }
}
