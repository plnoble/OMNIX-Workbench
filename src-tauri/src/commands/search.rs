use tauri::State;
use std::sync::Arc;
use crate::db::DbManager;
use crate::input_validation;

// ── Web Search ───────────────────────────────────────────

/// Search provider configuration DTO
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SearchProvider {
    pub id: String,
    pub name: String,
    pub api_type: String,
    pub api_key: String,
    pub api_address: String,
    pub is_enabled: bool,
}

/// A single web search result
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WebSearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
    pub source: String,
    pub position: i32,
}

/// Search history entry DTO
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SearchHistoryEntry {
    pub id: String,
    pub query: String,
    pub provider_id: String,
    pub result_count: i32,
    pub created_at: String,
}

/// Get all configured search providers.
#[tauri::command]
pub fn get_search_providers(
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<SearchProvider>, String> {
    let rows = db.get_search_providers().map_err(|e| e.to_string())?;
    // **不把完整 Key 交给前端**——这个列表只是给人看的，编辑表单需要的是「有没有
    // 配过」而不是 Key 本身。提交时若原样退回掩码，`save_search_provider` 会认出
    // 「没改过」并保留原值。
    Ok(rows.into_iter().map(|(id, name, api_type, api_key, api_address, is_enabled)| {
        SearchProvider {
            id,
            name,
            api_type,
            api_key: crate::crypto::mask_secret(&crate::crypto::decrypt(&api_key)),
            api_address,
            is_enabled,
        }
    }).collect())
}

/// Save (upsert) a search provider configuration.
#[tauri::command]
pub fn save_search_provider(
    provider: SearchProvider,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    // 列表给的是掩码，编辑表单会原样提交回来——不识别就会把真 Key 覆盖成
    // `abcd...wxyz`。留空同样按「没改」处理。
    let existing_plain = db
        .get_search_providers()
        .ok()
        .and_then(|rows| {
            rows.into_iter()
                .find(|r| r.0 == provider.id)
                .map(|r| crate::crypto::decrypt(&r.3))
        });
    let key_to_store = match existing_plain.as_deref() {
        Some(prev) if crate::crypto::is_masked_form_of(&provider.api_key, prev) => prev.to_string(),
        Some(prev) if provider.api_key.trim().is_empty() => prev.to_string(),
        _ => provider.api_key.clone(),
    };
    db.save_search_provider(
        &provider.id,
        &provider.name,
        &provider.api_type,
        &crate::crypto::encrypt(&key_to_store),
        &provider.api_address,
        provider.is_enabled,
    )
    .map_err(|e| e.to_string())
}

/// Delete a search provider.
#[tauri::command]
pub fn delete_search_provider(
    id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    input_validation::validate_id(&id, "id")?;
    db.delete_search_provider(&id).map_err(|e| e.to_string())
}

/// Execute a web search using configured providers.
#[tauri::command]
pub async fn web_search(
    query: String,
    provider_id: Option<String>,
    limit: Option<u32>,
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<WebSearchResult>, String> {
    run_search(&db, &query, provider_id.as_deref(), limit.unwrap_or(10)).await
}

/// 搜索的实现本体，和 Tauri 无关。
///
/// 抽出来是为了给 MCP 服务器用（`mcp_server::WEB_SEARCH_TOOL`）——同一份供应商
/// 配置、同一份错误提示、同一份搜索历史，前端点的搜索和 agent 调的工具走的是
/// **同一条路**。复制一份实现出来迟早会分叉。
pub(crate) async fn run_search(
    db: &DbManager,
    query: &str,
    provider_id: Option<&str>,
    limit: u32,
) -> Result<Vec<WebSearchResult>, String> {
    let providers = db.get_search_providers().map_err(|e| e.to_string())?;
    let provider = if let Some(pid) = provider_id {
        providers.into_iter().find(|p| p.0 == pid && p.5)
            .ok_or_else(|| format!("搜索供应商「{pid}」不存在或已停用。请到「搜索」页左侧检查供应商列表。"))?
    } else {
        providers.into_iter().find(|p| p.5)
            .ok_or_else(|| "还没有启用任何搜索供应商。请到「搜索」页左侧点「+」新增一个（Brave / Tavily 有免费额度），或启用已有的那个。".to_string())?
    };
    let (provider_id, provider_name, api_type, api_key, api_address, _is_enabled) = provider;
    // 库里是密文。`decrypt` 对没有前缀的值原样返回，所以还没迁移的存量行也走得通。
    let api_key = crate::crypto::decrypt(&api_key);

    // Simple percent-encoding for search queries
    let encoded_query: String = query.replace(' ', "%20")
        .replace('+', "%2B")
        .replace('&', "%26")
        .replace('#', "%23")
        .replace('?', "%3F")
        .replace('/', "%2F");

    // SearXNG 支持已移除（唯一需要自己跑 Docker 的供应商，为它保留一条自托管
    // 分支不划算）。剩下的供应商都在公网，走系统代理是对的。
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());

    /// 非 2xx 时把状态码和响应体带出来。
    ///
    /// 四个供应商以前都是 `send() → json()` 直接往下走，**从不检查状态码**：
    /// Brave 少了 API Key 会回 401，body 解析不出 `web.results` → 0 条结果、
    /// 没有任何提示。用户看到的就是「点了没反应」。
    async fn ensure_ok(provider: &str, resp: reqwest::Response) -> Result<reqwest::Response, String> {
        let status = resp.status();
        if status.is_success() {
            return Ok(resp);
        }
        let body = resp.text().await.unwrap_or_default();
        let hint = match status.as_u16() {
            401 | 403 => "（多半是 API Key 没填或无效）",
            429 => "（触发限流，稍后再试）",
            _ => "",
        };
        Err(format!(
            "{provider} 返回 {status}{hint}：{}",
            body.chars().take(300).collect::<String>()
        ))
    }
    // Bing 已于 2025-08-11 被微软下线（旧 Key 返回 410），博查只有按次计费、
    // 没有免费额度——两条分支都已删除，别再让用户去配一个走不通的供应商。
    let results = match api_type.as_str() {
        "brave" => {
            let url = format!("https://api.search.brave.com/res/v1/web/search?q={}&count={}", encoded_query, limit);
            let mut req = client.get(&url)
                .timeout(std::time::Duration::from_secs(15));
            if !api_key.is_empty() {
                req = req.header("X-Subscription-Token", &api_key);
            }
            let resp = req.send().await
                .map_err(|e| format!("Brave 连接失败：{}", crate::proxy::describe_request_error(&e)))?;
            let resp = ensure_ok("Brave", resp).await?;
            let json: serde_json::Value = resp.json().await.map_err(|e| format!("Brave parse failed: {}", e))?;
            let mut out = Vec::new();
            if let Some(results) = json.get("web").and_then(|w| w.get("results")).and_then(|r| r.as_array()) {
                for (i, item) in results.iter().take(limit as usize).enumerate() {
                    out.push(WebSearchResult {
                        title: item.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        url: item.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        snippet: item.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        source: provider_name.clone(),
                        position: i as i32,
                    });
                }
            }
            out
        }
        "duckduckgo" => {
            let url = format!("https://api.duckduckgo.com/?q={}&format=json&no_html=1", encoded_query);
            let resp = client.get(&url)
                .timeout(std::time::Duration::from_secs(15))
                .send().await
                .map_err(|e| format!("DuckDuckGo 连接失败：{}", crate::proxy::describe_request_error(&e)))?;
            let resp = ensure_ok("DuckDuckGo", resp).await?;
            let json: serde_json::Value = resp.json().await.map_err(|e| format!("DuckDuckGo parse failed: {}", e))?;
            let mut out = Vec::new();
            // DDG instant answer
            if let Some(abstract_text) = json.get("AbstractText").and_then(|v| v.as_str()) {
                if !abstract_text.is_empty() {
                    out.push(WebSearchResult {
                        title: json.get("Heading").and_then(|v| v.as_str()).unwrap_or(query).to_string(),
                        url: json.get("AbstractURL").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        snippet: abstract_text.to_string(),
                        source: provider_name.clone(),
                        position: 0,
                    });
                }
            }
            // DDG related topics
            if let Some(topics) = json.get("RelatedTopics").and_then(|r| r.as_array()) {
                for (i, topic) in topics.iter().take(limit as usize).enumerate() {
                    if let Some(text) = topic.get("Text").and_then(|v| v.as_str()) {
                        out.push(WebSearchResult {
                            title: text.chars().take(80).collect(),
                            url: topic.get("FirstURL").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                            snippet: text.to_string(),
                            source: provider_name.clone(),
                            position: i as i32 + 1,
                        });
                    }
                }
            }
            out
        }
        "tavily" => {
            let url = "https://api.tavily.com/search";
            let body = serde_json::json!({
                "api_key": api_key,
                "query": query,
                "max_results": limit,
                "search_depth": "basic"
            });
            let resp = client.post(url)
                .timeout(std::time::Duration::from_secs(15))
                .json(&body)
                .send().await
                .map_err(|e| format!("Tavily 连接失败：{}", crate::proxy::describe_request_error(&e)))?;
            let resp = ensure_ok("Tavily", resp).await?;
            let json: serde_json::Value = resp.json().await.map_err(|e| format!("Tavily parse failed: {}", e))?;
            let mut out = Vec::new();
            if let Some(results) = json.get("results").and_then(|r| r.as_array()) {
                for (i, item) in results.iter().take(limit as usize).enumerate() {
                    out.push(WebSearchResult {
                        title: item.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        url: item.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        snippet: item.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        source: provider_name.clone(),
                        position: i as i32,
                    });
                }
            }
            out
        }
        "exa" => {
            let url = "https://api.exa.ai/search";
            let body = serde_json::json!({
                "query": query,
                "numResults": limit,
                "type": "auto",
                "contents": { "text": { "maxCharacters": 300 } }
            });
            let resp = client.post(url)
                .timeout(std::time::Duration::from_secs(15))
                .header("x-api-key", &api_key)
                .json(&body)
                .send().await
                .map_err(|e| format!("Exa 连接失败：{}", crate::proxy::describe_request_error(&e)))?;
            let resp = ensure_ok("Exa", resp).await?;
            let json: serde_json::Value = resp.json().await.map_err(|e| format!("Exa parse failed: {}", e))?;
            let mut out = Vec::new();
            if let Some(results) = json.get("results").and_then(|r| r.as_array()) {
                for (i, item) in results.iter().take(limit as usize).enumerate() {
                    let text = item.get("text").and_then(|v| v.as_str()).unwrap_or("");
                    out.push(WebSearchResult {
                        title: item.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        url: item.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        snippet: text.to_string(),
                        source: provider_name.clone(),
                        position: i as i32,
                    });
                }
            }
            out
        }
        "zhipu" => {
            // 这一条以前指向 `https://search.zhpu.ai/search`——**这个主机根本不存在**
            // （zhipu 拼成了 zhpu，而且智谱也从来没有这个域名），所以「智谱搜索」
            // 从写下来的那天起就没成功过一次，用户看到的只有连接失败。
            // 真正的接口在开放平台下面：POST /api/paas/v4/web_search，
            // 结果在 `search_result` 里，字段是 title / link / content。
            let url = if api_address.is_empty() {
                "https://open.bigmodel.cn/api/paas/v4/web_search".to_string()
            } else {
                api_address.clone()
            };
            let body = serde_json::json!({
                "search_query": query.chars().take(70).collect::<String>(), // 官方上限 70 字
                "search_engine": "search_std",
                "count": limit,
            });
            let resp = client.post(&url)
                .timeout(std::time::Duration::from_secs(15))
                .header("Authorization", format!("Bearer {}", api_key))
                .json(&body)
                .send().await
                .map_err(|e| format!("智谱搜索 连接失败：{}", crate::proxy::describe_request_error(&e)))?;
            let resp = ensure_ok("智谱搜索", resp).await?;
            let json: serde_json::Value = resp.json().await.map_err(|e| format!("Zhipu search parse failed: {}", e))?;
            let mut out = Vec::new();
            if let Some(results) = json.get("search_result").and_then(|r| r.as_array()) {
                for (i, item) in results.iter().take(limit as usize).enumerate() {
                    out.push(WebSearchResult {
                        title: item.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        url: item.get("link").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        snippet: item.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        source: provider_name.clone(),
                        position: i as i32,
                    });
                }
            }
            out
        }
        "jina" => {
            let url = format!("https://s.jina.ai/{}", encoded_query);
            let mut req = client.get(&url)
                .timeout(std::time::Duration::from_secs(15))
                .header("Accept", "application/json");
            if !api_key.is_empty() {
                req = req.header("Authorization", format!("Bearer {}", api_key));
            }
            let resp = req.send().await
                .map_err(|e| format!("Jina 连接失败：{}", crate::proxy::describe_request_error(&e)))?;
            let resp = ensure_ok("Jina", resp).await?;
            let json: serde_json::Value = resp.json().await.map_err(|e| format!("Jina search parse failed: {}", e))?;
            let mut out = Vec::new();
            if let Some(results) = json.get("data").and_then(|r| r.as_array()) {
                for (i, item) in results.iter().take(limit as usize).enumerate() {
                    out.push(WebSearchResult {
                        title: item.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        url: item.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        snippet: item.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string().chars().take(300).collect(),
                        source: provider_name.clone(),
                        position: i as i32,
                    });
                }
            }
            out
        }
        "google" => {
            // Google Custom Search API — requires API key + CX (custom search engine ID)
            // api_address can hold the CX value, or default to a general one
            let cx = if api_address.is_empty() { "".to_string() } else { api_address.clone() };
            let url = format!("https://www.googleapis.com/customsearch/v1?key={}&cx={}&q={}&num={}",
                api_key, cx, encoded_query, limit);
            let resp = client.get(&url)
                .timeout(std::time::Duration::from_secs(15))
                .send().await
                .map_err(|e| format!("Google 连接失败：{}", crate::proxy::describe_request_error(&e)))?;
            let resp = ensure_ok("Google", resp).await?;
            let json: serde_json::Value = resp.json().await.map_err(|e| format!("Google search parse failed: {}", e))?;
            let mut out = Vec::new();
            if let Some(results) = json.get("items").and_then(|r| r.as_array()) {
                for (i, item) in results.iter().take(limit as usize).enumerate() {
                    out.push(WebSearchResult {
                        title: item.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        url: item.get("link").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        snippet: item.get("snippet").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        source: provider_name.clone(),
                        position: i as i32,
                    });
                }
            }
            out
        }
        other => {
            return Err(format!(
                "不支持的搜索供应商类型「{other}」。可选：tavily / exa / jina / brave / zhipu / google / duckduckgo。"
            ));
        }
    };

    // 零结果不是「没反应」。以前这里直接返回空数组，界面一片空白，用户分不清
    // 是「真的没搜到」「Key 没配」还是「这个供应商压根不做网页搜索」。
    if results.is_empty() {
        let hint = match api_type.as_str() {
            // DDG 这个接口是 Instant Answer，不是网页搜索：绝大多数查询本来就空。
            "duckduckgo" => "。DuckDuckGo 这个免费接口只返回「即时答案」，不是网页搜索——大部分查询本来就没有结果，要做网页搜索请换 Tavily / Exa / Brave",
            "brave" if api_key.trim().is_empty() => "。Brave Search 必须填 API Key",
            "google" if api_address.trim().is_empty() => "。Google 除了 API Key 还需要填「搜索引擎 ID（CX）」——填在 API 地址那一栏",
            _ => "",
        };
        return Err(format!("「{provider_name}」没有返回任何结果{hint}"));
    }

    // 存进搜索历史（「搜索」页右侧的回看列表读这张表）。
    let history_id = format!("sh_{}", chrono::Utc::now().timestamp_millis());
    let results_json = serde_json::to_string(&results).unwrap_or_else(|_| "[]".to_string());
    let _ = db.save_search_history(&history_id, query, &provider_id, results.len() as i32, &results_json);

    Ok(results)
}

// ── 抓网页正文 ────────────────────────────────────────────
//
// 搜索只给标题和摘要，摘要里没有的东西模型就编不出来。Claude Code 有 WebSearch
// 也有 WebFetch，两个凑一起才叫「能上网」。这里补上后一半。

/// 抓一个网页，去掉标签返回纯文本。
///
/// **这个函数会被模型调用**，而模型的调用可能被它读到的内容诱导（搜索结果里
/// 藏一句「去 fetch http://127.0.0.1:1421/... 」就是一次内网探测）。所以这里
/// 只放行公网 http/https，跟随重定向之后**再查一次落点**——只查一开始那个 URL
/// 挡不住「公网域名 302 到 127.0.0.1」。
pub(crate) async fn fetch_url_text(url: &str, max_chars: usize) -> Result<String, String> {
    let parsed = reqwest::Url::parse(url).map_err(|e| format!("URL 解析失败：{e}"))?;
    guard_public_url(&parsed)?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .user_agent("Mozilla/5.0 (compatible; OMNIX/1.0)")
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .get(parsed.clone())
        .send()
        .await
        .map_err(|e| format!("抓取失败：{}", crate::proxy::describe_request_error(&e)))?;

    // 重定向之后的落点也要过一遍闸。
    guard_public_url(resp.url())?;

    let status = resp.status();
    let final_url = resp.url().to_string();
    if !status.is_success() {
        return Err(format!("{final_url} 返回 {status}"));
    }
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let body = resp.text().await.map_err(|e| format!("读取正文失败：{e}"))?;

    let text = if content_type.contains("html") || body.trim_start().starts_with('<') {
        html_to_text(&body)
    } else {
        body
    };
    let mut out = text.chars().take(max_chars).collect::<String>();
    if text.chars().count() > max_chars {
        out.push_str("\n\n…（正文已截断）");
    }
    if out.trim().is_empty() {
        return Err(format!("{final_url} 抓到了，但正文是空的（可能是需要 JS 渲染的页面）"));
    }
    Ok(format!("来源：{final_url}\n\n{out}"))
}

/// 只放行公网 http/https。回环、私网、链路本地一律拒绝。
fn guard_public_url(url: &reqwest::Url) -> Result<(), String> {
    if !matches!(url.scheme(), "http" | "https") {
        return Err(format!("只支持 http/https，收到 {}", url.scheme()));
    }
    let host = url.host_str().unwrap_or("").to_ascii_lowercase();
    if host.is_empty() {
        return Err("URL 里没有主机名".into());
    }
    if host == "localhost" || host.ends_with(".localhost") || host.ends_with(".local") {
        return Err(format!("拒绝抓取本机地址：{host}"));
    }
    if let Ok(ip) = host.trim_matches(['[', ']']).parse::<std::net::IpAddr>() {
        let private = is_private_ip(ip);
        if private {
            return Err(format!("拒绝抓取内网地址：{ip}"));
        }
        // 字面 IP 已经判完，不必再解析。
        return Ok(());
    }

    // 主机名要**解析之后**再判。只查字面量的话，一个指向 127.0.0.1 或
    // 169.254.169.254（云元数据）的域名就能长驱直入——这是 SSRF 的标准走法，
    // 而 `fetch_url` 是挂在 MCP 上的，等于把它交给了 agent。
    //
    // 残留窗口：解析和真正连接之间 DNS 可能变（rebinding）。完全堵住要接管
    // 连接、直接连解析出来的 IP；那是另一层机器。这里先把「随便一个域名就能打
    // 内网」这条大路封掉，窗口留在注释里，别假装它不存在。
    let port = url.port_or_known_default().unwrap_or(80);
    let resolved: Vec<std::net::IpAddr> = std::net::ToSocketAddrs::to_socket_addrs(&(host.as_str(), port))
        .map_err(|e| format!("无法解析主机名 {host}：{e}"))?
        .map(|addr| addr.ip())
        .collect();
    if resolved.is_empty() {
        return Err(format!("主机名 {host} 没有解析出任何地址"));
    }
    reject_if_any_resolved_address_is_private(&host, &resolved)
}

/// 解析结果里只要有一个落在内网就拒绝——不能因为它同时有一个公网地址就放行
/// （DNS 可以同时返回两条，攻击者只需要其中一条被用上）。
///
/// 和解析分开是为了**能离线测**：把真实 DNS 拉进单测，结果就取决于跑测试那台
/// 机器的解析器，是个必然会飘的测试。这里只测判定。
fn reject_if_any_resolved_address_is_private(
    host: &str,
    resolved: &[std::net::IpAddr],
) -> Result<(), String> {
    for ip in resolved {
        if is_private_ip(*ip) {
            return Err(format!(
                "拒绝抓取：{host} 解析到内网地址 {ip}（域名指向内网是 SSRF 的常见走法）"
            ));
        }
    }
    Ok(())
}

/// 私网 / 环回 / 链路本地 / 未指定地址。
fn is_private_ip(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => {
            v4.is_loopback() || v4.is_private() || v4.is_link_local() || v4.is_unspecified()
        }
        std::net::IpAddr::V6(v6) => {
            // IPv4-mapped (`::ffff:127.0.0.1`) is not `::1`, so `is_loopback()`
            // is false — treating it as public punched a hole in this cage.
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_private_ip(std::net::IpAddr::V4(v4));
            }
            // `is_unique_local` / `is_unicast_link_local` 还没稳定，手动判前缀。
            v6.is_loopback()
                || v6.is_unspecified()
                || (v6.segments()[0] & 0xfe00) == 0xfc00
                || (v6.segments()[0] & 0xffc0) == 0xfe80
        }
    }
}

/// 极简 HTML → 纯文本：先整块干掉 script/style，再逐字符剥标签。
///
/// 没引 HTML 解析库是刻意的——目标是「让模型读懂这页说了什么」，不是还原
/// DOM。为此多背一个解析器依赖不值。
fn html_to_text(html: &str) -> String {
    let mut cleaned = String::with_capacity(html.len());
    let lower = html.to_ascii_lowercase();
    let mut i = 0usize;
    // 按字节找 `<script`/`<style`，命中就跳到对应闭合标签之后。
    while i < html.len() {
        let rest = &lower[i..];
        let script = rest.find("<script").map(|p| (p, "</script>"));
        let style = rest.find("<style").map(|p| (p, "</style>"));
        let next = match (script, style) {
            (Some(a), Some(b)) => Some(if a.0 <= b.0 { a } else { b }),
            (a, b) => a.or(b),
        };
        match next {
            Some((offset, close)) => {
                cleaned.push_str(&html[i..i + offset]);
                let after = i + offset;
                match lower[after..].find(close) {
                    Some(end) => i = after + end + close.len(),
                    None => break, // 没有闭合标签，剩下的整段丢掉
                }
            }
            None => {
                cleaned.push_str(&html[i..]);
                break;
            }
        }
    }

    // 剥标签。块级标签换成换行，免得整页糊成一行。
    let mut out = String::with_capacity(cleaned.len() / 2);
    let mut in_tag = false;
    let mut tag = String::new();
    for ch in cleaned.chars() {
        match ch {
            '<' => {
                in_tag = true;
                tag.clear();
            }
            '>' if in_tag => {
                in_tag = false;
                let name = tag.trim_start_matches('/').split([' ', '\t', '\n']).next().unwrap_or("");
                if matches!(
                    name.to_ascii_lowercase().as_str(),
                    "p" | "br" | "div" | "li" | "tr" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
                        | "section" | "article" | "header" | "footer" | "blockquote" | "pre"
                ) {
                    out.push('\n');
                }
            }
            _ if in_tag => tag.push(ch),
            _ => out.push(ch),
        }
    }

    let decoded = out
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'");

    // 压空白：行内空白折成一个空格，连续空行折成一个。
    let mut lines: Vec<String> = Vec::new();
    for line in decoded.lines() {
        let squeezed = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if squeezed.is_empty() {
            if lines.last().map(|l: &String| l.is_empty()) == Some(false) {
                lines.push(String::new());
            }
        } else {
            lines.push(squeezed);
        }
    }
    lines.join("\n").trim().to_string()
}

/// Get search history entries.
#[tauri::command]
pub fn get_search_history(
    limit: u32,
    db: State<'_, Arc<DbManager>>,
) -> Result<Vec<SearchHistoryEntry>, String> {
    let rows = db.get_search_history(limit as i32).map_err(|e| e.to_string())?;
    Ok(rows.into_iter().map(|(id, query, provider_id, result_count, created_at)| {
        SearchHistoryEntry { id, query, provider_id, result_count, created_at }
    }).collect())
}

/// Delete a single search history entry.
#[tauri::command]
pub fn delete_search_history_item(
    id: String,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    input_validation::validate_id(&id, "id")?;
    db.delete_search_history_item(&id).map_err(|e| e.to_string())
}

/// Clear all search history.
#[tauri::command]
pub fn clear_search_history(
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    db.clear_search_history().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guard(url: &str) -> Result<(), String> {
        guard_public_url(&reqwest::Url::parse(url).expect("parse"))
    }

    /// 这条闸挡的是「模型被搜到的内容诱导去探本机」。逐个地址列出来，是因为
    /// 少写一类前缀就是漏一个洞，而漏洞不会有编译错误提醒。
    /// **域名解析到内网也要拒。**
    ///
    /// 这条闸以前只看字面主机名：URL 里写 IP 才判，写域名就直接放行。而
    /// 解析到 127.0.0.1 / 169.254.169.254 的公共域名是现成的——拿域名绕过字面
    /// IP 检查是 SSRF 的标准走法，而 `fetch_url` 挂在 MCP 上，等于把这条路
    /// 交给了 agent。
    ///
    /// 只测判定、不测解析：把真实 DNS 拉进单测，结果就取决于跑测试那台机器的
    /// 解析器（我第一版就是这么写的，本机 DNS 把它解析到了公网地址，测试当场
    /// 变红——飘的是测试不是代码）。
    #[test]
    fn a_hostname_resolving_into_the_intranet_is_refused() {
        use std::net::IpAddr;
        let cases: [(&str, IpAddr); 4] = [
            ("evil.example", "127.0.0.1".parse().unwrap()),
            ("meta.example", "169.254.169.254".parse().unwrap()), // 云元数据
            ("lan.example", "192.168.1.10".parse().unwrap()),
            ("v6.example", "::1".parse().unwrap()),
        ];
        for (host, ip) in cases {
            assert!(
                super::reject_if_any_resolved_address_is_private(host, &[ip]).is_err(),
                "{host} 解析到 {ip} 应被拒绝"
            );
        }
    }

    /// 一个域名同时解析出公网和内网地址时，**也要拒**——攻击者只需要内网那条
    /// 被用上。
    #[test]
    fn a_mixed_resolution_is_still_refused() {
        use std::net::IpAddr;
        let ips: Vec<IpAddr> = vec![
            "93.184.216.34".parse().unwrap(), // 公网
            "10.0.0.5".parse().unwrap(),      // 内网
        ];
        assert!(super::reject_if_any_resolved_address_is_private("mixed.example", &ips).is_err());
    }

    /// 纯公网解析要放行——过度拦截会让联网抓取整个不能用。
    #[test]
    fn public_resolutions_pass() {
        use std::net::IpAddr;
        let ips: Vec<IpAddr> = vec!["93.184.216.34".parse().unwrap()];
        assert!(super::reject_if_any_resolved_address_is_private("ok.example", &ips).is_ok());
    }

    #[test]
    fn internal_addresses_are_refused() {
        for url in [
            "http://127.0.0.1:1421/v1/messages",
            "http://127.9.9.9/",
            "http://localhost/",
            "http://foo.localhost/",
            "http://printer.local/",
            "http://10.0.0.5/",
            "http://192.168.1.1/",
            "http://172.16.3.4/",
            "http://169.254.169.254/latest/meta-data/", // 云元数据服务
            "http://[::1]/",
            "http://[::ffff:127.0.0.1]:1421/v1/messages",
            "http://[::ffff:169.254.169.254]/latest/meta-data/",
            "http://[::ffff:10.0.0.1]/",
            "http://[fd00::1]/",
            "http://[fe80::1]/",
            "http://0.0.0.0/",
            "ftp://example.com/x",
            "file:///C:/Windows/win.ini",
        ] {
            assert!(guard(url).is_err(), "{url} 应被拒绝");
        }
    }

    #[test]
    fn public_http_urls_pass() {
        // 只用**字面公网 IP**：`guard` 现在会对主机名做真实 DNS 解析，放
        // `https://example.com/` 进来就等于把这条测试绑到跑测试那台机器的网络上，
        // 断网的 CI 会无缘无故变红。主机名那一路由
        // `a_hostname_resolving_into_the_intranet_is_refused` 等三条离线覆盖。
        for url in ["http://93.184.216.34/", "https://93.184.216.34/a?b=c"] {
            assert!(guard(url).is_ok(), "{url} 应放行：{:?}", guard(url));
        }
    }

    /// script/style 的内容混进正文会污染模型读到的东西——JS 里的字符串、
    /// CSS 选择器，看起来都像正文。
    #[test]
    fn script_and_style_bodies_never_reach_the_text() {
        let html = "<html><head><style>.a{color:red}</style>\
            <script>var secret = '不该出现';</script></head>\
            <body><h1>标题</h1><p>第一段</p><p>第二段</p></body></html>";
        let text = html_to_text(html);
        assert!(!text.contains("不该出现"), "{text}");
        assert!(!text.contains("color:red"), "{text}");
        assert!(text.contains("标题"), "{text}");
        // 块级标签要换行，否则整页糊成一行、模型读不出段落边界。
        assert!(!text.contains("第一段第二段"), "段落之间必须断开：{text:?}");
        assert!(text.contains("第一段\n"), "{text:?}");
    }

    #[test]
    fn entities_are_decoded_and_whitespace_collapsed() {
        let text = html_to_text("<p>a &amp;   b</p>\n\n\n<p>&lt;tag&gt;</p>");
        assert!(text.contains("a & b"), "{text:?}");
        assert!(text.contains("<tag>"), "{text:?}");
        assert!(!text.contains("\n\n\n"), "{text:?}");
    }

    /// 没有闭合标签的 `<script` 不能把后面的正文一起吞掉之后再抛出去——
    /// 要么安全丢弃，要么正常渲染，但绝不能 panic 或输出脚本内容。
    #[test]
    fn unclosed_script_does_not_leak_or_panic() {
        let text = html_to_text("<p>前面</p><script>var x = '泄漏'");
        assert!(text.contains("前面"), "{text:?}");
        assert!(!text.contains("泄漏"), "{text:?}");
    }
}
