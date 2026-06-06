import React, { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface AgentAccount {
  id: string;
  account_name: string;
  api_key: string;
  api_host: string;
  target_model: string;
  is_active: boolean;
}

interface ApiResult {
  accountName: string;
  model: string;
  content: string;
  loading: boolean;
  error?: string;
}

interface WebExpert {
  id: string;
  name: string;
  url: string;
  selector: string; // Used for generic target inputs
}

const WEB_EXPERTS: WebExpert[] = [
  {
    id: "deepseek",
    name: "DeepSeek (网页版)",
    url: "https://chat.deepseek.com",
    selector: "#chat-input"
  },
  {
    id: "chatgpt",
    name: "ChatGPT (网页版)",
    url: "https://chatgpt.com",
    selector: "#prompt-textarea"
  },
  {
    id: "doubao",
    name: "豆包 (网页版)",
    url: "https://www.doubao.com",
    selector: ".chat-input-editor"
  },
  {
    id: "gemini",
    name: "Gemini (网页版)",
    url: "https://gemini.google.com",
    selector: ".textarea"
  },
  {
    id: "yuanbao",
    name: "腾讯元宝 (网页版)",
    url: "https://yuanbao.tencent.com",
    selector: ".input-area"
  }
];

interface CompareHubProps {
  proxyPort: string;
}

export const CompareHub: React.FC<CompareHubProps> = ({ proxyPort }) => {
  const [mode, setMode] = useState<"api" | "web">("api");
  const [prompt, setPrompt] = useState("");
  const [accounts, setAccounts] = useState<AgentAccount[]>([]);
  
  // API Mode States
  const [selectedApiAccs, setSelectedApiAccs] = useState<string[]>([]);
  const [apiResults, setApiResults] = useState<{ [accId: string]: ApiResult }>({});
  
  // Web Mode States
  const [selectedWebExps, setSelectedWebExps] = useState<string[]>(["deepseek", "chatgpt"]);
  const [webActive, setWebActive] = useState(false);
  const [extractedTexts, setExtractedTexts] = useState<{ [expId: string]: string }>({});
  const [selectorError, setSelectorError] = useState<string | null>(null);
  
  // Fusion Summary States
  const [fusionContent, setFusionContent] = useState("");
  const [fusionLoading, setFusionLoading] = useState(false);

  const containerRef = useRef<HTMLDivElement | null>(null);
  const resizeRef = useRef<number | null>(null);
  const abortControllersRef = useRef<AbortController[]>([]);
  const fusionAbortControllerRef = useRef<AbortController | null>(null);

  // Load agent accounts on mount
  useEffect(() => {
    loadAccounts();
    return () => {
      // Hide Native Webview windows when leaving CompareHub tab (pooling)
      invoke("hide_compare_windows").catch(err => console.error(err));
      // Abort all active fetch requests
      abortControllersRef.current.forEach(controller => controller.abort());
      if (fusionAbortControllerRef.current) {
        fusionAbortControllerRef.current.abort();
      }
    };
  }, []);

  // Listen to extracted text and failures from sub Webviews
  useEffect(() => {
    let unlistenPromise = listen<{ label: string; text: string }>("expert-text-extracted", (event) => {
      const expId = event.payload.label.replace("expert-", "");
      setExtractedTexts(prev => ({
        ...prev,
        [expId]: event.payload.text
      }));
    });

    let unlistenFailPromise = listen<{ expert: string }>("expert-selector-failed", (event) => {
      setSelectorError(`无法在 [${event.payload.expert}] 页面中自动定位输入框。已自动将 Prompt 复制至剪贴板，您可以在页面中手动粘贴 (Ctrl+V) 并发送。`);
    });

    return () => {
      unlistenPromise.then(unlisten => unlisten());
      unlistenFailPromise.then(unlisten => unlisten());
    };
  }, []);

  // Handle window resizing to keep sub Webviews aligned with DOM placeholders
  useEffect(() => {
    if (!webActive || mode !== "web") return;

    const handleResize = () => {
      if (resizeRef.current) cancelAnimationFrame(resizeRef.current);
      resizeRef.current = requestAnimationFrame(updateWebviewLayout);
    };

    window.addEventListener("resize", handleResize);
    return () => {
      window.removeEventListener("resize", handleResize);
      if (resizeRef.current) cancelAnimationFrame(resizeRef.current);
    };
  }, [webActive, mode, selectedWebExps]);

  const loadAccounts = async () => {
    try {
      const list = await invoke<AgentAccount[]>("get_agent_accounts");
      setAccounts(list);
      // Select first two connected accounts as default
      const activeIds = list.filter(acc => acc.api_key.trim().length > 0).map(acc => acc.id);
      setSelectedApiAccs(activeIds.slice(0, 3));
    } catch (e) {
      console.error("Failed to load accounts for compare hub:", e);
    }
  };

  // API Concurrent Dispatcher (SSE reader)
  const handleApiCompareSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!prompt.trim() || selectedApiAccs.length === 0) return;

    // Abort existing requests
    abortControllersRef.current.forEach(controller => controller.abort());
    abortControllersRef.current = [];

    const initialResults: { [accId: string]: ApiResult } = {};
    selectedApiAccs.forEach(accId => {
      const acc = accounts.find(a => a.id === accId);
      if (acc) {
        initialResults[accId] = {
          accountName: acc.account_name,
          model: acc.target_model,
          content: "",
          loading: true
        };
      }
    });
    setApiResults(initialResults);
    setFusionContent("");

    // Concurrently trigger fetch requests
    selectedApiAccs.forEach(async (accId) => {
      const controller = new AbortController();
      abortControllersRef.current.push(controller);
      try {
        const response = await fetch(`http://localhost:${proxyPort}/v1/chat/completions`, {
          method: "POST",
          headers: {
            "Content-Type": "application/json",
            "Authorization": "Bearer bypass",
            "x-omnix-account-id": accId
          },
          body: JSON.stringify({
            model: "Auto",
            messages: [{ role: "user", content: prompt }],
            stream: true
          }),
          signal: controller.signal
        });

        if (!response.ok) {
          throw new Error(`HTTP ${response.status}: ${await response.text()}`);
        }

        const reader = response.body?.getReader();
        const decoder = new TextDecoder();
        if (!reader) throw new Error("Null reader response");

        let done = false;
        let accumText = "";

        while (!done) {
          const { value, done: doneReading } = await reader.read();
          done = doneReading;
          if (value) {
            const chunk = decoder.decode(value, { stream: true });
            const lines = chunk.split("\n").filter(l => l.trim() !== "");
            
            for (const line of lines) {
              if (line.startsWith("data: [DONE]")) {
                done = true;
                break;
              }
              if (line.startsWith("data: ")) {
                try {
                  const dataObj = JSON.parse(line.substring(6));
                  const delta = dataObj.choices?.[0]?.delta?.content || "";
                  accumText += delta;
                  
                  setApiResults(prev => ({
                    ...prev,
                    [accId]: {
                      ...prev[accId],
                      content: accumText,
                      loading: !done
                    }
                  }));
                } catch (err) {
                  // Sometimes line is partial, ignore JSON parsing errors
                }
              }
            }
          }
        }

        setApiResults(prev => ({
          ...prev,
          [accId]: {
            ...prev[accId],
            loading: false
          }
        }));

      } catch (err: any) {
        if (err.name === 'AbortError') return;
        console.error("API dispatch error for account:", accId, err);
        setApiResults(prev => ({
          ...prev,
          [accId]: {
            ...prev[accId],
            loading: false,
            error: err.message || "请求失败"
          }
        }));
      } finally {
        abortControllersRef.current = abortControllersRef.current.filter(c => c !== controller);
      }
    });
  };

  // Sync Layout calculation and positioning of sub-Webview windows over HTML placeholders
  const updateWebviewLayout = () => {
    if (!containerRef.current || selectedWebExps.length === 0) return;

    const cards = containerRef.current.querySelectorAll(".web-placeholder-card");
    const layouts = [];

    for (let i = 0; i < cards.length; i++) {
      const card = cards[i];
      const expId = card.getAttribute("data-exp-id");
      const exp = WEB_EXPERTS.find(e => e.id === expId);
      if (exp) {
        const rect = card.getBoundingClientRect();
        layouts.push({
          label: `expert-${exp.id}`,
          url: exp.url,
          // Calculate positions relative to Tauri main window outer position
          x: rect.left,
          y: rect.top,
          width: rect.width,
          height: rect.height
        });
      }
    }

    if (layouts.length > 0) {
      invoke("set_compare_windows_layout", { layout: layouts })
        .catch(err => console.error("Failed to set webviews layout:", err));
    }
  };

  const handleLaunchWebCompare = () => {
    if (selectedWebExps.length === 0) {
      alert("请至少选择一个网页版 AI 进行比对！");
      return;
    }
    setWebActive(true);
    setFusionContent("");
    setExtractedTexts({});
    // Delay slightly to allow React placeholders to mount before positioning Webviews
    setTimeout(updateWebviewLayout, 250);
  };

  const handleCloseWebCompare = () => {
    setWebActive(false);
    invoke("close_compare_windows").catch(err => console.error(err));
  };

  // Sync prompt text to all Native Webview windows and trigger send clicks
  const handleWebSyncPrompt = async () => {
    if (!prompt.trim()) return;

    // Write to clipboard as a safety copy
    try {
      navigator.clipboard.writeText(prompt);
    } catch (_) {}

    selectedWebExps.forEach(async (expId) => {
      const exp = WEB_EXPERTS.find(e => e.id === expId);
      if (!exp) return;

      // Select specific DOM interaction scripts depending on the host
      let jsScript = "";
      
      if (expId === "chatgpt") {
        jsScript = `
          (function() {
            try {
              var ta = document.querySelector('textarea#prompt-textarea') || document.querySelector('textarea');
              if (ta) {
                ta.value = ${JSON.stringify(prompt)};
                ta.dispatchEvent(new Event('input', { bubbles: true }));
                setTimeout(function() {
                  var btn = document.querySelector('button[data-testid="send-button"]') || document.querySelector('button[aria-label="Send prompt"]');
                  if (btn) btn.click();
                }, 200);
              } else {
                window.__TAURI__.event.emit('expert-selector-failed', { expert: 'ChatGPT' });
              }
            } catch (e) {
              window.__TAURI__.event.emit('expert-selector-failed', { expert: 'ChatGPT' });
            }
          })();
        `;
      } else if (expId === "deepseek") {
        jsScript = `
          (function() {
            try {
              var ta = document.getElementById('chat-input') || document.querySelector('textarea');
              if (ta) {
                ta.value = ${JSON.stringify(prompt)};
                ta.dispatchEvent(new Event('input', { bubbles: true }));
                setTimeout(function() {
                  var btn = document.querySelector('div[role="button"]') || document.querySelector('button');
                  if (btn) btn.click();
                }, 200);
              } else {
                window.__TAURI__.event.emit('expert-selector-failed', { expert: 'DeepSeek' });
              }
            } catch (e) {
              window.__TAURI__.event.emit('expert-selector-failed', { expert: 'DeepSeek' });
            }
          })();
        `;
      } else if (expId === "doubao") {
        jsScript = `
          (function() {
            try {
              var ta = document.querySelector('.chat-input-editor') || document.querySelector('textarea') || document.querySelector('input');
              if (ta) {
                if (ta.tagName === 'TEXTAREA' || ta.tagName === 'INPUT') {
                  ta.value = ${JSON.stringify(prompt)};
                } else {
                  ta.innerText = ${JSON.stringify(prompt)};
                }
                ta.dispatchEvent(new Event('input', { bubbles: true }));
                setTimeout(function() {
                  var btn = document.querySelector('.send-btn') || document.querySelector('.chat-input-send-button') || document.querySelector('button');
                  if (btn) btn.click();
                }, 200);
              } else {
                window.__TAURI__.event.emit('expert-selector-failed', { expert: '豆包' });
              }
            } catch (e) {
              window.__TAURI__.event.emit('expert-selector-failed', { expert: '豆包' });
            }
          })();
        `;
      } else if (expId === "gemini") {
        jsScript = `
          (function() {
            try {
              var ta = document.querySelector('.textarea') || document.querySelector('textarea') || document.querySelector('[role="textbox"]');
              if (ta) {
                if (ta.tagName === 'TEXTAREA' || ta.tagName === 'INPUT') {
                  ta.value = ${JSON.stringify(prompt)};
                } else {
                  ta.innerText = ${JSON.stringify(prompt)};
                }
                ta.dispatchEvent(new Event('input', { bubbles: true }));
                setTimeout(function() {
                  var btn = document.querySelector('.send-button') || document.querySelector('button[aria-label="Send message"]');
                  if (btn) btn.click();
                }, 200);
              } else {
                window.__TAURI__.event.emit('expert-selector-failed', { expert: 'Gemini' });
              }
            } catch (e) {
              window.__TAURI__.event.emit('expert-selector-failed', { expert: 'Gemini' });
            }
          })();
        `;
      } else if (expId === "yuanbao") {
        jsScript = `
          (function() {
            try {
              var ta = document.querySelector('.input-area') || document.querySelector('textarea') || document.querySelector('[contenteditable="true"]');
              if (ta) {
                if (ta.tagName === 'TEXTAREA' || ta.tagName === 'INPUT') {
                  ta.value = ${JSON.stringify(prompt)};
                } else {
                  ta.innerText = ${JSON.stringify(prompt)};
                }
                ta.dispatchEvent(new Event('input', { bubbles: true }));
                setTimeout(function() {
                  var btn = document.querySelector('.send-button') || document.querySelector('button');
                  if (btn) btn.click();
                }, 200);
              } else {
                window.__TAURI__.event.emit('expert-selector-failed', { expert: '腾讯元宝' });
              }
            } catch (e) {
              window.__TAURI__.event.emit('expert-selector-failed', { expert: '腾讯元宝' });
            }
          })();
        `;
      }

      if (jsScript) {
        invoke("eval_compare_window", { label: `expert-${expId}`, js: jsScript })
          .catch(err => console.error("Failed to eval webview:", expId, err));
      }
    });
  };

  // Run document.body.innerText extraction across all active sub-Webviews
  const triggerWebtextExtraction = () => {
    selectedWebExps.forEach(expId => {
      const jsScript = `
        (function() {
          try {
            var txt = document.body.innerText;
            // Emit to Tauri main window
            window.__TAURI__.event.emit('expert-text-extracted', { label: 'expert-${expId}', text: txt });
          } catch(e) {
            console.error("Text extraction failed", e);
          }
        })();
      `;
      invoke("eval_compare_window", { label: `expert-${expId}`, js: jsScript })
        .catch(err => console.error("Text extraction eval error:", expId, err));
    });
  };

  // Combine results from either API streams or Web page extracts, and feed it to target LLM
  const handleFusionSummary = async () => {
    const textDict: { [name: string]: string } = {};

    if (mode === "api") {
      Object.entries(apiResults).forEach(([_, res]) => {
        if (res.content.trim()) {
          textDict[res.accountName] = res.content;
        }
      });
    } else {
      // Trigger Webtext extraction first
      triggerWebtextExtraction();
      // Wait briefly for emit events to settle
      await new Promise(resolve => setTimeout(resolve, 800));
      
      selectedWebExps.forEach(expId => {
        const text = extractedTexts[expId];
        const exp = WEB_EXPERTS.find(e => e.id === expId);
        if (text && text.trim().length > 100 && exp) {
          textDict[exp.name] = text;
        }
      });
    }

    if (Object.keys(textDict).length === 0) {
      alert("无可熔炼的专家回答内容！请确保 AI 回答完全生成后再试。");
      return;
    }

    setFusionLoading(true);
    setFusionContent("");

    const sources = Object.entries(textDict)
      .map(([name, text]) => `【AI 专家 ${name} 的回复】：\n${text.slice(0, 3500)}`)
      .join("\n\n");

    const fusionPrompt = `您是 OMNIX 高级系统融合架构师。用户提出了以下开发问题，并且好几个 AI 专家给出了不同的思考回复：

【问题】：${prompt}

${sources}

请你作为首席架构评审，全面比对上述不同 AI 专家的内容，去伪存真，剔除他们可能存在的反模式、Tokio 异步死锁、CORS 越权或者过度设计的漏洞，将所有优点整理并提炼成一份最专业、最具实用指导性、线程安全的【最佳系统开发决策方案】：`;

    if (fusionAbortControllerRef.current) {
      fusionAbortControllerRef.current.abort();
    }
    const controller = new AbortController();
    fusionAbortControllerRef.current = controller;

    try {
      const response = await fetch(`http://localhost:${proxyPort}/v1/chat/completions`, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          "Authorization": "Bearer bypass"
        },
        body: JSON.stringify({
          model: "Auto", // Route through Auto router
          messages: [{ role: "user", content: fusionPrompt }],
          stream: true
        }),
        signal: controller.signal
      });

      if (!response.ok) {
        throw new Error(`HTTP ${response.status}: ${await response.text()}`);
      }

      const reader = response.body?.getReader();
      const decoder = new TextDecoder();
      if (!reader) throw new Error("Null response body");

      let done = false;
      let accumText = "";

      while (!done) {
        const { value, done: doneReading } = await reader.read();
        done = doneReading;
        if (value) {
          const chunk = decoder.decode(value, { stream: true });
          const lines = chunk.split("\n").filter(l => l.trim() !== "");
          
          for (const line of lines) {
            if (line.startsWith("data: [DONE]")) {
              done = true;
              break;
            }
            if (line.startsWith("data: ")) {
              try {
                const dataObj = JSON.parse(line.substring(6));
                const delta = dataObj.choices?.[0]?.delta?.content || "";
                accumText += delta;
                setFusionContent(accumText);
              } catch (err) {}
            }
          }
        }
      }

    } catch (e: any) {
      if (e.name === 'AbortError') return;
      console.error("Fusion furnace summary error:", e);
      setFusionContent("熔炼总结发生错误: " + (e.message || e));
    } finally {
      if (fusionAbortControllerRef.current === controller) {
        fusionAbortControllerRef.current = null;
      }
      setFusionLoading(false);
    }
  };

  const handleCopyText = (text: string) => {
    navigator.clipboard.writeText(text);
    alert("文本已复制到剪贴板！");
  };

  return (
    <div className="compare-hub-container" ref={containerRef} style={{ display: "flex", flexDirection: "column", height: "100%", padding: "20px", overflowY: "auto", gap: "20px" }}>
      
      {/* Title & Engine Mode Switcher */}
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
        <div>
          <h2 style={{ margin: 0, fontSize: "20px", display: "flex", alignItems: "center", gap: "8px" }}>
            ⚖️ AI 专家比对与最佳结论熔炼炉
          </h2>
          <span style={{ fontSize: "12px", color: "var(--text-muted)" }}>
            一问多答模式，免除重复发问，多维度并排参考得出系统开发最佳解决方案
          </span>
        </div>

        <div className="tab-switcher" style={{ display: "flex", background: "rgba(255,255,255,0.03)", padding: "4px", borderRadius: "8px", border: "1px solid var(--border-color)" }}>
          <button 
            className={`btn-tab ${mode === "api" ? "active" : ""}`}
            style={{ padding: "6px 16px", background: mode === "api" ? "var(--accent-color)" : "transparent", border: "none", color: "white", borderRadius: "6px", fontSize: "12px", cursor: "pointer" }}
            onClick={() => { setMode("api"); handleCloseWebCompare(); }}
            disabled={webActive}
          >
            🔌 API 并行极速比对
          </button>
          <button 
            className={`btn-tab ${mode === "web" ? "active" : ""}`}
            style={{ padding: "6px 16px", background: mode === "web" ? "var(--accent-color)" : "transparent", border: "none", color: "white", borderRadius: "6px", fontSize: "12px", cursor: "pointer" }}
            onClick={() => setMode("web")}
            disabled={webActive}
          >
            🌐 Web 网页原生比对
          </button>
        </div>
      </div>

      {/* Selector Error Banner */}
      {selectorError && (
        <div className="card" style={{ padding: "12px", display: "flex", justifyContent: "space-between", alignItems: "center", background: "rgba(239, 68, 68, 0.08)", border: "1px dashed rgba(239, 68, 68, 0.4)", borderRadius: "8px" }}>
          <span style={{ fontSize: "13px", color: "#f87171", fontWeight: "500" }}>
            ⚠️ {selectorError}
          </span>
          <button 
            className="btn btn-secondary" 
            onClick={() => setSelectorError(null)} 
            style={{ padding: "4px 10px", fontSize: "11px", border: "1px solid rgba(239,68,68,0.2)", color: "#f87171", background: "transparent", cursor: "pointer" }}
          >
            我知道了
          </button>
        </div>
      )}

      {/* API Configuration Options */}
      {mode === "api" && (
        <div className="card" style={{ padding: "16px" }}>
          <span style={{ fontSize: "13px", fontWeight: "600", color: "var(--text-secondary)", display: "block", marginBottom: "8px" }}>
            选择连通的 API 专家（最少 1 个，最多 4 个）
          </span>
          <div style={{ display: "flex", flexWrap: "wrap", gap: "10px" }}>
            {accounts.map(acc => {
              const connected = acc.api_key.trim().length > 0;
              return (
                <label 
                  key={acc.id}
                  className={`checkbox-label ${selectedApiAccs.includes(acc.id) ? "checked" : ""}`}
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: "8px",
                    padding: "8px 12px",
                    borderRadius: "8px",
                    background: selectedApiAccs.includes(acc.id) ? "rgba(168, 85, 247, 0.12)" : "rgba(255,255,255,0.02)",
                    border: selectedApiAccs.includes(acc.id) ? "1px solid #a855f7" : "1px solid var(--border-color)",
                    cursor: connected ? "pointer" : "not-allowed",
                    opacity: connected ? 1 : 0.4
                  }}
                  title={connected ? `模型: ${acc.target_model}` : "未配置 API Key"}
                >
                  <input 
                    type="checkbox"
                    checked={selectedApiAccs.includes(acc.id)}
                    disabled={!connected}
                    onChange={(e) => {
                      if (e.target.checked) {
                        setSelectedApiAccs(prev => [...prev, acc.id]);
                      } else {
                        setSelectedApiAccs(prev => prev.filter(id => id !== acc.id));
                      }
                    }}
                    style={{ cursor: connected ? "pointer" : "not-allowed" }}
                  />
                  <div>
                    <span style={{ fontSize: "13px", fontWeight: "500", display: "block" }}>{acc.account_name}</span>
                    <span style={{ fontSize: "11px", color: "var(--text-muted)" }}>{acc.target_model}</span>
                  </div>
                </label>
              );
            })}
          </div>
        </div>
      )}

      {/* Web Configuration Options */}
      {mode === "web" && !webActive && (
        <div className="card" style={{ padding: "16px" }}>
          <span style={{ fontSize: "13px", fontWeight: "600", color: "var(--text-secondary)", display: "block", marginBottom: "8px" }}>
            选择要开启比对的原生 AI 网页（建议 2 - 3 栏以防窗口过挤）
          </span>
          <div style={{ display: "flex", flexWrap: "wrap", gap: "10px", marginBottom: "16px" }}>
            {WEB_EXPERTS.map(exp => (
              <label 
                key={exp.id}
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: "8px",
                  padding: "8px 12px",
                  borderRadius: "8px",
                  background: selectedWebExps.includes(exp.id) ? "rgba(236, 72, 153, 0.1)" : "rgba(255,255,255,0.02)",
                  border: selectedWebExps.includes(exp.id) ? "1px solid #ec4899" : "1px solid var(--border-color)",
                  cursor: "pointer"
                }}
              >
                <input 
                  type="checkbox"
                  checked={selectedWebExps.includes(exp.id)}
                  onChange={(e) => {
                    if (e.target.checked) {
                      setSelectedWebExps(prev => [...prev, exp.id]);
                    } else {
                      setSelectedWebExps(prev => prev.filter(id => id !== exp.id));
                    }
                  }}
                  style={{ cursor: "pointer" }}
                />
                <span style={{ fontSize: "13px", fontWeight: "500" }}>{exp.name}</span>
              </label>
            ))}
          </div>
          <button 
            className="btn btn-primary"
            onClick={handleLaunchWebCompare}
            style={{ width: "100%", display: "flex", alignItems: "center", justifyContent: "center", gap: "6px" }}
          >
            🌐 启动网页版并排比对窗 (Launch Web Compare)
          </button>
        </div>
      )}

      {/* Web Active Floating Controller */}
      {mode === "web" && webActive && (
        <div className="card" style={{ padding: "12px", display: "flex", justifyContent: "space-between", alignItems: "center", background: "rgba(236,72,153,0.06)", border: "1px dashed rgba(236,72,153,0.4)" }}>
          <span style={{ fontSize: "13px", color: "#ec4899", fontWeight: "500" }}>
            ⚡ 网页并行比对中，您可以通过输入下方 Prompt 并点击【同步发送】进行提问。
          </span>
          <button className="btn btn-secondary" onClick={handleCloseWebCompare} style={{ padding: "4px 12px", fontSize: "12px" }}>
            ❌ 关闭所有子网页
          </button>
        </div>
      )}

      {/* Central Input Prompt Form */}
      {(!webActive || mode === "web") && (
        <form onSubmit={mode === "api" ? handleApiCompareSubmit : (e) => e.preventDefault()} className="card" style={{ padding: "16px", display: "flex", flexDirection: "column", gap: "12px" }}>
          <div className="form-group">
            <label style={{ display: "flex", justifyContent: "space-between" }}>
              <span>输入开发提问/提示词 (System Prompt)</span>
              <span style={{ fontSize: "11px", color: "var(--text-muted)" }}>
                快捷模板：点选常用 CORS 域名跨域、异步 Tokio 死锁、线程安全缓存
              </span>
            </label>
            <textarea
              className="form-input"
              rows={3}
              placeholder="例如：分析以下 Rust Tokio 并发死锁的根本原因，并给出优化好的线程安全锁方案..."
              value={prompt}
              onChange={(e) => setPrompt(e.target.value)}
              required
            />
          </div>

          <div style={{ display: "flex", gap: "10px" }}>
            {mode === "api" ? (
              <button type="submit" className="btn btn-primary" style={{ flex: 1, display: "flex", alignItems: "center", justifyContent: "center", gap: "6px" }}>
                🎯 开始 API 并行比对
              </button>
            ) : (
              <button 
                type="button" 
                className="btn btn-primary"
                onClick={handleWebSyncPrompt}
                disabled={!webActive}
                style={{ flex: 1, display: "flex", alignItems: "center", justifyContent: "center", gap: "6px" }}
              >
                🚀 全局同步发问 (Sync Web Prompt)
              </button>
            )}
          </div>
          
          <div style={{ display: "flex", flexWrap: "wrap", gap: "6px" }}>
            <span className="cap-badge speedy" style={{ cursor: "pointer" }} onClick={() => setPrompt("如何解决 Node.js 跨域请求（CORS）中首发 OPTIONS 预检请求抛出的 403 跨域失败错误？")}>CORS OPTIONS 预检</span>
            <span className="cap-badge reas" style={{ cursor: "pointer" }} onClick={() => setPrompt("分析以下 Rust 代码在使用 tokio::sync::Mutex 时为什么在多路 select 中造成死锁，如何用 std 或 ParkingLot 锁修复？")}>Tokio 异步死锁</span>
            <span className="cap-badge cod" style={{ cursor: "pointer" }} onClick={() => setPrompt("编写一个用 Rust 泛型实现的高并发 Thread-Safe LruCache 缓存模块，要求附带生命周期淘汰逻辑与单元测试用例。")}>高并发线程安全缓存</span>
          </div>
        </form>
      )}

      {/* Side-by-Side Display Columns */}
      
      {/* API Columns */}
      {mode === "api" && Object.keys(apiResults).length > 0 && (
        <div style={{ display: "grid", gridTemplateColumns: `repeat(${selectedApiAccs.length}, 1fr)`, gap: "15px", minHeight: "260px" }}>
          {selectedApiAccs.map(accId => {
            const res = apiResults[accId];
            if (!res) return null;
            return (
              <div key={accId} className="card" style={{ display: "flex", flexDirection: "column", height: "100%", padding: "16px", background: "rgba(18, 18, 24, 0.75)" }}>
                <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", borderBottom: "1px solid var(--border-color)", paddingBottom: "10px", marginBottom: "12px" }}>
                  <div>
                    <strong style={{ fontSize: "14px", display: "block" }}>{res.accountName}</strong>
                    <span style={{ fontSize: "11px", color: "var(--text-muted)" }}>{res.model}</span>
                  </div>
                  {res.loading ? (
                    <span className="pulse-dot active" title="正在生成实时流..." />
                  ) : (
                    <button className="btn-icon" onClick={() => handleCopyText(res.content)} title="复制代码" style={{ border: "none", background: "transparent", cursor: "pointer", fontSize: "14px" }}>
                      📋
                    </button>
                  )}
                </div>
                
                <div style={{ flex: 1, minHeight: "180px", overflowY: "auto", fontSize: "13px", lineHeight: "1.6", whiteSpace: "pre-wrap", color: "#e5e7eb" }}>
                  {res.error ? (
                    <span style={{ color: "#ef4444" }}>🚫 错误: {res.error}</span>
                  ) : (
                    res.content || <span style={{ color: "var(--text-muted)" }}>等待回答流生成中...</span>
                  )}
                </div>
              </div>
            );
          })}
        </div>
      )}

      {/* Web Columns Placeholders */}
      {mode === "web" && webActive && (
        <div style={{ display: "grid", gridTemplateColumns: `repeat(${selectedWebExps.length}, 1fr)`, gap: "12px", height: "450px", border: "1px solid var(--border-color)", borderRadius: "12px", background: "rgba(0,0,0,0.15)", padding: "8px", overflow: "hidden" }}>
          {selectedWebExps.map(expId => {
            const exp = WEB_EXPERTS.find(e => e.id === expId);
            return (
              <div 
                key={expId}
                className="web-placeholder-card"
                data-exp-id={expId}
                style={{
                  height: "100%",
                  borderRadius: "8px",
                  border: "1px dashed rgba(255,255,255,0.06)",
                  background: "rgba(0,0,0,0.45)",
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "center",
                  position: "relative"
                }}
              >
                {/* Visual indicator for HTML placeholder bounding box */}
                <div style={{ textAlign: "center", opacity: 0.15, pointerEvents: "none" }}>
                  <span style={{ fontSize: "28px", display: "block", marginBottom: "8px" }}>🌐</span>
                  <span style={{ fontSize: "12px" }}>{exp?.name} Native View</span>
                </div>
              </div>
            );
          })}
        </div>
      )}

      {/* Summary Fusion Furnace Card */}
      {((mode === "api" && Object.keys(apiResults).length > 0) || (mode === "web" && webActive)) && (
        <div className="card" style={{
          padding: "20px",
          background: "linear-gradient(135deg, rgba(168, 85, 247, 0.08) 0%, rgba(236, 72, 153, 0.08) 100%)",
          border: "1px solid rgba(168, 85, 247, 0.25)",
          boxShadow: "0 4px 20px rgba(168,85,247,0.15)",
          display: "flex",
          flexDirection: "column",
          gap: "16px"
        }}>
          <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
            <div>
              <strong style={{ fontSize: "15px", color: "#f3e8ff", display: "block" }}>🔮 AI 专家比对总结熔炼炉 (Fusion Summary Furnace)</strong>
              <span style={{ fontSize: "12px", color: "var(--text-muted)" }}>
                {mode === "api" 
                  ? "提取上述所有 API 专家的回答内容进行智能提炼，融合出最安全、无漏洞的最优系统级决策。"
                  : "自动抓取上述所有原生网页的内容文字（InnerText），并通过最强模型提炼最佳答案。"}
              </span>
            </div>
            
            <button 
              className="btn btn-primary"
              onClick={handleFusionSummary}
              disabled={fusionLoading}
              style={{ padding: "8px 20px", display: "flex", alignItems: "center", gap: "6px" }}
            >
              {fusionLoading ? "熔炼中..." : "🔥 开始点火熔炼"}
            </button>
          </div>

          {(fusionLoading || fusionContent) && (
            <div style={{
              background: "rgba(0,0,0,0.35)",
              border: "1px solid rgba(168,85,247,0.15)",
              borderRadius: "8px",
              padding: "16px",
              minHeight: "100px",
              fontSize: "13px",
              lineHeight: "1.6",
              color: "#e5e7eb",
              whiteSpace: "pre-wrap",
              position: "relative"
            }}>
              {fusionContent ? (
                <>
                  <div style={{ display: "flex", justifyContent: "flex-end", marginBottom: "8px", borderBottom: "1px solid rgba(255,255,255,0.04)", paddingBottom: "6px" }}>
                    <button className="btn-icon" onClick={() => handleCopyText(fusionContent)} title="复制熔炼方案" style={{ border: "none", background: "transparent", cursor: "pointer", fontSize: "14px" }}>
                      📋 复制方案
                    </button>
                  </div>
                  {fusionContent}
                </>
              ) : (
                <div style={{ textAlign: "center", padding: "20px 0" }}>
                  <span className="pulse-dot active" style={{ display: "inline-block", marginRight: "10px" }} />
                  <span style={{ color: "var(--text-muted)" }}>正在从各大网页与回答中深度提炼知识，生成首席架构师决策方案中...</span>
                </div>
              )}
            </div>
          )}
        </div>
      )}

    </div>
  );
};
