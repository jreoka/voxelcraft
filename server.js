#!/usr/bin/env node
'use strict';

/**
 * VoxelCraft Multiplayer Server + Built-in Anti-Cheat
 * Zero external dependencies — runs directly on any server with Node.js 18+:
 *   node server.js
 *
 * Serves the game client (index.html) over HTTP AND accepts WebSocket connections on the same port.
 * The server loads the game rules from that same client file (see core-loader.js) and is
 * authoritative for the world, mobs, dropped items, player inventories and health (see game.js).
 */

const http = require('http');
const crypto = require('crypto');
const fs = require('fs');
const path = require('path');
const readline = require('readline');
const { loadGameCore } = require('./core-loader');
const { Game } = require('./game');

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
function resolveClientPath() {
  const explicit = getArg('client', process.env.CLIENT_HTML || null);
  if (explicit) return path.resolve(explicit);
  // Prefer index.html (the file shipped in this repo), fall back to the legacy voxelcraft.html name
  for (const name of ['index.html', 'voxelcraft.html']) {
    const candidate = path.join(__dirname, name);
    if (fs.existsSync(candidate)) return candidate;
  }
  return path.join(__dirname, 'index.html');
}
const CLIENT_HTML_PATH = resolveClientPath();
let SERVER_VERSION = '2.1.0';
try { SERVER_VERSION = require('./package.json').version || SERVER_VERSION; } catch {}

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
  // Only trust X-Forwarded-For when running behind your own reverse proxy (nginx, Caddy, ...).
  // Otherwise any client can spoof its IP and bypass per-IP limits and IP bans.
  trustProxy: false,
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
  // Guard against hand-edited configs with wrong types
  if (!Array.isArray(cfg.ops)) cfg.ops = [];
  if (!Array.isArray(cfg.bans)) cfg.bans = [];
  cfg.ops = cfg.ops.map(String);
  cfg.bans = cfg.bans.map(String);
  if (!cfg.anticheat || typeof cfg.anticheat !== 'object') cfg.anticheat = { ...DEFAULT_CONFIG.anticheat };
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
// GAME CORE (the client's own simulation code, loaded from the HTML it serves)
// ============================================================================

if (!fs.existsSync(CLIENT_HTML_PATH)) {
  console.error(`[Fatal] Client file not found: ${CLIENT_HTML_PATH}
The server runs the game rules from the client, so it needs index.html (or --client <path>).`);
  process.exit(1);
}
const CORE = loadGameCore(CLIENT_HTML_PATH);
const B_SLIME_BLOCK = CORE.B.SLIME_BLOCK;

// 1 full day/night cycle in seconds (taken from the client's DayNightSystem)
const DAY_LENGTH_SEC = new CORE.DayNightSystem().dayLength;

// Enchantment names the client knows about (anything else is stripped from synced items)
const ENCHANT_NAMES = new Set(Object.keys(CORE.ENCHANTS));

function isValidItemId(id) {
  return Number.isInteger(id) && !!CORE.ITEMS.get(id);
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

  // Async scrypt so a burst of logins never stalls the game loop for every connected player
  _hashPassword(password, salt) {
    return new Promise((resolve, reject) => {
      crypto.scrypt(String(password), String(salt), 64, (err, key) => err ? reject(err) : resolve(key.toString('hex')));
    });
  }

  async _verifyPassword(password, salt, expectedHash) {
    try {
      const computed = Buffer.from(await this._hashPassword(password, salt), 'hex');
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

  async register(rawName, rawPassword) {
    const v = this.validateCredentials(rawName, rawPassword);
    if (!v.ok) return v;
    const key = v.name.toLowerCase();
    if (this.accounts.has(key)) {
      return { ok: false, reason: `Account "${this.accounts.get(key).name}" is already claimed. Switch to Sign In and enter your password.` };
    }
    const salt = crypto.randomBytes(16).toString('hex');
    const hash = await this._hashPassword(v.password, salt);
    // Re-check after the async hash: another connection may have claimed the name meanwhile
    if (this.accounts.has(key)) {
      return { ok: false, reason: `Account "${this.accounts.get(key).name}" is already claimed. Switch to Sign In and enter your password.` };
    }
    const now = new Date().toISOString();
    const account = { name: v.name, salt, hash, createdAt: now, lastLoginAt: now };
    this.accounts.set(key, account);
    this.save();
    return { ok: true, account, created: true };
  }

  async login(rawName, rawPassword) {
    const v = this.validateCredentials(rawName, rawPassword);
    if (!v.ok) return v;
    const key = v.name.toLowerCase();
    const account = this.accounts.get(key);
    if (!account) {
      return { ok: false, reason: `Account "${v.name}" is not claimed yet. Switch to Create Account to claim this name.` };
    }
    if (!(await this._verifyPassword(v.password, account.salt, account.hash))) {
      return { ok: false, reason: `Incorrect password for "${account.name}".`, badPassword: true };
    }
    account.lastLoginAt = new Date().toISOString();
    this.save();
    return { ok: true, account, created: false };
  }

  async changePassword(name, oldPassword, newPassword) {
    const key = String(name || '').toLowerCase();
    const account = this.accounts.get(key);
    if (!account) return { ok: false, reason: 'Account not found.' };
    if (!(await this._verifyPassword(oldPassword, account.salt, account.hash))) {
      return { ok: false, reason: 'Current password is incorrect.' };
    }
    const pw = String(newPassword || '');
    if (pw.length < 4 || pw.length > 64) {
      return { ok: false, reason: 'New password must be between 4 and 64 characters.' };
    }
    const salt = crypto.randomBytes(16).toString('hex');
    const hash = await this._hashPassword(pw, salt);
    account.salt = salt;
    account.hash = hash;
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

      // Control frames must be final and carry at most 125 bytes (RFC 6455 §5.5)
      if ((opcode & 0x8) && (!fin || payloadLen > 125)) {
        this.close(1002, 'Invalid control frame');
        return;
      }

      if (opcode === 0x8) { // Close
        this.close(1000, 'Closed');
        return;
      } else if (opcode === 0x9) { // Ping
        this._writeFrame(0xA, data);
      } else if (opcode === 0xA) { // Pong
        // Activity timestamp already updated
      } else if (opcode === 0x1 || opcode === 0x2) { // Text / Binary
        if (this.fragOpcode) {
          this.close(1002, 'New data frame during fragmented message');
          return;
        }
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
      ac.lastSafeX = x; ac.lastSafeY = y; ac.lastSafeZ = z;
      ac.lastGroundY = y; ac.jumpStartY = y;
      ac.lastMoveMs = now;
      this._acceptMove(p, msg, x, y, z, yaw, pitch);
      return true;
    }

    if (!this.isEnabled()) {
      ac.lastMoveMs = now;
      this._acceptMove(p, msg, x, y, z, yaw, pitch);
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
    if (this.server.world.getBlock(Math.floor(x), Math.floor(y - 0.35), Math.floor(z)) === B_SLIME_BLOCK) {
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
    this._acceptMove(p, msg, x, y, z, yaw, pitch);
    return true;
  }

  _acceptMove(p, msg, x, y, z, yaw, pitch) {
    p.x = x; p.y = y; p.z = z;
    p.yaw = yaw; p.pitch = Math.max(-Math.PI / 2, Math.min(Math.PI / 2, pitch));
    p.crouch = !!msg.crouch;
    const swing = Number(msg.swing);
    p.swing = Number.isFinite(swing) ? Math.max(0, Math.min(1, swing)) : 0;
    p.heldId = msg.heldId | 0;
    p.onGround = !!msg.onGround;
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

  checkPvpHit(attacker, target) {
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

  _sanitizeEnchant(e) {
    if (!e || typeof e !== 'object') return undefined;
    const raw = Array.isArray(e.list) ? e.list : [e];
    const list = [];
    const seen = new Set();
    for (const x of raw.slice(0, 8)) {
      if (!x || typeof x.name !== 'string' || !ENCHANT_NAMES.has(x.name) || seen.has(x.name)) continue;
      const lvl = x.lvl | 0;
      if (lvl < 1) continue;
      seen.add(x.name);
      list.push({ name: x.name, lvl: Math.min(5, lvl) });
    }
    if (!list.length) return undefined;
    return { name: list[0].name, lvl: list[0].lvl, list };
  }

}

// ============================================================================
// MULTIPLAYER SERVER CORE
// ============================================================================

class VoxelCraftServer {
  constructor() {
    this.game = new Game(this, CORE, { config, worldPath: WORLD_PATH, atomicWriteJson });
    this.world = this.game.world;
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
    this._stopping = false;
    this.failedLogins = new Map(); // ip -> { count, firstMs }
  }

  // Simple brute-force protection: max 6 failed password attempts per IP per 2 minutes
  _isLoginThrottled(ip) {
    const rec = this.failedLogins.get(ip);
    if (!rec) return false;
    if (Date.now() - rec.firstMs > 120000) { this.failedLogins.delete(ip); return false; }
    return rec.count >= 6;
  }

  _noteFailedLogin(ip) {
    const now = Date.now();
    const rec = this.failedLogins.get(ip);
    if (!rec || now - rec.firstMs > 120000) this.failedLogins.set(ip, { count: 1, firstMs: now });
    else rec.count++;
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
      console.log(`  VoxelCraft Server v${SERVER_VERSION} — Running on ${config.host}:${config.port}`);
      console.log(`  Web Client : http://localhost:${config.port}/`);
      console.log(`  WebSocket  : ws://<your-server-address>:${config.port}`);
      console.log(`  Status API : http://localhost:${config.port}/status`);
      console.log(`  World Seed : ${this.world.seed} | Mode: ${config.mode} | PvP: ${config.pvp ? 'ON' : 'OFF'}`);
      console.log(`  World File : ${WORLD_PATH} (${this.world.deltas.size} deltas)`);
      console.log(`  Accounts   : ${this.accounts.accounts.size} claimed`);
      console.log(`  Data Dir   : ${DATA_DIR}`);
      console.log(`  Client     : ${CLIENT_HTML_PATH}${fs.existsSync(CLIENT_HTML_PATH) ? '' : ' (NOT FOUND)'}`);
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

    // 2. Game simulation @ 20 Hz: world, redstone, furnaces, mobs, drops, player vitals
    let lastGameTick = Date.now();
    this._intervals.push(setInterval(() => {
      const now = Date.now();
      const dt = Math.min(0.25, Math.max(0, (now - lastGameTick) / 1000));
      lastGameTick = now;
      try { this.game.tick(dt); } catch (err) { console.error('[Game] Tick error:', err); }
    }, 50));

    // 3. World clock + Anti-Cheat VL decay @ 1 Hz (measured dt so timer drift never desyncs the clock)
    let lastTick = Date.now();
    this._intervals.push(setInterval(() => {
      const now = Date.now();
      const dt = Math.min(5, Math.max(0, (now - lastTick) / 1000));
      lastTick = now;
      this.world.updateTime(dt, DAY_LENGTH_SEC);
      this.ac.decayAll(dt);
      for (const [ip, rec] of this.failedLogins) {
        if (now - rec.firstMs > 120000) this.failedLogins.delete(ip);
      }
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
    if (this._stopping) return;
    this._stopping = true;
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
      // Don't hang forever on lingering keep-alive HTTP sockets
      if (exitProcess) setTimeout(() => process.exit(0), 1500).unref();
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
    if (!p || !p.pl) return;
    const prof = this.game.profileOf(p);
    if (!p.hasMoved) {
      // Never overwrite a saved position with the placeholder spawn of a player who hasn't moved yet
      const old = this.world.savedPlayers.get(p.name.toLowerCase());
      if (old) { prof.x = old.x; prof.y = old.y; prof.z = old.z; } else { prof.x = null; }
    }
    this.world.savedPlayers.set(p.name.toLowerCase(), prof);
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
        mobs: this.game.mobs.mobs.length,
        drops: this.game.drops.size,
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
        res.end(`Client file not found: ${CLIENT_HTML_PATH}\nPlace index.html next to server.js or pass --client <path>.`);
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

    const fwd = config.trustProxy ? req.headers['x-forwarded-for'] : null;
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
    let joinStarted = false;

    const ws = new WSConnection(
      socket,
      ip,
      (rawText) => {
        let msg;
        try { msg = JSON.parse(rawText); } catch { return; }
        if (!msg || typeof msg !== 'object') return;
        if (!session) {
          // Only one join attempt per socket; ignore everything else until it resolves
          if (msg.type === 'join' && !joinStarted) {
            joinStarted = true;
            this._handleJoin(ws, ip, msg).then((s) => { session = s; }).catch((err) => {
              console.error('[Join] Error:', err);
              ws.close(1011, 'Internal server error');
            });
          }
          return;
        }
        if (!this.ac.checkPacketRate(session)) return;
        this._handlePacket(session, msg);
      },
      () => {
        const c = (this.ipCounts.get(ip) || 1) - 1;
        if (c <= 0) this.ipCounts.delete(ip);
        else this.ipCounts.set(ip, c);

        if (session) session.online = false;
        if (session && this.players.get(session.id) === session) {
          this._savePlayerProfile(session);
          this.players.delete(session.id);
          if (this.world.dirty) this.scheduleWorldSave(1000);
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

  async _handleJoin(ws, ip, msg) {
    const reject = (code, reason) => {
      ws.sendJSON({ type: 'auth_error', reason });
      ws.close(code, reason);
      return null;
    };

    if (this.isBanned(null, ip)) return reject(4003, 'You are banned from this server.');
    if (this._isLoginThrottled(ip)) return reject(4006, 'Too many failed login attempts. Try again in a couple of minutes.');

    const authMode = (msg.authMode === 'register') ? 'register' : 'login';
    const rawName = String(msg.name || '').trim();
    const rawPassword = String(msg.password || '');

    const authRes = (authMode === 'register')
      ? await this.accounts.register(rawName, rawPassword)
      : await this.accounts.login(rawName, rawPassword);

    if (ws.closed) return null; // socket went away while hashing

    if (!authRes.ok) {
      if (authRes.badPassword) this._noteFailedLogin(ip);
      return reject(4004, authRes.reason);
    }
    this.failedLogins.delete(ip);

    const name = authRes.account.name;
    if (this.isBanned(name, ip)) return reject(4003, 'You are banned from this server.');

    // Same account reconnecting (e.g. after a dropped connection): the password proved ownership,
    // so replace the stale session instead of locking the player out until it times out.
    const existing = this.findPlayerByName(name);
    if (existing) {
      this._savePlayerProfile(existing);
      this.players.delete(existing.id);
      this.broadcast({ type: 'player_leave', id: existing.id, name: existing.name });
      this.kickPlayer(existing, 'Logged in from another location.');
    }

    if (this.players.size >= (config.maxPlayers || 32)) return reject(4002, 'Server is full.');

    const id = 'p' + (this.nextId++);
    const saved = this.world.savedPlayers.get(name.toLowerCase()) || null;

    const hasPos = !!(saved && Number.isFinite(saved.x) && Number.isFinite(saved.y) && Number.isFinite(saved.z));
    const player = {
      id, name, ip, ws,
      x: hasPos ? saved.x : 8.5,
      y: hasPos ? saved.y : 80,
      z: hasPos ? saved.z : 8.5,
      yaw: 0, pitch: 0,
      crouch: false, swing: 0, heldId: 0, onGround: true,
      hasMoved: hasPos,
      mode: (saved && saved.mode) || config.mode || 'survival',
      ping: 0,
      online: true,
      joinedAt: Date.now()
    };
    this.game.initPlayer(player, saved);

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
      player: {
        x: hasPos ? saved.x : null, y: hasPos ? saved.y : null, z: hasPos ? saved.z : null,
        health: player.pl.health, hunger: player.pl.hunger, saturation: player.pl.saturation,
        xp: player.pl.xp, level: player.pl.level, dead: !!player.pl.dead,
        slots: player.inv.slots, armor: player.inv.armor
      },
      players: existingPlayers,
      deltas: Array.from(this.world.deltas.values()),
      blockEntities: Array.from(this.world.blockEntities.entries()),
      ...this.game.welcomeExtras(player)
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
        this.game.handle(p, msg);
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

      default:
        // World, inventory, combat & mob packets are handled by the authoritative game layer
        if (typeof msg.type === 'string' && !msg.type.startsWith('_')) {
          try { this.game.handle(p, msg); } catch (err) { console.error(`[Game] Error handling '${msg.type}' from ${p.name}:`, err); }
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
      this.accounts.changePassword(sender.name, oldPw, newPw).then((res) => {
        reply(res.ok ? 'Account password updated.' : res.reason, res.ok ? 'sys' : 'err');
      });
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
      this.game.setMode(target, mode);
      target.ws.sendJSON({ type: 'mode_change', mode });
      reply(`Set ${target.name} to ${mode} mode.`);
    } else if (cmd === 'give') {
      const target = this.findPlayerByName(args[0]);
      const id = parseInt(args[1], 10);
      const cnt = Math.max(1, Math.min(64, parseInt(args[2], 10) || 1));
      if (!target || !isValidItemId(id)) { reply('Usage: /give <player> <itemId> [count]', 'warn'); return; }
      if (target.mode !== 'creative') target.ledger.add({ id, count: cnt });
      target.ws.sendJSON({ type: 'give_item', id, count: cnt });
      reply(`Gave ${cnt}x item #${id} to ${target.name}.`);
    } else if (cmd === 'tp') {
      if (args.length === 2) {
        const a = this.findPlayerByName(args[0]);
        const b = this.findPlayerByName(args[1]);
        if (!a || !b) { reply('Player(s) not found.', 'warn'); return; }
        a.x = b.x; a.y = b.y + 0.5; a.z = b.z; a.hasMoved = true;
        this.ac.grantTeleportGrace(a, 2000);
        a.ws.sendJSON({ type: 'rubberband', x: a.x, y: a.y, z: a.z, reason: `Teleported to ${b.name}.` });
        reply(`Teleported ${a.name} to ${b.name}.`);
      } else if (args.length === 4) {
        const a = this.findPlayerByName(args[0]);
        const tx = parseFloat(args[1]), ty = parseFloat(args[2]), tz = parseFloat(args[3]);
        if (!a || !Number.isFinite(tx) || !Number.isFinite(ty) || !Number.isFinite(tz)) { reply('Usage: /tp <player> <x> <y> <z>', 'warn'); return; }
        a.x = tx; a.y = ty; a.z = tz; a.hasMoved = true;
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
  // Keep the server alive for connected players, but make the failure visible
  console.error('[Error] Uncaught exception:', err);
});
process.on('unhandledRejection', (err) => {
  console.error('[Error] Unhandled promise rejection:', err);
});
