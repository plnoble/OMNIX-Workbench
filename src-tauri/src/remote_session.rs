//! 远程面板的凭据：一次性配对码 + 签名会话 Cookie。
//!
//! 换掉的是「永久令牌直接写在 URL 里」——`/remote?token=tok_…`。URL 会进浏览器
//! 历史、Referer、地址栏截图，二维码还会被拍照转发；而那个令牌是**永久**的，
//! 泄一次就一直有效，直到用户自己想起来去轮换。
//!
//! 拆成两段之后：
//! - **配对码**（`/remote?code=…`）：5 分钟有效、用一次即废。只够完成第一次导航，
//!   捡到一个用过的码没有任何意义。
//! - **会话 Cookie**（`omnix_remote`）：HttpOnly + SameSite=Strict，之后每个请求
//!   浏览器自动带上，不出现在任何 URL 里。
//! - **`x-omnix-remote-token` 头**：模型网关（`/v1/*`）继续用它——机器对机器没有
//!   浏览器历史这回事，头也不会被截进图里。
//!
//! Cookie 是自带签名的（HMAC-SHA256，密钥就是 `remote_token`），所以**不建表**：
//! 「轮换令牌」立刻让所有已发出的 Cookie 一起失效。UI 上本来就承诺「旧链接全部
//! 失效」，现在这句话对已配对的设备也成立了。

use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// 面板会话 Cookie 名。
pub const SESSION_COOKIE: &str = "omnix_remote";
/// 配对码有效期：够从电脑走到手机跟前扫一下，不够留给别人捡。
const CODE_TTL_SECS: i64 = 300;
/// 会话有效期。到期手机重新扫一次码。
const SESSION_TTL_SECS: i64 = 30 * 24 * 3600;
/// 同时有效的配对码上限——诊断页每刷新一次就发一个，得有个盖。
const MAX_LIVE_CODES: usize = 16;

/// 未核销的配对码（code → 到期 unix 秒）。只在内存里：它们活 5 分钟，
/// 重启期间正好跨过这个窗口的概率不值得为它建表。
fn codes() -> &'static Mutex<HashMap<String, i64>> {
    static CODES: OnceLock<Mutex<HashMap<String, i64>>> = OnceLock::new();
    CODES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 定长比较，不通过耗时泄漏「对了几个字符」。全仓库只有这一份实现，
/// `proxy_auth::token_matches` 也走它。
pub(crate) fn ct_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.as_bytes().iter().zip(b.as_bytes()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// HMAC-SHA256（RFC 2104）。
///
/// `sha2` 本来就在依赖里，为一个 Cookie 签名再拉一个 `hmac` crate 不划算；
/// 而直接写 `sha256(secret‖msg)` 有长度扩展问题——所以老老实实按 RFC 做一遍。
/// `hmac_matches_the_rfc4231_vector` 那条测试拿标准向量对过，不是「看着像」。
fn hmac_sha256(key: &[u8], msg: &[u8]) -> [u8; 32] {
    const BLOCK: usize = 64;
    let mut k = [0u8; BLOCK];
    if key.len() > BLOCK {
        k[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let mut ipad = [0x36u8; BLOCK];
    let mut opad = [0x5cu8; BLOCK];
    for i in 0..BLOCK {
        ipad[i] ^= k[i];
        opad[i] ^= k[i];
    }
    let inner = Sha256::new().chain_update(ipad).chain_update(msg).finalize();
    let outer = Sha256::new().chain_update(opad).chain_update(inner).finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&outer);
    out
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// 发一个配对码。没用掉的会在 5 分钟后自己过期。
///
/// CSPRNG 失败就报错，不退化到时间戳之类的可预测来源——和 `rotate_remote_token`
/// 一个立场。
pub fn mint_code(now: i64) -> Result<String, String> {
    let mut buf = [0u8; 16];
    getrandom::getrandom(&mut buf).map_err(|e| format!("CSPRNG (getrandom) 不可用：{e}"))?;
    let code = format!("pc_{}", to_hex(&buf));
    let mut map = codes().lock().map_err(|_| "配对码表已损坏".to_string())?;
    map.retain(|_, exp| *exp > now);
    while map.len() >= MAX_LIVE_CODES {
        let Some(oldest) = map.iter().min_by_key(|(_, e)| **e).map(|(k, _)| k.clone()) else {
            break;
        };
        map.remove(&oldest);
    }
    map.insert(code.clone(), now + CODE_TTL_SECS);
    Ok(code)
}

/// 核销一个配对码：有效则返回 true 并**把它删掉**（用一次即废）。
pub fn consume_code(code: &str, now: i64) -> bool {
    if code.is_empty() {
        return false;
    }
    let Ok(mut map) = codes().lock() else {
        return false;
    };
    map.retain(|_, exp| *exp > now);
    map.remove(code).is_some()
}

/// 轮换令牌时一并作废还没用掉的码。
pub fn clear_codes() {
    if let Ok(mut map) = codes().lock() {
        map.clear();
    }
}

/// 配对码还剩多久有效（秒）——诊断页拿它显示倒计时并在到期前自动换一个。
pub const fn code_ttl_secs() -> i64 {
    CODE_TTL_SECS
}

fn sign(secret: &str, exp: i64) -> String {
    to_hex(&hmac_sha256(secret.as_bytes(), exp.to_string().as_bytes()))
}

/// 签一个会话 Cookie 的值：`<到期时间>.<签名>`。
///
/// `secret` 为空（`remote_token` 还没生成或读不出来）时返回 None——空密钥签出来的
/// 东西谁都能造，宁可不发。
pub fn issue_session(secret: &str, now: i64) -> Option<String> {
    if secret.is_empty() {
        return None;
    }
    let exp = now + SESSION_TTL_SECS;
    Some(format!("{exp}.{}", sign(secret, exp)))
}

/// 验会话 Cookie：签名对得上且没过期。
pub fn session_valid(cookie: &str, secret: &str, now: i64) -> bool {
    if secret.is_empty() {
        return false;
    }
    let Some((exp_str, sig)) = cookie.split_once('.') else {
        return false;
    };
    let Ok(exp) = exp_str.parse::<i64>() else {
        return false;
    };
    if exp <= now {
        return false;
    }
    ct_eq(sig, &sign(secret, exp))
}

/// 从 `Cookie:` 请求头里取一个 cookie 的值。
pub fn cookie_value(raw_header: &str, name: &str) -> Option<String> {
    for part in raw_header.split(';') {
        let part = part.trim();
        if let Some(rest) = part.strip_prefix(name) {
            if let Some(value) = rest.strip_prefix('=') {
                return Some(value.to_string());
            }
        }
    }
    None
}

/// 组 `Set-Cookie` 的值。
///
/// - **不带 `Secure`**：面板走的是局域网明文 HTTP，带上 `Secure` 浏览器会直接把
///   这个 Cookie 丢掉，配对表面成功、实际一直登不上。
/// - `HttpOnly`：页面脚本读不到它，XSS 也偷不走。
/// - `SameSite=Strict`：别的站点发起的请求一律不带它，顺手挡掉 CSRF。
pub fn set_cookie_header(value: &str) -> String {
    format!(
        "{SESSION_COOKIE}={value}; Path=/; HttpOnly; SameSite=Strict; Max-Age={SESSION_TTL_SECS}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_760_000_000;

    #[test]
    fn hmac_matches_the_rfc4231_vector() {
        // RFC 4231 Test Case 2——自己手写的 HMAC 必须拿标准向量对，
        // 否则「签名一致」只能证明它跟自己一致。
        let mac = hmac_sha256(b"Jefe", b"what do ya want for nothing?");
        assert_eq!(
            to_hex(&mac),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn hmac_handles_keys_longer_than_the_block() {
        // RFC 4231 Test Case 6：131 字节的 key 要先被 SHA-256 压一遍。
        let key = vec![0xaa_u8; 131];
        let mac = hmac_sha256(&key, b"Test Using Larger Than Block-Size Key - Hash Key First");
        assert_eq!(
            to_hex(&mac),
            "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"
        );
    }

    #[test]
    fn a_code_works_exactly_once() {
        let code = mint_code(NOW).unwrap();
        assert!(consume_code(&code, NOW));
        assert!(!consume_code(&code, NOW), "配对码必须用一次就废");
    }

    #[test]
    fn an_expired_code_is_refused() {
        let code = mint_code(NOW).unwrap();
        assert!(!consume_code(&code, NOW + CODE_TTL_SECS + 1));
    }

    #[test]
    fn unknown_and_empty_codes_are_refused() {
        assert!(!consume_code("", NOW));
        assert!(!consume_code("pc_deadbeef", NOW));
    }

    #[test]
    fn session_round_trips_and_rejects_tampering() {
        let secret = "tok_abc123";
        let cookie = issue_session(secret, NOW).unwrap();
        assert!(session_valid(&cookie, secret, NOW));

        // 换了密钥（= 用户轮换了令牌）→ 所有已发出的 Cookie 立刻失效。
        assert!(!session_valid(&cookie, "tok_rotated", NOW));

        // 把到期时间往后改，签名就对不上了——延期必须重新签。
        let (exp, sig) = cookie.split_once('.').unwrap();
        let forged = format!("{}.{sig}", exp.parse::<i64>().unwrap() + 86_400);
        assert!(!session_valid(&forged, secret, NOW));

        // 改签名同样不行。
        assert!(!session_valid(&format!("{exp}.0000"), secret, NOW));
        assert!(!session_valid("garbage", secret, NOW));
        assert!(!session_valid("notanumber.abc", secret, NOW));
    }

    #[test]
    fn session_expires() {
        let secret = "tok_abc123";
        let cookie = issue_session(secret, NOW).unwrap();
        assert!(!session_valid(&cookie, secret, NOW + SESSION_TTL_SECS + 1));
    }

    #[test]
    fn an_empty_secret_neither_signs_nor_verifies() {
        // 读不出 remote_token 时绝不能变成「谁都放行」。
        assert!(issue_session("", NOW).is_none());
        assert!(!session_valid("whatever", "", NOW));
    }

    #[test]
    fn cookie_value_reads_the_right_pair() {
        let raw = "theme=dark; omnix_remote=abc.def; other=1";
        assert_eq!(cookie_value(raw, SESSION_COOKIE).as_deref(), Some("abc.def"));
        assert_eq!(cookie_value("omnix_remote=xyz", SESSION_COOKIE).as_deref(), Some("xyz"));
        assert_eq!(cookie_value("nothing=1", SESSION_COOKIE), None);
        // 前缀撞名不能算命中，否则 `omnix_remote_old` 会被当成会话 Cookie。
        assert_eq!(cookie_value("omnix_remote_old=xyz", SESSION_COOKIE), None);
    }

    #[test]
    fn set_cookie_is_httponly_strict_and_not_secure() {
        let header = set_cookie_header("abc.def");
        assert!(header.contains("HttpOnly"));
        assert!(header.contains("SameSite=Strict"));
        assert!(header.starts_with("omnix_remote=abc.def;"));
        // 局域网是明文 HTTP，带 Secure 会让浏览器直接丢掉 Cookie。
        assert!(!header.contains("Secure"));
    }

    #[test]
    fn ct_eq_is_exact() {
        assert!(ct_eq("abc", "abc"));
        assert!(!ct_eq("abc", "abd"));
        assert!(!ct_eq("abc", "abcd"));
        assert!(ct_eq("", ""));
    }
}
