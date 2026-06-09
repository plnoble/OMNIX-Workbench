mod db;
mod proxy;
mod agent;
mod knowledge;
mod selection;
mod tool_adapters;
mod sync_engine;
mod agent_templates;
mod skill_frontmatter;
mod commands;

#[cfg(test)]
mod tests;

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
    
    let proxy_server = proxy::ProxyServer::new();
    let proxy_state = std::sync::Mutex::new(proxy_server);
    
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(move |app| {
            // Start the proxy server inside the Tokio runtime context
            if let Ok(mut server) = proxy_state.lock() {
                server.start(Arc::clone(&db), Arc::clone(&agent_manager), port);
            }
            
            // Share references globally as Tauri App State
            app.manage(Arc::clone(&db));
            app.manage(Arc::clone(&agent_manager));
            app.manage(proxy_state);

            // Start background tasks once the Tokio runtime is fully initialized by Tauri
            agent_manager.start_services();

            // Initialize OMNIX Status Dock floating window
            let _status_dock = tauri::WebviewWindowBuilder::new(
                app,
                "status-dock",
                tauri::WebviewUrl::App("/?window=status-dock".into())
            )
            .title("OMNIX Status Dock")
            .inner_size(200.0, 48.0)
            .decorations(false)
            .transparent(true)
            .always_on_top(true)
            .resizable(false)
            .skip_taskbar(true)
            .build();

            // Initialize OMNIX Quick Assistant floating window (hidden by default)
            let _qa = tauri::WebviewWindowBuilder::new(
                app,
                "quick-assistant",
                tauri::WebviewUrl::App("/?window=quick-assistant".into())
            )
            .title("OMNIX Quick Assistant")
            .inner_size(420.0, 520.0)
            .decorations(false)
            .transparent(true)
            .always_on_top(true)
            .resizable(false)
            .skip_taskbar(true)
            .visible(false)
            .build();

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
            commands::uninstall_agent_cli,
            commands::repair_installed_agent,
            commands::sync_external_agent_configs,
            commands::get_all_skills,
            commands::get_skill_content,
            commands::save_skill_content,
            commands::toggle_skill_active,
            commands::update_skill_profile,
            commands::fuse_skills_api,
            commands::create_skill,
            commands::get_active_agent_model,
            commands::update_active_agent_model,
            commands::get_agent_accounts,
            commands::create_agent_account,
            commands::switch_agent_account,
            commands::delete_agent_account,
            commands::get_all_memories,
            commands::create_memory,
            commands::delete_memory,
            commands::distill_session_memory,
            commands::get_all_conversations,
            commands::get_conversation_tasks,
            commands::simulate_team_task_dispatch,
            commands::get_mailbox_messages,
            commands::get_remote_access_info,
            commands::get_all_models_metadata,
            commands::get_cron_tasks,
            commands::save_cron_task,
            commands::toggle_cron_task_active,
            commands::delete_cron_task,
            commands::get_cron_runs,
            commands::clear_cron_runs,
            commands::get_active_sessions,
            commands::trigger_cron_task,
            commands::set_compare_windows_layout,
            commands::hide_compare_windows,
            commands::close_compare_windows,
            commands::eval_compare_window,
            commands::focus_main_window,
            commands::toggle_status_dock,
            commands::get_model_platforms,
            commands::save_model_platform,
            commands::delete_model_platform,
            commands::get_platform_models,
            commands::save_platform_model,
            commands::delete_platform_model,
            commands::get_active_models,
            commands::fetch_remote_models,
            commands::check_model_status,
            commands::batch_check_models,
            commands::get_conversation_messages,
            commands::create_conversation,
            commands::add_conversation_message,
            commands::delete_conversation,
            commands::get_previewable_files,
            commands::read_file_content_utf8,
            commands::read_file_as_base64,
            commands::get_workspace_git_diff,
            commands::run_env_diagnostics,
            commands::repair_env_tool,
            commands::kb_list_documents,
            commands::kb_import_document,
            commands::kb_delete_document,
            commands::kb_get_chunks,
            commands::kb_generate_embeddings,
            commands::kb_hybrid_search,
            commands::kb_rag_query,
            commands::kb_get_embedding_models,
            commands::kb_import_file,
            commands::kb_import_directory,
            commands::toggle_quick_assistant,
            commands::show_quick_assistant_with_text,
            commands::qa_query,
            commands::qa_query_stream,
            commands::capture_selection_and_show,
            commands::get_selection_text,
            commands::get_selection_with_context,
            commands::get_selection_history,
            commands::delete_selection_history_item,
            commands::clear_selection_history,
            commands::translate_text,
            commands::detect_language,
            commands::get_translation_history,
            commands::delete_translation_history_item,
            commands::clear_translation_history,
            // Search
            commands::get_search_providers,
            commands::save_search_provider,
            commands::delete_search_provider,
            commands::web_search,
            commands::get_search_history,
            commands::delete_search_history_item,
            commands::clear_search_history,
            // MCP Servers
            commands::get_mcp_servers,
            commands::save_mcp_server,
            commands::delete_mcp_server,
            // Backup
            commands::get_backup_info,
            commands::export_backup,
            commands::import_backup,
            // Prompt Library
            commands::get_prompt_library,
            commands::save_prompt_entry,
            commands::delete_prompt_entry,
            // Activity Log
            commands::log_activity,
            commands::get_activity_log,
            // Skill Sync commands (P1 — DEC-018)
            commands::get_skill_tool_status,
            commands::sync_skill_to_tools,
            commands::unsync_skill_from_tool,
            commands::scan_all_tool_skills,
            commands::toggle_skill_starred,
            commands::get_skill_targets,
            // Skill Sync Engine commands (P2 — DEC-018)
            commands::check_sync_conflicts,
            commands::sync_skill_detailed,
            commands::sync_skill_to_many,
            commands::sync_skills_batch,
            commands::check_skill_drift,
            commands::check_all_drift,
            commands::resync_all_drifted,
            // Disk Scanner commands (P4 — DEC-018)
            commands::scan_disk_skills,
            commands::import_unmanaged_skills,
            // Skill Package & Category commands (P6 — DEC-018)
            commands::export_skill_package,
            commands::import_skill_package,
            commands::export_all_skills,
            commands::update_skill_category,
            commands::list_skill_packages,
            // Git Skill Source commands (P5 — DEC-018)
            commands::clone_skill_repo,
            commands::list_repo_skills,
            commands::import_git_skill,
            commands::check_git_updates,
            commands::pull_and_update_skill,
            commands::cleanup_skill_cache,
            // Agent Template commands (Multica-inspired)
            commands::get_agent_templates,
            commands::get_agent_template,
            // Skills Lock File commands (Multica-inspired)
            commands::get_skill_lock,
            commands::update_skill_lock,
            commands::verify_skill_lock,
            // Agent Execution Environment commands (Multica-inspired)
            commands::get_agent_exec_config,
            commands::save_agent_exec_config,
            // Autopilot commands (Multica-inspired)
            commands::get_autopilot_config,
            commands::save_autopilot_config,
            // Workspace GC commands (Multica-inspired)
            commands::get_gc_config,
            commands::save_gc_config,
            commands::run_workspace_gc,
            // Request Logs & Usage Stats (New API/Sub2API inspired)
            commands::get_request_logs,
            commands::get_usage_stats,
            commands::cleanup_request_logs,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(move |app_handle, event| {
        match event {
            tauri::RunEvent::WindowEvent { label, event: win_event, .. } => {
                // When the main window is closed, also close the status-dock and exit
                if label == "main" {
                    if let tauri::WindowEvent::CloseRequested { .. } = win_event {
                        // Close the status-dock and quick-assistant windows if they exist
                        if let Some(dock) = app_handle.get_webview_window("status-dock") {
                            let _ = dock.close();
                        }
                        if let Some(qa) = app_handle.get_webview_window("quick-assistant") {
                            let _ = qa.close();
                        }
                    }
                }
            },
            tauri::RunEvent::Exit => {
                let state: tauri::State<'_, std::sync::Mutex<proxy::ProxyServer>> = app_handle.state();
                let mut server = state.lock().expect("Failed to lock proxy server mutex");
                server.stop();
            },
            _ => {}
        }
    });
}
