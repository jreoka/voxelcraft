#!/usr/bin/env node
'use strict';

/**
 * VoxelCraft Multiplayer Server + Built-in Anti-Cheat
 * Zero external dependencies — runs directly on any server with Node.js 18+:
 *   node server.js
 *
 * Serves voxelcraft.html over HTTP AND accepts WebSocket connections on the same port.
 */

const http = require('http');
const crypto = require('crypto');
const fs = require('fs');
const path = require('path');
const readline = require('readline');

// ============================================================================
// CLI / ENVIRONMENT OPTIONS
// ============================================================================

const argv = process.argv.slice(2);
function getArg(name, fallback) {
  const idx = argv.indexOf('--' + name);
  if (idx !== -1 && idx + 1 < argv.length) return argv[idx + 1];
  return fallback;
}
const NO_CONSOLE = argv.includes('--no-console') || !process.stdin.isTTY;
const DATA_DIR = path.resolve(getArg('data-dir', process.env.DATA_DIR || __dirname));
const CLIENT_HTML_PATH = path.resolve(getArg('client', process.env.CLIENT_HTML || path.join(__dirname, 'voxelcraft.html')));

if (!fs.existsSync(DATA_DIR)) fs.mkdirSync(DATA_DIR, { recursive: true });

const CONFIG_PATH = path.join(DATA_DIR, 'server-config.json');
const WORLD_PATH = path.join(DATA_DIR, 'world-data.json');
const ACCOUNTS_PATH = path.join(DATA_DIR, 'accounts.json');

function atomicWriteJson(filePath, data, pretty = false) {
  const text = pretty ? JSON.stringify(data, null, 2) : JSON.stringify(data);
  const tmp = filePath + '.tmp';
  fs.writeFileSync(tmp, text, 'utf8');
  try {
    fs.renameSync(tmp, filePath);
  } catch {
    // Fallback for Windows file-lock contention during rename
    fs.writeFileSync(filePath, text, 'utf8');
    try { fs.unlinkSync(tmp); } catch {}
  }
}

// ============================================================================
// CONFIGURATION
// ============================================================================

const DEFAULT_CONFIG = {
  host: '0.0.0.0',
  port: 25565,
  serverName: 'VoxelCraft Survival Server',
  motd: 'Welcome to VoxelCraft! Press T to chat, /help for commands.',
  seed: 'voxelcraft-server-2026',
  mode: 'survival',
  difficulty: 'normal',
  pvp: true,
  maxPlayers: 32,
  maxPerIp: 4,
  saveIntervalSec: 60,
  ops: [],
  bans: [],
  anticheat: {
    enabled: true,
    warnVL: 8,
    kickVL: 18,
    vlDecayPerSec: 0.14,
    maxReach: 6.9,
    maxPvpReach: 4.8,
    maxHorizSpeed: 10.6,
    maxCreativeSpeed: 20.0,
    maxTeleportDist: 14.5,
    maxAirHoverSec: 2.6,
    maxJumpGain: 4.4,
    maxBlocksPerSec: 12,
    maxPacketsPerSec: 80,
    logFile: 'anticheat.log'
  }
};

function deepMerge(target, src) {
  const out = { ...target };
  if (!src || typeof src !== 'object') return out;
  for (const [k, v] of Object.entries(src)) {
    if (v && typeof v === 'object' && !Array.isArray(v) && target[k] && typeof target[k] === 'object' && !Array.isArray(target[k])) {
      out[k] = deepMerge(target[k], v);
    } else {
      out[k] = v;
    }
  }
  return out;
}

function loadConfig() {
  let cfg = deepMerge(DEFAULT_CONFIG, {});
  if (fs.existsSync(CONFIG_PATH)) {
    try {
      const raw = JSON.parse(fs.readFileSync(CONFIG_PATH, 'utf8'));
      cfg = deepMerge(DEFAULT_CONFIG, raw);
    } catch (err) {
      console.error('[Config] Failed to parse server-config.json, using defaults:', err.message);
    }
  } else {
    try {
      fs.writeFileSync(CONFIG_PATH, JSON.stringify(cfg, null, 2), 'utf8');
      console.log(`[Config] Created default config at ${CONFIG_PATH}`);
    } catch (err) {
      console.error('[Config] Could not write default config:', err.message);
    }
  }
  // Allow CLI / env overrides without mutating saved config unless saveConfig() is called
  if (process.env.HOST) cfg.host = process.env.HOST;
  if (process.env.PORT) cfg.port = parseInt(process.env.PORT, 10) || cfg.port;
  const cliHost = getArg('host', null);
  const cliPort = getArg('port', null);
  if (cliHost) cfg.host = cliHost;
  if (cliPort) cfg.port = parseInt(cliPort, 10) || cfg.port;
  return cfg;
}

const config = loadConfig();

function saveConfig() {
  try {
    atomicWriteJson(CONFIG_PATH, config, true);
  } catch (err) {
    console.error('[Config] Save failed:', err.message);
  }
}

// ============================================================================
// GAME CONSTANTS (MATCHES voxelcraft.html BLOCKS & ITEMS 1:1)
// ============================================================================

const WORLD_HEIGHT = 128;
const B_AIR = 0;
const B_BEDROCK = 14;
const B_OBSIDIAN = 72;
const B_OBSIDIAN_CRYING = 230;
const B_SLIME_BLOCK = 249;
const B_TNT = 89;
const B_WATER = 90;
const B_LAVA = 91;

// Valid placeable block IDs: 1..119 and 226..255
function isValidBlockId(id) {
  return Number.isInteger(id) && ((id >= 1 && id <= 119) || (id >= 226 && id <= 255));
}

// Valid item/block IDs for inventory validation
function isValidItemId(id) {
  return Number.isInteger(id) && (
    (id >= 1 && id <= 119) ||
    (id >= 120 && id <= 225) ||
    (id >= 226 && id <= 255) ||
    (id >= 260 && id <= 284)
  );
}

// High-hardness blocks (for Survival fast-break anti-cheat)
const BLOCK_HARDNESS = {
  3: 2.2, 4: 2.4, 5: 3.2, 9: 1.8, 18: 2.0, 19: 2.0, 20: 2.0,
  25: 2.6, 26: 3.2, 27: 3.0, 28: 3.4, 29: 3.5, 30: 3.5, 31: 4.5, 32: 4.5, 33: 7.0,
  57: 2.8, 59: 2.6, 60: 2.6, 63: 4.0, 64: 4.0, 65: 5.5,
  72: 12.0, 113: 2.4, 114: 2.0, 118: 2.5,
  230: 12.0, 231: 3.5, 232: 5.0, 233: 10.0, 234: 2.0, 235: 3.2, 236: 2.4, 245: 4.0, 251: 5.0, 255: 4.0
};

// Tool speed tiers (pickaxes, axes, shovels)
const PICKAXE_SPEED = {
  170: 2.5, // wood
  175: 4.2, // stone
  271: 5.0, // copper
  180: 6.2, // iron
  185: 10.5, // gold
  190: 8.5, // diamond
  276: 9.0, // emerald
  195: 10.5 // netherite
};

// Weapon base PvP damage (authoritative server-side table)
const WEAPON_DAMAGE = {
  174: 4, 179: 5, 275: 5.5, 184: 6, 189: 5, 194: 7.5, 280: 8, 199: 9, // swords
  171: 4, 176: 5, 272: 5, 181: 6, 186: 5, 191: 7, 277: 7.5, 196: 8   // axes
};

// ============================================================================
// WORLD STATE & PERSISTENCE
// ============================================================================

class WorldState {
  constructor() {
    this.seed = String(config.seed || 'voxelcraft-server-2026');
    this.day = 1;
    this.time = 0.28;
    this.weather = 'clear';
    this.weatherTimer = 240;
    this.deltas = new Map();        // "wx,wy,wz" -> [wx, wy, wz, id, meta]
    this.blockEntities = new Map(); // "wx,wy,wz" -> entObj
    this.savedPlayers = new Map();  // lowerName -> { name, x, y, z, health, hunger, xp, level, slots, armor, mode, lastSeen }
    this.dirty = false;
  }

  load() {
    if (!fs.existsSync(WORLD_PATH)) {
      // Persist initial world file on first boot so seed and map state exist on disk immediately
      this.save();
      console.log(`[World] Created new server world at ${WORLD_PATH} (Seed: ${this.seed}).`);
      return;
    }
    try {
      const data = JSON.parse(fs.readFileSync(WORLD_PATH, 'utf8'));
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
          if (typeof k === 'string' && ent && typeof ent === 'object') {
            this.blockEntities.set(k, ent);
          }
        }
      }
      if (data.players && typeof data.players === 'object') {
        for (const [k, v] of Object.entries(data.players)) {
          if (v && typeof v === 'object') this.savedPlayers.set(k.toLowerCase(), v);
        }
      }
      console.log(`[World] Loaded ${this.deltas.size} block deltas, ${this.blockEntities.size} containers, ${this.savedPlayers.size} player profiles (Seed: ${this.seed}, Day ${this.day}).`);
    } catch (err) {
      console.error('[World] Failed to load world-data.json:', err.message);
    }
  }

  save() {
    try {
      const playersObj = {};
      for (const [k, v] of this.savedPlayers.entries()) playersObj[k] = v;
      const payload = {
        version: 2,
        seed: this.seed,
        day: this.day,
        time: +this.time.toFixed(4),
        weather: this.weather,
        deltas: Array.from(this.deltas.values()),
        blockEntities: Array.from(this.blockEntities.entries()),
        players: playersObj,
        savedAt: new Date().toISOString()
      };
      atomicWriteJson(WORLD_PATH, payload, false);
      this.dirty = false;
      return true;
    } catch (err) {
      console.error('[World] Save failed:', err.message);
      return false;
    }
  }

  getDelta(x, y, z) {
    return this.deltas.get(`${x},${y},${z}`) || null;
  }

  setBlock(x, y, z, id, meta = 0) {
    const key = `${x},${y},${z}`;
    this.deltas.set(key, [x | 0, y | 0, z | 0, id | 0, (meta | 0) & 255]);
    if (id === B_AIR) this.blockEntities.delete(key);
    this.dirty = true;
  }

  setBlockEntity(x, y, z, ent) {
    const key = `${x},${y},${z}`;
    if (!ent) this.blockEntities.delete(key);
    else this.blockEntities.set(key, ent);
    this.dirty = true;
  }

  updateTime(dt) {
    // 1 full day = 600s (matches client DayNightSystem)
    this.time += dt / 600;
    if (this.time >= 1) {
      this.time -= 1;
      this.day++;
    }
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
// SERVER-SIDE ACCOUNT STORE (PASSWORD-PROTECTED CLAIMED USERNAMES)
// ============================================================================

class AccountStore {
  constructor() {
    this.accounts = new Map(); // lowerName -> { name, salt, hash, createdAt, lastLoginAt }
  }

  load() {
    if (!fs.existsSync(ACCOUNTS_PATH)) return;
    try {
      const raw = JSON.parse(fs.readFileSync(ACCOUNTS_PATH, 'utf8'));
      const mapObj = (raw && typeof raw.accounts === 'object') ? raw.accounts : raw;
      if (mapObj && typeof mapObj === 'object') {
        for (const [k, v] of Object.entries(mapObj)) {
          if (v && typeof v === 'object' && v.name && v.salt && v.hash) {
            this.accounts.set(k.toLowerCase(), v);
          }
        }
      }
      console.log(`[Accounts] Loaded ${this.accounts.size} claimed account(s).`);
    } catch (err) {
      console.error('[Accounts] Failed to load accounts.json:', err.message);
    }
  }

  save() {
    try {
      const accounts = {};
      for (const [k, v] of this.accounts.entries()) accounts[k] = v;
      atomicWriteJson(ACCOUNTS_PATH, { version: 1, accounts }, true);
      return true;
    } catch (err) {
      console.error('[Accounts] Save failed:', err.message);
      return false;
    }
  }

  _hashPassword(password, salt) {
    return crypto.scryptSync(String(password), String(salt), 64).toString('hex');
  }

  _verifyPassword(password, salt, expectedHash) {
    try {
      const computed = Buffer.from(this._hashPassword(password, salt), 'hex');
      const expected = Buffer.from(String(expectedHash), 'hex');
      if (computed.length !== expected.length) return false;
      return crypto.timingSafeEqual(computed, expected);
    } catch {
      return false;
    }
  }

  validateCredentials(name, password) {
    const cleanName = String(name || '').replace(/[^a-zA-Z0-9_\- ]/g, '').trim().slice(0, 20);
    if (cleanName.length < 2) {
      return { ok: false, reason: 'Account name must be at least 2 characters.' };
    }
    const pw = String(password || '');
    if (pw.length < 4) {
      return { ok: false, reason: 'Password must be at least 4 characters.' };
    }
    if (pw.length > 64) {
      return { ok: false, reason: 'Password must be 64 characters or fewer.' };
    }
    return { ok: true, name: cleanName, password: pw };
  }

  register(rawName, rawPassword) {
    const v = this.validateCredentials(rawName, rawPassword);
    if (!v.ok) return v;
    const key = v.name.toLowerCase();
    if (this.accounts.has(key)) {
      return { ok: false, reason: `Account "${this.accounts.get(key).name}" is already claimed. Switch to Sign In and enter your password.` };
    }
    const salt = crypto.randomBytes(16).toString('hex');
    const hash = this._hashPassword(v.password, salt);
    const now = new Date().toISOString();
    const account = { name: v.name, salt, hash, createdAt: now, lastLoginAt: now };
    this.accounts.set(key, account);
    this.save();
    return { ok: true, account, created: true };
  }

  login(rawName, rawPassword) {
    const v = this.validateCredentials(rawName, rawPassword);
    if (!v.ok) return v;
    const key = v.name.toLowerCase();
    const account = this.accounts.get(key);
    if (!account) {
      return { ok: false, reason: `Account "${v.name}" is not claimed yet. Switch to Create Account to claim this name.` };
    }
    if (!this._verifyPassword(v.password, account.salt, account.hash)) {
      return { ok: false, reason: `Incorrect password for "${account.name}".` };
    }
    account.lastLoginAt = new Date().toISOString();
    this.save();
    return { ok: true, account, created: false };
  }

  changePassword(name, oldPassword, newPassword) {
    const key = String(name || '').toLowerCase();
    const account = this.accounts.get(key);
    if (!account) return { ok: false, reason: 'Account not found.' };
    if (!this._verifyPassword(oldPassword, account.salt, account.hash)) {
      return { ok: false, reason: 'Current password is incorrect.' };
    }
    const pw = String(newPassword || '');
    if (pw.length < 4 || pw.length > 64) {
      return { ok: false, reason: 'New password must be between 4 and 64 characters.' };
    }
    const salt = crypto.randomBytes(16).toString('hex');
    account.salt = salt;
    account.hash = this._hashPassword(pw, salt);
    this.save();
    return { ok: true };
  }
}

// ============================================================================
// BUILT-IN RFC 6455 WEBSOCKET CONNECTION (ZERO DEPENDENCIES)
// ============================================================================

const WS_GUID = '258EAFA5-E914-47DA-95CA-C5AB0DC85B11';
const MAX_WS_PAYLOAD = 256 * 1024; // 256 KB per message DoS protection

class WSConnection {
  constructor(socket, ip, onMessage, onClose) {
    this.socket = socket;
    this.ip = ip;
    this.onMessage = onMessage;
    this.onClose = onClose;
    this.closed = false;
    this.buf = Buffer.alloc(0);
    this.fragOpcode = 0;
    this.fragBuffers = [];
    this.fragSize = 0;
    this.lastActivity = Date.now();

    socket.setNoDelay(true);
    socket.on('data', (chunk) => this._onData(chunk));
    socket.on('close', () => this._cleanup());
    socket.on('error', () => this._cleanup());
  }

  _cleanup() {
    if (this.closed) return;
    this.closed = true;
    this.buf = Buffer.alloc(0);
    this.fragBuffers = [];
    try { this.socket.destroy(); } catch {}
    if (this.onClose) this.onClose();
  }

  sendJSON(obj) {
    if (this.closed || this.socket.destroyed) return;
    this.sendText(JSON.stringify(obj));
  }

  sendText(str) {
    if (this.closed || this.socket.destroyed) return;
    const payload = Buffer.from(str, 'utf8');
    this._writeFrame(0x1, payload);
  }

  sendPing(payload = Buffer.alloc(0)) {
    if (this.closed || this.socket.destroyed) return;
    this._writeFrame(0x9, payload);
  }

  close(code = 1000, reason = '') {
    if (this.closed) return;
    const rBuf = Buffer.from(String(reason || '').slice(0, 120), 'utf8');
    const payload = Buffer.alloc(2 + rBuf.length);
    payload.writeUInt16BE(code, 0);
    rBuf.copy(payload, 2);
    try { this._writeFrame(0x8, payload); } catch {}
    setTimeout(() => this._cleanup(), 40);
  }

  _writeFrame(opcode, payload) {
    const len = payload.length;
    let header;
    if (len < 126) {
      header = Buffer.alloc(2);
      header[0] = 0x80 | opcode;
      header[1] = len;
    } else if (len < 65536) {
      header = Buffer.alloc(4);
      header[0] = 0x80 | opcode;
      header[1] = 126;
      header.writeUInt16BE(len, 2);
    } else {
      header = Buffer.alloc(10);
      header[0] = 0x80 | opcode;
      header[1] = 127;
      header.writeBigUInt64BE(BigInt(len), 2);
    }
    try {
      this.socket.write(Buffer.concat([header, payload]));
    } catch {
      this._cleanup();
    }
  }

  _onData(chunk) {
    if (this.closed) return;
    this.lastActivity = Date.now();
    this.buf = this.buf.length ? Buffer.concat([this.buf, chunk]) : chunk;

    if (this.buf.length > MAX_WS_PAYLOAD + 1024) {
      this.close(1009, 'Message too large');
      return;
    }

    while (this.buf.length >= 2) {
      const b0 = this.buf[0];
      const b1 = this.buf[1];
      const fin = (b0 & 0x80) !== 0;
      const opcode = b0 & 0x0f;
      const masked = (b1 & 0x80) !== 0;
      let payloadLen = b1 & 0x7f;
      let offset = 2;

      // RFC 6455 requires all client-to-server frames to be masked
      if (!masked) {
        this.close(1002, 'Client frames must be masked');
        return;
      }

      if (payloadLen === 126) {
        if (this.buf.length < 4) return;
        payloadLen = this.buf.readUInt16BE(2);
        offset = 4;
      } else if (payloadLen === 127) {
        if (this.buf.length < 10) return;
        const bigLen = this.buf.readBigUInt64BE(2);
        if (bigLen > BigInt(MAX_WS_PAYLOAD)) {
          this.close(1009, 'Payload too large');
          return;
        }
        payloadLen = Number(bigLen);
        offset = 10;
      }

      if (payloadLen > MAX_WS_PAYLOAD) {
        this.close(1009, 'Payload too large');
        return;
      }

      const totalLen = offset + 4 + payloadLen;
      if (this.buf.length < totalLen) return;

      const maskKey = this.buf.subarray(offset, offset + 4);
      const data = Buffer.allocUnsafe(payloadLen);
      const payloadStart = offset + 4;
      for (let i = 0; i < payloadLen; i++) {
        data[i] = this.buf[payloadStart + i] ^ maskKey[i & 3];
      }
      this.buf = this.buf.subarray(totalLen);

      if (opcode === 0x8) { // Close
        this.close(1000, 'Closed');
        return;
      } else if (opcode === 0x9) { // Ping
        this._writeFrame(0xA, data);
      } else if (opcode === 0xA) { // Pong
        // Activity timestamp already updated
      } else if (opcode === 0x1 || opcode === 0x2) { // Text / Binary
        if (fin) {
          if (opcode === 0x1 && this.onMessage) this.onMessage(data.toString('utf8'));
        } else {
          this.fragOpcode = opcode;
          this.fragBuffers = [data];
          this.fragSize = data.length;
        }
      } else if (opcode === 0x0) { // Continuation
        if (!this.fragOpcode) {
          this.close(1002, 'Unexpected continuation');
          return;
        }
        this.fragSize += data.length;
        if (this.fragSize > MAX_WS_PAYLOAD) {
          this.close(1009, 'Fragmented message too large');
          return;
        }
        this.fragBuffers.push(data);
        if (fin) {
          const full = Buffer.concat(this.fragBuffers);
          const op = this.fragOpcode;
          this.fragOpcode = 0;
          this.fragBuffers = [];
          this.fragSize = 0;
          if (op === 0x1 && this.onMessage) this.onMessage(full.toString('utf8'));
        }
      } else {
        this.close(1002, 'Unsupported opcode');
        return;
      }
    }
  }
}

// ============================================================================
// BUILT-IN ANTI-CHEAT ENGINE
// ============================================================================

class AntiCheat {
  constructor(server) {
    this.server = server;
    this.totalFlags = 0;
    this.totalKicks = 0;
    this.recentLogs = [];
  }

  isEnabled() {
    return !!(config.anticheat && config.anticheat.enabled);
  }

  initPlayer(p) {
    p.ac = {
      vl: 0,
      flags: {},
      lastMoveMs: 0,
      lastSafeX: p.x,
      lastSafeY: p.y,
      lastSafeZ: p.z,
      lastGroundY: p.y,
      airHoverSec: 0,
      jumpStartY: p.y,
      speedBurst: 0,
      packetWindowStart: Date.now(),
      packetCount: 0,
      breakTimestamps: [],
      chatTimestamps: [],
      lastPvpMs: 0,
      knockbackUntilMs: 0,
      slimeBounceUntilMs: 0,
      teleportGraceUntilMs: Date.now() + 2500,
      warned: false
    };
  }

  grantTeleportGrace(p, ms = 1800) {
    if (!p.ac) return;
    p.ac.teleportGraceUntilMs = Date.now() + ms;
    p.ac.lastSafeX = p.x;
    p.ac.lastSafeY = p.y;
    p.ac.lastSafeZ = p.z;
    p.ac.lastGroundY = p.y;
    p.ac.jumpStartY = p.y;
    p.ac.airHoverSec = 0;
    p.ac.speedBurst = 0;
    p.ac.lastMoveMs = Date.now();
  }

  flag(p, check, points, detail) {
    if (!this.isEnabled() || !p.ac) return false;
    p.ac.vl += points;
    p.ac.flags[check] = (p.ac.flags[check] || 0) + 1;
    this.totalFlags++;

    const ts = new Date().toISOString();
    const line = `[${ts}] [AC] ${p.name} (${p.ip}) failed ${check} (+${points} -> VL ${p.ac.vl.toFixed(1)}): ${detail}`;
    console.log(line);
    this.recentLogs.push(line);
    if (this.recentLogs.length > 100) this.recentLogs.shift();

    // Persist to anticheat.log
    if (config.anticheat.logFile) {
      try {
        fs.appendFileSync(path.join(DATA_DIR, config.anticheat.logFile), line + '\n', 'utf8');
      } catch {}
    }

    // Notify online OPs
    for (const op of this.server.players.values()) {
      if (this.server.isOp(op.name) && op.id !== p.id) {
        op.ws.sendJSON({ type: 'chat', kind: 'warn', text: `[AC] ${p.name} failed ${check} (VL ${p.ac.vl.toFixed(1)}): ${detail}` });
      }
    }

    const warnVL = config.anticheat.warnVL || 8;
    const kickVL = config.anticheat.kickVL || 18;

    if (p.ac.vl >= kickVL) {
      this.totalKicks++;
      this.server.kickPlayer(p, `Anti-Cheat kicked you (${check} · VL ${Math.round(p.ac.vl)})`);
      return true;
    }

    if (p.ac.vl >= warnVL && !p.ac.warned) {
      p.ac.warned = true;
      p.ws.sendJSON({ type: 'chat', kind: 'warn', text: `[Anti-Cheat] Warning: Unusual behavior detected (${check}). Continued flags will kick you.` });
    }
    return false;
  }

  decayAll(dt) {
    if (!this.isEnabled()) return;
    const rate = config.anticheat.vlDecayPerSec || 0.14;
    for (const p of this.server.players.values()) {
      if (!p.ac) continue;
      if (p.ac.vl > 0) {
        p.ac.vl = Math.max(0, p.ac.vl - rate * dt);
        if (p.ac.vl < (config.anticheat.warnVL || 8) * 0.5) p.ac.warned = false;
      }
    }
  }

  checkPacketRate(p) {
    if (!this.isEnabled() || !p.ac) return true;
    const now = Date.now();
    if (now - p.ac.packetWindowStart >= 1000) {
      p.ac.packetWindowStart = now;
      p.ac.packetCount = 0;
    }
    p.ac.packetCount++;
    const limit = config.anticheat.maxPacketsPerSec || 80;
    if (p.ac.packetCount > limit) {
      if (p.ac.packetCount === limit + 1 || p.ac.packetCount % 25 === 0) {
        this.flag(p, 'PACKET_FLOOD', 3, `${p.ac.packetCount} pkt/s`);
      }
      return false;
    }
    return true;
  }

  checkChatRate(p) {
    if (!p.ac) return true;
    const now = Date.now();
    p.ac.chatTimestamps = p.ac.chatTimestamps.filter(t => now - t < 6000);
    if (p.ac.chatTimestamps.length >= 5) {
      p.ws.sendJSON({ type: 'chat', kind: 'warn', text: 'Slow down! You are sending messages too quickly.' });
      return false;
    }
    p.ac.chatTimestamps.push(now);
    return true;
  }

  checkMove(p, msg) {
    const x = Number(msg.x), y = Number(msg.y), z = Number(msg.z);
    const yaw = Number(msg.yaw || 0), pitch = Number(msg.pitch || 0);

    // 1. Coordinate sanity check (always active, even in creative)
    if (!Number.isFinite(x) || !Number.isFinite(y) || !Number.isFinite(z) ||
        !Number.isFinite(yaw) || !Number.isFinite(pitch) ||
        Math.abs(x) > 1e7 || Math.abs(z) > 1e7 || y < -64 || y > 320) {
      this._rubberband(p, 'Invalid coordinates detected.');
      this.flag(p, 'BAD_COORDS', 6, `Invalid coordinates (${x}, ${y}, ${z})`);
      return false;
    }

    const now = Date.now();
    const ac = p.ac;

    // First movement packet initializes baseline position if player wasn't restored from save
    if (!p.hasMoved) {
      p.hasMoved = true;
      p.x = x; p.y = y; p.z = z;
      p.yaw = yaw; p.pitch = pitch;
      ac.lastSafeX = x; ac.lastSafeY = y; ac.lastSafeZ = z;
      ac.lastGroundY = y; ac.jumpStartY = y;
      ac.lastMoveMs = now;
      return true;
    }

    if (!this.isEnabled()) {
      p.x = x; p.y = y; p.z = z; p.yaw = yaw; p.pitch = pitch;
      return true;
    }

    const dt = Math.max(0.02, Math.min(1.5, (now - ac.lastMoveMs) / 1000));
    const dx = x - p.x;
    const dy = y - p.y;
    const dz = z - p.z;
    const horizDist = Math.hypot(dx, dz);
    const totalDist = Math.hypot(dx, dy, dz);

    const isCreative = p.mode === 'creative';
    const inGrace = now < ac.teleportGraceUntilMs;
    const inKnockback = now < ac.knockbackUntilMs;

    // Detect if player is standing on or just left a known Slime Block -> grant bounce grace
    const underKey = `${Math.floor(x)},${Math.floor(y - 0.35)},${Math.floor(z)}`;
    const underDelta = this.server.world.deltas.get(underKey);
    if (underDelta && underDelta[3] === B_SLIME_BLOCK) {
      ac.slimeBounceUntilMs = now + 2600;
    }
    const inSlimeGrace = now < ac.slimeBounceUntilMs;

    // 2. Illegal flight flag in Survival
    if (!isCreative && msg.flying) {
      this._rubberband(p, 'Flight is not allowed in Survival mode.');
      this.flag(p, 'ILLEGAL_FLY', 5, 'Client reported flying=true in survival');
      return false;
    }

    // 3. Teleport / Blink check
    const maxTp = isCreative ? (config.anticheat.maxTeleportDist * 1.8) : (config.anticheat.maxTeleportDist || 14.5);
    if (!inGrace && totalDist > maxTp) {
      this._rubberband(p, `Teleport/Blink detected (${totalDist.toFixed(1)}m).`);
      this.flag(p, 'TELEPORT', 5, `Moved ${totalDist.toFixed(1)}m in ${(dt * 1000).toFixed(0)}ms`);
      return false;
    }

    // 4. Horizontal Speed Check (leaky bucket so minor lag jitter never false-flags)
    if (!inGrace) {
      const baseMax = isCreative ? (config.anticheat.maxCreativeSpeed || 20.0) : (config.anticheat.maxHorizSpeed || 10.6);
      const maxSpeed = inKnockback ? baseMax * 1.65 : baseMax;
      const horizSpeed = horizDist / dt;
      if (horizSpeed > maxSpeed && horizDist > 0.45) {
        ac.speedBurst += (horizSpeed - maxSpeed) * dt;
        if (ac.speedBurst > 1.8 || horizSpeed > maxSpeed * 1.9) {
          ac.speedBurst = 0;
          this._rubberband(p, `Speed hack detected (${horizSpeed.toFixed(1)} m/s).`);
          this.flag(p, 'SPEED', 4, `Horiz speed ${horizSpeed.toFixed(1)} m/s (max ${maxSpeed.toFixed(1)})`);
          return false;
        }
      } else {
        ac.speedBurst = Math.max(0, ac.speedBurst - 2.4 * dt);
      }
    }

    // 5. Survival Fly / Hover / High-Jump Check
    if (!isCreative && !inGrace) {
      const supported = !!(msg.onGround || msg.inWater || msg.onLadder);
      if (supported) {
        ac.airHoverSec = 0;
        ac.lastGroundY = y;
        ac.jumpStartY = y;
        ac.lastSafeX = x; ac.lastSafeY = y; ac.lastSafeZ = z;
      } else {
        // Ascending or hovering without descending while unsupported
        if (dy >= -0.04) {
          ac.airHoverSec += dt;
        } else {
          // Falling normally resets hover timer gradually
          ac.airHoverSec = Math.max(0, ac.airHoverSec - dt * 0.7);
        }

        const jumpGain = y - ac.jumpStartY;
        const maxJump = inSlimeGrace ? 22.0 : (config.anticheat.maxJumpGain || 4.4);
        if (jumpGain > maxJump) {
          ac.airHoverSec = 0;
          this._rubberband(p, `Fly / High-Jump detected (+${jumpGain.toFixed(1)}m).`);
          this.flag(p, 'HIGH_JUMP', 4.5, `Ascended +${jumpGain.toFixed(2)}m without ground/ladder/water`);
          return false;
        }

        const maxHover = inSlimeGrace ? 5.0 : (config.anticheat.maxAirHoverSec || 2.6);
        if (ac.airHoverSec > maxHover) {
          const hoverSec = ac.airHoverSec;
          ac.airHoverSec = 0;
          this._rubberband(p, 'Fly / Hover detected.');
          this.flag(p, 'FLY_HOVER', 4.5, `Airborne non-falling for ${hoverSec.toFixed(2)}s`);
          return false;
        }
      }
    } else if (msg.onGround || isCreative) {
      ac.lastSafeX = x; ac.lastSafeY = y; ac.lastSafeZ = z;
      ac.lastGroundY = y; ac.jumpStartY = y;
      ac.airHoverSec = 0;
    }

    ac.lastMoveMs = now;
    p.x = x; p.y = y; p.z = z;
    p.yaw = yaw; p.pitch = pitch;
    p.crouch = !!msg.crouch;
    p.swing = Number(msg.swing) || 0;
    p.heldId = Number(msg.heldId) || 0;
    p.onGround = !!msg.onGround;
    return true;
  }

  _rubberband(p, reason) {
    if (!p.ac) return;
    p.x = p.ac.lastSafeX;
    p.y = p.ac.lastSafeY;
    p.z = p.ac.lastSafeZ;
    p.ac.lastMoveMs = Date.now();
    p.ac.teleportGraceUntilMs = Date.now() + 700;
    p.ws.sendJSON({ type: 'rubberband', x: p.x, y: p.y, z: p.z, reason });
  }

  checkReach(p, bx, by, bz) {
    if (!this.isEnabled()) return { ok: true, dist: 0, maxR: 999 };
    const eyeY = p.y + 1.62;
    const dist = Math.hypot((bx + 0.5) - p.x, (by + 0.5) - eyeY, (bz + 0.5) - p.z);
    const maxR = config.anticheat.maxReach || 6.9;
    return { ok: dist <= maxR, dist, maxR };
  }

  checkBlockBreak(p, x, y, z, toolId, mineMs, prevId, prevMeta) {
    if (!Number.isInteger(x) || !Number.isInteger(y) || !Number.isInteger(z)) {
      return { ok: false, revertId: -1, revertMeta: 0, reason: 'Invalid block coordinates.' };
    }

    // Authoritative server record if block was previously placed/modified
    const delta = this.server.world.getDelta(x, y, z);
    const knownId = delta ? delta[3] : (Number.isInteger(prevId) ? prevId : 0);
    const knownMeta = delta ? delta[4] : (Number.isInteger(prevMeta) ? prevMeta : 0);

    if (Math.abs(y) > 10000000) {
      return { ok: false, revertId: knownId || -1, revertMeta: knownMeta, reason: 'Out-of-bounds block.' };
    }

    if (!this.isEnabled()) return { ok: true };
    const isCreative = p.mode === 'creative';

    // 1. Survival Bedrock protection (explicit BEDROCK ID only — world depth is infinite)
    if (!isCreative && knownId === B_BEDROCK) {
      return {
        ok: false, revertId: B_BEDROCK, revertMeta: 0, reason: 'Cannot break Bedrock in Survival.',
        flag: ['BEDROCK_BREAK', 6, `Attempted to break bedrock at (${x},${y},${z})`]
      };
    }

    // 2. Reach check
    const reach = this.checkReach(p, x, y, z);
    if (!reach.ok) {
      return {
        ok: false, revertId: knownId || -1, revertMeta: knownMeta, reason: `Reach too far (${reach.dist.toFixed(1)}m).`,
        flag: ['REACH', 4, `Block break reach ${reach.dist.toFixed(1)}m > ${reach.maxR}m`]
      };
    }

    if (!isCreative) {
      // 3. Nuker / Block Break Rate Limit
      const now = Date.now();
      p.ac.breakTimestamps = p.ac.breakTimestamps.filter(t => now - t < 1000);
      p.ac.breakTimestamps.push(now);
      const maxRate = config.anticheat.maxBlocksPerSec || 12;
      if (p.ac.breakTimestamps.length > maxRate) {
        return {
          ok: false, revertId: knownId || -1, revertMeta: knownMeta, reason: 'Breaking blocks too fast (Nuker).',
          flag: ['NUKER', 5, `Broke ${p.ac.breakTimestamps.length} blocks/sec`]
        };
      }

      // 4. Fast-Break Hardness Validation
      const hard = BLOCK_HARDNESS[knownId] || 0;
      if (hard >= 2.2) {
        const speed = PICKAXE_SPEED[toolId] || 1.0;
        // Client mine time: hard / (speed * 1.65) seconds, with up to Efficiency III (1.9x) -> divide by 2.4 for generous anti-false-positive buffer
        const minExpectedMs = (hard / (speed * 1.65 * 2.4)) * 1000;
        const reportedMs = Number(mineMs) || 0;
        if (reportedMs < minExpectedMs) {
          return {
            ok: false, revertId: knownId || -1, revertMeta: knownMeta, reason: `Fast-Break detected on block #${knownId}.`,
            flag: ['FAST_BREAK', 4.5, `Broke block #${knownId} (h=${hard}) in ${reportedMs}ms (min ${Math.round(minExpectedMs)}ms, tool=${toolId})`]
          };
        }
      }
    }

    return { ok: true };
  }

  checkBlockPlace(p, x, y, z, id, meta) {
    if (!Number.isInteger(x) || !Number.isInteger(y) || !Number.isInteger(z)) {
      return { ok: false, revertId: B_AIR, revertMeta: 0, reason: 'Invalid block coordinates.' };
    }
    const prev = this.server.world.getDelta(x, y, z);
    const revId = prev ? prev[3] : B_AIR;
    const revMeta = prev ? prev[4] : 0;

    if (Math.abs(y) > 10000000) {
      return { ok: false, revertId: revId, revertMeta: revMeta, reason: 'Cannot build beyond coordinate limit.' };
    }

    if (!isValidBlockId(id)) {
      return {
        ok: false, revertId: revId, revertMeta: revMeta, reason: `Illegal block ID (${id}).`,
        flag: ['ILLEGAL_BLOCK', 6, `Tried placing invalid block ID ${id}`]
      };
    }

    if (!this.isEnabled()) return { ok: true };

    if (p.mode !== 'creative' && id === B_BEDROCK) {
      return {
        ok: false, revertId: revId, revertMeta: revMeta, reason: 'Cannot place Bedrock in Survival.',
        flag: ['ILLEGAL_BLOCK', 6, 'Tried placing Bedrock in Survival']
      };
    }

    const reach = this.checkReach(p, x, y, z);
    if (!reach.ok) {
      return {
        ok: false, revertId: revId, revertMeta: revMeta, reason: `Place reach too far (${reach.dist.toFixed(1)}m).`,
        flag: ['REACH', 4, `Block place reach ${reach.dist.toFixed(1)}m > ${reach.maxR}m`]
      };
    }

    return { ok: true };
  }

  checkPvpHit(attacker, target, weaponId) {
    if (!this.isEnabled()) return { ok: true };
    const now = Date.now();

    // 1. Attack speed / AutoClicker check
    if (now - attacker.ac.lastPvpMs < 200) {
      this.flag(attacker, 'PVP_AUTOCLICK', 3, `Hit interval ${now - attacker.ac.lastPvpMs}ms`);
      return { ok: false };
    }
    attacker.ac.lastPvpMs = now;

    // 2. PvP Reach check
    const dist = Math.hypot(target.x - attacker.x, (target.y + 0.9) - (attacker.y + 1.62), target.z - attacker.z);
    const maxR = config.anticheat.maxPvpReach || 4.8;
    if (dist > maxR) {
      this.flag(attacker, 'PVP_REACH', 4.5, `Hit ${target.name} from ${dist.toFixed(2)}m (max ${maxR}m)`);
      return { ok: false };
    }

    // 3. Look-angle (KillAura) check — client forward dir is (-sin(yaw)*cos(pitch), sin(pitch), -cos(yaw)*cos(pitch))
    const cp = Math.cos(attacker.pitch || 0);
    const lx = -Math.sin(attacker.yaw || 0) * cp;
    const ly = Math.sin(attacker.pitch || 0);
    const lz = -Math.cos(attacker.yaw || 0) * cp;
    const tx = target.x - attacker.x;
    const ty = (target.y + 1.0) - (attacker.y + 1.62);
    const tz = target.z - attacker.z;
    const tlen = Math.hypot(tx, ty, tz) || 1;
    const dot = (lx * tx + ly * ty + lz * tz) / tlen;
    if (dist > 1.2 && dot < 0.32) {
      this.flag(attacker, 'PVP_ANGLE', 4, `Hit ${target.name} outside look cone (dot=${dot.toFixed(2)})`);
      return { ok: false };
    }

    return { ok: true };
  }

  sanitizeInventory(p, slots, armor) {
    const isCreative = p.mode === 'creative';
    const cleanSlots = new Array(36).fill(null);
    const cleanArmor = new Array(5).fill(null);

    if (Array.isArray(slots)) {
      for (let i = 0; i < Math.min(36, slots.length); i++) {
        const s = slots[i];
        if (!s || typeof s !== 'object') continue;
        const id = s.id | 0;
        const cnt = s.count | 0;
        if (!isValidItemId(id) || cnt <= 0 || cnt > 64) {
          this.flag(p, 'ILLEGAL_INV', 3, `Invalid slot ${i}: id=${s.id}, count=${s.count}`);
          continue;
        }
        if (!isCreative && id === B_BEDROCK) {
          this.flag(p, 'ILLEGAL_INV', 4, 'Bedrock detected in Survival inventory');
          continue;
        }
        cleanSlots[i] = { id, count: cnt };
        if (Number.isFinite(s.dur)) cleanSlots[i].dur = Math.max(0, Math.min(4000, s.dur | 0));
        if (s.enchant && typeof s.enchant.name === 'string' && Number.isInteger(s.enchant.lvl)) {
          cleanSlots[i].enchant = { name: s.enchant.name.slice(0, 24), lvl: Math.max(1, Math.min(5, s.enchant.lvl)) };
        }
      }
    }

    if (Array.isArray(armor)) {
      for (let i = 0; i < Math.min(5, armor.length); i++) {
        const s = armor[i];
        if (!s || typeof s !== 'object') continue;
        const id = s.id | 0;
        if (!isValidItemId(id)) continue;
        cleanArmor[i] = { id, count: 1 };
        if (Number.isFinite(s.dur)) cleanArmor[i].dur = Math.max(0, Math.min(4000, s.dur | 0));
        if (s.enchant && typeof s.enchant.name === 'string' && Number.isInteger(s.enchant.lvl)) {
          cleanArmor[i].enchant = { name: s.enchant.name.slice(0, 24), lvl: Math.max(1, Math.min(5, s.enchant.lvl)) };
        }
      }
    }

    return { slots: cleanSlots, armor: cleanArmor };
  }
}

// ============================================================================
// MULTIPLAYER SERVER CORE
// ============================================================================

class VoxelCraftServer {
  constructor() {
    this.world = new WorldState();
    this.world.load();
    this.accounts = new AccountStore();
    this.accounts.load();
    this.ac = new AntiCheat(this);
    this.players = new Map(); // id -> PlayerSession
    this.ipCounts = new Map();
    this.nextId = 1;
    this.startedAt = Date.now();
    this.httpServer = null;
    this._intervals = [];
    this._saveDebounce = null;
  }

  scheduleWorldSave(delayMs = 5000) {
    if (this._saveDebounce) return;
    this._saveDebounce = setTimeout(() => {
      this._saveDebounce = null;
      this._snapshotOnlinePlayers();
      if (this.world.dirty) {
        if (this.world.save()) {
          console.log(`[Save] Auto-saved map (${this.world.deltas.size} deltas, ${this.world.blockEntities.size} containers).`);
        }
      }
    }, delayMs);
  }

  isOp(name) {
    const low = String(name || '').toLowerCase();
    return Array.isArray(config.ops) && config.ops.some(o => String(o).toLowerCase() === low);
  }

  isBanned(name, ip) {
    const low = String(name || '').toLowerCase();
    return Array.isArray(config.bans) && config.bans.some(b => {
      const bl = String(b).toLowerCase();
      return bl === low || (ip && bl === String(ip).toLowerCase());
    });
  }

  sanitizeName(raw) {
    const cleaned = String(raw || '').replace(/[^a-zA-Z0-9_\- ]/g, '').trim().slice(0, 18);
    return cleaned.length >= 2 ? cleaned : `Player${this.nextId}`;
  }

  findPlayerByName(name) {
    const low = String(name || '').toLowerCase();
    for (const p of this.players.values()) {
      if (p.name.toLowerCase() === low) return p;
    }
    return null;
  }

  broadcast(obj, excludeId = null) {
    const raw = JSON.stringify(obj);
    for (const p of this.players.values()) {
      if (p.id !== excludeId) p.ws.sendText(raw);
    }
  }

  broadcastChat(text, kind = 'sys', sender = null) {
    this.broadcast({ type: 'chat', text, kind, sender });
    console.log(sender ? `[Chat] <${sender}> ${text}` : `[Server] ${text}`);
  }

  kickPlayer(p, reason = 'Kicked from server.') {
    console.log(`[Kick] ${p.name}: ${reason}`);
    try { p.ws.sendJSON({ type: 'kick', reason }); } catch {}
    p.ws.close(4001, reason);
  }

  start() {
    this.httpServer = http.createServer((req, res) => this._handleHttp(req, res));
    this.httpServer.on('upgrade', (req, socket) => this._handleUpgrade(req, socket));

    this.httpServer.listen(config.port, config.host, () => {
      console.log('============================================================');
      console.log(`  VoxelCraft Server v2.0 — Running on ${config.host}:${config.port}`);
      console.log(`  Web Client : http://localhost:${config.port}/`);
      console.log(`  WebSocket  : ws://<your-server-address>:${config.port}`);
      console.log(`  Status API : http://localhost:${config.port}/status`);
      console.log(`  World Seed : ${this.world.seed} | Mode: ${config.mode} | PvP: ${config.pvp ? 'ON' : 'OFF'}`);
      console.log(`  World File : ${WORLD_PATH} (${this.world.deltas.size} deltas)`);
      console.log(`  Accounts   : ${this.accounts.accounts.size} claimed`);
      console.log(`  Data Dir   : ${DATA_DIR}`);
      console.log('============================================================');
      if (!NO_CONSOLE) console.log('Type /help in this terminal for server console commands.');
    });

    // 1. Player positions snapshot broadcast @ 15 Hz
    this._intervals.push(setInterval(() => {
      if (this.players.size === 0) return;
      const list = [];
      for (const p of this.players.values()) {
        list.push({
          id: p.id, name: p.name,
          x: +p.x.toFixed(3), y: +p.y.toFixed(3), z: +p.z.toFixed(3),
          yaw: +p.yaw.toFixed(3), pitch: +p.pitch.toFixed(3),
          crouch: p.crouch, swing: p.swing, ping: p.ping,
          mode: p.mode, op: this.isOp(p.name)
        });
      }
      this.broadcast({ type: 'players', list });
    }, 66));

    // 2. World clock + Anti-Cheat VL decay @ 1 Hz
    this._intervals.push(setInterval(() => {
      this.world.updateTime(1.0);
      this.ac.decayAll(1.0);
    }, 1000));

    // 3. Time/weather sync & keepalive pings @ 5s
    this._intervals.push(setInterval(() => {
      const now = Date.now();
      this.broadcast({
        type: 'time_sync',
        day: this.world.day,
        time: +this.world.time.toFixed(4),
        weather: this.world.weather
      });
      for (const p of this.players.values()) {
        if (now - p.ws.lastActivity > 45000) {
          this.kickPlayer(p, 'Timed out.');
        } else {
          p.ws.sendPing();
        }
      }
    }, 5000));

    // 4. Periodic world auto-save
    const saveMs = Math.max(15, config.saveIntervalSec || 60) * 1000;
    this._intervals.push(setInterval(() => {
      this._snapshotOnlinePlayers();
      if (this.world.dirty || this.players.size > 0) {
        if (this.world.save()) {
          console.log(`[Save] Saved world (${this.world.deltas.size} deltas, ${this.players.size} online).`);
        }
      }
    }, saveMs));

    if (!NO_CONSOLE) this._startConsole();
    return this.httpServer;
  }

  stop(exitProcess = true) {
    console.log('[Server] Shutting down and saving world...');
    if (this._saveDebounce) { clearTimeout(this._saveDebounce); this._saveDebounce = null; }
    for (const iv of this._intervals) clearInterval(iv);
    this._intervals = [];
    this._snapshotOnlinePlayers();
    this.world.save();
    this.accounts.save();
    for (const p of this.players.values()) {
      try { p.ws.sendJSON({ type: 'kick', reason: 'Server shutting down.' }); } catch {}
      p.ws.close(1001, 'Server shutting down');
    }
    this.players.clear();
    if (this.httpServer) {
      this.httpServer.close(() => {
        if (exitProcess) process.exit(0);
      });
    } else if (exitProcess) {
      process.exit(0);
    }
  }

  _snapshotOnlinePlayers() {
    for (const p of this.players.values()) {
      this._savePlayerProfile(p);
    }
  }

  _savePlayerProfile(p) {
    if (!p || !p.hasMoved) return;
    this.world.savedPlayers.set(p.name.toLowerCase(), {
      name: p.name,
      x: +p.x.toFixed(3), y: +p.y.toFixed(3), z: +p.z.toFixed(3),
      health: p.health, hunger: p.hunger, xp: p.xp, level: p.level,
      slots: p.slots, armor: p.armor, mode: p.mode,
      lastSeen: Date.now()
    });
    this.world.dirty = true;
  }

  _handleHttp(req, res) {
    const urlPath = (req.url || '/').split('?')[0];
    if (urlPath === '/status' || urlPath === '/api/status') {
      const body = JSON.stringify({
        ok: true,
        serverName: config.serverName,
        motd: config.motd,
        mode: config.mode,
        difficulty: config.difficulty,
        pvp: !!config.pvp,
        playersOnline: this.players.size,
        maxPlayers: config.maxPlayers,
        playerNames: Array.from(this.players.values()).map(p => p.name),
        day: this.world.day,
        weather: this.world.weather,
        uptimeSec: Math.floor((Date.now() - this.startedAt) / 1000),
        anticheat: {
          enabled: this.ac.isEnabled(),
          totalFlags: this.ac.totalFlags,
          totalKicks: this.ac.totalKicks
        }
      });
      res.writeHead(200, {
        'Content-Type': 'application/json; charset=utf-8',
        'Access-Control-Allow-Origin': '*',
        'Cache-Control': 'no-cache'
      });
      res.end(body);
      return;
    }

    if (urlPath === '/' || urlPath === '/index.html' || urlPath === '/voxelcraft.html') {
      if (fs.existsSync(CLIENT_HTML_PATH)) {
        res.writeHead(200, {
          'Content-Type': 'text/html; charset=utf-8',
          'Cache-Control': 'no-cache'
        });
        fs.createReadStream(CLIENT_HTML_PATH).pipe(res);
      } else {
        res.writeHead(404, { 'Content-Type': 'text/plain; charset=utf-8' });
        res.end('voxelcraft.html not found next to server.js.');
      }
      return;
    }

    res.writeHead(404, { 'Content-Type': 'text/plain; charset=utf-8' });
    res.end('Not found. Visit / to play VoxelCraft or /status for server status.');
  }

  _handleUpgrade(req, socket) {
    const upgrade = String(req.headers.upgrade || '').toLowerCase();
    const key = req.headers['sec-websocket-key'];
    if (upgrade !== 'websocket' || !key) {
      socket.write('HTTP/1.1 400 Bad Request\r\n\r\n');
      socket.destroy();
      return;
    }

    const fwd = req.headers['x-forwarded-for'];
    const ip = (fwd ? String(fwd).split(',')[0].trim() : (req.socket.remoteAddress || 'unknown')).replace(/^::ffff:/, '');

    // Enforce per-IP connection limit
    const curIpCount = this.ipCounts.get(ip) || 0;
    if (curIpCount >= (config.maxPerIp || 4)) {
      socket.write('HTTP/1.1 429 Too Many Requests\r\n\r\n');
      socket.destroy();
      return;
    }

    const accept = crypto.createHash('sha1').update(key + WS_GUID).digest('base64');
    socket.write(
      'HTTP/1.1 101 Switching Protocols\r\n' +
      'Upgrade: websocket\r\n' +
      'Connection: Upgrade\r\n' +
      `Sec-WebSocket-Accept: ${accept}\r\n\r\n`
    );

    this.ipCounts.set(ip, curIpCount + 1);
    let session = null;

    const ws = new WSConnection(
      socket,
      ip,
      (rawText) => {
        let msg;
        try { msg = JSON.parse(rawText); } catch { return; }
        if (!msg || typeof msg !== 'object') return;
        if (!session) {
          if (msg.type === 'join') session = this._handleJoin(ws, ip, msg);
          return;
        }
        if (!this.ac.checkPacketRate(session)) return;
        this._handlePacket(session, msg);
      },
      () => {
        const c = (this.ipCounts.get(ip) || 1) - 1;
        if (c <= 0) this.ipCounts.delete(ip);
        else this.ipCounts.set(ip, c);

        if (session && this.players.has(session.id)) {
          this._savePlayerProfile(session);
          this.players.delete(session.id);
          if (this.world.dirty) this.world.save();
          console.log(`[Leave] ${session.name} (${ip}) disconnected. Online: ${this.players.size}`);
          this.broadcast({ type: 'player_leave', id: session.id, name: session.name });
        }
      }
    );

    // Kick unauthenticated idle sockets after 8s
    setTimeout(() => {
      if (!session && !ws.closed) ws.close(4000, 'Join handshake timeout');
    }, 8000);
  }

  _handleJoin(ws, ip, msg) {
    if (this.players.size >= (config.maxPlayers || 32)) {
      ws.sendJSON({ type: 'auth_error', reason: 'Server is full.' });
      ws.close(4002, 'Server is full.');
      return null;
    }

    const authMode = (msg.authMode === 'register') ? 'register' : 'login';
    const rawName = String(msg.name || '').trim();
    const rawPassword = String(msg.password || '');

    const authRes = (authMode === 'register')
      ? this.accounts.register(rawName, rawPassword)
      : this.accounts.login(rawName, rawPassword);

    if (!authRes.ok) {
      ws.sendJSON({ type: 'auth_error', reason: authRes.reason });
      ws.close(4004, authRes.reason);
      return null;
    }

    const name = authRes.account.name;
    if (this.isBanned(name, ip)) {
      ws.sendJSON({ type: 'auth_error', reason: 'You are banned from this server.' });
      ws.close(4003, 'Banned from server.');
      return null;
    }

    if (this.findPlayerByName(name)) {
      ws.sendJSON({ type: 'auth_error', reason: `Account "${name}" is already online on this server.` });
      ws.close(4005, 'Account already online.');
      return null;
    }

    const id = 'p' + (this.nextId++);
    const saved = this.world.savedPlayers.get(name.toLowerCase()) || null;

    const player = {
      id, name, ip, ws,
      x: saved ? saved.x : 8.5,
      y: saved ? saved.y : 80,
      z: saved ? saved.z : 8.5,
      yaw: 0, pitch: 0,
      crouch: false, swing: 0, heldId: 0, onGround: true,
      hasMoved: !!saved,
      health: saved ? (saved.health ?? 20) : 20,
      hunger: saved ? (saved.hunger ?? 20) : 20,
      xp: saved ? (saved.xp ?? 0) : 0,
      level: saved ? (saved.level ?? 0) : 0,
      slots: saved && Array.isArray(saved.slots) ? saved.slots : null,
      armor: saved && Array.isArray(saved.armor) ? saved.armor : null,
      mode: (saved && saved.mode) || config.mode || 'survival',
      ping: 0,
      joinedAt: Date.now()
    };

    this.ac.initPlayer(player);
    this.players.set(id, player);

    const existingPlayers = [];
    for (const o of this.players.values()) {
      if (o.id === id) continue;
      existingPlayers.push({
        id: o.id, name: o.name, x: o.x, y: o.y, z: o.z,
        yaw: o.yaw, pitch: o.pitch, crouch: o.crouch, swing: o.swing, ping: o.ping,
        mode: o.mode, op: this.isOp(o.name)
      });
    }

    ws.sendJSON({
      type: 'welcome',
      id,
      name,
      accountCreated: !!authRes.created,
      serverName: config.serverName,
      motd: config.motd,
      maxPlayers: config.maxPlayers || 32,
      isOp: this.isOp(name),
      seed: this.world.seed,
      mode: player.mode,
      difficulty: config.difficulty,
      pvp: !!config.pvp,
      day: this.world.day,
      time: +this.world.time.toFixed(4),
      weather: this.world.weather,
      player: saved ? {
        x: saved.x, y: saved.y, z: saved.z,
        health: saved.health, hunger: saved.hunger, xp: saved.xp, level: saved.level,
        slots: saved.slots, armor: saved.armor
      } : null,
      players: existingPlayers,
      deltas: Array.from(this.world.deltas.values()),
      blockEntities: Array.from(this.world.blockEntities.entries())
    });

    console.log(`[Join] ${name} (${ip}) ${authRes.created ? 'claimed new account & ' : ''}joined as ${id}. Online: ${this.players.size}/${config.maxPlayers}`);
    this.broadcast({ type: 'player_join', id, name }, id);
    return player;
  }

  _handlePacket(p, msg) {
    switch (msg.type) {
      case 'ping': {
        const t = Number(msg.t) || Date.now();
        if (Number.isFinite(Number(msg.ping)) && Number(msg.ping) > 0) {
          p.ping = Math.max(1, Math.min(9999, Math.round(Number(msg.ping))));
        }
        p.ws.sendJSON({ type: 'pong', t });
        break;
      }

      case 'move': {
        this.ac.checkMove(p, msg);
        break;
      }

      case 'block_break': {
        const x = msg.x | 0, y = msg.y | 0, z = msg.z | 0;
        const toolId = msg.toolId | 0;
        const mineMs = Number(msg.mineMs) || 0;
        const prevId = Number.isInteger(msg.prevId) ? msg.prevId : 0;
        const prevMeta = Number.isInteger(msg.prevMeta) ? msg.prevMeta : 0;

        const check = this.ac.checkBlockBreak(p, x, y, z, toolId, mineMs, prevId, prevMeta);
        if (!check.ok) {
          p.ws.sendJSON({ type: 'block_revert', x, y, z, id: check.revertId, meta: check.revertMeta, reason: check.reason });
          if (check.flag) this.ac.flag(p, check.flag[0], check.flag[1], check.flag[2]);
          return;
        }
        this.world.setBlock(x, y, z, B_AIR, 0);
        this.scheduleWorldSave();
        this.broadcast({ type: 'block_set', x, y, z, id: B_AIR, meta: 0 }, p.id);
        break;
      }

      case 'block_place': {
        const x = msg.x | 0, y = msg.y | 0, z = msg.z | 0;
        const id = msg.id | 0;
        const meta = (msg.meta | 0) & 255;

        const check = this.ac.checkBlockPlace(p, x, y, z, id, meta);
        if (!check.ok) {
          p.ws.sendJSON({ type: 'block_revert', x, y, z, id: check.revertId, meta: check.revertMeta, reason: check.reason });
          if (check.flag) this.ac.flag(p, check.flag[0], check.flag[1], check.flag[2]);
          return;
        }
        this.world.setBlock(x, y, z, id, meta);
        this.scheduleWorldSave();
        this.broadcast({ type: 'block_set', x, y, z, id, meta }, p.id);
        break;
      }

      case 'block_interact': {
        const x = msg.x | 0, y = msg.y | 0, z = msg.z | 0;
        const action = String(msg.action || '');
        const val = (msg.val | 0) & 255;
        if (Math.abs(y) > 10000000) return;

        const reach = this.ac.checkReach(p, x, y, z);
        if (!reach.ok) {
          this.ac.flag(p, 'REACH', 3, `Interact (${action}) reach ${reach.dist.toFixed(1)}m`);
          return;
        }

        if (action === 'tnt') {
          const radius = Math.min(4.2, Math.max(1.0, (Number(msg.val) || 32) / 10));
          const r = Math.ceil(radius);
          for (let dy = -r; dy <= r; dy++) {
            for (let dz = -r; dz <= r; dz++) {
              for (let dx = -r; dx <= r; dx++) {
                if (dx*dx + dy*dy + dz*dz > radius * radius) continue;
                const bx = x + dx, by = y + dy, bz = z + dz;
                const prev = this.world.getDelta(bx, by, bz);
                if (prev && (prev[3] === B_BEDROCK || prev[3] === B_OBSIDIAN || prev[3] === B_OBSIDIAN_CRYING)) continue;
                this.world.setBlock(bx, by, bz, B_AIR, 0);
              }
            }
          }
          this.scheduleWorldSave();
          this.broadcast({ type: 'tnt_explode', x, y, z, radius: Math.round(radius * 10) }, p.id);
          return;
        }

        // Door / trapdoor / fence_gate / lever / button / plate state toggle
        const prev = this.world.getDelta(x, y, z);
        const bid = prev ? prev[3] : (isValidBlockId(msg.id | 0) ? (msg.id | 0) : 0);
        if (bid > 0) {
          this.world.setBlock(x, y, z, bid, val);
          this.scheduleWorldSave();
          this.broadcast({ type: 'block_set', x, y, z, id: bid, meta: val }, p.id);
        }
        break;
      }

      case 'block_entity': {
        const x = msg.x | 0, y = msg.y | 0, z = msg.z | 0;
        const reach = this.ac.checkReach(p, x, y, z);
        if (!reach.ok) {
          this.ac.flag(p, 'REACH', 3, `Container sync reach ${reach.dist.toFixed(1)}m`);
          return;
        }
        if (msg.ent && typeof msg.ent === 'object') {
          const rawStr = JSON.stringify(msg.ent);
          if (rawStr.length > 32768) return;
          this.world.setBlockEntity(x, y, z, msg.ent);
          this.scheduleWorldSave();
          this.broadcast({ type: 'block_entity', x, y, z, ent: msg.ent }, p.id);
        }
        break;
      }

      case 'pvp_hit': {
        if (!config.pvp) return;
        const target = this.players.get(String(msg.targetId || ''));
        if (!target || target.id === p.id || target.mode === 'creative') return;
        const weaponId = msg.weaponId | 0;
        const chk = this.ac.checkPvpHit(p, target, weaponId);
        if (!chk.ok) return;

        let dmg = WEAPON_DAMAGE[weaponId] || 1.5;
        if (msg.crit && !p.onGround) dmg *= 1.35;
        dmg = Math.round(dmg);

        const dx = target.x - p.x, dz = target.z - p.z;
        const d = Math.hypot(dx, dz) || 1;
        target.ac.knockbackUntilMs = Date.now() + 900;
        target.ws.sendJSON({
          type: 'pvp_damage',
          dmg,
          by: p.name,
          kx: +((dx / d) * 6.5).toFixed(2),
          kz: +((dz / d) * 6.5).toFixed(2)
        });
        break;
      }

      case 'inv_sync': {
        const clean = this.ac.sanitizeInventory(p, msg.slots, msg.armor);
        p.slots = clean.slots;
        p.armor = clean.armor;
        if (Number.isFinite(msg.health)) p.health = Math.max(0, Math.min(20, msg.health));
        if (Number.isFinite(msg.hunger)) p.hunger = Math.max(0, Math.min(20, msg.hunger));
        if (Number.isFinite(msg.xp)) p.xp = Math.max(0, Math.min(100000, msg.xp));
        if (Number.isFinite(msg.level)) p.level = Math.max(0, Math.min(1000, msg.level | 0));
        this._savePlayerProfile(p);
        break;
      }

      case 'respawn': {
        p.health = 20;
        p.hunger = 20;
        this.ac.grantTeleportGrace(p, 2500);
        break;
      }

      case 'chat': {
        const text = String(msg.text || '').replace(/[\r\n\x00-\x1f]/g, '').trim().slice(0, 200);
        if (!text) return;
        if (!this.ac.checkChatRate(p)) return;
        if (text.startsWith('/')) {
          this.executeCommand(text, p);
        } else {
          this.broadcastChat(text, 'normal', p.name);
        }
        break;
      }
    }
  }

  // ==========================================================================
  // SLASH COMMANDS (BOTH IN-GAME CHAT & SERVER TERMINAL CONSOLE)
  // ==========================================================================

  executeCommand(raw, sender = null) {
    const isConsole = !sender;
    const isOp = isConsole || this.isOp(sender.name);
    const reply = (text, kind = 'sys') => {
      if (isConsole) console.log(`[Console] ${text}`);
      else sender.ws.sendJSON({ type: 'chat', text, kind });
    };

    const parts = raw.replace(/^\//, '').trim().split(/\s+/);
    const cmd = (parts[0] || '').toLowerCase();
    const args = parts.slice(1);

    if (cmd === 'help') {
      reply('Player cmds: /list, /ping, /pos, /msg <player> <text>, /passwd <old> <new>');
      if (isOp) {
        reply('Admin cmds: /op, /deop, /kick, /ban, /unban, /tp, /gamemode, /give, /time, /weather, /pvp, /ac, /save, /say' + (isConsole ? ', /stop' : ''));
      }
      return;
    }

    if (cmd === 'passwd' && !isConsole) {
      const oldPw = args[0] || '';
      const newPw = args[1] || '';
      if (!oldPw || !newPw) { reply('Usage: /passwd <currentPassword> <newPassword>', 'warn'); return; }
      const res = this.accounts.changePassword(sender.name, oldPw, newPw);
      reply(res.ok ? 'Account password updated.' : res.reason, res.ok ? 'sys' : 'err');
      return;
    }

    if (cmd === 'list') {
      const names = Array.from(this.players.values()).map(p => `${p.name}${this.isOp(p.name) ? '*' : ''}`);
      reply(`Online (${names.length}/${config.maxPlayers}): ${names.join(', ') || 'none'}`);
      return;
    }

    if (cmd === 'ping') {
      reply(isConsole ? 'Server is running smoothly.' : `Your connection is active (VL: ${sender.ac.vl.toFixed(1)}).`);
      return;
    }

    if (cmd === 'pos' && !isConsole) {
      reply(`Position: X ${sender.x.toFixed(1)}, Y ${sender.y.toFixed(1)}, Z ${sender.z.toFixed(1)}`);
      return;
    }

    if (cmd === 'msg' || cmd === 'w' || cmd === 'tell') {
      const target = this.findPlayerByName(args[0]);
      const body = args.slice(1).join(' ');
      if (!target || !body) { reply('Usage: /msg <player> <message>', 'warn'); return; }
      const fromName = isConsole ? 'Server' : sender.name;
      target.ws.sendJSON({ type: 'chat', kind: 'sys', text: `[PM from ${fromName}] ${body}` });
      reply(`[PM to ${target.name}] ${body}`);
      return;
    }

    // All remaining commands require OP or server console
    if (!isOp) {
      reply('Permission denied. You must be a server OP (run "/op <name>" in server console).', 'err');
      return;
    }

    if (cmd === 'op') {
      const name = args[0];
      if (!name) { reply('Usage: /op <player>', 'warn'); return; }
      if (!this.isOp(name)) {
        config.ops.push(name);
        saveConfig();
      }
      reply(`Granted OP to ${name}.`);
      const target = this.findPlayerByName(name);
      if (target) target.ws.sendJSON({ type: 'chat', kind: 'sys', text: 'You were granted Server OP privileges!' });
    } else if (cmd === 'deop') {
      const name = (args[0] || '').toLowerCase();
      config.ops = (config.ops || []).filter(o => o.toLowerCase() !== name);
      saveConfig();
      reply(`Removed OP from ${args[0] || ''}.`);
    } else if (cmd === 'kick') {
      const target = this.findPlayerByName(args[0]);
      if (!target) { reply('Player not found.', 'warn'); return; }
      const reason = args.slice(1).join(' ') || 'Kicked by an administrator.';
      this.kickPlayer(target, reason);
      reply(`Kicked ${target.name}: ${reason}`);
    } else if (cmd === 'ban') {
      const name = args[0];
      if (!name) { reply('Usage: /ban <player> [reason]', 'warn'); return; }
      if (!this.isBanned(name)) {
        config.bans.push(name);
        saveConfig();
      }
      const target = this.findPlayerByName(name);
      if (target) this.kickPlayer(target, args.slice(1).join(' ') || 'Banned by an administrator.');
      reply(`Banned ${name}.`);
    } else if (cmd === 'unban') {
      const name = (args[0] || '').toLowerCase();
      config.bans = (config.bans || []).filter(b => b.toLowerCase() !== name);
      saveConfig();
      reply(`Unbanned ${args[0] || ''}.`);
    } else if (cmd === 'gamemode' || cmd === 'gm') {
      const m = (args[0] || '').toLowerCase();
      const mode = (m === 'c' || m === 'creative' || m === '1') ? 'creative' : (m === 's' || m === 'survival' || m === '0') ? 'survival' : null;
      if (!mode) { reply('Usage: /gamemode <survival|creative> [player]', 'warn'); return; }
      const target = args[1] ? this.findPlayerByName(args[1]) : sender;
      if (!target) { reply('Specify an online player.', 'warn'); return; }
      target.mode = mode;
      target.ws.sendJSON({ type: 'mode_change', mode });
      reply(`Set ${target.name} to ${mode} mode.`);
    } else if (cmd === 'give') {
      const target = this.findPlayerByName(args[0]);
      const id = parseInt(args[1], 10);
      const cnt = Math.max(1, Math.min(64, parseInt(args[2], 10) || 1));
      if (!target || !isValidItemId(id)) { reply('Usage: /give <player> <itemId> [count]', 'warn'); return; }
      target.ws.sendJSON({ type: 'give_item', id, count: cnt });
      reply(`Gave ${cnt}x item #${id} to ${target.name}.`);
    } else if (cmd === 'tp') {
      if (args.length === 2) {
        const a = this.findPlayerByName(args[0]);
        const b = this.findPlayerByName(args[1]);
        if (!a || !b) { reply('Player(s) not found.', 'warn'); return; }
        a.x = b.x; a.y = b.y + 0.5; a.z = b.z;
        this.ac.grantTeleportGrace(a, 2000);
        a.ws.sendJSON({ type: 'rubberband', x: a.x, y: a.y, z: a.z, reason: `Teleported to ${b.name}.` });
        reply(`Teleported ${a.name} to ${b.name}.`);
      } else if (args.length === 4) {
        const a = this.findPlayerByName(args[0]);
        const tx = parseFloat(args[1]), ty = parseFloat(args[2]), tz = parseFloat(args[3]);
        if (!a || !Number.isFinite(tx) || !Number.isFinite(ty) || !Number.isFinite(tz)) { reply('Usage: /tp <player> <x> <y> <z>', 'warn'); return; }
        a.x = tx; a.y = ty; a.z = tz;
        this.ac.grantTeleportGrace(a, 2000);
        a.ws.sendJSON({ type: 'rubberband', x: tx, y: ty, z: tz, reason: `Teleported to ${tx}, ${ty}, ${tz}.` });
        reply(`Teleported ${a.name} to ${tx}, ${ty}, ${tz}.`);
      } else {
        reply('Usage: /tp <p1> <p2> OR /tp <player> <x> <y> <z>', 'warn');
      }
    } else if (cmd === 'time') {
      const map = { day: 0.28, sunrise: 0.24, noon: 0.50, sunset: 0.75, night: 0.88 };
      const t = map[args[0]] ?? parseFloat(args[0]);
      if (!Number.isFinite(t)) { reply('Usage: /time <day|noon|sunset|night|0..1>', 'warn'); return; }
      this.world.time = ((t % 1) + 1) % 1;
      this.broadcast({ type: 'time_sync', day: this.world.day, time: this.world.time, weather: this.world.weather });
      this.broadcastChat(`Time set to ${args[0]}.`, 'sys');
    } else if (cmd === 'weather') {
      const w = (args[0] || '').toLowerCase();
      if (!['clear', 'rain', 'thunder'].includes(w)) { reply('Usage: /weather <clear|rain|thunder>', 'warn'); return; }
      this.world.weather = w;
      this.broadcast({ type: 'time_sync', day: this.world.day, time: this.world.time, weather: w });
      this.broadcastChat(`Weather set to ${w}.`, 'sys');
    } else if (cmd === 'pvp') {
      const v = (args[0] || '').toLowerCase();
      if (v === 'on' || v === 'true') config.pvp = true;
      else if (v === 'off' || v === 'false') config.pvp = false;
      else { reply(`PvP is currently ${config.pvp ? 'ON' : 'OFF'}. Usage: /pvp <on|off>`); return; }
      saveConfig();
      this.broadcastChat(`PvP is now ${config.pvp ? 'ENABLED' : 'DISABLED'}.`, 'sys');
    } else if (cmd === 'ac' || cmd === 'anticheat') {
      const sub = (args[0] || 'status').toLowerCase();
      if (sub === 'toggle') {
        config.anticheat.enabled = !config.anticheat.enabled;
        saveConfig();
        reply(`Anti-Cheat is now ${config.anticheat.enabled ? 'ENABLED' : 'DISABLED'}.`);
      } else if (sub === 'reset' && args[1]) {
        const t = this.findPlayerByName(args[1]);
        if (!t) { reply('Player not found.', 'warn'); return; }
        t.ac.vl = 0; t.ac.warned = false; t.ac.flags = {};
        reply(`Reset Anti-Cheat VL for ${t.name}.`);
      } else {
        const list = Array.from(this.players.values()).map(pl => `${pl.name}: VL ${pl.ac.vl.toFixed(1)}`).join(', ');
        reply(`Anti-Cheat ${this.ac.isEnabled() ? 'ON' : 'OFF'} | Flags: ${this.ac.totalFlags} | Kicks: ${this.ac.totalKicks} | Players: ${list || 'none'}`);
      }
    } else if (cmd === 'save') {
      this._snapshotOnlinePlayers();
      const ok = this.world.save();
      reply(ok ? `Saved world (${this.world.deltas.size} deltas).` : 'World save failed!', ok ? 'sys' : 'err');
    } else if (cmd === 'say') {
      const msg = args.join(' ');
      if (msg) this.broadcastChat(`[Server] ${msg}`, 'sys');
    } else if (cmd === 'stop' && isConsole) {
      this.stop(true);
    } else {
      reply(`Unknown command "/${cmd}". Type /help.`, 'warn');
    }
  }

  _startConsole() {
    const rl = readline.createInterface({ input: process.stdin, output: process.stdout, prompt: '' });
    rl.on('line', (line) => {
      const trimmed = line.trim();
      if (!trimmed) return;
      this.executeCommand(trimmed, null);
    });
  }
}

// ============================================================================
// BOOTSTRAP
// ============================================================================

const server = new VoxelCraftServer();
server.start();

process.on('SIGINT', () => server.stop(true));
process.on('SIGTERM', () => server.stop(true));
process.on('uncaughtException', (err) => {
  console.error('[Fatal] Uncaught exception:', err);
});
