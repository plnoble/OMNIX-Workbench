import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";

// Local fonts (replaces Google Fonts CDN — works offline & in China)
import "@fontsource/outfit/300.css";
import "@fontsource/outfit/400.css";
import "@fontsource/outfit/500.css";
import "@fontsource/outfit/600.css";
import "@fontsource/outfit/700.css";
import "@fontsource/inter/300.css";
import "@fontsource/inter/400.css";
import "@fontsource/inter/500.css";
import "@fontsource/inter/600.css";
import "@fontsource/inter/700.css";
import "@fontsource/fira-code/400.css";
import "@fontsource/fira-code/500.css";

import "./styles/globals.css";

// ── Error Boundary — prevents silent black screen ─────
class ErrorBoundary extends React.Component<
  { children: React.ReactNode },
  { error: Error | null }
> {
  state: { error: Error | null } = { error: null };

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  render() {
    if (this.state.error) {
      return (
        <div style={{
          padding: 32, color: "#ef4444", background: "#0a0b10",
          fontFamily: "system-ui, sans-serif", minHeight: "100vh",
          display: "flex", flexDirection: "column", alignItems: "center",
          justifyContent: "center", gap: 12,
        }}>
          <h1 style={{ fontSize: 20, margin: 0 }}>⚠️ OMNIX 渲染错误</h1>
          <pre style={{
            fontSize: 12, color: "#94a3b8", maxWidth: 600,
            overflow: "auto", padding: 16, background: "rgba(255,255,255,0.05)",
            borderRadius: 8, whiteSpace: "pre-wrap",
          }}>{this.state.error.message}</pre>
          <button onClick={() => this.setState({ error: null })}
            style={{
              padding: "8px 24px", border: "1px solid rgba(255,255,255,0.1)",
              borderRadius: 8, color: "#00f2fe", background: "transparent",
              cursor: "pointer", fontSize: 14,
            }}>
            重试
          </button>
        </div>
      );
    }
    return this.props.children;
  }
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ErrorBoundary>
      <App />
    </ErrorBoundary>
  </React.StrictMode>,
);
