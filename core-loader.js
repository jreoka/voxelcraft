'use strict';

/**
 * Loads the game's simulation code (blocks, items, recipes, terrain generator, lighting,
 * redstone, mobs, player rules) straight out of the client HTML, so the multiplayer server
 * runs exactly the same rules as the browser. Only the DOM bootstrap at the very end of the
 * client script is skipped; everything else is plain class/function/const definitions.
 */

const fs = require('fs');
const vm = require('vm');

const EXPORTS = [
  'B', 'IT', 'BLOCKS', 'ITEMS', 'CRAFTING', 'ENCHANTS', 'MOB_DEFS', 'MOB_TYPES', 'WOOL_PALETTE',
  'TRADER_TRADES', 'CAMPFIRE_COOK', 'World', 'Chunk', 'MobManager', 'Player', 'PlayerPhysics',
  'Inventory', 'DayNightSystem', 'enchLvl', 'enchList', 'enchKindsOf', 'makeEnchant',
  'computeBlockDrops', 'computeEnchantOffers', 'computeEnchantPower',
  'floorDiv', 'floorMod', 'CHUNK_SIZE', 'SEA_LEVEL',
  // unlimited-coordinate helpers
  'ORIGIN_STEP', 'REBASE_DIST', 'toBig', 'bigToNum', 'snapOrigin', 'absIntStr', 'absDeltaKey', 'absJson',
  'parseAbsCoord', 'formatAbsCoord'
];

function loadGameCore(htmlPath) {
  const html = fs.readFileSync(htmlPath, 'utf8');
  const start = html.indexOf('<script>');
  const end = html.lastIndexOf('</script>');
  if (start < 0 || end < 0) throw new Error(`No <script> block found in ${htmlPath}`);
  let src = html.slice(start + '<script>'.length, end);
  const boot = src.lastIndexOf("window.addEventListener('DOMContentLoaded'");
  if (boot < 0) throw new Error('Could not find the client bootstrap (DOMContentLoaded) to strip');
  src = src.slice(0, boot);
  src += `\n;globalThis.__core = { ${EXPORTS.join(', ')} };`;

  const noop = () => {};
  const sandbox = {
    console, performance, setTimeout, clearTimeout, setInterval, clearInterval,
    window: { addEventListener: noop }, document: {}, navigator: {}
  };
  vm.createContext(sandbox);
  vm.runInContext(src, sandbox, { filename: htmlPath });
  const core = sandbox.__core;
  for (const name of EXPORTS) {
    if (core[name] === undefined) throw new Error(`Game core is missing "${name}" — client and server versions differ`);
  }
  return core;
}

module.exports = { loadGameCore };
