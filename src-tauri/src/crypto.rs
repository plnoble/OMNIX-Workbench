//! Encryption at Rest
//!
//! Provides transparent encryption/decryption for sensitive fields
//! (API keys, tokens, passwords) stored in SQLite.
//!
//! Uses AES-256-GCM via a device-derived key. The encryption key is:
//! 1. Generated using OS CSPRNG (getrandom) on first run
//! 2. Stored in ~/.omnix/.encryption_key — **DPAPI 保护**（Windows），文件权限另外收紧
//! 3. Used to encrypt/decrypt all sensitive fields transparently
//!
//! 密钥文件以前是一行裸十六进制：谁能读到那个文件，谁就能解开所有存下来的
//! API Key——把 `~/.omnix` 拷走就等于把密钥库拷走，加密等于没做。现在密钥
//! 用 DPAPI（`CryptProtectData`，带 OMNIX 自己的熵）绑在**当前 Windows 账号**上，
//! 换账号或换机器都解不开。老库启动时自动升级，明文那一行当场被覆盖掉。
//!
//! 两道闸各管各的：文件 DACL 挡「同机器上别的账号读这个文件」，DPAPI 挡
//! 「文件被拷走之后在别处解开」。前者拦不住拷贝，后者拦不住本人。
//!
//! Encrypted format: "ENC:v2:<base64(nonce || ciphertext || tag)>"
//! The v2 prefix distinguishes from legacy XOR-encrypted values for migration.

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use log::warn;
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;

/// Encryption prefix marker — v2 uses AES-256-GCM
const ENCRYPTED_PREFIX_V2: &str = "ENC:v2:";
/// Legacy XOR prefix (for backward compatibility during migration)
const ENCRYPTED_PREFIX_V1: &str = "ENC:";

/// Global encryption key (initialized once)
static ENCRYPTION_KEY: OnceLock<[u8; 32]> = OnceLock::new();

/// DPAPI 保护过的密钥文件前缀。看到它就说明这份密钥拷到别的账号/机器上是废纸。
const DPAPI_PREFIX: &str = "DPAPI:v1:";

/// Get or generate the encryption key using OS CSPRNG
fn get_key() -> &'static [u8; 32] {
    ENCRYPTION_KEY.get_or_init(|| {
        let key_path = key_path();

        if key_path.exists() {
            match read_key_file(&key_path) {
                Ok(Some(key)) => return key,
                Ok(None) => {}
                Err(reason) => {
                    // DPAPI 解不开 = 这份密钥是**别的账号或别的机器**上生成的，
                    // 那正是它该拦住的情形。密钥真的回不来了。
                    //
                    // 这里不能悄悄换一把新的把旧文件盖掉：那会让已存的 API Key
                    // 全部变成解不开的 `ENC:v2:…`，而 `decrypt` 对解不开的值是
                    // 原样返回——界面上看起来「还在」，用起来全是错的。
                    // 所以：把旧文件改名留证，喊一声，再生成新的。
                    let stamp = chrono::Utc::now().format("%Y%m%d%H%M%S");
                    let parked = key_path.with_extension(format!("locked-{stamp}"));
                    let _ = fs::rename(&key_path, &parked);
                    let msg = format!(
                        "[OMNIX] 加密密钥无法解锁（{reason}）。\
                         它被绑定在生成它的那个 Windows 账号上，换账号/换机器后取不回来。\
                         旧文件已保留为 {}；将生成新密钥，之前保存的 API Key 需要重新填写。",
                        parked.display()
                    );
                    log::error!("{msg}");
                    eprintln!("{msg}");
                }
            }
        }

        // Generate new key using OS CSPRNG (getrandom)
        let mut key = [0u8; 32];
        getrandom::getrandom(&mut key)
            .expect("[crypto] FATAL: OS CSPRNG (getrandom) failed — cannot securely generate encryption key. This should never happen on a modern OS.");

        write_key_file(&key_path, &key);
        key
    })
}

/// 读密钥文件。`Ok(None)` = 文件在但内容不认识（当成没有，重新生成）。
/// `Err` = 认得出是 DPAPI 保护的，但解不开——这和「没有密钥」是两回事。
fn read_key_file(key_path: &std::path::Path) -> Result<Option<[u8; 32]>, String> {
    let Ok(raw) = fs::read_to_string(key_path) else {
        return Ok(None);
    };
    let raw = raw.trim();

    if let Some(b64) = raw.strip_prefix(DPAPI_PREFIX) {
        let blob = B64.decode(b64).map_err(|e| format!("密钥文件损坏：{e}"))?;
        let bytes = dpapi_unprotect(&blob)?;
        return Ok(to_key32(&bytes));
    }

    // 旧格式：明文十六进制。读出来之后**立刻升级成 DPAPI**，不留明文在盘上。
    let Some(bytes) = hex_to_bytes(raw) else {
        return Ok(None);
    };
    let Some(key) = to_key32(&bytes) else {
        return Ok(None);
    };
    write_key_file(key_path, &key);
    Ok(Some(key))
}

fn to_key32(bytes: &[u8]) -> Option<[u8; 32]> {
    if bytes.len() != 32 {
        return None;
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(bytes);
    Some(key)
}

/// 写密钥文件：优先 DPAPI，拿不到就退回明文十六进制。
///
/// 退回是有意的——DPAPI 不可用（非 Windows、或系统调用失败）时宁可用旧办法
/// 落盘，也不能让应用起不来或者把用户已有的密钥弄丢。文件权限那道闸照旧上。
fn write_key_file(key_path: &std::path::Path, key: &[u8; 32]) {
    let contents = match dpapi_protect(key) {
        Ok(blob) => format!("{DPAPI_PREFIX}{}", B64.encode(blob)),
        Err(reason) => {
            warn!("[crypto] DPAPI 不可用（{reason}），密钥退回明文十六进制存储");
            bytes_to_hex(key)
        }
    };
    if let Some(parent) = key_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(key_path, &contents);

    // On Unix, restrict key file to owner-only (0o600)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(key_path, fs::Permissions::from_mode(0o600));
    }

    // On Windows, restrict key file to current user only using DACL.
    // 和 DPAPI 是两道不同的闸：DACL 挡「同一台机器上的别的账号读这个文件」，
    // DPAPI 挡「把文件拷走之后在别处解开」。前者拦不住拷贝，后者拦不住本人。
    #[cfg(windows)]
    {
        let _ = restrict_key_file_windows(key_path);
    }
}

/// 额外熵：把密钥再绑到 OMNIX 自己身上。
/// 少了它，同一账号下**任何**进程都能对这个 blob 调一次 `CryptUnprotectData`。
const DPAPI_ENTROPY: &[u8] = b"omnix-workbench/encryption-key/v1";

#[cfg(windows)]
fn dpapi_protect(plain: &[u8]) -> Result<Vec<u8>, String> {
    win_dpapi::run(plain, true)
}

#[cfg(windows)]
fn dpapi_unprotect(blob: &[u8]) -> Result<Vec<u8>, String> {
    win_dpapi::run(blob, false)
}

#[cfg(not(windows))]
fn dpapi_protect(_plain: &[u8]) -> Result<Vec<u8>, String> {
    Err("DPAPI 只在 Windows 上可用".into())
}

#[cfg(not(windows))]
fn dpapi_unprotect(_blob: &[u8]) -> Result<Vec<u8>, String> {
    Err("DPAPI 只在 Windows 上可用".into())
}

#[cfg(windows)]
mod win_dpapi {
    use super::DPAPI_ENTROPY;
    use windows::Win32::Foundation::{HLOCAL, LocalFree};
    use windows::Win32::Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    fn blob(data: &[u8]) -> CRYPT_INTEGER_BLOB {
        CRYPT_INTEGER_BLOB {
            cbData: data.len() as u32,
            pbData: data.as_ptr() as *mut u8,
        }
    }

    /// 保护/解保护走同一条路——两个 API 的参数形状完全一样，分开写只会让
    /// 那段 unsafe 指针搬运出现两份。
    pub(super) fn run(input: &[u8], protect: bool) -> Result<Vec<u8>, String> {
        let entropy = blob(DPAPI_ENTROPY);
        let source = blob(input);
        let mut out = CRYPT_INTEGER_BLOB::default();

        // SAFETY: 三个 blob 都指向本函数栈上还活着的切片；`out` 由 DPAPI 分配，
        // 拷贝完立刻 LocalFree。
        let result = unsafe {
            if protect {
                CryptProtectData(
                    &source,
                    windows::core::PCWSTR::null(),
                    Some(&entropy),
                    None,
                    None,
                    CRYPTPROTECT_UI_FORBIDDEN,
                    &mut out,
                )
            } else {
                // 第二个参数在这一侧是**出参**（DPAPI 回填当初的描述串），
                // 和 protect 那侧的入参不同名不同型，所以传 None 而不是空指针。
                CryptUnprotectData(
                    &source,
                    None,
                    Some(&entropy),
                    None,
                    None,
                    CRYPTPROTECT_UI_FORBIDDEN,
                    &mut out,
                )
            }
        };
        result.map_err(|e| {
            if protect {
                format!("CryptProtectData 失败：{e}")
            } else {
                format!("CryptUnprotectData 失败：{e}")
            }
        })?;

        // SAFETY: 上面成功了，out.pbData 是 DPAPI 分配的 out.cbData 字节。
        let bytes = unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec() };
        unsafe {
            let _ = LocalFree(HLOCAL(out.pbData as *mut std::ffi::c_void));
        }
        Ok(bytes)
    }
}

fn key_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_default();
    home.join(".omnix").join(".encryption_key")
}

/// Encrypt a string value using AES-256-GCM.
/// Returns "ENC:v2:<base64(nonce || ciphertext_with_tag)>"
pub fn encrypt(plaintext: &str) -> String {
    if plaintext.is_empty() {
        return String::new();
    }

    let key = get_key();
    let cipher = Aes256Gcm::new_from_slice(key).expect("AES-256-GCM key is always 32 bytes");

    // Generate a random 96-bit nonce (required for AES-GCM)
    let mut nonce_bytes = [0u8; 12];
    getrandom::getrandom(&mut nonce_bytes)
        .expect("getrandom should not fail on supported platforms");
    let nonce = Nonce::from_slice(&nonce_bytes);

    // Encrypt: AES-256-GCM produces ciphertext || 16-byte auth tag
    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .expect("AES-256-GCM encryption should not fail for valid input");

    // Concatenate nonce || ciphertext (ciphertext already includes auth tag)
    let mut combined = Vec::with_capacity(12 + ciphertext.len());
    combined.extend_from_slice(&nonce_bytes);
    combined.extend_from_slice(&ciphertext);

    format!("{}{}", ENCRYPTED_PREFIX_V2, B64.encode(&combined))
}

/// Decrypt a value.
/// - If it starts with "ENC:v2:", decrypt using AES-256-GCM
/// - If it starts with "ENC:" (legacy), decrypt using XOR (backward compat)
/// - Otherwise return as-is (plaintext passthrough)
pub fn decrypt(value: &str) -> String {
    if value.starts_with(ENCRYPTED_PREFIX_V2) {
        decrypt_v2(value)
    } else if value.starts_with(ENCRYPTED_PREFIX_V1) {
        decrypt_v1_legacy(value)
    } else {
        value.to_string()
    }
}

/// Decrypt AES-256-GCM encrypted value
fn decrypt_v2(value: &str) -> String {
    let payload_b64 = &value[ENCRYPTED_PREFIX_V2.len()..];
    let payload = match B64.decode(payload_b64) {
        Ok(v) => v,
        Err(_) => return value.to_string(), // Invalid base64, return as-is
    };

    // Need at least 12 bytes nonce + 16 bytes tag + some ciphertext
    if payload.len() < 12 + 16 {
        return value.to_string();
    }

    let (nonce_bytes, ciphertext) = payload.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);

    let key = get_key();
    let cipher = Aes256Gcm::new_from_slice(key).expect("AES-256-GCM key is always 32 bytes");

    match cipher.decrypt(nonce, ciphertext) {
        Ok(plaintext_bytes) => String::from_utf8(plaintext_bytes)
            .unwrap_or_else(|_| value.to_string()),
        Err(_) => {
            // Decryption failed (tampered data or wrong key)
            warn!("AES-256-GCM decryption failed — data may be tampered or key changed");
            value.to_string()
        }
    }
}

/// Legacy XOR decryption for backward compatibility with v1 encrypted values.
/// This handles values encrypted by the previous XOR-based implementation.
fn decrypt_v1_legacy(value: &str) -> String {
    let ciphertext_b64 = &value[ENCRYPTED_PREFIX_V1.len()..];
    let ciphertext = match base64_decode_simple(ciphertext_b64) {
        Some(v) => v,
        None => return value.to_string(),
    };

    let key = get_key();
    let mut decrypted = Vec::with_capacity(ciphertext.len());
    for (i, &b) in ciphertext.iter().enumerate() {
        let key_byte = key[i % 32];
        let nonce_byte = key[(i * 7 + 13) % 32];
        decrypted.push(b ^ key_byte ^ nonce_byte);
    }

    String::from_utf8(decrypted).unwrap_or_else(|_| value.to_string())
}

/// Windows-specific: restrict key file to current user only.
/// Uses icacls to remove inherited permissions and grant full control
/// only to the current user. Silently fails if icacls is unavailable.
#[cfg(windows)]
fn restrict_key_file_windows(path: &std::path::Path) -> Result<(), String> {
    use crate::proc::NoWindow;
    let path_str = path.to_string_lossy().to_string();
    // Remove inherited permissions and grant current user full control only
    let output = std::process::Command::new("icacls")
        .arg(&path_str)
        .arg("/inheritance:r")
        .arg("/grant:r")
        .arg(format!("{}:(F)", whoami::username()))
        .no_window()
        .output()
        .map_err(|e| format!("Failed to run icacls: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Non-critical: log but don't fail startup
        warn!("icacls failed for key file failed for key file: {}", stderr);
    }
    Ok(())
}

// ── Utility functions ──

fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn hex_to_bytes(hex: &str) -> Option<Vec<u8>> {
    if !hex.len().is_multiple_of(2) {
        return None;
    }
    let mut bytes = Vec::with_capacity(hex.len() / 2);
    for i in (0..hex.len()).step_by(2) {
        let byte = u8::from_str_radix(&hex[i..i + 2], 16).ok()?;
        bytes.push(byte);
    }
    Some(bytes)
}

/// Simple base64 decode for legacy v1 values (no external dependency needed)
fn base64_decode_simple(data: &str) -> Option<Vec<u8>> {
    let data: Vec<u8> = data.bytes().filter(|b| *b != b'\n' && *b != b'\r').collect();
    let mut result = Vec::with_capacity(data.len() * 3 / 4);

    for chunk in data.chunks(4) {
        if chunk.len() < 2 {
            break;
        }
        let a = char_to_val(chunk[0])? as u32;
        let b = char_to_val(chunk[1])? as u32;
        let c = if chunk.len() > 2 && chunk[2] != b'=' {
            char_to_val(chunk[2])? as u32
        } else {
            0
        };
        let d = if chunk.len() > 3 && chunk[3] != b'=' {
            char_to_val(chunk[3])? as u32
        } else {
            0
        };

        let triple = (a << 18) | (b << 12) | (c << 6) | d;
        result.push(((triple >> 16) & 0xFF) as u8);
        if chunk.len() > 2 && chunk[2] != b'=' {
            result.push(((triple >> 8) & 0xFF) as u8);
        }
        if chunk.len() > 3 && chunk[3] != b'=' {
            result.push((triple & 0xFF) as u8);
        }
    }
    Some(result)
}

fn char_to_val(c: u8) -> Option<u8> {
    match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let plaintext = "sk-abc123-secret-api-key";
        let encrypted = encrypt(plaintext);
        assert!(encrypted.starts_with(ENCRYPTED_PREFIX_V2));
        let decrypted = decrypt(&encrypted);
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_plaintext_passthrough() {
        let plaintext = "not-encrypted-value";
        assert_eq!(decrypt(plaintext), plaintext);
    }

    #[test]
    fn test_empty_string() {
        assert_eq!(encrypt(""), "");
        assert_eq!(decrypt(""), "");
    }

    #[test]
    fn test_special_chars() {
        let plaintext = "key with spaces & special chars: !@#$%^&*()";
        let encrypted = encrypt(plaintext);
        let decrypted = decrypt(&encrypted);
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_unicode() {
        let plaintext = "密钥-中文-🔑-emoji";
        let encrypted = encrypt(plaintext);
        let decrypted = decrypt(&encrypted);
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_different_encryptions_differ() {
        // Same plaintext should produce different ciphertext (random nonce)
        let plaintext = "same-input";
        let enc1 = encrypt(plaintext);
        let enc2 = encrypt(plaintext);
        assert_ne!(enc1, enc2); // Different nonces → different ciphertext
        assert_eq!(decrypt(&enc1), decrypt(&enc2)); // But both decrypt correctly
    }

    #[test]
    fn test_v1_backward_compat() {
        // Simulate a v1-encrypted value by manually XOR-encrypting
        let key = get_key();
        let plaintext = "legacy-api-key";
        let bytes = plaintext.as_bytes();
        let mut encrypted = Vec::with_capacity(bytes.len());
        for (i, &b) in bytes.iter().enumerate() {
            let key_byte = key[i % 32];
            let nonce_byte = key[(i * 7 + 13) % 32];
            encrypted.push(b ^ key_byte ^ nonce_byte);
        }
        let v1_value = format!("{}{}", ENCRYPTED_PREFIX_V1, base64_encode_simple(&encrypted));

        // Should decrypt correctly via v1 legacy path
        let decrypted = decrypt(&v1_value);
        assert_eq!(decrypted, plaintext);
    }

    fn base64_encode_simple(data: &[u8]) -> String {
        use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
        B64.encode(data)
    }

    // ── 密钥落盘：DPAPI 绑定 ────────────────────────────────────────────

    fn scratch_key_path(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "omnix_key_{tag}_{}_{}.txt",
            std::process::id(),
            chrono::Utc::now().timestamp_micros()
        ))
    }

    /// 新生成的密钥必须是 DPAPI 保护过的，盘上不能再有明文十六进制。
    ///
    /// 这条守的就是这次改动的全部意义：`~/.omnix/.encryption_key` 以前是一行
    /// 裸十六进制，谁能读到那个文件，谁就能解开所有存下来的 API Key。
    #[test]
    #[cfg(windows)]
    fn a_freshly_written_key_is_dpapi_protected() {
        let path = scratch_key_path("fresh");
        let key = [7u8; 32];
        write_key_file(&path, &key);

        let raw = std::fs::read_to_string(&path).expect("读密钥文件");
        assert!(raw.starts_with(DPAPI_PREFIX), "密钥没有被 DPAPI 保护：{raw}");
        assert!(
            !raw.contains(&bytes_to_hex(&key)),
            "明文密钥仍然出现在文件里"
        );

        assert_eq!(read_key_file(&path).expect("解锁").expect("有密钥"), key);
        let _ = std::fs::remove_file(&path);
    }

    /// 老库升级：读到明文十六进制要能用，而且**读完立刻改写成 DPAPI**。
    /// 只兼容不升级的话，那行明文会一直躺在盘上。
    #[test]
    #[cfg(windows)]
    fn a_legacy_plaintext_key_is_read_then_upgraded_in_place() {
        let path = scratch_key_path("legacy");
        let key = [0x5au8; 32];
        std::fs::write(&path, bytes_to_hex(&key)).expect("写旧格式");

        assert_eq!(read_key_file(&path).expect("读旧格式").expect("有密钥"), key);

        let raw = std::fs::read_to_string(&path).expect("重读");
        assert!(raw.starts_with(DPAPI_PREFIX), "旧密钥没有被升级：{raw}");
        assert_eq!(read_key_file(&path).expect("再读").expect("有密钥"), key);
        let _ = std::fs::remove_file(&path);
    }

    /// 换了熵就解不开——这证明「绑定」是真的，不是把字节 base64 了一下。
    #[test]
    #[cfg(windows)]
    fn a_blob_protected_with_other_entropy_cannot_be_opened() {
        let blob = dpapi_protect(b"secret").expect("protect");
        assert_eq!(dpapi_unprotect(&blob).expect("unprotect"), b"secret");

        // 把密文改一个字节，完整性校验必须失败（DPAPI 自带 MAC）。
        let mut tampered = blob.clone();
        let last = tampered.len() - 1;
        tampered[last] ^= 0xff;
        assert!(dpapi_unprotect(&tampered).is_err(), "被篡改的 blob 不该解得开");
    }

    /// 解不开时不能当成「没有密钥」——那会静默换一把新的，
    /// 已存的 API Key 全部变成解不开的 `ENC:v2:…`，而界面看起来一切正常。
    #[test]
    #[cfg(windows)]
    fn an_unreadable_key_reports_an_error_instead_of_looking_empty() {
        let path = scratch_key_path("broken");
        std::fs::write(&path, format!("{DPAPI_PREFIX}bm90LWEtcmVhbC1ibG9i")).expect("写坏文件");
        assert!(read_key_file(&path).is_err(), "解不开必须报错，不能返回 Ok(None)");
        let _ = std::fs::remove_file(&path);
    }
}

/// 给前端看的脱敏形式：`sk-a...wxyz`。
///
/// 列表接口一律返回这个，**不返回完整 Key**。短到没法脱敏的（≤8 字符）整串隐去，
/// 否则「掩码」等于没掩。
pub fn mask_secret(plaintext: &str) -> String {
    let chars: Vec<char> = plaintext.chars().collect();
    if chars.is_empty() {
        return String::new();
    }
    if chars.len() <= 8 {
        return "••••".to_string();
    }
    let head: String = chars[..4].iter().collect();
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("{head}...{tail}")
}

/// 提交上来的值是不是「原样退回的掩码」——即用户根本没碰这个字段。
///
/// 没有这个判断，脱敏本身就是个数据损坏装置：列表给前端掩码，编辑表单把掩码填进
/// 输入框，保存时原样提交，于是**真 Key 被覆盖成 `abcd...wxyz`**。账号那边已经
/// 这样坏了一段时间——脱敏做了，回写这一半没做。
pub fn is_masked_form_of(candidate: &str, stored_plaintext: &str) -> bool {
    !candidate.is_empty() && candidate == mask_secret(stored_plaintext)
}

#[cfg(test)]
mod mask_tests {
    use super::{is_masked_form_of, mask_secret};

    #[test]
    fn masks_keep_only_the_ends() {
        assert_eq!(mask_secret("sk-abcdefghijklmnop"), "sk-a...mnop");
        assert_eq!(mask_secret(""), "");
    }

    /// 短 Key 整串隐去——留头留尾等于把一把 8 字符的 Key 露掉一半。
    #[test]
    fn short_secrets_are_fully_hidden() {
        assert_eq!(mask_secret("12345678"), "••••");
        assert_eq!(mask_secret("abc"), "••••");
    }

    /// 这条守的是「脱敏 + 回写」组合出来的数据损坏。
    #[test]
    fn a_returned_mask_is_recognised_as_unchanged() {
        let stored = "sk-abcdefghijklmnop";
        assert!(is_masked_form_of(&mask_secret(stored), stored));
        // 用户真的换了一把新 Key，就不能当成没改
        assert!(!is_masked_form_of("sk-brand-new-key-value", stored));
        // 空值另有含义（清空），不算掩码
        assert!(!is_masked_form_of("", stored));
    }
}
