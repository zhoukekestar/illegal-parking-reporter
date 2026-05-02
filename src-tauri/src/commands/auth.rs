// P6: 用户认证 + 设置 + 清空数据 命令

use serde::Serialize;

use crate::db::settings::AppSettings;

#[derive(Debug, Serialize)]
pub struct AuthState {
    /// 用户已设密码
    pub has_password: bool,
    /// 当前 session 是否解锁 (没设密码恒为 true)
    pub unlocked: bool,
}

#[tauri::command]
pub async fn auth_state() -> Result<AuthState, String> {
    tokio::task::spawn_blocking(|| -> anyhow::Result<AuthState> {
        let auth = crate::auth::load_or_init()?;
        let unlocked = !auth.has_password() || crate::auth::is_unlocked();
        Ok(AuthState {
            has_password: auth.has_password(),
            unlocked,
        })
    })
    .await
    .map_err(|e| format!("blocking task panic: {e}"))?
    .map_err(|e| format!("查询 auth 状态失败: {e:#}"))
}

/// 设置/修改密码: old_password 仅当已设密码时需要
#[tauri::command]
pub async fn set_password(
    old_password: Option<String>,
    new_password: String,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        let mut auth = crate::auth::load_or_init()?;
        if auth.has_password() {
            let old = old_password.unwrap_or_default();
            let hash = auth.password_hash.as_deref().unwrap_or("");
            if !crate::auth::verify_password(&old, hash)? {
                anyhow::bail!("旧密码错误");
            }
        }
        if new_password.is_empty() {
            anyhow::bail!("新密码不能为空");
        }
        auth.password_hash = Some(crate::auth::hash_password(&new_password)?);
        crate::auth::save(&auth)?;
        crate::auth::set_unlocked(true);
        Ok(())
    })
    .await
    .map_err(|e| format!("blocking task panic: {e}"))?
    .map_err(|e| format!("设置密码失败: {e:#}"))
}

#[tauri::command]
pub async fn unlock(password: String) -> Result<bool, String> {
    tokio::task::spawn_blocking(move || -> anyhow::Result<bool> {
        let auth = crate::auth::load_or_init()?;
        let hash = match &auth.password_hash {
            Some(h) => h.clone(),
            None => {
                crate::auth::set_unlocked(true);
                return Ok(true);
            }
        };
        let ok = crate::auth::verify_password(&password, &hash)?;
        if ok {
            crate::auth::set_unlocked(true);
        }
        Ok(ok)
    })
    .await
    .map_err(|e| format!("blocking task panic: {e}"))?
    .map_err(|e| format!("解锁失败: {e:#}"))
}

#[tauri::command]
pub fn lock() {
    crate::auth::set_unlocked(false);
}

// ====== 设置 ======

#[tauri::command]
pub async fn get_settings() -> Result<AppSettings, String> {
    tokio::task::spawn_blocking(|| -> anyhow::Result<AppSettings> {
        let lock = crate::db::conn()?;
        let conn = lock.lock().map_err(|e| anyhow::anyhow!("DB mutex 中毒: {e}"))?;
        crate::db::settings::load(&conn)
    })
    .await
    .map_err(|e| format!("blocking task panic: {e}"))?
    .map_err(|e| format!("读取设置失败: {e:#}"))
}

#[tauri::command]
pub async fn save_settings(settings: AppSettings) -> Result<(), String> {
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        let lock = crate::db::conn()?;
        let conn = lock.lock().map_err(|e| anyhow::anyhow!("DB mutex 中毒: {e}"))?;
        crate::db::settings::save(&conn, &settings)
    })
    .await
    .map_err(|e| format!("blocking task panic: {e}"))?
    .map_err(|e| format!("保存设置失败: {e:#}"))
}

// ====== 清空数据 ======

#[tauri::command]
pub async fn purge_data() -> Result<(), String> {
    tokio::task::spawn_blocking(|| -> anyhow::Result<()> {
        crate::db::purge_all()?;
        // evidence 目录也清掉
        if let Ok(p) = crate::evidence::evidence_root() {
            let _ = std::fs::remove_dir_all(&p);
            let _ = std::fs::create_dir_all(&p);
        }
        Ok(())
    })
    .await
    .map_err(|e| format!("blocking task panic: {e}"))?
    .map_err(|e| format!("清空数据失败: {e:#}"))
}
