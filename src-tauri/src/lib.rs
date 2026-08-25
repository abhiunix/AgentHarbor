mod commands;
mod adapters;
mod analytics;
mod models;
mod registry;
mod tray;
mod utils;

use tauri::Manager;

use commands::registry::{
    get_all_agents, get_all_capabilities, get_capabilities_by_type, search_agents,
    search_capabilities,
};
use commands::agents::{save_agent, delete_agent, preview_import_agents, import_agents_from_dir};
use commands::project::{
    select_project_folder, detect_adapters, get_recent_projects, add_recent_project,
    remove_recent_project, get_global_config, open_in_app,
};
use commands::deploy::{preview_deploy, execute_deploy};
use commands::presets::{
    get_presets, save_preset, update_preset, delete_preset,
    add_capability_to_preset, remove_capability_from_preset,
};
use commands::config::{
    get_settings, update_settings, update_general_settings,
    update_registry_settings, update_deploy_settings, update_analytics_settings,
    get_username, get_author_id,
    get_claude_code_settings, apply_claude_code_provider,
};
use commands::backup::{
    create_project_backup, restore_project_backup, get_project_backups,
    cleanup_backups, delete_project_backup, run_backup_cleanup_on_launch,
};
use commands::sync::{
    sync_registry_now, get_sync_status, start_registry_polling,
    stop_registry_polling, is_registry_polling_active, get_community_registry_dir,
};
use commands::custom::{
    save_custom_capability, delete_custom_capability, get_custom_capabilities_dir,
    list_custom_capabilities,
};
use commands::projects::{
    get_all_projects, get_project_detail, add_project, remove_project,
    record_deployment, open_project_in_finder, open_project_in_cursor,
    open_project_in_vscode, open_project_in_terminal, start_claude_in_project,
    start_kimi_in_project, get_project_installed_items, remove_project_item,
};
use commands::importexport::{
    export_data, import_data, validate_import_data,
};
use commands::drift::{
    detect_drift, accept_drift, restore_drift, get_drift_diff,
};
use commands::memory::{
    list_agent_memory, list_global_agent_memory, clear_agent_memory,
    clear_all_agent_memory, clear_all_global_memory,
    read_project_memory, write_project_memory,
    list_memory_files, read_memory_file, write_memory_file,
    create_memory_file, delete_memory_file,
};
use commands::secrets::{
    store_secret, get_secret, delete_secret, list_secrets, get_secrets_count,
};
use commands::global_config::{
    read_claude_settings, write_claude_settings, read_claude_memory, write_claude_memory,
    read_claude_desktop_config, write_claude_desktop_config, get_claude_desktop_config_path,
    read_global_config_raw, write_global_config_raw, add_global_mcp_server, remove_global_mcp_server,
    discover_capabilities, test_ollama_connection, launch_claude_via_ollama,
};
use commands::discovery::{discover_plugins, discover_skills};
use commands::usage::{read_project_usage_files, list_projects_with_mcp};
use commands::session_stats::get_claude_session_stats;
use commands::prompt_history::{get_prompt_history, search_prompt_history, get_prompt_stats, build_resume_command, start_claude_session};
use commands::transcripts::{
    delete_transcript_backup, list_transcript_sessions, prune_transcript_backups, read_transcript,
    open_in_text_editor, read_transcript_raw, replace_in_transcript, save_transcript_raw,
    search_transcript_matches, search_transcripts, update_transcript_message,
};
use commands::extensions::{
    list_claude_plugins, list_cursor_extensions, list_cursor_plugins, toggle_claude_plugin,
};
use commands::plans::{
    list_plans, list_project_plans, read_plan, delete_plan_file,
    list_hidden_debate_plans, hide_plan_from_debate,
    unhide_plan_from_debate, clear_hidden_debate_plans,
    list_todos, get_todo_stats,
};
use commands::permissions::{
    get_claude_permissions, update_claude_permissions,
    get_claude_project_permissions, update_claude_project_permissions,
    get_cursor_permissions, update_cursor_permissions,
};
use commands::ai_tracking::{
    get_ai_commit_scores, get_ai_code_summary, get_conversation_summaries,
    get_ai_file_type_breakdown, get_ai_tracking_model_breakdown,
};
use commands::notes::{
    list_notes_entries, read_note_content, write_note_content,
    create_notes_folder, create_notes_file, rename_notes_entry, delete_notes_entry,
    move_notes_entry,
};
use commands::cursor_rules::{
    list_cursor_rules, read_cursor_rule, write_cursor_rule, delete_cursor_rule,
    list_global_cursor_rules,
};
use commands::cursor_hooks::{
    list_cursor_hooks, save_cursor_hooks, add_cursor_hook, remove_cursor_hook,
};
use commands::windsurf_rules::{
    list_windsurf_rules, read_windsurf_rules_file, write_windsurf_rules_file,
    delete_windsurf_rule, read_windsurf_legacy_rules, write_windsurf_legacy_rules,
};
use commands::test_bridge::{test_bridge_read_cmd, test_bridge_write_result};
use commands::github_fetch::{fetch_github_skill, get_official_skills_index, get_openclaw_skills_index, search_openclaw_skills};
use commands::mcp_discovery::{discover_mcp_tools, discover_mcp_tools_http};
use commands::mcp_registry::{search_mcp_registry, get_mcp_registry_popular};
use commands::gemini::{
    read_gemini_settings, write_gemini_settings, get_gemini_mcp_servers,
    add_gemini_mcp_server, remove_gemini_mcp_server,
    read_gemini_memory, write_gemini_memory,
    read_gemini_project_memory, write_gemini_project_memory,
    get_gemini_hooks, list_gemini_skills, list_gemini_agents,
    read_gemini_agent, list_gemini_extensions,
};
use commands::codex::{
    list_codex_skills, read_codex_skill_file, read_codex_config, write_codex_config,
};
use commands::debate::{
    start_debate, cancel_debate, replace_plan_file, save_refined_plan, check_debate_credentials,
    discover_project_plans,
};
use commands::debate_history::{list_debates, get_debate, delete_debate};
use commands::model_routing::analyze_model_routing;
use analytics::commands::{
    get_all_provider_status, get_provider_analytics, get_all_provider_analytics,
    save_provider_token, delete_provider_token, has_provider_token,
    get_all_provider_info, copilot_start_device_flow, copilot_poll_device_flow,
    get_tray_summary, update_tray_tooltip, set_tray_active_provider,
};
use commands::claude_history::{get_claude_history, get_claude_active_sessions};
use commands::claude_metadata::{
    get_claude_app_info, get_claude_installed_plugins, get_claude_settings_info,
    get_claude_custom_commands, get_claude_todos_summary, get_claude_plans_summary,
    get_claude_hooks_summary, get_claude_file_history_stats,
};
use analytics::claude::{
    claude_check_silent_credentials, claude_import_from_keychain,
    claude_store_manual_token, claude_disconnect,
    claude_start_oauth, claude_exchange_oauth_code,
};
use analytics::claude_v2::{
    get_claude_v2_overview, get_claude_v2_token_timeseries, get_claude_v2_model_timeseries,
    get_claude_v2_message_log, get_claude_v2_prompt_history, export_claude_v2_csv,
};
use analytics::kimi_v2::{
    get_kimi_v2_overview, get_kimi_v2_prompt_history, get_kimi_v2_connection_status,
};
use analytics::kimi_prompts::{
    get_kimi_prompt_history, search_kimi_prompt_history, get_kimi_prompt_stats,
    build_kimi_resume_command, start_kimi_session,
};
use analytics::kimi_transcripts::{
    list_kimi_transcript_sessions, read_kimi_transcript, search_kimi_transcripts,
};
use analytics::kimi_plans::{
    list_kimi_plans, read_kimi_plan, list_kimi_todo_groups,
};
use analytics::kimi_control::{
    get_kimi_control_settings, set_kimi_default_model, set_kimi_control_flag,
    get_kimi_config_tunables, set_kimi_config_value,
};
use analytics::cursor_v2::{
    get_cursor_v2_connection_status, get_cursor_v2_overview, get_cursor_v2_usage_events,
    get_cursor_v2_ai_commits, get_cursor_v2_model_usage, get_cursor_v2_composer_tabs,
    get_cursor_v2_team_spend, get_cursor_v2_request_breakdown, disconnect_cursor_v2,
    set_cursor_v2_cache_ttl, get_cursor_v2_cache_info,
};

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}


/// `tauri dev` runs the bare binary outside an .app bundle, so macOS falls back
/// to the generic exec icon in the Dock. Set it from the bundled PNG at runtime.
/// Release builds read the icon from the bundle and must not carry this.
#[cfg(all(debug_assertions, target_os = "macos"))]
fn set_dev_dock_icon() {
    use objc2::{AnyThread, MainThreadMarker};
    use objc2_app_kit::{NSApplication, NSImage};
    use objc2_foundation::NSData;

    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let data = NSData::with_bytes(include_bytes!("../icons/128x128@2x.png"));
    if let Some(img) = NSImage::initWithData(NSImage::alloc(), &data) {
        unsafe { NSApplication::sharedApplication(mtm).setApplicationIconImage(Some(&img)) };
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            #[cfg(all(debug_assertions, target_os = "macos"))]
            set_dev_dock_icon();

            // The tray is always present; the main window detaches from it on close.
            let _ = tray::setup_tray(app.handle(), true);
            
            let handle = app.handle().clone();
            let main_window = app.get_webview_window("main");
            if let Some(window) = main_window {
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        // Probe the real tray state: if tray setup ever failed there is
                        // nothing left to reopen from, so let the close become a quit.
                        if handle.tray_by_id("main-tray").is_some() {
                            api.prevent_close();
                            if let Some(w) = handle.get_webview_window("main") {
                                let _ = w.hide();
                            }
                            // Leave the Dock/Cmd-Tab so a detached window reads as closed.
                            #[cfg(target_os = "macos")]
                            let _ = handle.set_activation_policy(tauri::ActivationPolicy::Accessory);
                        }
                    }
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            get_all_capabilities,
            get_all_agents,
            get_capabilities_by_type,
            search_capabilities,
            search_agents,
            save_agent,
            delete_agent,
            preview_import_agents,
            import_agents_from_dir,
            select_project_folder,
            detect_adapters,
            get_recent_projects,
            add_recent_project,
            remove_recent_project,
            open_in_app,
            preview_deploy,
            execute_deploy,
            get_presets,
            save_preset,
            update_preset,
            delete_preset,
            add_capability_to_preset,
            remove_capability_from_preset,
            get_settings,
            update_settings,
            update_general_settings,
            update_registry_settings,
            update_deploy_settings,
            update_analytics_settings,
            get_username,
            get_author_id,
            get_global_config,
            create_project_backup,
            restore_project_backup,
            get_project_backups,
            cleanup_backups,
            delete_project_backup,
            run_backup_cleanup_on_launch,
            sync_registry_now,
            get_sync_status,
            start_registry_polling,
            stop_registry_polling,
            is_registry_polling_active,
            get_community_registry_dir,
            save_custom_capability,
            delete_custom_capability,
            get_custom_capabilities_dir,
            list_custom_capabilities,
            get_all_projects,
            get_project_detail,
            add_project,
            remove_project,
            record_deployment,
            open_project_in_finder,
            open_project_in_cursor,
            open_project_in_vscode,
            open_project_in_terminal,
            start_claude_in_project,
            start_kimi_in_project,
            get_project_installed_items,
            remove_project_item,
            export_data,
            import_data,
            validate_import_data,
            detect_drift,
            accept_drift,
            restore_drift,
            get_drift_diff,
            list_agent_memory,
            list_global_agent_memory,
            clear_agent_memory,
            clear_all_agent_memory,
            clear_all_global_memory,
            read_project_memory,
            write_project_memory,
            list_memory_files,
            read_memory_file,
            write_memory_file,
            create_memory_file,
            delete_memory_file,
            store_secret,
            get_secret,
            delete_secret,
            list_secrets,
            get_secrets_count,
            read_claude_settings,
            write_claude_settings,
            get_claude_code_settings,
            apply_claude_code_provider,
            test_ollama_connection,
            launch_claude_via_ollama,
            read_claude_memory,
            write_claude_memory,
            read_claude_desktop_config,
            write_claude_desktop_config,
            get_claude_desktop_config_path,
            read_global_config_raw,
            write_global_config_raw,
            add_global_mcp_server,
            remove_global_mcp_server,
            discover_capabilities,
            discover_skills,
            discover_plugins,
            read_project_usage_files,
            list_projects_with_mcp,
            list_notes_entries,
            read_note_content,
            write_note_content,
            create_notes_folder,
            create_notes_file,
            rename_notes_entry,
            delete_notes_entry,
            move_notes_entry,
            get_ai_commit_scores,
            get_ai_code_summary,
            get_conversation_summaries,
            get_ai_file_type_breakdown,
            get_ai_tracking_model_breakdown,
            get_claude_session_stats,
            get_prompt_history,
            search_prompt_history,
            get_prompt_stats,
            build_resume_command,
            start_claude_session,
            list_transcript_sessions,
            read_transcript,
            search_transcripts,
            read_transcript_raw,
            search_transcript_matches,
            replace_in_transcript,
            save_transcript_raw,
            update_transcript_message,
            open_in_text_editor,
            delete_transcript_backup,
            prune_transcript_backups,
            list_claude_plugins,
            list_cursor_extensions,
            list_cursor_plugins,
            toggle_claude_plugin,
            list_plans,
            list_project_plans,
            read_plan,
            delete_plan_file,
            list_hidden_debate_plans,
            hide_plan_from_debate,
            unhide_plan_from_debate,
            clear_hidden_debate_plans,
            list_todos,
            get_todo_stats,
            get_claude_permissions,
            update_claude_permissions,
            get_claude_project_permissions,
            update_claude_project_permissions,
            get_cursor_permissions,
            update_cursor_permissions,
            // Cursor rules
            list_cursor_rules,
            read_cursor_rule,
            write_cursor_rule,
            delete_cursor_rule,
            list_global_cursor_rules,
            // Cursor hooks
            list_cursor_hooks,
            save_cursor_hooks,
            add_cursor_hook,
            remove_cursor_hook,
            // Windsurf rules
            list_windsurf_rules,
            read_windsurf_rules_file,
            write_windsurf_rules_file,
            delete_windsurf_rule,
            read_windsurf_legacy_rules,
            write_windsurf_legacy_rules,
            // Gemini
            read_gemini_settings,
            write_gemini_settings,
            get_gemini_mcp_servers,
            add_gemini_mcp_server,
            remove_gemini_mcp_server,
            read_gemini_memory,
            write_gemini_memory,
            read_gemini_project_memory,
            write_gemini_project_memory,
            get_gemini_hooks,
            list_gemini_skills,
            list_gemini_agents,
            read_gemini_agent,
            list_gemini_extensions,
            // Codex
            list_codex_skills,
            read_codex_skill_file,
            read_codex_config,
            write_codex_config,
            // Debate
            start_debate,
            cancel_debate,
            replace_plan_file,
            save_refined_plan,
            check_debate_credentials,
            discover_project_plans,
            // Debate history
            list_debates,
            get_debate,
            delete_debate,
            // Test bridge
            test_bridge_read_cmd,
            test_bridge_write_result,
            // GitHub skill fetch
            fetch_github_skill,
            get_official_skills_index,
            // OpenClaw skill registry
            get_openclaw_skills_index,
            search_openclaw_skills,
            // MCP tool discovery
            discover_mcp_tools,
            discover_mcp_tools_http,
            // MCP Registry
            search_mcp_registry,
            get_mcp_registry_popular,
            // Analytics
            get_all_provider_status,
            get_provider_analytics,
            get_all_provider_analytics,
            save_provider_token,
            delete_provider_token,
            has_provider_token,
            get_all_provider_info,
            copilot_start_device_flow,
            copilot_poll_device_flow,
            // Tray popover
            get_tray_summary,
            update_tray_tooltip,
            set_tray_active_provider,
            crate::analytics::commands::refresh_tray_data,
            crate::analytics::commands::force_refresh_provider,
            crate::tray::show_main_window,
            crate::tray::quit_app,
            // Claude Code Auth Flow
            claude_check_silent_credentials,
            claude_import_from_keychain,
            claude_store_manual_token,
            claude_disconnect,
            claude_start_oauth,
            claude_exchange_oauth_code,
            // Claude Code Analytics V2
            get_claude_v2_overview,
            get_claude_v2_token_timeseries,
            get_claude_v2_model_timeseries,
            get_claude_v2_message_log,
            get_claude_v2_prompt_history,
            export_claude_v2_csv,
            // Kimi Code Analytics V2 (local)
            get_kimi_v2_overview,
            get_kimi_v2_prompt_history,
            get_kimi_v2_connection_status,
            // Kimi Prompt History (own section)
            get_kimi_prompt_history,
            search_kimi_prompt_history,
            get_kimi_prompt_stats,
            build_kimi_resume_command,
            start_kimi_session,
            // Kimi Code Transcripts (read-only)
            list_kimi_transcript_sessions,
            read_kimi_transcript,
            search_kimi_transcripts,
            // Kimi Code Plans & Todos (read-only)
            list_kimi_plans,
            read_kimi_plan,
            list_kimi_todo_groups,
            // Kimi Permissions & Control
            get_kimi_control_settings,
            set_kimi_default_model,
            set_kimi_control_flag,
            get_kimi_config_tunables,
            set_kimi_config_value,
            get_claude_history,
            get_claude_active_sessions,
            get_claude_app_info,
            get_claude_installed_plugins,
            get_claude_settings_info,
            get_claude_custom_commands,
            get_claude_todos_summary,
            get_claude_plans_summary,
            get_claude_hooks_summary,
            get_claude_file_history_stats,
            // Token Optimizer
            analyze_model_routing,
            // Cursor Analytics V2
            get_cursor_v2_connection_status,
            get_cursor_v2_overview,
            get_cursor_v2_usage_events,
            get_cursor_v2_ai_commits,
            get_cursor_v2_model_usage,
            get_cursor_v2_composer_tabs,
            get_cursor_v2_team_spend,
            get_cursor_v2_request_breakdown,
            disconnect_cursor_v2,
            set_cursor_v2_cache_ttl,
            get_cursor_v2_cache_info,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
