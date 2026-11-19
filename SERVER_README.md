# VoxelCraft VPS Multiplayer Server

A zero-dependency Node.js multiplayer server (`server.js`) with a built-in **Anti-Cheat engine**, **world persistence**, **interactive admin console**, and **integrated HTTP web client hosting** (serves `voxelcraft.html` directly on the same port).

---

## Quick Start (Local or VPS)

Requires **Node.js 18+**. No `npm install` is needed.

```bash
# Upload server.js and voxelcraft.html to your VPS (e.g. /opt/voxelcraft/)
node server.js
```

On first run, the server creates:
- `server-config.json` — server settings, OPs, bans, and anti-cheat thresholds
- `world-data.json` — persistent world block edits, chests/furnaces, time/weather, and player inventories/positions
- `anticheat.log` — timestamped log of every anti-cheat violation

### How Players Connect
Players have **two ways** to join:
1. **Direct Browser URL**: Open `http://<YOUR_VPS_IP>:25565` in any modern browser, click **Multiplayer · Connect to Server**, and click **Connect** (auto-fills your server address!).
2. **Local HTML File**: Open `voxelcraft.html` locally, click **Multiplayer · Connect to Server**, enter `ws://<YOUR_VPS_IP>:25565`, choose a username, and click **Connect**.

---

## Deploying on a Linux VPS (Ubuntu / Debian)

### 1. Install Node.js & Open Firewall Port
```bash
sudo apt update && sudo apt install -y nodejs
sudo ufw allow 25565/tcp
```

### 2. Copy Files
```bash
sudo mkdir -p /opt/voxelcraft
# Copy server.js, voxelcraft.html, and package.json into /opt/voxelcraft
```

### 3. Run as a Background `systemd` Service (Auto-Restart on Reboot)
Create `/etc/systemd/system/voxelcraft.service`:

```ini
[Unit]
Description=VoxelCraft Multiplayer Server
After=network.target

[Service]
Type=simple
WorkingDirectory=/opt/voxelcraft
ExecStart=/usr/bin/node /opt/voxelcraft/server.js --no-console
Restart=always
RestartSec=3
Environment=PORT=25565
Environment=HOST=0.0.0.0

[Install]
WantedBy=multi-user.target
```

Enable and start it:
```bash
sudo systemctl daemon-reload
sudo systemctl enable --now voxelcraft
sudo journalctl -u voxelcraft -f
```

### 4. Optional: HTTPS + `wss://` with Nginx & Let's Encrypt
If you have a domain (e.g. `play.example.com`), proxy HTTP & WebSocket over TLS:

```nginx
server {
    listen 443 ssl http2;
    server_name play.example.com;

    ssl_certificate     /etc/letsencrypt/live/play.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/play.example.com/privkey.pem;

    location / {
        proxy_pass http://127.0.0.1:25565;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_read_timeout 3600s;
    }
}
```

Then players can visit `https://play.example.com` and connect via `wss://play.example.com`.

---

## Client Graphics Overhaul (v2.1)

The browser client (`voxelcraft.html`) was overhauled. No server changes are needed; existing worlds load as before.

**Lighting**
- **Smooth lighting.** Each block-face corner averages the light of the 4 cells around it, so light fades gradually across caves, overhangs and torch-lit rooms instead of in flat steps.
- **Real-time sun and moon shadows.** A shadow map with soft PCF edges is cast from the sun by day and the moon by night, including leaf-shaped shadows from trees. Settings → *Sun & Moon Shadows*: High (72 blocks) / Low (44) / Off.
- **New shared lighting model.** Terrain, mobs, players, dropped items and particles all use the same lighting: hemispheric sky ambient, directional sun/moon light, and warm block light with a steeper falloff. Mobs and items no longer glow in dark caves.
- **Water** reflects the sky with Fresnel and has a shadowed sun glint. Glass and ice no longer scroll like fluids.
- **Sky:** sunset band toward the sun, Mie halo, moon glow, colored twinkling stars, dithering to remove banding, and sun-tinted fog in-scattering.
- Settings → *Brightness* slider.

**Dropped items**
- Blocks drop as fully textured mini-cubes (grass tinted, slabs half-height). Tools and items drop as 3D extruded pixel-art models built from their icons.
- Stacks show 1–4 copies depending on count. Items bob and spin, cast a soft blob shadow, and blink before they despawn.
- Physics: box collision, a small bounce, ground friction, floating in water, burning in lava, and being pushed out when buried.
- Nearby identical stacks merge. Pickup uses a magnetic pull and a short fly-to-player animation.

**First-person view and effects**
- The held item is rendered in 3D (textured block, extruded tool or item sprite, or the bare arm), with an equip animation and sway that lags behind the camera.
- Block-breaking debris uses the block's own texture. Cracks grow in 10 stages while mining.
- Torches emit flame and smoke particles; campfires emit fire and rising smoke.
- Mobs and players cast blob shadows.

**Fixes**
- Fixed the WebGL error `drawArrays: no buffer is bound to enabled attribute`, caused by stale vertex attributes pointing at deleted chunk buffers.
- Fixed water rendering speckled on some GPUs: the block-flag varying was interpolated, so some water pixels were classified as lava.
- Uniform locations are now cached, and chunks are drawn front-to-back for better early depth rejection.
- The shadow map is reused across frames and re-anchored to the camera, so shadows cost about one extra terrain pass every 3–5 frames.

---

## Built-in Anti-Cheat Protection

All checks run server-side with a decaying **Violation Level (VL)** system (warns at `warnVL`, auto-kicks at `kickVL`, logs to `anticheat.log`, and alerts online OPs):

| Check | What It Catches | Action |
|---|---|---|
| `SPEED` | Horizontal speed hacks (with lag/knockback leaky-bucket tolerance) | Rubberband + VL |
| `TELEPORT` | Instant blink / coordinate teleport hacks | Rubberband + VL |
| `ILLEGAL_FLY` | Client sending `flying: true` in Survival | Rubberband + VL |
| `FLY_HOVER` / `HIGH_JUMP` | Hovering or ascending without ground/water/ladder/slime-bounce | Rubberband + VL |
| `BAD_COORDS` | `NaN`, `Infinity`, or out-of-bounds coordinates | Rubberband + VL |
| `REACH` | Breaking, placing, or interacting with blocks beyond `6.9m` | Block revert + VL |
| `BEDROCK_BREAK` | Breaking Bedrock or `y <= 0` in Survival | Block revert + VL |
| `ILLEGAL_BLOCK` | Placing Bedrock or invalid block IDs | Block revert + VL |
| `NUKER` / `FAST_BREAK` | Breaking `> 12` blocks/sec or insta-mining Obsidian/Ores/Stone | Block revert + VL |
| `PVP_REACH` / `PVP_ANGLE` / `PVP_AUTOCLICK` | KillAura, reach hits, and rapid autoclickers + server-authoritative weapon damage | Cancel hit + VL |
| `ILLEGAL_INV` | Invalid item IDs, stacks `> 64`, or Bedrock in Survival inventory | Strip item + VL |
| `PACKET_FLOOD` & Rate Limits | Packet flooding, chat spam, oversized frames (>256 KB), >4 connections/IP | Drop / Disconnect |

---

## Commands (In-Game Chat `T` or VPS Console)

### Player Commands
- `/help` — Show available commands
- `/list` — List online players
- `/ping` — Show connection status & Anti-Cheat VL
- `/pos` — Show your current coordinates
- `/msg <player> <message>` — Send a private message

### Admin / OP Commands (Always available in VPS terminal; grant in-game via `/op <name>`)
- `/op <player>` / `/deop <player>` — Grant or revoke admin privileges
- `/kick <player> [reason]` — Kick a player
- `/ban <player> [reason]` / `/unban <player>` — Ban or unban a username or IP
- `/tp <p1> <p2>` or `/tp <player> <x> <y> <z>` — Teleport players
- `/gamemode <survival|creative> [player]` — Switch game mode
- `/give <player> <itemId> [count]` — Give items/blocks to a player
- `/time <day|noon|sunset|night|0..1>` — Set world time
- `/weather <clear|rain|thunder>` — Set world weather
- `/pvp <on|off>` — Toggle PvP combat
- `/ac [status|toggle|reset <player>]` — Inspect or manage Anti-Cheat
- `/save` — Force-save world & players immediately
- `/say <message>` — Broadcast a server announcement
- `/stop` *(console only)* — Save world and shut down cleanly

---

## Status Endpoint

Query `GET http://<YOUR_VPS_IP>:25565/status` for JSON server stats (players online, uptime, day, weather, and anti-cheat counters).
