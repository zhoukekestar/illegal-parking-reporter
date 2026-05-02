// 本地用户认证 (P6)
//
// 设计:
//   - 单用户, 不联网
//   - argon2 哈希用户密码 (Argon2id, 默认参数)
//   - SQLCipher 加密 key 用一个独立的随机 32 字节 secret + 用户密码派生
//     - secret 存在 auth.json 里
//     - SQLCipher key = argon2(password, salt = secret) 取 32 字节
//   - 用户密码可为空 ("跳过密码"): 此时 secret 直接作为 SQLCipher key (仍有"硬盘加密"价值)
//
// auth.json 字段:
//   {
//     "version": 1,
//     "password_hash": "$argon2id$v=19$m=...",  // 可选, 用户设密码后才有
//     "secret_b64": "..."                          // 32 字节 base64
//   }

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use argon2::{password_hash::{PasswordHasher, SaltString, PasswordHash, PasswordVerifier}, Argon2};
use base64::Engine;
use rand::RngCore;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthFile {
    pub version: u32,
    /// argon2 编码后的密码 hash (None = 未设密码)
    #[serde(default)]
    pub password_hash: Option<String>,
    /// 32 字节随机 secret 的 base64
    pub secret_b64: String,
}

impl AuthFile {
    pub fn new_random() -> Self {
        let mut secret = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut secret);
        Self {
            version: 1,
            password_hash: None,
            secret_b64: base64::engine::general_purpose::STANDARD.encode(secret),
        }
    }

    pub fn secret_bytes(&self) -> Result<Vec<u8>> {
        base64::engine::general_purpose::STANDARD
            .decode(&self.secret_b64)
            .context("解码 secret_b64 失败")
    }

    pub fn has_password(&self) -> bool {
        self.password_hash.is_some()
    }
}

/// auth.json 路径 (与 db 同目录)
pub fn auth_path() -> Result<PathBuf> {
    let db = crate::db::db_path()?;
    let dir = db.parent().context("DB 路径无 parent")?;
    Ok(dir.join("auth.json"))
}

pub fn load_or_init() -> Result<AuthFile> {
    let path = auth_path()?;
    if path.exists() {
        let s = std::fs::read_to_string(&path)?;
        let a: AuthFile = serde_json::from_str(&s)?;
        Ok(a)
    } else {
        let a = AuthFile::new_random();
        save(&a)?;
        Ok(a)
    }
}

pub fn save(auth: &AuthFile) -> Result<()> {
    let path = auth_path()?;
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p)?;
    }
    let s = serde_json::to_string_pretty(auth)?;
    std::fs::write(&path, s).with_context(|| format!("写 auth.json 失败: {}", path.display()))?;
    Ok(())
}

pub fn hash_password(plain: &str) -> Result<String> {
    let salt = SaltString::generate(&mut rand::thread_rng());
    let hasher = Argon2::default();
    let h = hasher
        .hash_password(plain.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("argon2 hash 失败: {e}"))?;
    Ok(h.to_string())
}

pub fn verify_password(plain: &str, hash: &str) -> Result<bool> {
    let parsed = PasswordHash::new(hash).map_err(|e| anyhow::anyhow!("解析 hash 失败: {e}"))?;
    Ok(Argon2::default().verify_password(plain.as_bytes(), &parsed).is_ok())
}

/// 由密码 + secret 派生 SQLCipher key (hex 形式, 64 字符 = 32 字节)
///
/// 没设密码时 = secret 直接 hex
pub fn derive_sqlcipher_key(auth: &AuthFile, password: Option<&str>) -> Result<String> {
    let secret = auth.secret_bytes()?;
    match (password, &auth.password_hash) {
        (Some(pw), Some(_hash)) => {
            // 用 argon2 raw output (32 bytes) 作为 key
            let salt = SaltString::encode_b64(&secret[..16]).map_err(|e| anyhow::anyhow!("salt 编码失败: {e}"))?;
            let mut output = [0u8; 32];
            Argon2::default()
                .hash_password_into(pw.as_bytes(), salt.as_str().as_bytes(), &mut output)
                .map_err(|e| anyhow::anyhow!("argon2 派生 key 失败: {e}"))?;
            Ok(hex::encode(output))
        }
        _ => Ok(hex::encode(&secret)),
    }
}

// ===== 解锁状态 (内存中的 session 标志) =====

use std::sync::atomic::{AtomicBool, Ordering};

static UNLOCKED: AtomicBool = AtomicBool::new(false);

pub fn set_unlocked(v: bool) {
    UNLOCKED.store(v, Ordering::Relaxed);
}

pub fn is_unlocked() -> bool {
    UNLOCKED.load(Ordering::Relaxed)
}

pub fn _unused_keep(_p: &Path) {}
