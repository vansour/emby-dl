use std::path::PathBuf;

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use getrandom::fill;
use rusqlite::Connection;

pub struct AuthDb {
    conn: Connection,
    key: [u8; 32],
}

#[derive(Debug, Clone)]
pub struct StoredCredentials {
    pub server_url: String,
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone)]
pub struct StoredAuth {
    pub server_url: String,
    pub username: String,
    pub access_token: String,
    pub user_id: String,
}

impl AuthDb {
    pub fn open() -> anyhow::Result<Self> {
        let path = db_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(&path)?;
        let key = load_or_create_key()?;
        let db = Self { conn, key };
        db.init_tables()?;
        Ok(db)
    }

    fn init_tables(&self) -> anyhow::Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS credentials (
                id          INTEGER PRIMARY KEY CHECK (id = 1),
                server_url  TEXT NOT NULL,
                username    TEXT NOT NULL,
                password    BLOB NOT NULL
            );
            CREATE TABLE IF NOT EXISTS auth_tokens (
                id           INTEGER PRIMARY KEY CHECK (id = 1),
                server_url   TEXT NOT NULL,
                username     TEXT NOT NULL,
                access_token BLOB NOT NULL,
                user_id      TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS proxy (
                id         INTEGER PRIMARY KEY CHECK (id = 1),
                proxy_url  TEXT NOT NULL
            );",
        )?;
        Ok(())
    }

    pub fn save_proxy(&self, proxy_url: &str) -> anyhow::Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO proxy (id, proxy_url) VALUES (1, ?1)",
            rusqlite::params![proxy_url],
        )?;
        Ok(())
    }

    pub fn load_proxy(&self) -> anyhow::Result<Option<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT proxy_url FROM proxy WHERE id = 1")?;
        match stmt.query_row([], |row| row.get(0)) {
            Ok(url) => Ok(Some(url)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn remove_proxy(&self) -> anyhow::Result<()> {
        self.conn.execute("DELETE FROM proxy WHERE id = 1", [])?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn save_credentials(
        &self,
        server_url: &str,
        username: &str,
        password: &str,
    ) -> anyhow::Result<()> {
        let encrypted = encrypt(password, &self.key)?;
        self.conn.execute(
            "INSERT OR REPLACE INTO credentials (id, server_url, username, password)
             VALUES (1, ?1, ?2, ?3)",
            rusqlite::params![server_url, username, encrypted],
        )?;
        Ok(())
    }

    pub fn load_credentials(&self) -> anyhow::Result<Option<StoredCredentials>> {
        let mut stmt = self
            .conn
            .prepare("SELECT server_url, username, password FROM credentials WHERE id = 1")?;
        let row = stmt
            .query_row([], |row| {
                let server_url: String = row.get(0)?;
                let username: String = row.get(1)?;
                let blob: Vec<u8> = row.get(2)?;
                Ok((server_url, username, blob))
            });
        let row = match row {
            Ok(row) => Some(row),
            Err(rusqlite::Error::QueryReturnedNoRows) => None,
            Err(e) => return Err(e.into()),
        };
        match row {
            Some((server_url, username, blob)) => {
                let password = decrypt_credential(&blob, &self.key).map_err(|e| {
                    anyhow::anyhow!("解密凭据失败，请重新运行 --auth: {}", e)
                })?;
                Ok(Some(StoredCredentials {
                    server_url,
                    username,
                    password,
                }))
            }
            None => Ok(None),
        }
    }

    pub fn save_auth(
        &self,
        server_url: &str,
        username: &str,
        access_token: &str,
        user_id: &str,
    ) -> anyhow::Result<()> {
        let encrypted = encrypt(access_token, &self.key)?;
        self.conn.execute(
            "INSERT OR REPLACE INTO auth_tokens (id, server_url, username, access_token, user_id)
             VALUES (1, ?1, ?2, ?3, ?4)",
            rusqlite::params![server_url, username, encrypted, user_id],
        )?;
        Ok(())
    }

    pub fn load_auth(&self) -> anyhow::Result<Option<StoredAuth>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT server_url, username, access_token, user_id FROM auth_tokens WHERE id = 1",
            )?;
        let row = stmt
            .query_row([], |row| {
                let server_url: String = row.get(0)?;
                let username: String = row.get(1)?;
                let blob: Vec<u8> = row.get(2)?;
                let user_id: String = row.get(3)?;
                Ok((server_url, username, blob, user_id))
            });
        let row = match row {
            Ok(row) => Some(row),
            Err(rusqlite::Error::QueryReturnedNoRows) => None,
            Err(e) => return Err(e.into()),
        };
        match row {
            Some((server_url, username, blob, user_id)) => {
                let access_token = decrypt_credential(&blob, &self.key).map_err(|e| {
                    anyhow::anyhow!("解密 token 失败，请重新运行 --auth: {}", e)
                })?;
                Ok(Some(StoredAuth {
                    server_url,
                    username,
                    access_token,
                    user_id,
                }))
            }
            None => Ok(None),
        }
    }
}

fn db_path() -> PathBuf {
    if let Some(home) = std::env::var_os("HOME") {
        let mut path = PathBuf::from(home);
        path.push(".config");
        path.push("emby-dl");
        path.push("auth.db");
        path
    } else {
        PathBuf::from("./emby-dl.db")
    }
}

fn key_path() -> PathBuf {
    if let Some(home) = std::env::var_os("HOME") {
        let mut path = PathBuf::from(home);
        path.push(".config");
        path.push("emby-dl");
        path.push("key");
        path
    } else {
        PathBuf::from("./emby-dl.key")
    }
}

fn load_or_create_key() -> anyhow::Result<[u8; 32]> {
    let path = key_path();
    if path.exists() {
        let data = std::fs::read(&path)?;
        if data.len() == 32 {
            let mut key = [0u8; 32];
            key.copy_from_slice(&data);
            return Ok(key);
        }
        anyhow::bail!("密钥文件损坏，请删除 {} 后重试", path.display());
    }
    let mut key = [0u8; 32];
    fill(&mut key)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, key)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(key)
}

fn encrypt(plaintext: &str, key: &[u8; 32]) -> anyhow::Result<Vec<u8>> {
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| anyhow::anyhow!("密钥初始化失败: {}", e))?;
    let mut nonce_bytes = [0u8; 12];
    fill(&mut nonce_bytes)?;
    let nonce = Nonce::try_from(&nonce_bytes[..])
        .map_err(|_| anyhow::anyhow!("nonce 初始化失败"))?;
    let ciphertext = cipher
        .encrypt(&nonce, plaintext.as_bytes())
        .map_err(|e| anyhow::anyhow!("加密失败: {}", e))?;
    let mut result = nonce_bytes.to_vec();
    result.extend(ciphertext);
    Ok(result)
}

fn decrypt(data: &[u8], key: &[u8; 32]) -> anyhow::Result<String> {
    if data.len() < 12 {
        anyhow::bail!("无效的加密数据");
    }
    let (nonce_bytes, ciphertext) = data.split_at(12);
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| anyhow::anyhow!("密钥初始化失败: {}", e))?;
    let nonce = Nonce::try_from(nonce_bytes)
        .map_err(|_| anyhow::anyhow!("nonce 初始化失败"))?;
    let plaintext = cipher
        .decrypt(&nonce, ciphertext)
        .map_err(|_| anyhow::anyhow!("解密失败（数据可能已损坏）"))?;
    String::from_utf8(plaintext).map_err(|e| anyhow::anyhow!("解密结果不是有效文本: {}", e))
}

fn decrypt_credential(data: &[u8], key: &[u8; 32]) -> anyhow::Result<String> {
    // 尝试解密；若数据是纯文本（旧版兼容），直接返回
    decrypt(data, key).or_else(|_| {
        String::from_utf8(data.to_vec())
            .map_err(|_| anyhow::anyhow!("凭据数据无法解密且不是旧版纯文本格式"))
    })
}

impl AuthDb {
    #[cfg(test)]
    fn open_in_memory_with_key(key: [u8; 32]) -> anyhow::Result<Self> {
        let conn = Connection::open_in_memory()?;
        let db = Self { conn, key };
        db.init_tables()?;
        Ok(db)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key() -> [u8; 32] {
        let mut key = [0u8; 32];
        key[..4].copy_from_slice(b"test");
        key
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let key = test_key();
        let plaintext = "my_secret_password_123!@#";
        let encrypted = encrypt(plaintext, &key).unwrap();
        assert_ne!(encrypted, plaintext.as_bytes());
        let decrypted = decrypt(&encrypted, &key).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encrypt_produces_different_ciphertexts() {
        let key = test_key();
        let plaintext = "same_password";
        let a = encrypt(plaintext, &key).unwrap();
        let b = encrypt(plaintext, &key).unwrap();
        assert_ne!(a, b, "每次加密应产生不同的密文（随机 nonce）");
    }

    #[test]
    fn test_wrong_key_fails() {
        let mut key1 = [0u8; 32];
        let mut key2 = [0u8; 32];
        key1[0] = 1;
        key2[0] = 2;
        let encrypted = encrypt("secret", &key1).unwrap();
        assert!(decrypt(&encrypted, &key2).is_err());
    }

    #[test]
    fn test_save_and_load_credentials() {
        let db = AuthDb::open_in_memory_with_key(test_key()).unwrap();
        assert!(db.load_credentials().unwrap().is_none());
        db.save_credentials("https://example.com", "alice", "secret123")
            .unwrap();
        let cred = db.load_credentials().unwrap().unwrap();
        assert_eq!(cred.server_url, "https://example.com");
        assert_eq!(cred.username, "alice");
        assert_eq!(cred.password, "secret123");
    }

    #[test]
    fn test_save_and_load_auth() {
        let db = AuthDb::open_in_memory_with_key(test_key()).unwrap();
        assert!(db.load_auth().unwrap().is_none());
        db.save_auth("https://example.com", "alice", "token_xyz", "user_42")
            .unwrap();
        let auth = db.load_auth().unwrap().unwrap();
        assert_eq!(auth.server_url, "https://example.com");
        assert_eq!(auth.username, "alice");
        assert_eq!(auth.access_token, "token_xyz");
        assert_eq!(auth.user_id, "user_42");
    }

    #[test]
    fn test_replace_credentials() {
        let db = AuthDb::open_in_memory_with_key(test_key()).unwrap();
        db.save_credentials("https://old.com", "old", "old_pass").unwrap();
        db.save_credentials("https://new.com", "new", "new_pass").unwrap();
        let cred = db.load_credentials().unwrap().unwrap();
        assert_eq!(cred.server_url, "https://new.com");
        assert_eq!(cred.username, "new");
    }

    #[test]
    fn test_replace_auth() {
        let db = AuthDb::open_in_memory_with_key(test_key()).unwrap();
        db.save_auth("https://old.com", "old", "old_token", "old_id")
            .unwrap();
        db.save_auth("https://new.com", "new", "new_token", "new_id")
            .unwrap();
        let auth = db.load_auth().unwrap().unwrap();
        assert_eq!(auth.server_url, "https://new.com");
        assert_eq!(auth.username, "new");
    }

    #[test]
    fn test_empty_db_returns_none() {
        let db = AuthDb::open_in_memory_with_key(test_key()).unwrap();
        assert!(db.load_credentials().unwrap().is_none());
        assert!(db.load_auth().unwrap().is_none());
    }

    #[test]
    fn test_auth_and_credentials_independent() {
        let db = AuthDb::open_in_memory_with_key(test_key()).unwrap();
        db.save_credentials("https://srv", "bob", "bob_pass").unwrap();
        assert!(db.load_auth().unwrap().is_none());
        db.save_auth("https://srv", "bob", "tok", "uid").unwrap();
        let cred = db.load_credentials().unwrap().unwrap();
        assert_eq!(cred.username, "bob");
        let auth = db.load_auth().unwrap().unwrap();
        assert_eq!(auth.username, "bob");
    }

    #[test]
    fn test_legacy_plaintext_compat() {
        // 模拟旧版明文存储，验证 decrypt_credential 能回退
        let key = test_key();
        let data = b"plaintext_password";
        let result = decrypt_credential(data, &key).unwrap();
        assert_eq!(result, "plaintext_password");
    }
}
