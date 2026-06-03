mod db;
mod proxy;
mod agent;
mod commands;

use std::sync::Arc;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 1. Initialize SQLite Database Schema
    let db = Arc::new(db::DbManager::new());
    
    // 2. Initialize Agent Subprocess watchdog Manager
    let agent_manager = Arc::new(agent::AgentManager::new(Arc::clone(&db)));
    
    // 3. Resolve port and launch background HTTP translation proxy
    let port_str = db.get_setting("proxy_port").unwrap_or(None).unwrap_or_else(|| "1421".to_string());
    let port = port_str.parse::<u16>().unwrap_or(1421);
    
    let mut proxy_server = proxy::ProxyServer::new();
    proxy_server.start(Arc::clone(&db), port);
    
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(move |app| {
            // Share references globally as Tauri App State
            app.manage(db);
            app.manage(agent_manager);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_app_setting,
            commands::set_app_setting,
            commands::detect_installed_agents,
            commands::start_agent_session,
            commands::send_agent_stdin,
            commands::stop_agent_session,
            commands::install_agent_cli,
            commands::repair_installed_agent,
            commands::sync_external_agent_configs
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
