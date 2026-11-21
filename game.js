'use strict';

/**
 * Authoritative multiplayer game layer.
 *
 * The server runs the client's own simulation code (loaded by core-loader.js) so it knows the real
 * terrain, and owns everything that affects other players or a player's progress:
 *   - the world: generated terrain + player edits, redstone, furnaces, falling blocks, explosions
 *   - mobs (shared by everyone), their AI, drops and XP
 *   - dropped items (shared entities that must be picked up through the server)
 *   - player health / hunger / XP / death
 *   - player inventories: every way of gaining an item goes through a server-validated event, and a
 *     client's inventory sync can never contain more of an item than the server's ledger allows.
 */

const fs = require('fs');

const CONTAINER_TYPES = { 80: 'chest', 250: 'barrel', 78: 'furnace', 79: 'furnace' };
const MAX_DROPS = 1500;
const DROP_LIFETIME_MS = 240000;
const LOAD_RADIUS_H = 3;   // chunks loaded around each player (horizontal)
const LOAD_RADIUS_V = 2;   // and vertical
const UNLOAD_RADIUS_H = 6;
const UNLOAD_RADIUS_V = 4;

// Damage the client may report about itself (environmental damage the server cannot simulate)
const SELF_DAMAGE_REASONS = new Set([
  'Fell from a high place.', 'Stepped on molten Magma.', 'Pricked to death by a Cactus.',
  'Drowned underwater.', 'Tried to swim in lava.'
]);

function enchSig(e) {
  if (!e) return '';
  const list = Array.isArray(e.list) ? e.list : [{ name: e.name, lvl: e.lvl }];
  return list.map(x => `${x.name}:${x.lvl}`).sort().join(',');
}
function stackKey(s) { return `${s.id}#${enchSig(s.enchant)}`; }

// ============================================================================
// ITEM LEDGER: the multiset of items a player is entitled to (per item id + enchantments)
// ============================================================================

class Ledger {
  constructor(C) {
    this.C = C;
    this.m = new Map(); // key -> { count, durs: number[] (durable items only), proto: { id, enchant } }
  }

  static fromStacks(C, lists) {
    const L = new Ledger(C);
    for (const arr of lists) for (const s of arr) if (s) L.add(s);
    return L;
  }

  _maxDur(id) { const it = this.C.ITEMS.get(id); return it && it.maxDur ? it.maxDur : 0; }

  add(s) {
    if (!s || s.count <= 0) return;
    const k = stackKey(s);
    let e = this.m.get(k);
    if (!e) { e = { count: 0, durs: [], proto: { id: s.id, enchant: s.enchant } }; this.m.set(k, e); }
    e.count += s.count;
    const md = this._maxDur(s.id);
    if (md) for (let i = 0; i < s.count; i++) e.durs.push(Number.isFinite(s.dur) ? Math.min(md, s.dur) : md);
  }

  count(k) { const e = this.m.get(k); return e ? e.count : 0; }

  // Removes n units; returns the removed durabilities (empty for plain items) or null if not owned
  remove(k, n, durHint) {
    const e = this.m.get(k);
    if (!e || e.count < n) return null;
    e.count -= n;
    const removed = [];
    if (e.durs.length) {
      for (let i = 0; i < n && e.durs.length; i++) {
        // Prefer the unit whose durability is closest to (but not below) the hint
        let best = -1;
        for (let j = 0; j < e.durs.length; j++) {
          const d = e.durs[j];
          if (best < 0) { best = j; continue; }
          const bd = e.durs[best];
          if (Number.isFinite(durHint)) {
            const score = (v) => (v >= durHint ? v - durHint : 1e6 + (durHint - v));
            if (score(d) < score(bd)) best = j;
          } else if (d > bd) best = j;
        }
        removed.push(e.durs.splice(best, 1)[0]);
      }
    }
    if (e.count <= 0) this.m.delete(k);
    return removed;
  }

  // Clamps the given stack arrays (mutated in place, in order) so they never exceed this ledger.
  // Durable items are additionally capped to the durabilities the ledger knows about.
  clamp(arrays) {
    const budget = new Map();
    for (const [k, e] of this.m) budget.set(k, { count: e.count, durs: e.durs.slice().sort((a, b) => b - a) });
    let changed = false;
    const durUnits = new Map(); // key -> [{arr, i}]
    for (const arr of arrays) {
      for (let i = 0; i < arr.length; i++) {
        const s = arr[i];
        if (!s) continue;
        const k = stackKey(s);
        const b = budget.get(k);
        const avail = b ? b.count : 0;
        if (avail <= 0) { arr[i] = null; changed = true; continue; }
        if (s.count > avail) { s.count = avail; changed = true; }
        b.count -= s.count;
        if (b.durs.length) {
          if (!durUnits.has(k)) durUnits.set(k, []);
          durUnits.get(k).push(s);
        }
      }
    }
    for (const [k, stacks] of durUnits) {
      const durs = budget.get(k).durs; // sorted high -> low
      stacks.sort((a, b) => (b.dur || 0) - (a.dur || 0));
      for (let j = 0; j < stacks.length; j++) {
        const cap = durs[j] !== undefined ? durs[j] : 0;
        if (!Number.isFinite(stacks[j].dur) || stacks[j].dur > cap) { stacks[j].dur = cap; changed = true; }
      }
    }
    return changed;
  }
}

// ============================================================================
// WORLD STATE: persisted edits + a live server copy of the real (generated) world
// ============================================================================

class WorldState {
  constructor(C, config, worldPath, atomicWriteJson) {
    this.C = C;
    this.worldPath = worldPath;
    this.atomicWriteJson = atomicWriteJson;
    this.seed = String(config.seed || 'voxelcraft-server-2026');
    this.day = 1;
    this.time = 0.28;
    this.weather = 'clear';
    this.weatherTimer = 240;
    this.deltas = new Map();        // "wx,wy,wz" -> [wx, wy, wz, id, meta]
    this.blockEntities = new Map(); // "wx,wy,wz" -> entObj (shared with the core world)
    this.savedPlayers = new Map();
    this.dirty = false;
    this.changeLog = null;          // when an array, block changes are collected for broadcasting
    this.core = null;
  }

  load() {
    if (fs.existsSync(this.worldPath)) {
      try {
        const data = JSON.parse(fs.readFileSync(this.worldPath, 'utf8'));
        if (data.seed) this.seed = String(data.seed);
        if (Number.isFinite(data.day)) this.day = data.day;
        if (Number.isFinite(data.time)) this.time = data.time;
        if (data.weather) this.weather = data.weather;
        if (Array.isArray(data.deltas)) {
          for (const d of data.deltas) {
            if (Array.isArray(d) && d.length >= 4) {
              const [wx, wy, wz, id, meta] = d;
              this.deltas.set(`${wx},${wy},${wz}`, [wx | 0, wy | 0, wz | 0, id | 0, (meta | 0) & 255]);
            }
          }
        }
        if (Array.isArray(data.blockEntities)) {
          for (const [k, ent] of data.blockEntities) {
            if (typeof k === 'string' && ent && typeof ent === 'object') this.blockEntities.set(k, ent);
          }
        }
        if (data.players && typeof data.players === 'object') {
          for (const [k, v] of Object.entries(data.players)) if (v && typeof v === 'object') this.savedPlayers.set(k.toLowerCase(), v);
        }
        console.log(`[World] Loaded ${this.deltas.size} block deltas, ${this.blockEntities.size} containers, ${this.savedPlayers.size} player profiles (Seed: ${this.seed}, Day ${this.day}).`);
      } catch (err) {
        console.error('[World] Failed to load world-data.json:', err.message);
      }
      this._initCore();
    } else {
      this._initCore();
      this.save();
      console.log(`[World] Created new server world at ${this.worldPath} (Seed: ${this.seed}).`);
    }
  }

  _initCore() {
    const ws = this;
    const C = this.C;
    class ServerCoreWorld extends C.World {
      setBlock(wx, wy, wz, id, meta = 0, recordDelta = true) {
        super.setBlock(wx, wy, wz, id, meta, recordDelta);
        if (recordDelta) ws._onCoreSet(wx | 0, wy | 0, wz | 0, id | 0, (meta | 0) & 255);
      }
    }
    this.core = new ServerCoreWorld(this.seed, null);
    this.core.blockEntities = this.blockEntities;
    // Mirror persisted edits into the core's chunk modification table so generation applies them
    for (const [wx, wy, wz, id, meta] of this.deltas.values()) {
      const cs = C.CHUNK_SIZE;
      const key = `${C.floorDiv(wx, cs)},${C.floorDiv(wy, cs)},${C.floorDiv(wz, cs)}`;
      let map = this.core.modifications.get(key);
      if (!map) { map = new Map(); this.core.modifications.set(key, map); }
      map.set(C.Chunk.idx(C.floorMod(wx, cs), C.floorMod(wy, cs), C.floorMod(wz, cs)), [id, meta]);
    }
  }

  _onCoreSet(x, y, z, id, meta) {
    const key = `${x},${y},${z}`;
    this.deltas.set(key, [x, y, z, id, meta]);
    if (!CONTAINER_TYPES[id]) this.blockEntities.delete(key);
    this.dirty = true;
    if (this.changeLog) this.changeLog.push([x, y, z, id, meta]);
  }

  save() {
    try {
      const playersObj = {};
      for (const [k, v] of this.savedPlayers.entries()) playersObj[k] = v;
      this.atomicWriteJson(this.worldPath, {
        version: 3,
        seed: this.seed,
        day: this.day,
        time: +this.time.toFixed(4),
        weather: this.weather,
        deltas: Array.from(this.deltas.values()),
        blockEntities: Array.from(this.blockEntities.entries()),
        players: playersObj,
        savedAt: new Date().toISOString()
      }, false);
      this.dirty = false;
      return true;
    } catch (err) {
      console.error('[World] Save failed:', err.message);
      return false;
    }
  }

  ensureChunk(cx, cy, cz) {
    const C = this.C;
    const key = `${cx},${cy},${cz}`;
    let ch = this.core.chunks.get(key);
    if (!ch) {
      ch = new C.Chunk(cx, cy, cz);
      this.core.generator.generateChunk(ch, this.core);
      this.core.chunks.set(key, ch);
      this.core.lighting.computeLocalLight(ch);
      ch.lightDirty = false;
    }
    return ch;
  }

  ensureChunkAt(x, y, z) {
    const cs = this.C.CHUNK_SIZE;
    return this.ensureChunk(Math.floor(x / cs), Math.floor(y / cs), Math.floor(z / cs));
  }

  getBlock(x, y, z) { this.ensureChunkAt(x, y, z); return this.core.getBlock(x, y, z); }
  getMeta(x, y, z) { this.ensureChunkAt(x, y, z); return this.core.getMeta(x, y, z); }
  setBlock(x, y, z, id, meta = 0) { this.ensureChunkAt(x, y, z); this.core.setBlock(x, y, z, id, meta, true); }
  getDelta(x, y, z) { return this.deltas.get(`${x},${y},${z}`) || null; }

  setBlockEntity(x, y, z, ent) {
    const key = `${x},${y},${z}`;
    if (!ent) this.blockEntities.delete(key);
    else this.blockEntities.set(key, ent);
    this.dirty = true;
  }

  // Sand/gravel settle instantly server-side (clients animate the fall themselves)
  processGravity() {
    const q = this.core._gravityQueue;
    const BLOCKS = this.C.BLOCKS;
    let n = 0;
    while (q.length && n++ < 256) {
      const [x, y, z] = q.shift();
      const id = this.getBlock(x, y, z);
      if (!BLOCKS.get(id).gravity) continue;
      let ty = y;
      while (ty > y - 64) {
        const below = BLOCKS.get(this.getBlock(x, ty - 1, z));
        if (below.id === 0 || below.liquid || below.cross) ty--; else break;
      }
      if (ty !== y) {
        this.setBlock(x, y, z, 0, 0);
        this.setBlock(x, ty, z, id, 0);
      }
    }
  }

  // Keep relighting cheap: recompute local light for chunks touched by edits
  relightDirty() {
    for (const ch of this.core.chunks.values()) {
      if (ch.lightDirty) { this.core.lighting.computeLocalLight(ch); ch.lightDirty = false; }
    }
  }

  unloadFar(players) {
    const cs = this.C.CHUNK_SIZE;
    for (const [key, ch] of this.core.chunks) {
      let near = false;
      for (const p of players) {
        const pcx = Math.floor(p.x / cs), pcy = Math.floor(p.y / cs), pcz = Math.floor(p.z / cs);
        if (Math.abs(ch.cx - pcx) <= UNLOAD_RADIUS_H && Math.abs(ch.cz - pcz) <= UNLOAD_RADIUS_H && Math.abs(ch.cy - pcy) <= UNLOAD_RADIUS_V) { near = true; break; }
      }
      if (!near) this.core.chunks.delete(key);
    }
  }

  updateTime(dt, dayLength) {
    this.time += dt / dayLength;
    if (this.time >= 1) { this.time -= 1; this.day++; }
    this.weatherTimer -= dt;
    if (this.weatherTimer <= 0) {
      const r = Math.random();
      if (this.weather === 'clear') {
        this.weather = r < 0.72 ? 'clear' : (r < 0.90 ? 'rain' : 'thunder');
        this.weatherTimer = 140 + Math.random() * 200;
      } else {
        this.weather = 'clear';
        this.weatherTimer = 220 + Math.random() * 260;
      }
    }
  }
}

// ============================================================================
// GAME
// ============================================================================

class Game {
  constructor(server, C, opts) {
    this.server = server;
    this.C = C;
    this.config = opts.config;
    this.world = new WorldState(C, opts.config, opts.worldPath, opts.atomicWriteJson);
    this.world.load();
    this.drops = new Map();
    this.nextDropId = 1;
    this.tntQueue = [];
    this.dayNight = new C.DayNightSystem();

    const noop = () => {};
    this.fx = { spawnBurst: noop, spawnSplash: noop, spawnBlockBurst: noop, spawn: noop, playSound: noop, playMaterial: noop };
    this.uiStub = { showDeath: noop, flashDamage: noop, refreshHotbar: noop, toast: noop, addChat: noop };
    this.serverMining = {
      detonateTNT: (x, y, z) => this.explode(x, y, z, 3.2, { tnt: true }),
      explodeAt: (x, y, z, r) => this.explode(x, y, z, r || 3.0, {}),
      spawnDrop: (x, y, z, id, count, dur, enchant) => this.spawnDrop(x, y, z, { id, count, dur, enchant })
    };
    // Stand-in "player" for damage without an attacker (explosions, fire)
    this.envActor = { sprinting: false, miningRef: this.serverMining, addXP: () => false };

    this.mobs = new C.MobManager(this.world.core);
    const origSpawnArrow = this.mobs.spawnArrow.bind(this.mobs);
    this.mobs.spawnArrow = (x, y, z, vx, vy, vz, owner, dmg, extra = null) => {
      origSpawnArrow(x, y, z, vx, vy, vz, owner, dmg, extra);
      const exclude = extra && extra.shooterId ? extra.shooterId : null;
      this._broadcastNear(x, z, 96, { type: 'arrow', x: +x.toFixed(2), y: +y.toFixed(2), z: +z.toFixed(2), vx: +vx.toFixed(2), vy: +vy.toFixed(2), vz: +vz.toFixed(2) }, exclude);
    };

    this.ServerPlayer = this._makeServerPlayerClass();
    this._snapAccum = 0;
    this._unloadAccum = 0;
  }

  _makeServerPlayerClass() {
    const game = this;
    const C = this.C;
    return class ServerPlayer extends C.Player {
      constructor(session, mode) {
        super('survival');
        this.mode = mode;
        this.flying = false;
        this.session = session;
        this.miningRef = game.serverMining;
        this._regen = 0;
        this._starve = 0;
        this._selfReport = false;
        this.statsDirty = true;
        this.enchSeed = (Math.random() * 4294967296) >>> 0;
      }
      hurt(dmg, reason, audio, ui, kx = 0, ky = 0, kz = 0) {
        const self = this._selfReport;
        this._selfReport = false;
        if (this.dead || !this.session.online) return false;
        const landed = super.hurt(dmg, reason, null, null, kx, ky, kz);
        if (landed) game.onPlayerHurt(this, reason, kx, ky, kz, self);
        return landed;
      }
      addXP(amt) {
        const r = super.addXP(amt);
        this.statsDirty = true;
        return r;
      }
    };
  }

  // ---------------------------------------------------------------- helpers

  get ac() { return this.server.ac; }
  get B() { return this.C.B; }

  onlinePlayers() { return Array.from(this.server.players.values()); }

  _broadcastNear(x, z, radius, obj, excludeId = null) {
    const raw = JSON.stringify(obj);
    for (const p of this.server.players.values()) {
      if (p.id === excludeId) continue;
      if (Math.abs(p.x - x) > radius || Math.abs(p.z - z) > radius) continue;
      p.ws.sendText(raw);
    }
  }

  _revert(p, x, y, z, reason) {
    p.ws.sendJSON({ type: 'block_revert', x, y, z, id: this.world.getBlock(x, y, z), meta: this.world.getMeta(x, y, z), reason });
  }

  // Validated copy of a stack: real item id, count within its stack size, sane durability/enchants
  cleanStack(s) {
    if (!s || typeof s !== 'object') return null;
    const id = s.id | 0;
    const it = this.C.ITEMS.get(id);
    if (!it) return null;
    const max = it.stack || 64;
    const count = Math.min(max, s.count | 0);
    if (count <= 0) return null;
    const out = { id, count };
    if (it.maxDur) out.dur = Number.isFinite(s.dur) ? Math.max(0, Math.min(it.maxDur, s.dur | 0)) : it.maxDur;
    const ench = this.ac._sanitizeEnchant(s.enchant);
    if (ench && this.C.enchKindsOf(it).length) out.enchant = ench;
    return out;
  }

  _cleanArray(arr, len) {
    const out = new Array(len).fill(null);
    if (Array.isArray(arr)) for (let i = 0; i < Math.min(len, arr.length); i++) out[i] = this.cleanStack(arr[i]);
    return out;
  }

  cleanBlockEntity(ent, blockId) {
    const type = CONTAINER_TYPES[blockId];
    if (!type || !ent || typeof ent !== 'object') return null;
    if (type === 'furnace') {
      return { type, input: this.cleanStack(ent.input), fuel: this.cleanStack(ent.fuel), output: this.cleanStack(ent.output), burnTime: 0, burnMax: 1, progress: 0 };
    }
    return { type, slots: this._cleanArray(ent.slots, 27) };
  }

  _entStacks(ent) {
    if (!ent) return [];
    if (ent.type === 'furnace') return [ent.input, ent.fuel, ent.output];
    return Array.isArray(ent.slots) ? ent.slots : [];
  }

  defaultEntity(blockId) {
    const type = CONTAINER_TYPES[blockId];
    if (type === 'furnace') return { type, input: null, fuel: null, output: null, burnTime: 0, burnMax: 1, progress: 0 };
    return { type, slots: new Array(27).fill(null) };
  }

  // The stack a player is holding; `claimedId` lets the client name the item it uses, but only
  // items the server knows the player owns are honoured.
  heldStack(p, claimedId) {
    const s = p.inv.slots[p.sel | 0];
    if (!claimedId) return s || null;
    if (s && s.id === claimedId) return s;
    for (const st of p.inv.slots) if (st && st.id === claimedId) return st;
    return null;
  }

  eyeOf(p) { return { x: p.x, y: p.y + (p.crouch ? 1.32 : 1.6), z: p.z }; }
  lookOf(p) {
    const cp = Math.cos(p.pitch || 0);
    return { x: -Math.sin(p.yaw || 0) * cp, y: Math.sin(p.pitch || 0), z: -Math.cos(p.yaw || 0) * cp };
  }

  nearBlock(p, ids, r) {
    const bx = Math.floor(p.x), by = Math.floor(p.y), bz = Math.floor(p.z);
    for (let dy = -r; dy <= r; dy++) for (let dz = -r; dz <= r; dz++) for (let dx = -r; dx <= r; dx++) {
      if (ids.includes(this.world.getBlock(bx + dx, by + dy, bz + dz))) return true;
    }
    return false;
  }

  isSurvival(p) { return p.mode !== 'creative'; }

  // ---------------------------------------------------------------- players

  initPlayer(p, saved) {
    const pl = new this.ServerPlayer(p, p.mode);
    pl.x = p.x; pl.y = p.y; pl.z = p.z;
    if (saved) {
      pl.health = Number.isFinite(saved.health) ? Math.max(0, Math.min(20, saved.health)) : 20;
      pl.hunger = Number.isFinite(saved.hunger) ? Math.max(0, Math.min(20, saved.hunger)) : 20;
      pl.saturation = Number.isFinite(saved.saturation) ? Math.max(0, Math.min(20, saved.saturation)) : 5;
      pl.xp = Number.isFinite(saved.xp) ? Math.max(0, saved.xp) : 0;
      pl.level = Number.isFinite(saved.level) ? Math.max(0, saved.level | 0) : 0;
      pl.dead = !!saved.dead || pl.health <= 0;
    }
    p.pl = pl;
    p.sel = 0;
    const slots = this._cleanArray(saved && saved.slots, 36);
    const armor = this._cleanArray(saved && saved.armor, 5);
    // Items that were on the cursor / in a crafting grid when the player left go back into the inventory
    if (saved && Array.isArray(saved.limbo)) {
      const inv = new this.C.Inventory();
      inv.slots = slots;
      for (const s of saved.limbo) {
        const c = this.cleanStack(s);
        if (c) inv.addItem(c.id, c.count, c.dur, c.enchant);
      }
    }
    this._setInventory(p, slots, armor, new Array(12).fill(null));
  }

  _setInventory(p, slots, armor, limbo) {
    p.inv = { slots, armor, limbo };
    p.pl.inventory.slots = slots;
    p.pl.inventory.armor = armor;
    p.pl.inventory.selected = p.sel | 0;
    p.ledger = Ledger.fromStacks(this.C, [slots, armor, limbo]);
  }

  sendInventory(p) {
    p.ws.sendJSON({ type: 'inv_set', slots: p.inv.slots, armor: p.inv.armor, limbo: p.inv.limbo });
  }

  profileOf(p) {
    const pl = p.pl;
    return {
      name: p.name,
      x: +p.x.toFixed(3), y: +p.y.toFixed(3), z: +p.z.toFixed(3),
      health: pl.health, hunger: pl.hunger, saturation: pl.saturation, xp: pl.xp, level: pl.level, dead: !!pl.dead,
      slots: p.inv.slots, armor: p.inv.armor, limbo: p.inv.limbo.filter(Boolean), mode: p.mode,
      lastSeen: Date.now()
    };
  }

  statsOf(p) {
    const pl = p.pl;
    return { type: 'stats', h: +pl.health.toFixed(2), f: +pl.hunger.toFixed(2), s: +pl.saturation.toFixed(2), x: pl.xp, l: pl.level, d: !!pl.dead, e: pl.enchSeed };
  }

  sendStats(p) {
    p.pl.statsDirty = false;
    p.lastStatsMs = Date.now();
    p.ws.sendJSON(this.statsOf(p));
  }

  welcomeExtras(p) {
    const drops = [];
    for (const d of this.drops.values()) drops.push([d.id, d.x, d.y, d.z, d.s]);
    return { stats: this.statsOf(p), drops, mobTypes: this.C.MOB_TYPES };
  }

  onPlayerHurt(pl, reason, kx, ky, kz, selfReported) {
    const p = pl.session;
    pl.statsDirty = true;
    p.ac.knockbackUntilMs = Date.now() + 900;
    if (!selfReported) {
      p.ws.sendJSON({ type: 'damage', h: +pl.health.toFixed(2), kx: +(kx || 0).toFixed(2), ky: +(ky || 0).toFixed(2), kz: +(kz || 0).toFixed(2), r: reason });
    }
    if (pl.dead) {
      this.server.broadcastChat(`${p.name}: ${reason || 'died.'}`, 'sys');
    }
    this.sendStats(p);
  }

  setMode(p, mode) {
    p.mode = mode;
    p.pl.mode = mode;
    // Switching modes re-bases the ledger on whatever the client holds next
    p.acceptNextSync = true;
  }

  // ---------------------------------------------------------------- drops

  settleY(x, y, z) {
    const bx = Math.floor(x), bz = Math.floor(z);
    const BLOCKS = this.C.BLOCKS;
    for (let yy = Math.floor(y); yy > Math.floor(y) - 48; yy--) {
      if (BLOCKS.get(this.world.getBlock(bx, yy - 1, bz)).solid) return yy;
    }
    return y;
  }

  spawnDrop(x, y, z, stack, opts = {}) {
    const s = this.cleanStack(stack);
    if (!s) return null;
    const id = this.nextDropId++;
    const vx = opts.vx !== undefined ? opts.vx : (Math.random() - 0.5) * 2.4;
    const vy = opts.vy !== undefined ? opts.vy : 3.2 + Math.random() * 1.4;
    const vz = opts.vz !== undefined ? opts.vz : (Math.random() - 0.5) * 2.4;
    const d = { id, s, x: +x.toFixed(2), y: +y.toFixed(2), z: +z.toFixed(2), gy: this.settleY(x, y, z), born: Date.now(), delay: opts.delay || 0.4 };
    this.drops.set(id, d);
    if (this.drops.size > MAX_DROPS) {
      const oldest = this.drops.keys().next().value;
      this.drops.delete(oldest);
      this.server.broadcast({ type: 'drop_remove', id: oldest });
    }
    this.server.broadcast({ type: 'drop_spawn', id, x: d.x, y: d.y, z: d.z, vx: +vx.toFixed(2), vy: +vy.toFixed(2), vz: +vz.toFixed(2), s, delay: d.delay });
    return d;
  }

  handlePickup(p, msg) {
    const d = this.drops.get(msg.id | 0);
    if (!d || p.pl.dead) { p.ws.sendJSON({ type: 'drop_remove', id: msg.id | 0 }); return; }
    const age = (Date.now() - d.born) / 1000;
    const dy = p.y - d.gy;
    if (age < d.delay - 0.25 || Math.hypot(p.x - d.x, p.z - d.z) > 6 || dy < -6 || dy > 6) {
      p.ws.sendJSON({ type: 'pickup_fail', id: d.id });
      return;
    }
    this.drops.delete(d.id);
    if (this.isSurvival(p)) p.ledger.add(d.s);
    p.ws.sendJSON({ type: 'pickup_ok', id: d.id, s: d.s });
    this.server.broadcast({ type: 'drop_remove', id: d.id, by: p.id }, p.id);
  }

  handleDropItem(p, msg) {
    const s = this.cleanStack(msg.s);
    if (!s || p.pl.dead) return;
    const src = msg.src;
    if (src && Number.isInteger(src.x)) {
      // Dropped straight out of an open container slot
      const ent = this.world.blockEntities.get(`${src.x},${src.y},${src.z}`);
      if (!ent || !this.ac.checkReach(p, src.x, src.y, src.z).ok) return;
      const k = stackKey(s);
      let left = s.count;
      const take = (st) => { if (!st || stackKey(st) !== k || left <= 0) return st; const t = Math.min(st.count, left); st.count -= t; left -= t; return st.count > 0 ? st : null; };
      if (ent.type === 'furnace') { ent.input = take(ent.input); ent.fuel = take(ent.fuel); ent.output = take(ent.output); }
      else for (let i = 0; i < ent.slots.length; i++) ent.slots[i] = take(ent.slots[i]);
      if (left === s.count) return;
      s.count -= left;
      this.world.dirty = true;
    } else if (this.isSurvival(p)) {
      const removed = p.ledger.remove(stackKey(s), s.count, s.dur);
      if (!removed) { this.sendInventory(p); return; }
      if (removed.length) s.dur = removed[0];
    }
    const eye = this.eyeOf(p), dir = this.lookOf(p);
    this.spawnDrop(eye.x + dir.x * 0.6, eye.y - 0.3, eye.z + dir.z * 0.6, s, { vx: dir.x * 5.5, vy: dir.y * 5 + 2.2, vz: dir.z * 5.5, delay: 1.5 });
  }

  // ---------------------------------------------------------------- explosions

  explode(x, y, z, radius, opts = {}) {
    const B = this.B;
    x |= 0; y |= 0; z |= 0;
    if (opts.tnt) {
      if (this.world.getBlock(x, y, z) !== B.TNT) return;
      this.world.setBlock(x, y, z, B.AIR, 0);
    }
    const cells = [];
    const r = Math.ceil(radius);
    for (let dy = -r; dy <= r; dy++) {
      for (let dz = -r; dz <= r; dz++) {
        for (let dx = -r; dx <= r; dx++) {
          if ((dx === 0 && dy === 0 && dz === 0) || dx * dx + dy * dy + dz * dz > radius * radius) continue;
          const bx = x + dx, by = y + dy, bz = z + dz;
          const id = this.world.getBlock(bx, by, bz);
          if (id === B.AIR || id === B.BEDROCK || id === B.OBSIDIAN || id === B.OBSIDIAN_CRYING) continue;
          if (id === B.TNT) { this.queueTnt(bx, by, bz); continue; }
          const ent = this.world.blockEntities.get(`${bx},${by},${bz}`);
          for (const s of this._entStacks(ent)) if (s) this.spawnDrop(bx + 0.5, by + 0.4, bz + 0.5, s);
          cells.push([dx, dy, dz]);
          this.world.setBlock(bx, by, bz, B.AIR, 0);
        }
      }
    }
    this.server.broadcast({ type: 'explosion', x, y, z, radius: Math.round(radius * 10), cells });
    this.server.scheduleWorldSave();

    const cx = x + 0.5, cy = y + 0.5, cz = z + 0.5;
    for (const p of this.onlinePlayers()) {
      if (p.mode === 'creative' || p.pl.dead) continue;
      const pdx = p.x - cx, pdy = (p.y + 0.9) - cy, pdz = p.z - cz;
      const dist = Math.hypot(pdx, pdy, pdz) || 0.1;
      if (dist >= radius + 3.5) continue;
      const k = 1 - dist / (radius + 3.5);
      const kx = (pdx / dist) * k * 14, ky = Math.max(4.5, (pdy / dist) * k * 10 + 5.5), kz = (pdz / dist) * k * 14;
      p.pl.hurt(Math.round(k * 12), 'Blew up in an explosion.', null, null, kx, ky, kz);
    }
    for (const m of this.mobs.mobs) {
      if (m.dying) continue;
      const mdx = m.x - cx, mdy = (m.y + 0.6) - cy, mdz = m.z - cz;
      const md = Math.hypot(mdx, mdy, mdz) || 0.1;
      if (md < radius + 4.0) {
        const mk = 1 - md / (radius + 4.0);
        this.mobs._damageMob(m, Math.round(mk * 16), (mdx / md) * mk * 12, 6.5, (mdz / md) * mk * 12, opts.by || this.envActor, this.fx, this.fx, this.uiStub);
      }
    }
  }

  queueTnt(x, y, z) {
    if (this.tntQueue.length > 512 || this.tntQueue.some(q => q.x === x && q.y === y && q.z === z)) return;
    this.tntQueue.push({ x, y, z, t: 0.35 + Math.random() * 0.4 });
  }

  // ---------------------------------------------------------------- redstone

  // Runs fn while collecting every block change it causes, then relays them to everyone but `exceptId`
  withChanges(fn, exceptId = null) {
    const outer = this.world.changeLog;
    this.world.changeLog = [];
    try { fn(); } finally {
      const changes = this.world.changeLog;
      this.world.changeLog = outer;
      if (outer) outer.push(...changes);
      else if (changes.length) this.server.broadcast({ type: 'blocks', list: changes }, exceptId);
      if (changes.length) this.server.scheduleWorldSave();
    }
  }

  // ---------------------------------------------------------------- tick

  tick(dt) {
    const players = this.onlinePlayers();
    const C = this.C;
    const cs = C.CHUNK_SIZE;

    // 1. Keep the world loaded around every player (budgeted so joins don't stall the loop)
    let budget = 8;
    for (const p of players) {
      const pcx = Math.floor(p.x / cs), pcy = Math.floor(p.y / cs), pcz = Math.floor(p.z / cs);
      for (let r = 0; r <= LOAD_RADIUS_H && budget > 0; r++) {
        for (let dx = -r; dx <= r && budget > 0; dx++) for (let dz = -r; dz <= r && budget > 0; dz++) {
          if (Math.max(Math.abs(dx), Math.abs(dz)) !== r) continue;
          for (let dy = -LOAD_RADIUS_V; dy <= LOAD_RADIUS_V && budget > 0; dy++) {
            if (!this.world.core.chunks.has(`${pcx + dx},${pcy + dy},${pcz + dz}`)) {
              this.world.ensureChunk(pcx + dx, pcy + dy, pcz + dz);
              budget--;
            }
          }
        }
      }
    }
    this._unloadAccum += dt;
    if (this._unloadAccum > 10) {
      this._unloadAccum = 0;
      if (players.length) this.world.unloadFar(players);
      this.world.relightDirty();
    }

    // 2. Block simulation: redstone timers, furnaces, chained TNT
    this.withChanges(() => {
      this.world.core.redstone.update(dt, this.serverMining);
      let burning = false;
      for (const ent of this.world.blockEntities.values()) if (ent.type === 'furnace' && ent.burnTime > 0) { burning = true; break; }
      this.world.core.updateFurnaces(dt);
      if (burning) this.world.dirty = true;
    });
    for (let i = this.tntQueue.length - 1; i >= 0; i--) {
      const q = this.tntQueue[i];
      q.t -= dt;
      if (q.t <= 0) { this.tntQueue.splice(i, 1); this.explode(q.x, q.y, q.z, 3.2, { tnt: true }); }
    }
    this.world.processGravity();
    this._cropAccum = (this._cropAccum || 0) + dt;
    if (this._cropAccum >= 0.35) {
      this._cropAccum = 0;
      this.withChanges(() => { for (const p of players) this._randomTicks(p); });
    }

    // 3. Mobs (shared by all players)
    this.dayNight.time = this.world.time;
    this.dayNight.update(0, null);
    const pls = players.map(p => p.pl);
    if (pls.length) this.mobs.update(dt, pls, this.dayNight, this.config.difficulty || 'normal', this.fx, this.fx, this.uiStub);
    else this.mobs.mobs.length = 0;

    // 4. Player vitals: hurt cooldown, natural regeneration, starvation, stat pushes
    const now = Date.now();
    for (const p of players) {
      const pl = p.pl;
      if (pl.hurtCooldown > 0) pl.hurtCooldown -= dt;
      if (!pl.dead && p.mode === 'survival') {
        if (pl.hunger >= 17 && pl.health < pl.maxHealth) {
          pl._regen += dt;
          if (pl._regen >= 2.2) {
            pl._regen = 0;
            pl.health = Math.min(pl.maxHealth, pl.health + 1);
            pl.hunger = Math.max(0, pl.hunger - 0.35);
            pl.statsDirty = true;
          }
        } else if (pl.hunger <= 0) {
          pl._starve += dt;
          if (pl._starve >= 3.5) { pl._starve = 0; pl._selfReport = true; pl.hurt(1, 'Starved to death.'); }
        } else {
          pl._regen = 0;
        }
      }
      if (pl.statsDirty && now - (p.lastStatsMs || 0) > 200) this.sendStats(p);
    }

    // 5. Dropped items expire
    for (const [id, d] of this.drops) {
      if (now - d.born > DROP_LIFETIME_MS) { this.drops.delete(id); this.server.broadcast({ type: 'drop_remove', id }); }
    }

    // 6. Mob snapshots @ 10 Hz, only mobs near each player
    this._snapAccum += dt;
    if (this._snapAccum >= 0.1) {
      this._snapAccum = 0;
      const typeIdx = new Map(C.MOB_TYPES.map((t, i) => [t, i]));
      for (const p of players) {
        const list = [];
        for (const m of this.mobs.mobs) {
          if (Math.abs(m.x - p.x) > 90 || Math.abs(m.z - p.z) > 90) continue;
          const flags = (m.hurtTimer > 0 ? 1 : 0) | (m.dying ? 2 : 0) | (m.baby ? 4 : 0) | (m.arctic ? 8 : 0) | ((m.fireTimer || 0) > 0 ? 16 : 0);
          list.push([m.id, typeIdx.get(m.type), +m.x.toFixed(2), +m.y.toFixed(2), +m.z.toFixed(2), +m.yaw.toFixed(2), +m.headYaw.toFixed(2), +m.headPitch.toFixed(2), flags, +(m.fuse || 0).toFixed(2), m.woolIdx | 0]);
        }
        p.ws.sendJSON({ type: 'mobs', list });
      }
    }
  }

  // Crop growth & farmland hydration around a player (same rules the singleplayer client uses)
  _randomTicks(p) {
    const B = this.B, w = this.world;
    const px = Math.floor(p.x), py = Math.floor(p.y), pz = Math.floor(p.z);
    for (let k = 0; k < 14; k++) {
      const wx = px + Math.floor((Math.random() - 0.5) * 28);
      const wz = pz + Math.floor((Math.random() - 0.5) * 28);
      const wy = py + Math.floor((Math.random() - 0.5) * 14);
      const id = w.getBlock(wx, wy, wz);
      if (id === B.FARMLAND) {
        let nearW = false;
        for (let dz = -4; dz <= 4 && !nearW; dz++) for (let dx = -4; dx <= 4 && !nearW; dx++) {
          if (w.getBlock(wx + dx, wy, wz + dz) === B.WATER) nearW = true;
        }
        if (nearW && (w.getMeta(wx, wy, wz) & 1) === 0) w.setBlock(wx, wy, wz, B.FARMLAND, 1);
      } else if (id === B.WHEAT) {
        const m = w.getMeta(wx, wy, wz);
        if (m < 3 && Math.random() < 0.28) {
          const wet = w.getBlock(wx, wy - 1, wz) === B.FARMLAND && (w.getMeta(wx, wy - 1, wz) & 1);
          if (wet || Math.random() < 0.4) w.setBlock(wx, wy, wz, B.WHEAT, m + 1);
        }
      }
    }
  }

  // ---------------------------------------------------------------- packet handling

  // Returns true when the packet type belongs to the game layer
  handle(p, msg) {
    if (typeof msg.type !== 'string' || msg.type.startsWith('_') || !Object.prototype.hasOwnProperty.call(HANDLERS, msg.type)) return false;
    const fn = this.handlers[msg.type];
    if (!fn) return false;
    fn.call(this, p, msg);
    return true;
  }

  get handlers() {
    return HANDLERS;
  }
}

// Every handler runs with `this` = Game, (p = session, msg = packet)
const HANDLERS = {
  move(p, msg) {
    const pl = p.pl;
    pl.x = p.x; pl.y = p.y; pl.z = p.z; pl.yaw = p.yaw; pl.pitch = p.pitch;
    pl.crouching = !!msg.crouch;
    pl.sprinting = !!msg.sprint;
    pl.blocking = !!msg.block;
    pl.onGround = !!msg.onGround;
    if (Number.isInteger(msg.sel) && msg.sel >= 0 && msg.sel < 9) { p.sel = msg.sel; pl.inventory.selected = msg.sel; }
    // Hunger only ever goes down from client reports (movement drain); food raises it via 'eat'
    if (Number.isFinite(msg.f) && msg.f < pl.hunger) { pl.hunger = Math.max(0, msg.f); }
    if (Number.isFinite(msg.s) && msg.s < pl.saturation) { pl.saturation = Math.max(0, msg.s); }
  },

  inv_sync(p, msg) {
    const slots = this._cleanArray(msg.slots, 36);
    const armor = this._cleanArray(msg.armor, 5);
    const limbo = this._cleanArray(msg.limbo, 12);
    for (let i = 0; i < 5; i++) {
      const it = armor[i] && this.C.ITEMS.get(armor[i].id);
      if (armor[i] && (!it || it.armorSlot !== i)) {
        // Not wearable in that slot: move it into the first free inventory slot instead
        const free = slots.indexOf(null);
        if (free >= 0) slots[free] = armor[i];
        armor[i] = null;
      }
    }
    let changed = false;
    if (this.isSurvival(p) && !p.acceptNextSync) changed = p.ledger.clamp([slots, armor, limbo]);
    p.acceptNextSync = false;
    this._setInventory(p, slots, armor, limbo);
    if (changed) {
      this.ac.flag(p, 'INV_MISMATCH', 1.5, 'Inventory contained items the server never granted (corrected)');
      this.sendInventory(p);
    }
    this.server._savePlayerProfile(p);
  },

  pickup(p, msg) { this.handlePickup(p, msg); },
  drop_item(p, msg) { this.handleDropItem(p, msg); },

  self_damage(p, msg) {
    if (!this.isSurvival(p) || p.pl.dead) return;
    const reason = String(msg.r || '');
    if (!SELF_DAMAGE_REASONS.has(reason)) return;
    const amt = Number(msg.a);
    if (!Number.isFinite(amt) || amt <= 0) return;
    p.pl._selfReport = true;
    p.pl.hurt(Math.min(40, amt), reason);
  },

  respawn(p) {
    const now = Date.now();
    const pl = p.pl;
    if (pl.dead) {
      pl.dead = false;
      pl.health = pl.maxHealth; pl.hunger = pl.maxHunger; pl.saturation = 5;
      pl.statsDirty = true;
      this.ac.grantTeleportGrace(p, 2500);
      this.sendStats(p);
      return;
    }
    // Not dead: still allow the occasional teleport grace (client/server death mismatch), rate-limited
    if (now - (p.lastRespawnMs || 0) < 10000) return;
    p.lastRespawnMs = now;
    this.ac.grantTeleportGrace(p, 2500);
  },

  eat(p, msg) {
    const it = this.C.ITEMS.get(msg.id | 0);
    if (!it || !it.food || p.pl.dead) return;
    const now = Date.now();
    if (now - (p.lastEatMs || 0) < 900) return;
    p.lastEatMs = now;
    if (this.isSurvival(p)) {
      if (!p.ledger.remove(`${it.id}#`, 1)) { this.sendInventory(p); return; }
    }
    const pl = p.pl;
    pl.hunger = Math.min(pl.maxHunger, pl.hunger + it.food);
    pl.saturation = Math.min(20, pl.saturation + (it.sat || 2));
    if (it.heal) pl.health = Math.min(pl.maxHealth, pl.health + it.heal);
    this.sendStats(p);
  },

  craft(p, msg) {
    const C = this.C;
    const r = C.CRAFTING.recipes[msg.r | 0];
    const times = Math.max(1, Math.min(64, msg.times | 0));
    if (!r) return;
    if (r.needsTable && !this.nearBlock(p, [this.B.CRAFTING_TABLE, this.B.WORKSTATION], 6)) {
      this.ac.flag(p, 'CRAFT', 2, `Crafted #${r.outId} without a crafting table nearby`);
      this.sendInventory(p);
      return;
    }
    if (!this.isSurvival(p)) return;
    // Verify the reported ingredients satisfy the recipe exactly
    const used = [];
    for (const u of Array.isArray(msg.used) ? msg.used.slice(0, 16) : []) {
      if (Array.isArray(u) && C.ITEMS.get(u[0] | 0) && (u[1] | 0) > 0) used.push([u[0] | 0, u[1] | 0]);
    }
    const pool = used.map(u => u.slice());
    for (const [req, need] of r.ingredients) {
      let left = need * times;
      for (const u of pool) {
        if (left <= 0) break;
        if (u[1] > 0 && C.CRAFTING.matchesIngredient(u[0], req)) { const t = Math.min(u[1], left); u[1] -= t; left -= t; }
      }
      if (left > 0) { this.sendInventory(p); return; }
    }
    if (pool.some(u => u[1] > 0)) { this.sendInventory(p); return; }
    for (const [id, cnt] of used) if (p.ledger.count(`${id}#`) < cnt) { this.sendInventory(p); return; }
    for (const [id, cnt] of used) p.ledger.remove(`${id}#`, cnt);
    p.ledger.add({ id: r.outId, count: r.outCount * times });
  },

  trade(p, msg) {
    const tr = this.C.TRADER_TRADES[msg.idx | 0];
    if (!tr) return;
    const near = this.mobs.mobs.some(m => m.type === 'trader' && !m.dying && Math.hypot(m.x - p.x, m.z - p.z) < 7);
    if (!near) { this.sendInventory(p); return; }
    if (!this.isSurvival(p)) return;
    if (!p.ledger.remove(`${tr.give[0]}#`, tr.give[1])) { this.sendInventory(p); return; }
    p.ledger.add({ id: tr.get[0], count: tr.get[1] });
  },

  enchant(p, msg) {
    const C = this.C;
    const x = msg.x | 0, y = msg.y | 0, z = msg.z | 0;
    if (this.world.getBlock(x, y, z) !== this.B.ENCHANT_TABLE || !this.ac.checkReach(p, x, y, z).ok) { this.sendInventory(p); return; }
    const itemId = msg.itemId | 0;
    const pl = p.pl;
    const power = C.computeEnchantPower(this.world.core, x, y, z);
    const offers = C.computeEnchantOffers({ id: itemId, count: 1 }, power, pl.enchSeed);
    const off = offers[msg.offer | 0];
    if (!off || !off.list.length) { this.sendInventory(p); this.sendStats(p); return; }
    if (this.isSurvival(p)) {
      if (pl.level < off.cost || p.ledger.count(`${C.IT.LAPIS}#`) < off.lapis || p.ledger.count(`${itemId}#`) < 1) {
        this.sendInventory(p); this.sendStats(p); return;
      }
      const durs = p.ledger.remove(`${itemId}#`, 1, Number(msg.dur));
      p.ledger.remove(`${C.IT.LAPIS}#`, off.lapis);
      p.ledger.add({ id: itemId, count: 1, dur: durs && durs.length ? durs[0] : undefined, enchant: C.makeEnchant(off.list) });
      pl.level = Math.max(0, pl.level - off.lapis);
    }
    pl.enchSeed = (Math.random() * 4294967296) >>> 0;
    this.sendStats(p);
  },

  cook(p, msg) {
    const x = msg.x | 0, y = msg.y | 0, z = msg.z | 0;
    const out = this.C.CAMPFIRE_COOK[msg.id | 0];
    if (!out || this.world.getBlock(x, y, z) !== this.B.CAMPFIRE || !this.ac.checkReach(p, x, y, z).ok) { this.sendInventory(p); return; }
    if (!this.isSurvival(p)) return;
    if (!p.ledger.remove(`${msg.id | 0}#`, 1)) { this.sendInventory(p); return; }
    p.ledger.add({ id: out, count: 1 });
  },

  container_open(p, msg) {
    const x = msg.x | 0, y = msg.y | 0, z = msg.z | 0;
    const id = this.world.getBlock(x, y, z);
    if (!CONTAINER_TYPES[id] || !this.ac.checkReach(p, x, y, z).ok) return;
    const key = `${x},${y},${z}`;
    let ent = this.world.blockEntities.get(key);
    if (!ent) { ent = this.defaultEntity(id); this.world.setBlockEntity(x, y, z, ent); }
    p.ws.sendJSON({ type: 'block_entity', x, y, z, ent, open: true });
  },

  container_commit(p, msg) {
    const x = msg.x | 0, y = msg.y | 0, z = msg.z | 0;
    const id = this.world.getBlock(x, y, z);
    const key = `${x},${y},${z}`;
    const oldEnt = this.world.blockEntities.get(key) || (CONTAINER_TYPES[id] ? this.defaultEntity(id) : null);
    const reach = this.ac.checkReach(p, x, y, z);
    const newEnt = this.cleanBlockEntity(msg.ent, id);
    if (!oldEnt || !newEnt || reach.dist > reach.maxR + 2) {
      // Not a container (any more): treat as a plain inventory sync
      HANDLERS.inv_sync.call(this, p, msg);
      return;
    }
    const slots = this._cleanArray(msg.slots, 36);
    const armor = this._cleanArray(msg.armor, 5);
    const limbo = this._cleanArray(msg.limbo, 12);
    const newEntStacks = this._entStacks(newEnt);
    let changed = false;
    if (this.isSurvival(p) && !p.acceptNextSync) {
      // Conservation: (inventory + container) after <= (ledger + container) before
      const pool = Ledger.fromStacks(this.C, [this._entStacks(oldEnt)]);
      for (const e of p.ledger.m.values()) {
        const md = pool._maxDur(e.proto.id);
        if (md) for (const d of e.durs) pool.add({ id: e.proto.id, count: 1, dur: d, enchant: e.proto.enchant });
        else if (e.count > 0) pool.add({ id: e.proto.id, count: e.count, enchant: e.proto.enchant });
      }
      changed = pool.clamp([slots, armor, limbo, newEntStacks]);
    }
    p.acceptNextSync = false;
    if (newEnt.type === 'furnace') {
      newEnt.input = newEntStacks[0]; newEnt.fuel = newEntStacks[1]; newEnt.output = newEntStacks[2];
      newEnt.burnTime = oldEnt.burnTime || 0; newEnt.burnMax = oldEnt.burnMax || 1; newEnt.progress = oldEnt.progress || 0;
    }
    this.world.setBlockEntity(x, y, z, newEnt);
    this._setInventory(p, slots, armor, limbo);
    this.server.broadcast({ type: 'block_entity', x, y, z, ent: newEnt }, p.id);
    if (changed) {
      this.ac.flag(p, 'INV_MISMATCH', 1.5, `Container transfer at (${x},${y},${z}) created items (corrected)`);
      this.sendInventory(p);
      p.ws.sendJSON({ type: 'block_entity', x, y, z, ent: newEnt });
    }
    this.server._savePlayerProfile(p);
    this.server.scheduleWorldSave();
  },

  block_break(p, msg) {
    const B = this.B, C = this.C;
    const x = msg.x | 0, y = msg.y | 0, z = msg.z | 0;
    if (p.pl.dead) { this._revert(p, x, y, z); return; }
    const id = this.world.getBlock(x, y, z);
    const meta = this.world.getMeta(x, y, z);
    if (msg.bucket) { HANDLERS._bucketFill.call(this, p, x, y, z, id, meta); return; }
    if (id === B.AIR) return; // nothing here server-side (e.g. a client-only flowing fluid)
    const blk = C.BLOCKS.get(id);
    const creative = p.mode === 'creative';

    if (this.ac.isEnabled()) {
      const reach = this.ac.checkReach(p, x, y, z);
      if (!reach.ok) {
        this._revert(p, x, y, z, `Reach too far (${reach.dist.toFixed(1)}m).`);
        this.ac.flag(p, 'REACH', 4, `Block break reach ${reach.dist.toFixed(1)}m > ${reach.maxR}m`);
        return;
      }
      if (!creative) {
        if (blk.hardness >= 9999 || blk.liquid) {
          this._revert(p, x, y, z, `Cannot break ${blk.name}.`);
          this.ac.flag(p, 'UNBREAKABLE', 6, `Tried to break ${blk.name} at (${x},${y},${z})`);
          return;
        }
        const now = Date.now();
        p.ac.breakTimestamps = p.ac.breakTimestamps.filter(t => now - t < 1000);
        p.ac.breakTimestamps.push(now);
        if (p.ac.breakTimestamps.length > (this.config.anticheat.maxBlocksPerSec || 12)) {
          this._revert(p, x, y, z, 'Breaking blocks too fast (Nuker).');
          this.ac.flag(p, 'NUKER', 5, `Broke ${p.ac.breakTimestamps.length} blocks/sec`);
          return;
        }
        // Fast-break: the real mining time from the real block and the tool the player owns,
        // checked against both the reported dig time and the server-measured gap between breaks.
        const held = this.heldStack(p, msg.toolId | 0);
        const hit = held ? C.ITEMS.get(held.id) : null;
        let speed = 1.0;
        if (hit && hit.toolType === blk.tool) speed = (hit.speed || 2.0) * (1 + C.enchLvl(held, 'Efficiency') * 0.32);
        else if (blk.minTier > 0) speed = 0.28;
        const mineMs = Math.max(0.06, blk.hardness / speed) * 1000;
        const minMs = mineMs * 0.55 - 80;
        const gap = now - (p.lastBreakMs || 0);
        p.lastBreakMs = now;
        if (mineMs > 250 && (gap < minMs || (Number(msg.mineMs) || 0) < minMs)) {
          this._revert(p, x, y, z, `Fast-Break detected on ${blk.name}.`);
          this.ac.flag(p, 'FAST_BREAK', 4.5, `Broke ${blk.name} after ${Math.min(gap, Number(msg.mineMs) || 0)}ms (min ${Math.round(minMs)}ms)`);
          return;
        }
      }
    }

    const ent = this.world.blockEntities.get(`${x},${y},${z}`);
    this.world.setBlock(x, y, z, B.AIR, 0);
    this.server.broadcast({ type: 'block_set', x, y, z, id: B.AIR, meta: 0 }, p.id);
    if (blk.shape === 'door') {
      const py = (meta & 8) ? y - 1 : y + 1;
      if (this.world.getBlock(x, py, z) === id) {
        this.world.setBlock(x, py, z, B.AIR, 0);
        this.server.broadcast({ type: 'block_set', x, y: py, z, id: B.AIR, meta: 0 }, p.id);
      }
    }
    for (const s of this._entStacks(ent)) if (s) this.spawnDrop(x + 0.5, y + 0.4, z + 0.5, s);
    if (!creative) {
      const held = this.heldStack(p, msg.toolId | 0);
      const res = C.computeBlockDrops(blk, meta, held);
      for (const d of res.drops) this.spawnDrop(x + 0.5, y + 0.4, z + 0.5, d);
      if (res.xp) p.pl.addXP(res.xp);
    }
    this.server.scheduleWorldSave();
  },

  _bucketFill(p, x, y, z, id, meta) {
    const B = this.B, IT = this.C.IT;
    if ((id !== B.WATER && id !== B.LAVA) || (meta & 7) !== 0 || !this.ac.checkReach(p, x, y, z).ok) {
      this._revert(p, x, y, z);
      if (this.isSurvival(p)) this.sendInventory(p);
      return;
    }
    if (this.isSurvival(p)) {
      if (!p.ledger.remove(`${IT.BUCKET}#`, 1)) { this._revert(p, x, y, z); this.sendInventory(p); return; }
      p.ledger.add({ id: id === B.WATER ? IT.WATER_BUCKET : IT.LAVA_BUCKET, count: 1 });
    }
    this.world.setBlock(x, y, z, B.AIR, 0);
    this.server.broadcast({ type: 'block_set', x, y, z, id: B.AIR, meta: 0 }, p.id);
    this.server.scheduleWorldSave();
  },

  block_place(p, msg) {
    const B = this.B, C = this.C, IT = C.IT;
    const x = msg.x | 0, y = msg.y | 0, z = msg.z | 0;
    const id = msg.id | 0;
    const meta = (msg.meta | 0) & 255;
    const cause = String(msg.cause || '');
    const blk = C.BLOCKS.blocks[id];
    if (p.pl.dead || !blk || id === B.AIR) { this._revert(p, x, y, z); return; }
    const reach = this.ac.checkReach(p, x, y, z);
    if (!reach.ok) {
      this._revert(p, x, y, z, `Place reach too far (${reach.dist.toFixed(1)}m).`);
      this.ac.flag(p, 'REACH', 4, `Block place reach ${reach.dist.toFixed(1)}m > ${reach.maxR}m`);
      return;
    }
    const cur = this.world.getBlock(x, y, z);
    const curB = C.BLOCKS.get(cur);
    const creative = p.mode === 'creative';
    let take = null, give = null, needFree = true;

    if (cause === 'hoe') {
      const hoe = this.heldStack(p, msg.toolId | 0);
      const hoeIt = hoe && C.ITEMS.get(hoe.id);
      if (id !== B.FARMLAND || ![B.GRASS, B.DIRT, B.COARSE_DIRT, B.PODZOL].includes(cur) || (!creative && !(hoeIt && hoeIt.toolType === 'hoe'))) { this._revert(p, x, y, z); return; }
      needFree = false;
    } else if (cause === 'bonemeal') {
      if (id !== B.WHEAT || cur !== B.WHEAT) { this._revert(p, x, y, z); return; }
      take = IT.BONE_MEAL; needFree = false;
    } else if (id === B.WATER) { take = IT.WATER_BUCKET; give = IT.BUCKET; }
    else if (id === B.LAVA) { take = IT.LAVA_BUCKET; give = IT.BUCKET; }
    else if (id === B.REDSTONE_WIRE) { take = IT.REDSTONE; }
    else if (id === B.WHEAT) {
      if (this.world.getBlock(x, y - 1, z) !== B.FARMLAND) { this._revert(p, x, y, z); return; }
      take = IT.WHEAT_SEEDS;
    } else if (blk.shape === 'door' && (meta & 8)) {
      if (this.world.getBlock(x, y - 1, z) !== id) { this._revert(p, x, y, z); return; } // top half of a door just placed
    } else if (id === B.FARMLAND) {
      this._revert(p, x, y, z); return;
    } else {
      const it = C.ITEMS.get(id);
      if (!it || it.blockId !== id) { this._revert(p, x, y, z); return; }
      take = id;
    }

    if (needFree && cur !== B.AIR && !curB.liquid && !curB.cross) { this._revert(p, x, y, z); return; }
    if (!creative && take) {
      if (!p.ledger.remove(`${take}#`, 1)) {
        this._revert(p, x, y, z, 'You do not have that item.');
        this.ac.flag(p, 'NO_ITEM', 1.5, `Placed ${blk.name} without owning it`);
        this.sendInventory(p);
        return;
      }
      if (give) p.ledger.add({ id: give, count: 1 });
    }
    this.world.setBlock(x, y, z, id, meta);
    this.server.broadcast({ type: 'block_set', x, y, z, id, meta }, p.id);
    this.server.scheduleWorldSave();
  },

  bonemeal(p, msg) {
    const B = this.B, IT = this.C.IT;
    const x = msg.x | 0, y = msg.y | 0, z = msg.z | 0;
    if (this.world.getBlock(x, y, z) !== B.GRASS || !this.ac.checkReach(p, x, y, z).ok) return;
    if (this.isSurvival(p) && !p.ledger.remove(`${IT.BONE_MEAL}#`, 1)) { this.sendInventory(p); return; }
    const allowed = [B.TALL_GRASS, B.POPPY, B.DANDELION, B.ORCHID];
    const list = [];
    for (const pl of Array.isArray(msg.plants) ? msg.plants.slice(0, 8) : []) {
      if (!Array.isArray(pl)) continue;
      const px = pl[0] | 0, pz = pl[2] | 0, pid = pl[3] | 0;
      if (Math.abs(px - x) > 2 || Math.abs(pz - z) > 2 || !allowed.includes(pid)) continue;
      if (this.world.getBlock(px, y, pz) !== B.GRASS || this.world.getBlock(px, y + 1, pz) !== B.AIR) continue;
      this.world.setBlock(px, y + 1, pz, pid, 0);
      list.push([px, y + 1, pz, pid, 0]);
    }
    if (list.length) this.server.broadcast({ type: 'blocks', list }, p.id);
  },

  block_interact(p, msg) {
    const B = this.B;
    const x = msg.x | 0, y = msg.y | 0, z = msg.z | 0;
    const action = String(msg.action || '');
    const val = (msg.val | 0) & 255;
    if (p.pl.dead) return;
    const reach = this.ac.checkReach(p, x, y, z);
    const id = this.world.getBlock(x, y, z);
    const meta = this.world.getMeta(x, y, z);

    if (action === 'tnt') {
      // Redstone wiring (up to 14 blocks) can detonate TNT beyond normal reach
      if (id !== B.TNT) return;
      if (this.ac.isEnabled() && reach.dist > reach.maxR + 24) { this.ac.flag(p, 'REACH', 3, `TNT detonation ${reach.dist.toFixed(1)}m away`); return; }
      this.explode(x, y, z, 3.2, { tnt: true, by: p.pl });
      return;
    }
    if (!reach.ok) {
      this.ac.flag(p, 'REACH', 3, `Interact (${action}) reach ${reach.dist.toFixed(1)}m`);
      this._revert(p, x, y, z);
      return;
    }
    const shape = this.C.BLOCKS.get(id).shape;
    if (action === 'door' || action === 'trapdoor' || action === 'fence_gate') {
      // Only the open bit may change, and iron doors only open through redstone
      if (shape !== action || id === B.IRON_DOOR || (val ^ meta) !== 4) { this._revert(p, x, y, z); return; }
      this.world.setBlock(x, y, z, id, val);
      this.server.broadcast({ type: 'block_set', x, y, z, id, meta: val }, p.id);
      this.server.scheduleWorldSave();
      return;
    }
    if (action === 'lever') {
      if (id !== B.LEVER || (val ^ meta) !== 1) { this._revert(p, x, y, z); return; }
      this.withChanges(() => {
        this.world.setBlock(x, y, z, B.LEVER, val);
        this.world.core.redstone.propagateFrom(x, y, z, (val & 1) !== 0, this.serverMining);
      }, p.id);
      return;
    }
    if (action === 'button') {
      if (id !== B.STONE_BUTTON) { this._revert(p, x, y, z); return; }
      this.withChanges(() => this.world.core.redstone.pressButton(x, y, z, this.serverMining), p.id);
      return;
    }
    if (action === 'plate') {
      if (id !== B.PRESSURE_PLATE || (meta & 1) === (val & 1)) return;
      if ((val & 1) && Math.hypot(p.x - (x + 0.5), p.z - (z + 0.5)) > 2.5) return;
      this.withChanges(() => {
        this.world.setBlock(x, y, z, B.PRESSURE_PLATE, val & 1);
        this.world.core.redstone.propagateFrom(x, y, z, (val & 1) !== 0, this.serverMining);
      }, p.id);
    }
  },

  mob_hit(p, msg) {
    const now = Date.now();
    if (p.pl.dead || now - (p.lastMobHitMs || 0) < 200) return;
    const m = this.mobs.mobs.find(mm => mm.id === (msg.id | 0));
    if (!m || m.dying) return;
    const def = this.C.MOB_DEFS[m.type];
    const eye = this.eyeOf(p);
    const tx = m.x - eye.x, ty = (m.y + def.h * 0.55) - eye.y, tz = m.z - eye.z;
    const dist = Math.hypot(tx, ty, tz);
    if (this.ac.isEnabled() && dist > 4.8) { this.ac.flag(p, 'MOB_REACH', 2, `Hit ${m.type} from ${dist.toFixed(1)}m`); return; }
    const dir = this.lookOf(p);
    if (this.ac.isEnabled() && dist > 1.5 && (tx * dir.x + ty * dir.y + tz * dir.z) / (dist || 1) < 0.3) { this.ac.flag(p, 'MOB_ANGLE', 2, `Hit ${m.type} outside look cone`); return; }
    p.lastMobHitMs = now;
    const held = this.heldStack(p, msg.w | 0);
    this.mobs.applyMelee(m, p.pl, held, dir, !!msg.crit && !p.pl.onGround, this.fx, this.fx, this.uiStub);
  },

  feed(p, msg) {
    const C = this.C;
    const id = msg.id | 0;
    const foods = [C.IT.WHEAT_ITEM, C.IT.APPLE, C.IT.CARROT, C.IT.SWEET_BERRIES, C.B.BAMBOO];
    if (!foods.includes(id) || p.pl.dead) return;
    if (this.isSurvival(p) && !p.ledger.remove(`${id}#`, 1)) { this.sendInventory(p); return; }
    const eye = this.eyeOf(p);
    if (!this.mobs.tryFeedAnimal(p.pl, eye, this.lookOf(p), this.fx, this.fx) && this.isSurvival(p)) {
      p.ledger.add({ id, count: 1 }); // nothing to feed: refund
    }
  },

  shoot(p, msg) {
    const C = this.C;
    if (p.pl.dead) return;
    const bow = this.heldStack(p, C.IT.BOW);
    if (!bow) return;
    const now = Date.now();
    if (now - (p.lastShotMs || 0) < 180) return;
    p.lastShotMs = now;
    const o = Array.isArray(msg.o) ? msg.o.map(Number) : null;
    const v = Array.isArray(msg.v) ? msg.v.map(Number) : null;
    if (!o || !v || o.length < 3 || v.length < 3 || ![...o, ...v].every(Number.isFinite)) return;
    const eye = this.eyeOf(p);
    if (Math.hypot(o[0] - eye.x, o[1] - eye.y, o[2] - eye.z) > 2.5) return;
    const spd = Math.hypot(v[0], v[1], v[2]);
    if (spd > 32) { const k = 32 / spd; v[0] *= k; v[1] *= k; v[2] *= k; }
    if (this.isSurvival(p) && !C.enchLvl(bow, 'Infinity')) {
      if (!p.ledger.remove(`${C.IT.ARROW}#`, 1)) { this.sendInventory(p); return; }
    }
    const pow = C.enchLvl(bow, 'Power');
    this.mobs.spawnArrow(o[0], o[1], o[2], v[0], v[1], v[2], 'player', 8 * (pow ? 1 + 0.25 * (pow + 1) : 1),
      { punch: C.enchLvl(bow, 'Punch'), flame: C.enchLvl(bow, 'Flame') > 0, shooter: p.pl, shooterId: p.id });
  },

  pvp_hit(p, msg) {
    if (!this.config.pvp || p.pl.dead) return;
    const target = this.server.players.get(String(msg.targetId || ''));
    if (!target || target.id === p.id || target.mode === 'creative' || target.pl.dead) return;
    const held = this.heldStack(p, msg.weaponId | 0);
    const chk = this.ac.checkPvpHit(p, target);
    if (!chk.ok) return;
    const it = held ? this.C.ITEMS.get(held.id) : null;
    let dmg = (it && it.dmg) ? it.dmg : 1.5;
    dmg += this.C.enchLvl(held, 'Sharpness') * 1.25;
    if (msg.crit && !p.pl.onGround) dmg *= 1.35;
    const kb = 1 + this.C.enchLvl(held, 'Knockback') * 0.9;
    const dx = target.x - p.x, dz = target.z - p.z;
    const d = Math.hypot(dx, dz) || 1;
    target.pl.hurt(Math.round(dmg), `Slain by ${p.name}.`, null, null, (dx / d) * 6.5 * kb, 5.0, (dz / d) * 6.5 * kb);
  }
};

module.exports = { Game, Ledger, stackKey, enchSig };
