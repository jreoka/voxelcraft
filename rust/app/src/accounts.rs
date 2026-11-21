use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use chrono::Utc;
use indexmap::IndexMap;
use rand::RngCore;
use scrypt::{Params, scrypt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use subtle::ConstantTimeEq;

use crate::config::atomic_write_json;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub name: String,
    pub salt: String,
    pub hash: String,
    pub created_at: String,
    pub last_login_at: String,
}

#[derive(Serialize)]
struct AccountsFile<'a> {
    version: u32,
    accounts: &'a IndexMap<String, Account>,
}

pub struct AuthResult {
    pub ok: bool,
    pub account: Option<Account>,
    pub created: bool,
    pub reason: String,
    pub bad_password: bool,
}

pub struct AccountStore {
    path: PathBuf,
    accounts: Mutex<IndexMap<String, Account>>,
}

impl AccountStore {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            accounts: Mutex::new(IndexMap::new()),
        }
    }

    pub fn len(&self) -> usize {
        self.accounts.lock().unwrap().len()
    }

    pub fn load(&self) {
        if !self.path.exists() {
            return;
        }
        let Ok(text) = fs::read_to_string(&self.path) else {
            return;
        };
        let Ok(raw) = serde_json::from_str::<Value>(&text) else {
            eprintln!("[Accounts] Failed to parse accounts.json");
            return;
        };
        let map_obj = raw.get("accounts").filter(|v| v.is_object()).unwrap_or(&raw);
        let mut guard = self.accounts.lock().unwrap();
        if let Some(obj) = map_obj.as_object() {
            for (k, v) in obj {
                if let Ok(acc) = serde_json::from_value::<Account>(v.clone())
                    && !acc.name.is_empty() && !acc.salt.is_empty() && !acc.hash.is_empty() {
                        guard.insert(k.to_lowercase(), acc);
                    }
            }
        }
        println!("[Accounts] Loaded {} claimed account(s).", guard.len());
    }

    pub fn save(&self) -> bool {
        let guard = self.accounts.lock().unwrap();
        let file = AccountsFile {
            version: 1,
            accounts: &guard,
        };
        match atomic_write_json(&self.path, &file, true) {
            Ok(()) => true,
            Err(e) => {
                eprintln!("[Accounts] Save failed: {}", e);
                false
            }
        }
    }

    /// Node.js `crypto.scrypt(password, salt, 64)` default params: N=16384 (log_n=14), r=8, p=1, dkLen=64.
    pub async fn hash_password(password: String, salt: String) -> Result<String, String> {
        tokio::task::spawn_blocking(move || {
            let params = Params::new(14, 8, 1, 64).map_err(|e| e.to_string())?;
            let mut out = [0u8; 64];
            scrypt(password.as_bytes(), salt.as_bytes(), &params, &mut out)
                .map_err(|e| e.to_string())?;
            Ok(hex::encode(out))
        })
        .await
        .map_err(|e| e.to_string())?
    }

    pub async fn verify_password(password: &str, salt: &str, expected_hex: &str) -> bool {
        let Ok(computed_hex) = Self::hash_password(password.to_string(), salt.to_string()).await else {
            return false;
        };
        let Ok(computed) = hex::decode(computed_hex) else {
            return false;
        };
        let Ok(expected) = hex::decode(expected_hex) else {
            return false;
        };
        if computed.len() != expected.len() {
            return false;
        }
        bool::from(computed.ct_eq(&expected))
    }

    pub fn validate_credentials(name: &str, password: &str) -> Result<(String, String), String> {
        let clean: String = name
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-' || *c == ' ')
            .collect();
        let clean: String = clean.trim().chars().take(20).collect();
        if clean.len() < 2 {
            return Err("Account name must be at least 2 characters.".to_string());
        }
        if password.chars().count() < 4 {
            return Err("Password must be at least 4 characters.".to_string());
        }
        if password.chars().count() > 64 {
            return Err("Password must be 64 characters or fewer.".to_string());
        }
        Ok((clean, password.to_string()))
    }

    pub async fn register(&self, raw_name: &str, raw_password: &str) -> AuthResult {
        let (clean_name, pw) = match Self::validate_credentials(raw_name, raw_password) {
            Ok(v) => v,
            Err(reason) => {
                return AuthResult {
                    ok: false,
                    account: None,
                    created: false,
                    reason,
                    bad_password: false,
                };
            }
        };
        let key = clean_name.to_lowercase();
        {
            let guard = self.accounts.lock().unwrap();
            if let Some(ex) = guard.get(&key) {
                return AuthResult {
                    ok: false,
                    account: None,
                    created: false,
                    reason: format!(
                        "Account \"{}\" is already claimed. Switch to Sign In and enter your password.",
                        ex.name
                    ),
                    bad_password: false,
                };
            }
        }
        let mut salt_bytes = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut salt_bytes);
        let salt = hex::encode(salt_bytes);
        let hash = match Self::hash_password(pw, salt.clone()).await {
            Ok(h) => h,
            Err(e) => {
                return AuthResult {
                    ok: false,
                    account: None,
                    created: false,
                    reason: format!("Password hashing failed: {}", e),
                    bad_password: false,
                };
            }
        };
        let now = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let account = Account {
            name: clean_name,
            salt,
            hash,
            created_at: now.clone(),
            last_login_at: now,
        };
        {
            let mut guard = self.accounts.lock().unwrap();
            if let Some(ex) = guard.get(&key) {
                return AuthResult {
                    ok: false,
                    account: None,
                    created: false,
                    reason: format!(
                        "Account \"{}\" is already claimed. Switch to Sign In and enter your password.",
                        ex.name
                    ),
                    bad_password: false,
                };
            }
            guard.insert(key, account.clone());
        }
        self.save();
        AuthResult {
            ok: true,
            account: Some(account),
            created: true,
            reason: String::new(),
            bad_password: false,
        }
    }

    pub async fn login(&self, raw_name: &str, raw_password: &str) -> AuthResult {
        let (clean_name, pw) = match Self::validate_credentials(raw_name, raw_password) {
            Ok(v) => v,
            Err(reason) => {
                return AuthResult {
                    ok: false,
                    account: None,
                    created: false,
                    reason,
                    bad_password: false,
                };
            }
        };
        let key = clean_name.to_lowercase();
        let account = {
            let guard = self.accounts.lock().unwrap();
            guard.get(&key).cloned()
        };
        let Some(mut account) = account else {
            return AuthResult {
                ok: false,
                account: None,
                created: false,
                reason: format!(
                    "Account \"{}\" is not claimed yet. Switch to Create Account to claim this name.",
                    clean_name
                ),
                bad_password: false,
            };
        };
        if !Self::verify_password(&pw, &account.salt, &account.hash).await {
            return AuthResult {
                ok: false,
                account: None,
                created: false,
                reason: format!("Incorrect password for \"{}\".", account.name),
                bad_password: true,
            };
        }
        account.last_login_at = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        {
            let mut guard = self.accounts.lock().unwrap();
            if let Some(a) = guard.get_mut(&key) {
                a.last_login_at = account.last_login_at.clone();
            }
        }
        self.save();
        AuthResult {
            ok: true,
            account: Some(account),
            created: false,
            reason: String::new(),
            bad_password: false,
        }
    }

    pub async fn change_password(&self, name: &str, old_pw: &str, new_pw: &str) -> Result<(), String> {
        let key = name.to_lowercase();
        let account = {
            let guard = self.accounts.lock().unwrap();
            guard.get(&key).cloned()
        }
        .ok_or_else(|| "Account not found.".to_string())?;
        if !Self::verify_password(old_pw, &account.salt, &account.hash).await {
            return Err("Current password is incorrect.".to_string());
        }
        let len = new_pw.chars().count();
        if !(4..=64).contains(&len) {
            return Err("New password must be between 4 and 64 characters.".to_string());
        }
        let mut salt_bytes = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut salt_bytes);
        let salt = hex::encode(salt_bytes);
        let hash = Self::hash_password(new_pw.to_string(), salt.clone()).await?;
        {
            let mut guard = self.accounts.lock().unwrap();
            if let Some(a) = guard.get_mut(&key) {
                a.salt = salt;
                a.hash = hash;
            }
        }
        self.save();
        Ok(())
    }
}
