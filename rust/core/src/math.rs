use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use num_bigint::BigInt;
use num_traits::{Signed, ToPrimitive, Zero};
use serde_json::Value;

pub const CHUNK_SIZE: i32 = 16;
pub const CHUNK_VOL: usize = (CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE) as usize;
pub const SEA_LEVEL: i32 = 46;

pub const ORIGIN_STEP: i64 = 15 * 1_048_576;
pub const REBASE_DIST: f64 = ORIGIN_STEP as f64;
pub const NOISE_REGION: f64 = 1_048_576.0;
pub const PLAIN_LIMIT: f64 = (1_i64 << 36) as f64;
pub const NUM_SAFE: i64 = 1_i64 << 50;

pub static ORIGIN_STEP_BIG: LazyLock<BigInt> = LazyLock::new(|| BigInt::from(ORIGIN_STEP));
pub static NUM_SAFE_BIG: LazyLock<BigInt> = LazyLock::new(|| BigInt::from(NUM_SAFE));
pub static BIG_2_64: LazyLock<BigInt> = LazyLock::new(|| BigInt::from(1u64) << 64);
pub static BIG_2_63: LazyLock<BigInt> = LazyLock::new(|| BigInt::from(1u64) << 63);
pub static BIG_2_41: LazyLock<BigInt> = LazyLock::new(|| BigInt::from(1u64) << 41);
pub static SUPER: LazyLock<BigInt> = LazyLock::new(|| BigInt::from(1u64) << 56);
pub static SUPER_HALF: LazyLock<BigInt> = LazyLock::new(|| BigInt::from(1u64) << 55);
pub const SUPER_BLEND: f64 = 4096.0;
pub const SUPER_BLEND_HALF: i64 = 2048;

#[inline]
pub fn clamp(v: f64, lo: f64, hi: f64) -> f64 {
    if v < lo {
        lo
    } else if v > hi {
        hi
    } else {
        v
    }
}

#[inline]
pub fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

#[inline]
pub fn smoothstep(t: f64) -> f64 {
    t * t * (3.0 - 2.0 * t)
}

#[inline]
pub fn floor_div(a: i32, b: i32) -> i32 {
    a.div_euclid(b)
}

#[inline]
pub fn floor_mod(a: i32, b: i32) -> i32 {
    a.rem_euclid(b)
}

#[inline]
pub fn floor_div_i64(a: i64, b: i64) -> i64 {
    a.div_euclid(b)
}

#[inline]
pub fn floor_mod_i64(a: i64, b: i64) -> i64 {
    a.rem_euclid(b)
}

#[inline]
pub fn js_math_round(v: f64) -> f64 {
    (v + 0.5).floor()
}

/// FNV-1a 32-bit hash matching JS `hashString(str)`.
pub fn hash_string(s: &str) -> u32 {
    let mut h: u32 = 2166136261;
    for unit in s.encode_utf16() {
        h ^= unit as u32;
        h = h.wrapping_mul(16777619);
    }
    if h == 0 { 1337 } else { h }
}

/// Core 2D integer hash on explicit 32-bit words matching JS `hash2w`.
#[inline]
pub fn hash2w(x_lo: i32, x_hi: i32, z_lo: i32, z_hi: i32, seed: u32, ext: u32) -> u32 {
    let mut h: u32 = seed ^ 0x9e37_79b9;
    h = (h ^ (x_lo as u32)).wrapping_mul(0x85eb_ca6b);
    let xh_mul = ((x_hi as f64) * (0x27d4_eb2d_u32 as f64)) as i64 as u32;
    h = (h ^ xh_mul).wrapping_mul(0xc2b2_ae35);
    h = (h ^ (z_lo as u32)).wrapping_mul(0x1656_67b1);
    let zh_mul = ((z_hi as f64) * (0x9e37_79b1_u32 as f64)) as i64 as u32;
    h = (h ^ zh_mul).wrapping_mul(0x85eb_ca6b);
    if ext != 0 {
        h = (h ^ ext).wrapping_mul(0x2c1b_3c6d);
        h ^= h >> 13;
    }
    h ^= h >> 16;
    h = h.wrapping_mul(0x7feb_352d);
    h ^= h >> 15;
    h
}

#[inline]
pub fn hash2i(x: i64, z: i64, seed: u32) -> u32 {
    let x_lo = x as i32;
    let x_hi = (x >> 32) as i32;
    let z_lo = z as i32;
    let z_hi = (z >> 32) as i32;
    hash2w(x_lo, x_hi, z_lo, z_hi, seed, 0)
}

#[inline]
pub fn hash_mix_y(h: u32, y: i32) -> u32 {
    let mut h = (h ^ (y as u32)).wrapping_mul(0x27d4_eb2d);
    h ^= h >> 15;
    h
}

#[inline]
pub fn hash3i(x: i64, y: i32, z: i64, seed: u32) -> u32 {
    hash_mix_y(hash2i(x, z, seed), y)
}

#[inline]
pub fn hash_float2(x: i64, z: i64, seed: u32) -> f64 {
    (hash2i(x, z, seed) as f64) / 4294967296.0
}

pub fn big_floor_div(a: &BigInt, b: &BigInt) -> BigInt {
    let q = a / b;
    let r = a % b;
    if !r.is_zero() && (a.is_negative() != b.is_negative()) {
        q - 1
    } else {
        q
    }
}

pub fn big_mod(a: &BigInt, m: &BigInt) -> BigInt {
    let r = a % m;
    if r.is_negative() { r + m } else { r }
}

pub fn big_to_num(b: &BigInt) -> Option<i64> {
    if b > &(-&*NUM_SAFE_BIG) && b < &*NUM_SAFE_BIG {
        b.to_i64()
    } else {
        None
    }
}

pub fn to_big_str(s: &str) -> Result<BigInt, String> {
    s.trim()
        .parse::<BigInt>()
        .map_err(|e| format!("Invalid BigInt '{}': {}", s, e))
}

pub fn to_big_value(v: &Value) -> Result<BigInt, String> {
    match v {
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(BigInt::from(i))
            } else if let Some(f) = n.as_f64() {
                if f.is_finite() {
                    Ok(BigInt::from(f.floor() as i64))
                } else {
                    Err("Non-finite number".to_string())
                }
            } else {
                Err("Invalid number".to_string())
            }
        }
        Value::String(s) => to_big_str(s),
        _ => Err("Expected number or string".to_string()),
    }
}

pub fn snap_origin(b: &BigInt) -> BigInt {
    let half = &*ORIGIN_STEP_BIG / 2;
    big_floor_div(&(b + half), &ORIGIN_STEP_BIG) * &*ORIGIN_STEP_BIG
}

pub fn abs_int_str(o: &BigInt, o_num: Option<i64>, l: i64) -> String {
    if let Some(on) = o_num {
        let v = on + l;
        if v > -9_007_199_254_740_992 && v < 9_007_199_254_740_992 {
            return v.to_string();
        }
    }
    (o + BigInt::from(l)).to_string()
}

pub fn abs_delta_key(xs: &Value, y: i32, zs: &Value) -> (String, usize) {
    let split = |v: &Value| -> (String, usize) {
        if let Some(n) = v.as_i64() {
            return (
                floor_div_i64(n, CHUNK_SIZE as i64).to_string(),
                floor_mod_i64(n, CHUNK_SIZE as i64) as usize,
            );
        }
        let s = match v {
            Value::String(st) => st.clone(),
            _ => v.to_string(),
        };
        if s.len() < 16
            && let Ok(n) = s.parse::<i64>() {
                return (
                    floor_div_i64(n, CHUNK_SIZE as i64).to_string(),
                    floor_mod_i64(n, CHUNK_SIZE as i64) as usize,
                );
            }
        let b = s.parse::<BigInt>().unwrap_or_default();
        let cs = BigInt::from(CHUNK_SIZE);
        let cx = big_floor_div(&b, &cs);
        let lx = big_mod(&b, &cs).to_usize().unwrap_or(0);
        (cx.to_string(), lx)
    };
    let (cx, lx) = split(xs);
    let (cz, lz) = split(zs);
    let cy = floor_div(y, CHUNK_SIZE);
    let ly = floor_mod(y, CHUNK_SIZE) as usize;
    let key = format!("{},{},{}", cx, cy, cz);
    let idx = (ly << 8) | (lz << 4) | lx;
    (key, idx)
}

pub fn abs_json(s: &str) -> Value {
    if s.len() < 16
        && let Ok(n) = s.parse::<i64>() {
            return Value::Number(n.into());
        }
    Value::String(s.to_string())
}

pub fn abs_coord_json(v: &Value) -> Option<Value> {
    if let Some(n) = v.as_i64() {
        if (-9_007_199_254_740_991..=9_007_199_254_740_991).contains(&n) {
            return Some(Value::Number(n.into()));
        }
        return Some(Value::String(n.to_string()));
    }
    if let Some(f) = v.as_f64() {
        if f.is_finite() {
            let fl = f.floor();
            if fl.abs() <= 9_007_199_254_740_991.0 {
                return Some(Value::Number((fl as i64).into()));
            }
        }
        return None;
    }
    let s = v.as_str()?.trim();
    if s.is_empty() {
        return None;
    }
    let bytes = s.as_bytes();
    let start = if bytes[0] == b'-' { 1 } else { 0 };
    if start >= bytes.len() || !bytes[start..].iter().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if s.len() < 16 {
        s.parse::<i64>().ok().map(|n| Value::Number(n.into()))
    } else {
        s.parse::<BigInt>().ok().map(|b| Value::String(b.to_string()))
    }
}

pub fn parse_abs_coord(s: &str) -> Option<(BigInt, f64)> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    let (neg, rest) = if let Some(r) = t.strip_prefix('-') {
        (true, r)
    } else if let Some(r) = t.strip_prefix('+') {
        (false, r)
    } else {
        (false, t)
    };
    let (int_part, frac_part) = match rest.split_once('.') {
        Some((a, b)) => (a, Some(b)),
        None => (rest, None),
    };
    if !int_part.is_empty()
        && int_part.bytes().all(|b| b.is_ascii_digit())
        && frac_part.is_none_or(|f| f.bytes().all(|b| b.is_ascii_digit()))
    {
        let mut i = int_part.parse::<BigInt>().ok()?;
        let mut f = match frac_part {
            Some(fp) if !fp.is_empty() => format!("0.{}", fp).parse::<f64>().unwrap_or(0.0),
            _ => 0.0,
        };
        if neg {
            i = -i;
            if f > 0.0 {
                i -= 1;
                f = 1.0 - f;
            }
        }
        return Some((i, f));
    }
    let n = t.parse::<f64>().ok()?;
    if !n.is_finite() {
        return None;
    }
    let fl = n.floor();
    Some((BigInt::from(fl as i64), n - fl))
}

pub fn format_abs_coord(o: &BigInt, o_num: Option<i64>, l: f64, digits: usize) -> String {
    if let Some(on) = o_num
        && on.abs() < (1_i64 << 44) {
            return format!("{:.*}", digits, (on as f64) + l);
        }
    let fl = l.floor();
    let mut ip = o + BigInt::from(fl as i64);
    let fr = l - fl;
    let mut fs = format!("{:.*}", digits, fr);
    if fs.starts_with('1') {
        ip += 1;
        fs = format!("{:.*}", digits, 0.0_f64);
    }
    if ip.is_negative() && fr > 0.0 {
        let nf = format!("{:.*}", digits, 1.0 - fr);
        let neg_ip = -(ip + BigInt::from(1));
        return format!("-{}{}", neg_ip, &nf[1..]);
    }
    format!("{}{}", ip, &fs[1..])
}

fn ext_word(b: &BigInt) -> u32 {
    if b < &*BIG_2_63 && b >= &(-&*BIG_2_63) {
        return 0;
    }
    let q = big_floor_div(b, &BIG_2_63);
    hash_string(&q.to_string()) | 1
}

#[derive(Clone, Debug)]
pub struct AbsAxis {
    pub o: BigInt,
    pub num: Option<i64>,
    pub lo: u64,
    pub hi: u64,
    pub ext: i64, // >=0 cached, -1 dynamic
    pub w_lo: i32,
    pub w_hi: i32,
    pub w_ext: u32,
}

impl AbsAxis {
    pub fn new(o: BigInt) -> Self {
        let num = big_to_num(&o);
        let m = big_mod(&o, &BIG_2_64);
        let mask32 = BigInt::from(0xffff_ffff_u64);
        let lo = (&m & &mask32).to_u64().unwrap_or(0);
        let hi_big: BigInt = &m >> 32;
        let hi = hi_big.to_u64().unwrap_or(0);
        let e0 = ext_word(&(&o - &*BIG_2_41));
        let e1 = ext_word(&(&o + &*BIG_2_41));
        let ext = if e0 == e1 { e0 as i64 } else { -1 };
        Self {
            o,
            num,
            lo,
            hi,
            ext,
            w_lo: 0,
            w_hi: 0,
            w_ext: 0,
        }
    }

    pub fn words(&mut self, l: i64) {
        let s = (self.lo as i128) + (l as i128);
        let carry = s.div_euclid(4294967296);
        self.w_lo = (s - carry * 4294967296) as i32;
        self.w_hi = ((self.hi as i128) + carry).rem_euclid(4294967296) as i32;
        self.w_ext = if self.ext >= 0 {
            self.ext as u32
        } else {
            ext_word(&(&self.o + BigInt::from(l)))
        };
    }
}

pub fn exact_mul_mod_256(r: &BigInt, c: f64) -> f64 {
    if c == 0.0 || r.is_zero() {
        return 0.0;
    }
    let bits = c.to_bits();
    let sign = if (bits >> 63) != 0 { -1_i64 } else { 1_i64 };
    let raw_exp = ((bits >> 52) & 0x7ff) as i32;
    let frac = bits & ((1u64 << 52) - 1);
    let (mant_u, exp) = if raw_exp == 0 {
        (frac, -1022 - 52)
    } else {
        (frac | (1u64 << 52), raw_exp - 1023 - 52)
    };
    let mant = BigInt::from(sign) * BigInt::from(mant_u);
    let p = r * mant;
    if exp >= 0 {
        return big_mod(&(p << (exp as usize)), &BigInt::from(256)).to_f64().unwrap_or(0.0);
    }
    let d = (-exp) as usize;
    let modulus = BigInt::from(256) << d;
    let rem = big_mod(&p, &modulus);
    let ip = &rem >> d;
    let frac_big = &rem - (&ip << d);
    ip.to_f64().unwrap_or(0.0) + frac_big.to_f64().unwrap_or(0.0) / 2.0_f64.powi(-exp)
}

static K_CACHE: LazyLock<Mutex<HashMap<BigInt, HashMap<u64, f64>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

static SHIFT_CACHE: LazyLock<Mutex<HashMap<BigInt, BigInt>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn k_term(r: &BigInt, c: f64) -> f64 {
    let mut guard = K_CACHE.lock().unwrap();
    if guard.len() > 96 {
        guard.clear();
    }
    let entry = guard.entry(r.clone()).or_default();
    let key = c.to_bits();
    if let Some(&v) = entry.get(&key) {
        return v;
    }
    let v = exact_mul_mod_256(r, c);
    entry.insert(key, v);
    v
}

pub fn super_shift(s: &BigInt) -> BigInt {
    if s.is_zero() {
        return BigInt::zero();
    }
    let mut guard = SHIFT_CACHE.lock().unwrap();
    if let Some(v) = guard.get(s) {
        return v.clone();
    }
    let ss = s.to_string();
    let ha: BigInt = BigInt::from(hash_string(&format!("vxs-a{}", ss))) << 48;
    let hb: BigInt = BigInt::from(hash_string(&format!("vxs-b{}", ss))) << 24;
    let hc: BigInt = BigInt::from(hash_string(&format!("vxs-c{}", ss)));
    let v: BigInt = ha ^ hb ^ hc;
    if guard.len() > 64 {
        guard.clear();
    }
    guard.insert(s.clone(), v.clone());
    v
}

#[derive(Clone, Debug)]
pub struct AxisVariant {
    pub r: BigInt,
    pub w: f64,
}

#[derive(Clone, Debug)]
pub struct NoiseFrame {
    pub ax: AbsAxis,
    pub az: AbsAxis,
    pub xq: f64,
    pub zq: f64,
    pub x_v: Option<Vec<AxisVariant>>,
    pub z_v: Option<Vec<AxisVariant>>,
    pub plain: bool,
    last_lx: f64,
    last_lz: f64,
}

impl Default for NoiseFrame {
    fn default() -> Self {
        Self::new()
    }
}

impl NoiseFrame {
    pub fn new() -> Self {
        Self {
            ax: AbsAxis::new(BigInt::zero()),
            az: AbsAxis::new(BigInt::zero()),
            xq: 0.0,
            zq: 0.0,
            x_v: None,
            z_v: None,
            plain: true,
            last_lx: f64::NAN,
            last_lz: f64::NAN,
        }
    }

    pub fn set_origin(&mut self, ax: AbsAxis, az: AbsAxis) {
        self.ax = ax;
        self.az = az;
        self.last_lx = f64::NAN;
        self.last_lz = f64::NAN;
    }

    fn axis(a: &AbsAxis, l: f64) -> (f64, Option<Vec<AxisVariant>>) {
        if let Some(an) = a.num {
            let abs_a = (an as f64) + l;
            if abs_a > -PLAIN_LIMIT && abs_a < PLAIN_LIMIT {
                return (abs_a, None);
            }
        }
        let k = (l / NOISE_REGION).floor() * NOISE_REGION;
        let r = &a.o + BigInt::from(k as i64);
        let q = l - k;
        let abs_a = &r + BigInt::from(q.floor() as i64);
        let s = big_floor_div(&(&abs_a + &*SUPER_HALF), &SUPER);
        let t = &abs_a + &*SUPER_HALF - &s * &*SUPER;
        let make_v = |sv: BigInt, w: f64| AxisVariant {
            r: &r + super_shift(&sv),
            w,
        };
        if t < BigInt::from(SUPER_BLEND_HALF) {
            let w = smoothstep(0.5 + t.to_f64().unwrap_or(0.0) / SUPER_BLEND);
            return (q, Some(vec![make_v(s.clone(), w), make_v(s - 1, 1.0 - w)]));
        }
        let d = &*SUPER - &t;
        if d <= BigInt::from(SUPER_BLEND_HALF) {
            let w = smoothstep(0.5 + d.to_f64().unwrap_or(0.0) / SUPER_BLEND);
            return (q, Some(vec![make_v(s.clone(), w), make_v(s + 1, 1.0 - w)]));
        }
        (q, Some(vec![make_v(s, 1.0)]))
    }

    pub fn set(&mut self, lx: f64, lz: f64) {
        if lx == self.last_lx && lz == self.last_lz {
            return;
        }
        self.last_lx = lx;
        self.last_lz = lz;
        let (xq, xv) = Self::axis(&self.ax, lx);
        let (zq, zv) = Self::axis(&self.az, lz);
        self.xq = xq;
        self.x_v = xv;
        self.zq = zq;
        self.z_v = zv;
        self.plain = self.x_v.is_none() && self.z_v.is_none();
    }
}

#[derive(Clone, Debug)]
pub struct SeededRng {
    pub state: u32,
}

impl SeededRng {
    pub fn new(seed: u32) -> Self {
        Self {
            state: if seed == 0 { 0x6d2b_79f5 } else { seed },
        }
    }

    #[expect(clippy::should_implement_trait, reason = "Matches JS SeededRNG.from_str helper")]
    pub fn from_str(seed: &str) -> Self {
        Self::new(hash_string(seed))
    }

    #[expect(clippy::should_implement_trait, reason = "Matches JS SeededRNG.next() API")]
    pub fn next(&mut self) -> f64 {
        self.state = self.state.wrapping_add(0x6d2b_79f5);
        let mut t = self.state;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        ((t ^ (t >> 14)) as f64) / 4294967296.0
    }

    pub fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + self.next() * (hi - lo)
    }

    pub fn int(&mut self, lo: i32, hi: i32) -> i32 {
        lo + (self.next() * ((hi - lo + 1) as f64)).floor() as i32
    }

    pub fn pick<'a, T>(&mut self, arr: &'a [T]) -> &'a T {
        let idx = (self.next() * (arr.len() as f64)).floor() as usize;
        &arr[idx.min(arr.len() - 1)]
    }
}

/// Standalone mulberry32 RNG matching JS `seededRng(seed)` (note: does NOT remap seed=0!).
pub struct RawSeededRng {
    a: u32,
}

impl RawSeededRng {
    pub fn new(seed: u32) -> Self {
        Self { a: seed }
    }

    #[expect(clippy::should_implement_trait, reason = "Matches JS seededRng() next API")]
    pub fn next(&mut self) -> f64 {
        self.a = self.a.wrapping_add(0x6d2b_79f5);
        let mut t = self.a;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        ((t ^ (t >> 14)) as f64) / 4294967296.0
    }
}

const GRAD3: [[f64; 3]; 12] = [
    [1.0, 1.0, 0.0],
    [-1.0, 1.0, 0.0],
    [1.0, -1.0, 0.0],
    [-1.0, -1.0, 0.0],
    [1.0, 0.0, 1.0],
    [-1.0, 0.0, 1.0],
    [1.0, 0.0, -1.0],
    [-1.0, 0.0, -1.0],
    [0.0, 1.0, 1.0],
    [0.0, -1.0, 1.0],
    [0.0, 1.0, -1.0],
    [0.0, -1.0, -1.0],
];

#[derive(Clone)]
pub struct NoiseEngine {
    pub seed: u32,
    perm: [u8; 512],
    perm12: [u8; 512],
    f2: f64,
    g2: f64,
    f3: f64,
    g3: f64,
}

impl NoiseEngine {
    pub fn new(seed: u32) -> Self {
        let mut rng = SeededRng::new(seed);
        let mut p = [0u8; 256];
        for (i, slot) in p.iter_mut().enumerate() {
            *slot = i as u8;
        }
        for i in (1..256).rev() {
            let j = (rng.next() * ((i + 1) as f64)).floor() as usize;
            p.swap(i, j);
        }
        let mut perm = [0u8; 512];
        let mut perm12 = [0u8; 512];
        for i in 0..512 {
            perm[i] = p[i & 255];
            perm12[i] = perm[i] % 12;
        }
        let sqrt3 = 3.0_f64.sqrt();
        Self {
            seed,
            perm,
            perm12,
            f2: 0.5 * (sqrt3 - 1.0),
            g2: (3.0 - sqrt3) / 6.0,
            f3: 1.0 / 3.0,
            g3: 1.0 / 6.0,
        }
    }

    pub fn simplex2(&self, xin: f64, yin: f64) -> f64 {
        let s = (xin + yin) * self.f2;
        let i = (xin + s).floor();
        let j = (yin + s).floor();
        let t = (i + j) * self.g2;
        let x0 = xin - (i - t);
        let y0 = yin - (j - t);
        let ii = (((i % 256.0) + 256.0) as i64 & 255) as usize;
        let jj = (((j % 256.0) + 256.0) as i64 & 255) as usize;
        self.simplex2_tail(ii, jj, x0, y0)
    }

    fn simplex2_tail(&self, ii: usize, jj: usize, x0: f64, y0: f64) -> f64 {
        let g2 = self.g2;
        let (i1, j1) = if x0 > y0 { (1usize, 0usize) } else { (0usize, 1usize) };
        let x1 = x0 - (i1 as f64) + g2;
        let y1 = y0 - (j1 as f64) + g2;
        let x2 = x0 - 1.0 + 2.0 * g2;
        let y2 = y0 - 1.0 + 2.0 * g2;
        let mut n0 = 0.0;
        let mut t0 = 0.5 - x0 * x0 - y0 * y0;
        if t0 >= 0.0 {
            t0 *= t0;
            let g = GRAD3[self.perm12[ii + self.perm[jj] as usize] as usize];
            n0 = t0 * t0 * (g[0] * x0 + g[1] * y0);
        }
        let mut n1 = 0.0;
        let mut t1 = 0.5 - x1 * x1 - y1 * y1;
        if t1 >= 0.0 {
            t1 *= t1;
            let g = GRAD3[self.perm12[ii + i1 + self.perm[jj + j1] as usize] as usize];
            n1 = t1 * t1 * (g[0] * x1 + g[1] * y1);
        }
        let mut n2 = 0.0;
        let mut t2 = 0.5 - x2 * x2 - y2 * y2;
        if t2 >= 0.0 {
            t2 *= t2;
            let g = GRAD3[self.perm12[ii + 1 + self.perm[jj + 1] as usize] as usize];
            n2 = t2 * t2 * (g[0] * x2 + g[1] * y2);
        }
        70.0 * (n0 + n1 + n2)
    }

    pub fn simplex3(&self, xin: f64, yin: f64, zin: f64) -> f64 {
        let s = (xin + yin + zin) * self.f3;
        let i = (xin + s).floor();
        let j = (yin + s).floor();
        let k = (zin + s).floor();
        let t = (i + j + k) * self.g3;
        let x0 = xin - (i - t);
        let y0 = yin - (j - t);
        let z0 = zin - (k - t);
        let ii = (((i % 256.0) + 256.0) as i64 & 255) as usize;
        let jj = (((j % 256.0) + 256.0) as i64 & 255) as usize;
        let kk = (((k % 256.0) + 256.0) as i64 & 255) as usize;
        self.simplex3_tail(ii, jj, kk, x0, y0, z0)
    }

    fn simplex3_tail(&self, ii: usize, jj: usize, kk: usize, x0: f64, y0: f64, z0: f64) -> f64 {
        let g3 = self.g3;
        let (i1, j1, k1, i2, j2, k2) = if x0 >= y0 {
            if y0 >= z0 {
                (1, 0, 0, 1, 1, 0)
            } else if x0 >= z0 {
                (1, 0, 0, 1, 0, 1)
            } else {
                (0, 0, 1, 1, 0, 1)
            }
        } else if y0 < z0 {
            (0, 0, 1, 0, 1, 1)
        } else if x0 < z0 {
            (0, 1, 0, 0, 1, 1)
        } else {
            (0, 1, 0, 1, 1, 0)
        };
        let x1 = x0 - (i1 as f64) + g3;
        let y1 = y0 - (j1 as f64) + g3;
        let z1 = z0 - (k1 as f64) + g3;
        let x2 = x0 - (i2 as f64) + 2.0 * g3;
        let y2 = y0 - (j2 as f64) + 2.0 * g3;
        let z2 = z0 - (k2 as f64) + 2.0 * g3;
        let x3 = x0 - 1.0 + 3.0 * g3;
        let y3 = y0 - 1.0 + 3.0 * g3;
        let z3 = z0 - 1.0 + 3.0 * g3;

        let mut n0 = 0.0;
        let mut t0 = 0.6 - x0 * x0 - y0 * y0 - z0 * z0;
        if t0 >= 0.0 {
            t0 *= t0;
            let g = GRAD3[self.perm12[ii + self.perm[jj + self.perm[kk] as usize] as usize] as usize];
            n0 = t0 * t0 * (g[0] * x0 + g[1] * y0 + g[2] * z0);
        }
        let mut n1 = 0.0;
        let mut t1 = 0.6 - x1 * x1 - y1 * y1 - z1 * z1;
        if t1 >= 0.0 {
            t1 *= t1;
            let g = GRAD3
                [self.perm12[ii + i1 + self.perm[jj + j1 + self.perm[kk + k1] as usize] as usize] as usize];
            n1 = t1 * t1 * (g[0] * x1 + g[1] * y1 + g[2] * z1);
        }
        let mut n2 = 0.0;
        let mut t2 = 0.6 - x2 * x2 - y2 * y2 - z2 * z2;
        if t2 >= 0.0 {
            t2 *= t2;
            let g = GRAD3
                [self.perm12[ii + i2 + self.perm[jj + j2 + self.perm[kk + k2] as usize] as usize] as usize];
            n2 = t2 * t2 * (g[0] * x2 + g[1] * y2 + g[2] * z2);
        }
        let mut n3 = 0.0;
        let mut t3 = 0.6 - x3 * x3 - y3 * y3 - z3 * z3;
        if t3 >= 0.0 {
            t3 *= t3;
            let g = GRAD3
                [self.perm12[ii + 1 + self.perm[jj + 1 + self.perm[kk + 1] as usize] as usize] as usize];
            n3 = t3 * t3 * (g[0] * x3 + g[1] * y3 + g[2] * z3);
        }
        32.0 * (n0 + n1 + n2 + n3)
    }

    pub fn fbm2(&self, x: f64, z: f64, octaves: usize, lacunarity: f64, gain: f64) -> f64 {
        let (mut sum, mut amp, mut freq, mut norm) = (0.0, 1.0, 1.0, 0.0);
        for _ in 0..octaves {
            sum += amp * self.simplex2(x * freq, z * freq);
            norm += amp;
            amp *= gain;
            freq *= lacunarity;
        }
        sum / norm
    }

    pub fn ridged2(&self, x: f64, z: f64, octaves: usize, lacunarity: f64, gain: f64) -> f64 {
        let (mut sum, mut amp, mut freq, mut norm) = (0.0, 1.0, 1.0, 0.0);
        for _ in 0..octaves {
            let n = 1.0 - self.simplex2(x * freq, z * freq).abs();
            sum += amp * (n * n);
            norm += amp;
            amp *= gain;
            freq *= lacunarity;
        }
        sum / norm
    }

    pub fn s2(&self, f: &NoiseFrame, x: f64, z: f64, s: f64, ox: f64, oz: f64) -> f64 {
        if f.plain {
            return self.simplex2(x * s + ox, z * s + oz);
        }
        self.simplex2_x(f, x, z, s, ox, oz)
    }

    #[expect(clippy::too_many_arguments, reason = "Matches JS NoiseEngine.s3 signature")]
    pub fn s3(
        &self,
        f: &NoiseFrame,
        x: f64,
        y: f64,
        z: f64,
        s: f64,
        sy: f64,
        ox: f64,
        oy: f64,
        oz: f64,
    ) -> f64 {
        if f.plain {
            return self.simplex3(x * s + ox, y * sy + oy, z * s + oz);
        }
        self.simplex3_x(f, x, y, z, s, sy, ox, oy, oz)
    }

    pub fn fbm2_f(&self, f: &NoiseFrame, x: f64, z: f64, s: f64, octaves: usize) -> f64 {
        if f.plain {
            return self.fbm2(x * s, z * s, octaves, 2.0, 0.5);
        }
        let (mut sum, mut amp, mut freq, mut norm) = (0.0, 1.0, 1.0, 0.0);
        for _ in 0..octaves {
            sum += amp * self.simplex2_x(f, x, z, s * freq, 0.0, 0.0);
            norm += amp;
            amp *= 0.5;
            freq *= 2.0;
        }
        sum / norm
    }

    pub fn ridged2_f(&self, f: &NoiseFrame, x: f64, z: f64, s: f64, octaves: usize) -> f64 {
        if f.plain {
            return self.ridged2(x * s, z * s, octaves, 2.05, 0.5);
        }
        let (mut sum, mut amp, mut freq, mut norm) = (0.0, 1.0, 1.0, 0.0);
        for _ in 0..octaves {
            let n = 1.0 - self.simplex2_x(f, x, z, s * freq, 0.0, 0.0).abs();
            sum += amp * (n * n);
            norm += amp;
            amp *= 0.5;
            freq *= 2.05;
        }
        sum / norm
    }

    fn simplex2_x(&self, f: &NoiseFrame, x: f64, z: f64, s: f64, ox: f64, oz: f64) -> f64 {
        let xv: &[Option<&AxisVariant>] = match &f.x_v {
            Some(v) => &v.iter().map(Some).collect::<Vec<_>>(),
            None => &[None],
        };
        let zv: &[Option<&AxisVariant>] = match &f.z_v {
            Some(v) => &v.iter().map(Some).collect::<Vec<_>>(),
            None => &[None],
        };
        if xv.len() == 1 && zv.len() == 1 {
            return self.simplex2_v(x, z, s, ox, oz, xv[0], zv[0]);
        }
        let mut sum = 0.0;
        for &vx in xv {
            for &vz in zv {
                let wx = vx.map_or(1.0, |v| v.w);
                let wz = vz.map_or(1.0, |v| v.w);
                sum += wx * wz * self.simplex2_v(x, z, s, ox, oz, vx, vz);
            }
        }
        sum
    }

    #[expect(clippy::too_many_arguments, reason = "Matches JS NoiseEngine._simplex2V signature")]
    fn simplex2_v(
        &self,
        x: f64,
        z: f64,
        s: f64,
        ox: f64,
        oz: f64,
        vx: Option<&AxisVariant>,
        vz: Option<&AxisVariant>,
    ) -> f64 {
        let xin = x * s + ox;
        let yin = z * s + oz;
        let sk = (xin + yin) * self.f2;
        let mut xs = xin + sk;
        let mut ys = yin + sk;
        if let Some(v) = vx {
            xs += k_term(&v.r, s * (1.0 + self.f2));
            ys += k_term(&v.r, s * self.f2);
        }
        if let Some(v) = vz {
            xs += k_term(&v.r, s * self.f2);
            ys += k_term(&v.r, s * (1.0 + self.f2));
        }
        let i = xs.floor();
        let j = ys.floor();
        let fx = xs - i;
        let fy = ys - j;
        let u = (fx + fy) * self.g2;
        let ii = (((i % 256.0) + 256.0) as i64 & 255) as usize;
        let jj = (((j % 256.0) + 256.0) as i64 & 255) as usize;
        self.simplex2_tail(ii, jj, fx - u, fy - u)
    }

    #[expect(clippy::too_many_arguments, reason = "Matches JS NoiseEngine._simplex3X signature")]
    fn simplex3_x(
        &self,
        f: &NoiseFrame,
        x: f64,
        y: f64,
        z: f64,
        s: f64,
        sy: f64,
        ox: f64,
        oy: f64,
        oz: f64,
    ) -> f64 {
        let xv: Vec<Option<&AxisVariant>> = match &f.x_v {
            Some(v) => v.iter().map(Some).collect(),
            None => vec![None],
        };
        let zv: Vec<Option<&AxisVariant>> = match &f.z_v {
            Some(v) => v.iter().map(Some).collect(),
            None => vec![None],
        };
        if xv.len() == 1 && zv.len() == 1 {
            return self.simplex3_v(x, y, z, s, sy, ox, oy, oz, xv[0], zv[0]);
        }
        let mut sum = 0.0;
        for &vx in &xv {
            for &vz in &zv {
                let wx = vx.map_or(1.0, |v| v.w);
                let wz = vz.map_or(1.0, |v| v.w);
                sum += wx * wz * self.simplex3_v(x, y, z, s, sy, ox, oy, oz, vx, vz);
            }
        }
        sum
    }

    #[allow(clippy::too_many_arguments)]
    fn simplex3_v(
        &self,
        x: f64,
        y: f64,
        z: f64,
        s: f64,
        sy: f64,
        ox: f64,
        oy: f64,
        oz: f64,
        vx: Option<&AxisVariant>,
        vz: Option<&AxisVariant>,
    ) -> f64 {
        let xin = x * s + ox;
        let yin = y * sy + oy;
        let zin = z * s + oz;
        let sk = (xin + yin + zin) * self.f3;
        let mut xs = xin + sk;
        let mut ys = yin + sk;
        let mut zs = zin + sk;
        if let Some(v) = vx {
            let a = k_term(&v.r, s * (1.0 + self.f3));
            let b = k_term(&v.r, s * self.f3);
            xs += a;
            ys += b;
            zs += b;
        }
        if let Some(v) = vz {
            let a = k_term(&v.r, s * (1.0 + self.f3));
            let b = k_term(&v.r, s * self.f3);
            xs += b;
            ys += b;
            zs += a;
        }
        let i = xs.floor();
        let j = ys.floor();
        let k = zs.floor();
        let fx = xs - i;
        let fy = ys - j;
        let fz = zs - k;
        let u = (fx + fy + fz) * self.g3;
        let ii = (((i % 256.0) + 256.0) as i64 & 255) as usize;
        let jj = (((j % 256.0) + 256.0) as i64 & 255) as usize;
        let kk = (((k % 256.0) + 256.0) as i64 & 255) as usize;
        self.simplex3_tail(ii, jj, kk, fx - u, fy - u, fz - u)
    }
}
