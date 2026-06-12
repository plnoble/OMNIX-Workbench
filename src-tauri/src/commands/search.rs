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
    Ok(rows.into_iter().map(|(id, name, api_type, api_key, api_address, is_enabled)| {
        SearchProvider { id, name, api_type, api_key, api_address, is_enabled }
    }).collect())
}

/// Save (upsert) a search provider configuration.
#[tauri::command]
pub fn save_search_provider(
    provider: SearchProvider,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    db.save_search_provider(&provider.id, &provider.name, &provider.api_type, &provider.api_key, &provider.api_address, provider.is_enabled)
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
    let limit = limit.unwrap_or(10);
    let providers = db.get_search_providers().map_err(|e| e.to_string())?;
    let provider = if let Some(pid) = provider_id {
        providers.into_iter().find(|p| p.0 == pid && p.5)
            .ok_or_else(|| format!("Search provider '{}' not found or disabled", pid))?
    } else {
        providers.into_iter().find(|p| p.5)
            .ok_or_else(|| "No enabled search provider found".to_string())?
    };
    let (provider_id, provider_name, api_type, api_key, api_address, _is_enabled) = provider;

    // Simple percent-encoding for search queries
    let encoded_query: String = query.replace(' ', "%20")
        .replace('+', "%2B")
        .replace('&', "%26")
        .replace('#', "%23")
        .replace('?', "%3F")
        .replace('/', "%2F");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());
    let results = match api_type.as_str() {
        "searxng" => {
            let url = format!("{}/search?q={}&format=json&categories=general", api_address.trim_end_matches('/'), encoded_query);
            let resp = client.get(&url)
                .timeout(std::time::Duration::from_secs(15))
                .send().await.map_err(|e| format!("SearXNG request failed: {}", e))?;
            let json: serde_json::Value = resp.json().await.map_err(|e| format!("SearXNG parse failed: {}", e))?;
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
        "brave" => {
            let url = format!("https://api.search.brave.com/res/v1/web/search?q={}&count={}", encoded_query, limit);
            let mut req = client.get(&url)
                .timeout(std::time::Duration::from_secs(15));
            if !api_key.is_empty() {
                req = req.header("X-Subscription-Token", &api_key);
            }
            let resp = req.send().await.map_err(|e| format!("Brave request failed: {}", e))?;
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
                .send().await.map_err(|e| format!("DuckDuckGo request failed: {}", e))?;
            let json: serde_json::Value = resp.json().await.map_err(|e| format!("DuckDuckGo parse failed: {}", e))?;
            let mut out = Vec::new();
            // DDG instant answer
            if let Some(abstract_text) = json.get("AbstractText").and_then(|v| v.as_str()) {
                if !abstract_text.is_empty() {
                    out.push(WebSearchResult {
                        title: json.get("Heading").and_then(|v| v.as_str()).unwrap_or(&query).to_string(),
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
                .send().await.map_err(|e| format!("Tavily request failed: {}", e))?;
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
                .send().await.map_err(|e| format!("Exa request failed: {}", e))?;
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
            let url = format!("https://search.zhpu.ai/search?q={}&limit={}", encoded_query, limit);
            let mut req = client.get(&url)
                .timeout(std::time::Duration::from_secs(15));
            if !api_key.is_empty() {
                req = req.header("Authorization", format!("Bearer {}", api_key));
            }
            let resp = req.send().await.map_err(|e| format!("Zhipu search request failed: {}", e))?;
            let json: serde_json::Value = resp.json().await.map_err(|e| format!("Zhipu search parse failed: {}", e))?;
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
        "bocha" => {
            let url = format!("https://api.bochaai.com/v1/web-search?q={}&count={}", encoded_query, limit);
            let mut req = client.get(&url)
                .timeout(std::time::Duration::from_secs(15));
            if !api_key.is_empty() {
                req = req.header("Authorization", format!("Bearer {}", api_key));
            }
            let resp = req.send().await.map_err(|e| format!("Bocha request failed: {}", e))?;
            let json: serde_json::Value = resp.json().await.map_err(|e| format!("Bocha parse failed: {}", e))?;
            let mut out = Vec::new();
            if let Some(results) = json.get("data").and_then(|d| d.get("webPages")).and_then(|w| w.get("value")).and_then(|v| v.as_array()) {
                for (i, item) in results.iter().take(limit as usize).enumerate() {
                    out.push(WebSearchResult {
                        title: item.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        url: item.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        snippet: item.get("snippet").and_then(|v| v.as_str()).unwrap_or("").to_string(),
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
            let resp = req.send().await.map_err(|e| format!("Jina search request failed: {}", e))?;
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
                .send().await.map_err(|e| format!("Google search request failed: {}", e))?;
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
        "bing" => {
            let url = format!("https://api.bing.microsoft.com/v7.0/search?q={}&count={}", encoded_query, limit);
            let resp = client.get(&url)
                .timeout(std::time::Duration::from_secs(15))
                .header("Ocp-Apim-Subscription-Key", &api_key)
                .send().await.map_err(|e| format!("Bing search request failed: {}", e))?;
            let json: serde_json::Value = resp.json().await.map_err(|e| format!("Bing search parse failed: {}", e))?;
            let mut out = Vec::new();
            if let Some(results) = json.get("webPages").and_then(|w| w.get("value")).and_then(|v| v.as_array()) {
                for (i, item) in results.iter().take(limit as usize).enumerate() {
                    out.push(WebSearchResult {
                        title: item.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        url: item.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        snippet: item.get("snippet").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        source: provider_name.clone(),
                        position: i as i32,
                    });
                }
            }
            out
        }
        other => return Err(format!("Unsupported search provider type: {}", other)),
    };

    // Save to search history
    let history_id = format!("sh_{}", chrono::Utc::now().timestamp_millis());
    let results_json = serde_json::to_string(&results).unwrap_or_else(|_| "[]".to_string());
    let _ = db.save_search_history(&history_id, &query, &provider_id, results.len() as i32, &results_json);

    Ok(results)
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
