mod agent;
mod agent_templates;
mod backup;
mod circuit_breaker;
mod code_graph;
mod commands;
mod crypto;
mod db;
mod db_schema;
mod event_bus;
mod hash;
mod input_validation;
mod knowledge;
mod local_models;
mod media;
mod model_knowledge;
mod oauth;
mod pptx;
mod proc;
mod prompt_guard;
mod proxy;
mod proxy_types;
mod responses_bridge;
mod runtime;
mod runtime_acp;
mod runtime_manager;
mod selection;
mod skill_dag;
mod skill_frontmatter;
mod skill_library;
mod slides;
mod storage;
mod sync_engine;
mod token_economy;
mod tool_adapters;

#[cfg(test)]
mod tests;

use std::sync::Arc;
use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::GlobalShortcutExt;

fn fitted_main_window_size(logical_width: f64, logical_height: f64) -> (f64, f64, f64, f64) {
    let width = (logical_width * 0.92).min(1280.0);
    let height = (logical_height * 0.88).min(800.0);
    let min_width = width.min(640.0);
    let min_height = height.min(520.0);
    (width, height, min_width, min_height)
}

fn webview_zoom_correction(reported_width: u32, native_width: u32) -> Option<f64> {
    if reported_width == 0 || native_width == 0 {
        return None;
    }
    let ratio = native_width as f64 / reported_width as f64;
    (ratio < 0.8).then(|| ratio.clamp(0.5, 1.0))
}

#[cfg(windows)]
#[repr(C)]
struct NativeRect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[cfg(windows)]
#[link(name = "user32")]
extern "system" {
    fn GetClientRect(window: isize, rect: *mut NativeRect) -> i32;
}

#[cfg(windows)]
fn native_client_width<R: tauri::Runtime>(window: &tauri::WebviewWindow<R>) -> Option<u32> {
    let hwnd = window.hwnd().ok()?;
    let mut rect = NativeRect {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    let success = unsafe { GetClientRect(hwnd.0 as isize, &mut rect) };
    (success != 0 && rect.right > rect.left).then_some((rect.right - rect.left) as u32)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 0. Initialize logging
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn"))
        .format_timestamp_secs()
        .try_init();

    // 1. Initialize SQLite Database Schema
    let db = Arc::new(db::DbManager::new());
    // Load user-configured storage locations (backups/exports/skills store).
    storage::init_from_db(&db);

    // 2. Initialize Agent Subprocess watchdog Manager
    let agent_manager = Arc::new(agent::AgentManager::new(Arc::clone(&db)));
    let runtime_manager = Arc::new(runtime_manager::RuntimeManager::new(Arc::clone(&db)));

    // 3. Resolve port and launch background HTTP translation proxy
    // The proxy port is an internal constant — users configure API keys/addresses
    // per-platform in the Model Hub, not here.
    let port: u16 = 1421;

    let proxy_server = proxy::ProxyServer::new();
    let proxy_state = std::sync::Mutex::new(proxy_server);

    // Parse QA shortcut for toggle handler.
    // Selection shortcut removed — auto-capture is the only trigger mode now.
    let qa_shortcut_str = db
        .get_setting("quick_assistant_shortcut")
        .unwrap_or(None)
        .unwrap_or_else(|| "Ctrl+Shift+Space".to_string());

    let qa_id: Option<u32> = qa_shortcut_str
        .parse::<tauri_plugin_global_shortcut::Shortcut>()
        .map(|s| s.id())
        .ok();

    if qa_id.is_none() {
        log::warn!("Failed to parse QA shortcut '{}'", qa_shortcut_str);
    }

    let qa_str_for_register = qa_shortcut_str.clone();

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        // Register the global-shortcut plugin with handler only — NO with_shortcuts().
        // Shortcut registration is deferred to after the event loop starts,
        // so that a conflict (e.g., Alt+Space already taken) doesn't crash the app.
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, shortcut, event| {
                    use tauri_plugin_global_shortcut::ShortcutState;
                    if event.state == ShortcutState::Pressed {
                        if Some(shortcut.id()) == qa_id {
                            // Quick Assistant toggle
                            use tauri::Manager;
                            if let Some(qa) = app.get_webview_window("quick-assistant") {
                                if qa.is_visible().unwrap_or(false) {
                                    let _ = qa.hide();
                                } else {
                                    let _ = qa.show();
                                    let _ = qa.set_focus();
                                }
                            }
                        }
                    }
                })
                .build(),
        )
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(move |app| {
            // Start the proxy server inside the Tokio runtime context
            if let Ok(mut server) = proxy_state.lock() {
                server.start(Arc::clone(&db), Arc::clone(&agent_manager), Arc::clone(&runtime_manager), port);
            }

            // Share references globally as Tauri App State
            app.manage(Arc::clone(&db));
            app.manage(Arc::clone(&agent_manager));
            app.manage(Arc::clone(&runtime_manager));
            app.manage(proxy_state);

            let mut runtime_events = runtime_manager.subscribe();
            let runtime_app = app.handle().clone();
            let hooks_db = Arc::clone(&db);
            tauri::async_runtime::spawn(async move {
                while let Ok(envelope) = runtime_events.recv().await {
                    // Fire user-state hooks before forwarding to the UI. The
                    // engine drops its DB guard before any action/spawn, so it
                    // never holds a lock across the loop's await.
                    commands::evaluate_hooks(&hooks_db, &runtime_app, &envelope);
                    // Auto-record key events (errors/approvals) into the project
                    // protocol for workspaces that have it enabled.
                    commands::protocol_auto_record(&hooks_db, &envelope);
                    let _ = runtime_app.emit("agent-session-event", envelope);
                }
            });

            // Start background tasks once the Tokio runtime is fully initialized by Tauri
            agent_manager.start_services();

            // Poll pending video-generation tasks (Agnes AI etc.) and push
            // progress to the Studio via `media-task-update` events.
            commands::start_media_poller(app.handle().clone(), Arc::clone(&db));

            // OAuth auth center: refresh subscription tokens before they expire.
            commands::start_oauth_refresher(app.handle().clone());

            // Fit the initial logical size to the current monitor. Windows can otherwise
            // clamp a 1280x800 window at high DPI while WebView keeps the oversized layout.
            if let Some(main) = app.get_webview_window("main") {
                if let Ok(Some(monitor)) = main.current_monitor() {
                    let scale = monitor.scale_factor();
                    let monitor_size = monitor.size();
                    let logical_width = monitor_size.width as f64 / scale;
                    let logical_height = monitor_size.height as f64 / scale;
                    let (width, height, min_width, min_height) =
                        fitted_main_window_size(logical_width, logical_height);
                    let _ = main.set_min_size(Some(tauri::LogicalSize::new(min_width, min_height)));
                    let _ = main.set_size(tauri::LogicalSize::new(width, height));
                    let _ = main.center();
                } else {
                    let _ = main.set_min_size(Some(tauri::LogicalSize::new(640.0, 520.0)));
                }

                #[cfg(windows)]
                if let (Ok(reported_size), Some(native_width)) =
                    (main.inner_size(), native_client_width(&main))
                {
                    if let Some(zoom) =
                        webview_zoom_correction(reported_size.width, native_width)
                    {
                        if let Err(error) = main.set_zoom(zoom) {
                            log::warn!("Unable to correct WebView DPI mapping: {error}");
                        } else {
                            log::info!(
                                "Corrected WebView DPI mapping: reported={} native={} zoom={:.2}",
                                reported_size.width,
                                native_width,
                                zoom
                            );
                        }
                    }
                }
            }

            // Register global shortcuts after the event loop starts.
            // We do NOT use with_shortcuts() in the plugin builder because if ANY
            // shortcut conflicts (e.g., Alt+Space taken by another app), the entire
            // plugin init fails and crashes the app. Instead, register each one
            // individually here with graceful error handling.
            {
                let app_handle = app.handle().clone();
                let qa_str = qa_str_for_register.clone();
                tauri::async_runtime::spawn(async move {
                    // Wait for the event loop to start so run_main_thread! can dispatch
                    tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

                    let gs = app_handle.global_shortcut();

                    if let Err(e) = gs.register(qa_str.as_str()) {
                        log::warn!(
                            "[Shortcut] Failed to register QA shortcut '{}': {}",
                            qa_str,
                            e
                        );
                    } else {
                        log::warn!("[Shortcut] Registered QA shortcut: {}", qa_str);
                    }
                });
            }

            // Initialize OMNIX Status Dock floating window
            let _status_dock = tauri::WebviewWindowBuilder::new(
                app,
                "status-dock",
                tauri::WebviewUrl::App("/?window=status-dock".into()),
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
                tauri::WebviewUrl::App("/?window=quick-assistant".into()),
            )
            .title("OMNIX Quick Assistant")
            .inner_size(420.0, 520.0)
            .min_inner_size(300.0, 180.0)
            .decorations(false)
            .transparent(true)
            .always_on_top(true)
            .resizable(true)
            .skip_taskbar(true)
            .visible(false)
            .build();

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_app_setting,
            commands::set_app_setting,
            commands::create_workspace_run,
            commands::list_workspace_runs,
            commands::get_workspace_run,
            commands::propose_team_plan,
            commands::get_team_plan,
            commands::approve_team_plan,
            commands::start_agent_run,
            commands::list_agent_runs,
            commands::team_generate_plan,
            commands::team_get_run_detail,
            commands::team_start_approved_run,
            commands::team_stop_run,
            commands::team_retry_worker,
            commands::team_respond_worker_approval,
            commands::list_lab_features,
            commands::protocol_get_status,
            commands::protocol_preview_init,
            commands::protocol_init_workspace,
            commands::protocol_set_enabled,
            commands::protocol_remove_workspace,
            commands::protocol_record_event,
            commands::protocol_archive_and_distill,
            commands::protocol_list_runs,
            commands::protocol_list_events,
            commands::protocol_list_actions,
            commands::protocol_apply_action,
            commands::protocol_list_evolution_proposals,
            commands::protocol_apply_evolution_proposal,
            commands::detect_installed_agents,
            commands::runtime_get_agent_catalog,
            commands::runtime_get_model_options,
            commands::runtime_get_agent_model_preference,
            commands::runtime_set_agent_model_preference,
            commands::runtime_start_session,
            commands::runtime_send_message,
            commands::runtime_respond_approval,
            commands::runtime_set_session_model,
            commands::runtime_stop_session,
            commands::runtime_resume_session,
            commands::runtime_get_session,
            commands::runtime_get_events,
            commands::runtime_list_conversation_sessions,
            commands::start_agent_session,
            commands::send_agent_stdin,
            commands::stop_agent_session,
            commands::install_agent_cli,
            commands::check_agent_updates,
            commands::get_profile_stats,
            commands::oauth_start,
            commands::oauth_complete,
            commands::oauth_list_accounts,
            commands::oauth_delete_account,
            commands::oauth_refresh_account,
            commands::cli_takeover_apply,
            commands::cli_takeover_revert,
            commands::cli_takeover_status,
            commands::detect_hardware,
            commands::recommend_local_models,
            commands::media_generate_image,
            commands::media_create_video_task,
            commands::media_list_tasks,
            commands::media_delete_task,
            commands::media_read_file,
            commands::media_read_attachment,
            commands::media_model_suggestions,
            commands::uninstall_agent_cli,
            commands::repair_installed_agent,
            commands::sync_external_agent_configs,
            commands::get_all_skills,
            commands::get_skill_content,
            commands::save_skill_content,
            commands::toggle_skill_active,
            commands::update_skill_profile,
            commands::create_skill,
            commands::get_active_agent_model,
            commands::update_active_agent_model,
            commands::get_agent_accounts,
            commands::save_agent_account,
            commands::switch_agent_account,
            commands::delete_agent_account,
            commands::list_agent_upstream_accounts,
            commands::set_active_upstream_account,
            commands::get_active_upstream_account,
            commands::get_all_memories,
            commands::create_memory,
            commands::delete_memory,
            commands::distill_conversation_to_inbox,
            commands::distill_workspace_to_inbox,
            commands::list_distillation_inbox,
            commands::review_distillation_candidate,
            // Evolution loop — preview what gets auto-injected back to agents
            commands::get_lessons_preview,
            commands::reindex_memory_embeddings,
            commands::refresh_workspace_profile,
            commands::consolidate_memories,
            commands::get_all_conversations,
            commands::get_conversation_tasks,
            commands::get_mailbox_messages,
            commands::get_remote_access_info,
            commands::set_remote_access,
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
            commands::pick_directory,
            commands::pick_file,
            commands::toggle_status_dock,
            commands::get_model_platforms,
            commands::save_model_platform,
            commands::delete_model_platform,
            commands::get_platform_models,
            commands::save_platform_model,
            commands::delete_platform_model,
            commands::get_active_models,
            commands::get_available_models,
            commands::fetch_remote_models,
            commands::check_model_status,
            commands::batch_check_models,
            commands::reinfer_model_capabilities,
            // Multi-Key API Key Management
            commands::add_platform_api_key,
            commands::list_platform_api_keys,
            commands::select_platform_api_key,
            commands::delete_platform_api_key,
            commands::reveal_platform_api_key,
            commands::get_conversation_messages,
            commands::create_conversation,
            commands::get_conversation_goal,
            commands::set_conversation_goal,
            commands::set_conversation_goal_status,
            commands::clear_conversation_goal,
            commands::sdd_reserve_plan_path,
            commands::sdd_write_plan,
            commands::sdd_list_plans,
            commands::sdd_read_plan,
            commands::sdd_toggle_plan_todo,
            commands::sdd_clarify_prompt,
            commands::sdd_plan_prompt,
            commands::autopilot_list,
            commands::autopilot_create,
            commands::autopilot_update,
            commands::autopilot_set_enabled,
            commands::autopilot_delete,
            commands::autopilot_run_now,
            commands::autopilot_take_queued_runs,
            commands::autopilot_mark_run,
            commands::autopilot_list_runs,
            commands::write_list_spaces,
            commands::write_add_space,
            commands::write_remove_space,
            commands::write_list_files,
            commands::write_read_file,
            commands::write_save_file,
            commands::write_create_file,
            commands::write_rename_file,
            commands::write_delete_file,
            commands::write_export_html,
            commands::add_conversation_message,
            commands::delete_conversation,
            commands::get_archived_conversations,
            commands::archive_conversation,
            commands::unarchive_conversation,
            commands::get_previewable_files,
            commands::read_file_content_utf8,
            commands::read_file_as_base64,
            commands::get_workspace_git_diff,
            commands::get_workspace_snapshot,
            commands::read_workspace_file,
            commands::run_env_diagnostics,
            commands::repair_env_tool,
            commands::kb_list_bases,
            commands::kb_create_base,
            commands::kb_update_base,
            commands::kb_delete_base,
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
            // Agent Template commands
            commands::get_agent_templates,
            commands::get_agent_template,
            // Skills Lock File commands
            commands::get_skill_lock,
            commands::update_skill_lock,
            commands::verify_skill_lock,
            // Agent Execution Environment commands
            commands::get_agent_exec_config,
            commands::save_agent_exec_config,
            // Autopilot commands
            commands::get_autopilot_config,
            commands::save_autopilot_config,
            // Workspace GC commands
            commands::get_gc_config,
            commands::save_gc_config,
            commands::run_workspace_gc,
            // Request Logs & Usage Stats
            commands::get_request_logs,
            commands::get_usage_stats,
            commands::get_platform_usage,
            commands::get_usage_timeseries,
            commands::cleanup_request_logs,
            // Platform Health Management
            commands::get_platform_health,
            commands::reset_platform_health,
            commands::update_platform_routing,
            // Upstream Model Auto-Sync
            commands::sync_upstream_models,
            commands::apply_model_sync,
            commands::sync_all_upstream_models,
            // Platform Health Check
            commands::check_all_platform_health,
            // Agent Task Lifecycle
            commands::get_task_list,
            commands::task_start,
            commands::task_complete,
            commands::task_fail,
            commands::task_archive,
            commands::get_task_stats,
            // Skill Compound Interest
            commands::record_skill_usage,
            commands::get_top_skills_by_usage,
            // Autopilot Enhancement
            commands::save_autopilot_result_to_kb,
            // Security & safety features
            commands::wrap_untrusted_content,
            commands::scan_prompt_injection,
            // Selection Auto-Capture
            commands::toggle_selection_auto_capture,
            commands::checklist_add,
            commands::checklist_update,
            commands::checklist_get,
            commands::checklist_summary,
            commands::estimate_tokens,
            commands::get_context_budget,
            commands::run_skill_audit,
            commands::register_event_trigger,
            commands::get_event_triggers,
            commands::encrypt_value,
            commands::decrypt_value,
            commands::send_desktop_notification,
            commands::send_ntfy_notification,
            commands::compact_conversation_context,
            commands::get_model_recommendations,
            commands::get_model_database,
            commands::recommend_for_gpu,
            commands::get_gpu_database,
            commands::analyze_codebase,
            // Config Backup
            commands::backup_config_file,
            commands::list_backups,
            commands::restore_backup,
            // API Provider Preset
            commands::apply_api_preset,
            // MCP Presets
            commands::get_mcp_presets,
            commands::apply_mcp_preset,
            // MCP sync to Agent native config
            commands::mcp_sync_to_agents,
            commands::mcp_remove_from_agent,
            commands::mcp_import_from_agent,
            commands::mcp_get_agent_states,
            // Workspace checkpoints + diff review
            commands::create_checkpoint,
            commands::list_checkpoints,
            commands::get_workspace_diff,
            commands::restore_checkpoint,
            commands::revert_file,
            // Parallel sessions via Git worktrees
            commands::create_worktree,
            commands::list_worktrees,
            commands::remove_worktree,
            commands::merge_worktree,
            // User-state hooks: event → action rules
            commands::list_hooks,
            commands::save_hook,
            commands::toggle_hook,
            commands::delete_hook,
            commands::test_hook,
            commands::get_hook_runs,
            commands::clear_hook_runs,
            // In-session background tasks / sub-agents (own worktree, concurrent session)
            commands::create_subagent,
            commands::list_subagents,
            commands::update_subagent_status,
            commands::delete_subagent,
            // Custom Quick Assistant actions (划词助手深挖)
            commands::list_quick_actions,
            commands::save_quick_action,
            commands::delete_quick_action,
            // Notes (笔记)
            commands::list_notes,
            commands::save_note,
            commands::delete_note,
            commands::get_notes_dir,
            commands::open_notes_folder,
            // Custom assistants (助手库: 自定义 + 分享)
            commands::list_custom_assistants,
            commands::save_custom_assistant,
            commands::delete_custom_assistant,
            // Knowledge-base portability (export / import)
            commands::kb_export_base,
            commands::kb_import_base,
            // Output Styles
            commands::get_output_styles,
            commands::get_output_style_prompt,
            // Architecture Graph
            commands::build_architecture_graph,
            commands::save_architecture_graph,
            commands::load_architecture_graph,
            commands::get_ignore_patterns,
            // Skill Library Features
            commands::match_skills_for_injection,
            commands::test_skill_sandbox,
            commands::intercept_protocols,
            commands::execute_protocol,
            commands::search_skill_market,
            commands::preview_market_skill,
            commands::import_market_skill,
            commands::distill_from_project,
            // Session control features
            commands::compress_tool_result,
            commands::push_steering_message,
            commands::get_steering_messages,
            commands::consume_steering_messages,
            commands::detect_file_change,
            // Agent-Platform Bindings
            commands::get_agent_bindings,
            commands::set_agent_binding,
            commands::remove_agent_binding,
            commands::toggle_agent_binding,
            // Circuit Breaker & Session Usage
            commands::get_circuit_status,
            commands::reset_circuit_breaker,
            commands::get_model_pricing,
            commands::estimate_model_cost,
            // Skill DAG
            commands::search_skills_dag,
            commands::check_skill_set,
            commands::expand_skill_set,
            commands::add_skill_edge,
            commands::remove_skill_edge,
            // Async Agent Mailbox
            commands::send_mail,
            commands::get_mail,
            commands::mark_mail_read,
            // Enhanced Task Dependencies
            commands::set_task_blocks,
            commands::auto_unblock_tasks,
            // YOLO Mode
            commands::get_yolo_mode,
            commands::set_yolo_mode,
            commands::get_yolo_mode_config,
            commands::set_yolo_mode_config,
            commands::check_yolo_permission,
            // Persistent Cron
            commands::get_persistent_cron_tasks,
            commands::create_persistent_cron,
            commands::delete_persistent_cron,
            // Skill Rule Generator
            // Conversation Skills Indicator
            commands::get_conversation_skills,
            // Tool Call Confirmation Queue
            commands::queue_tool_confirmation,
            commands::resolve_tool_confirmation,
            commands::get_pending_confirmations,
            commands::get_pending_confirmation_count,
            // PPT / Presentation panel (结构化幻灯模型 + 网关生成/编辑)
            commands::list_decks,
            commands::get_deck,
            commands::create_deck,
            commands::save_deck,
            commands::delete_deck,
            commands::render_deck,
            commands::generate_deck,
            commands::edit_deck_ai,
            commands::export_deck_html,
            commands::export_deck_pdf,
            commands::export_deck_pptx,
            commands::generate_outline,
            commands::expand_outline,
            commands::edit_slide_ai,
            commands::suggest_slide_image_prompt,
            commands::generate_slide_image,
            commands::list_brands,
            commands::save_brand,
            commands::delete_brand,
            // Skill pool governance (#3 技能池: 待定/审核/正式 + 网关直调)
            commands::list_skill_pool,
            commands::skill_pool_stats,
            commands::collect_all_skills,
            commands::cleanup_scattered_skills,
            commands::review_skill_ai,
            commands::set_skill_pool,
            commands::get_skill_pool_content,
            commands::summarize_skill_ai,
            commands::reform_skill_ai,
            commands::apply_skill_reform,
            commands::fuse_pool_skills_ai,
            commands::apply_pool_fusion,
            commands::delete_pool_skill,
            // Agent installations (R3 统一安装)
            commands::scan_agent_installations,
            commands::remove_agent_installation,
            // Remote Dev (Labs)
            commands::list_ssh_hosts,
            commands::save_ssh_host,
            commands::delete_ssh_host,
            commands::test_ssh_host,
            commands::probe_remote_hardware,
            commands::detect_remote_agents,
            commands::install_remote_agent,
            commands::test_remote_model_host,
            commands::start_remote_run,
            commands::stop_remote_run,
            // Storage locations (R1 存储位置中心)
            commands::get_storage_config,
            commands::set_storage_dir,
            commands::migrate_skills_store,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(move |app_handle, event| {
        match event {
            tauri::RunEvent::WindowEvent {
                label,
                event: win_event,
                ..
            } => {
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
            }
            tauri::RunEvent::Exit => {
                let state: tauri::State<'_, std::sync::Mutex<proxy::ProxyServer>> =
                    app_handle.state();
                let mut server = state.lock().expect("Failed to lock proxy server mutex");
                server.stop();
            }
            _ => {}
        }
    });
}

#[cfg(test)]
mod window_layout_tests {
    use super::{fitted_main_window_size, webview_zoom_correction};

    #[test]
    fn high_dpi_logical_monitor_does_not_receive_oversized_window() {
        let (width, height, min_width, min_height) = fitted_main_window_size(768.0, 432.0);
        assert!(width <= 768.0);
        assert!(height <= 432.0);
        assert!(min_width <= width);
        assert!(min_height <= height);
    }

    #[test]
    fn large_monitor_keeps_product_default_ceiling() {
        let (width, height, _, _) = fitted_main_window_size(2560.0, 1440.0);
        assert_eq!(width, 1280.0);
        assert_eq!(height, 800.0);
    }

    #[test]
    fn mismatched_native_window_receives_zoom_correction() {
        assert_eq!(webview_zoom_correction(2560, 1280), Some(0.5));
        assert_eq!(webview_zoom_correction(1920, 1280), Some(2.0 / 3.0));
    }

    #[test]
    fn matching_native_window_keeps_default_zoom() {
        assert_eq!(webview_zoom_correction(1280, 1280), None);
        assert_eq!(webview_zoom_correction(0, 1280), None);
    }
}
