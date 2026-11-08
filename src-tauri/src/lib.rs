#![cfg_attr(target_os = "macos", allow(unexpected_cfgs))]

pub mod agents;
pub mod api;
pub mod app_data;
mod app_mcp;
pub mod automation;
pub mod backup;
pub mod capture_geometry;
pub mod chat;
pub mod commands;
mod computer_control;
pub mod connectors;
pub mod desktop_pet;
pub mod dock;
#[path = "../../extensions/sdk/native.rs"]
mod extension_sdk;
mod extensions;
pub mod external_agents;
pub mod fonts;
mod html_preview;
mod html_render;
pub mod lens;
pub mod lens_commands;
#[cfg(any(target_os = "macos", test))]
mod macos_hang_watchdog;
#[cfg(target_os = "macos")]
pub mod macos_ocr;
pub mod market;
pub mod mcp;
pub mod native_tools;
pub mod notes;
pub mod offline_models;
mod opencode_free;
pub mod path_env;
pub mod plugins;
pub mod proc;
pub mod prompts;
pub mod provider_oauth;
pub mod provider_request;
pub mod rapidocr;
#[path = "../../extensions/study/native/mod.rs"]
pub mod reading;
pub mod replace_translation;
pub mod scheduled_tasks;
#[cfg(target_os = "macos")]
pub mod sck;
pub mod screenshot;
pub mod self_config;
pub mod settings;
#[cfg(not(target_os = "android"))]
pub mod shortcuts;
pub mod skills;
pub mod state;
pub mod updates;
pub mod usage;
pub mod utils;
mod voice_codex;
mod voice_live;
pub mod web_search;
#[cfg(any(test, target_os = "macos"))]
pub(crate) mod window_focus;
pub mod windows;
#[cfg(target_os = "windows")]
pub mod windows_ocr;
mod android_platform;
#[cfg(test)]
mod mobile_scope;

use std::time::Duration;

use tauri::{Emitter, Manager, State};

use api::build_http_client;
use native_tools::cleanup_stale_sandbox_exports;
use screenshot::cleanup_orphan_temp_files;
use settings::load_settings;
#[cfg(target_os = "macos")]
use shortcuts::set_macos_regular_activation_policy;
#[cfg(not(target_os = "android"))]
use shortcuts::{hide_chat_window, open_chat_window};
use state::AppState;
use updates::check_github_latest_release;
#[cfg(target_os = "macos")]
use windows::{ensure_overlay_panel, restore_previous_frontmost_app};

/// 自启动参数，用于区分用户手动启动和系统自动启动
const AUTOSTART_ARG: &str = "--from-autostart";

#[cfg(target_os = "macos")]
const USER_WINDOW_LABELS: &[&str] = &["chat", "translator"];

#[cfg(target_os = "macos")]
fn first_visible_user_window(app: &tauri::AppHandle) -> Option<tauri::WebviewWindow> {
    USER_WINDOW_LABELS
        .iter()
        .find_map(|label| {
            app.get_webview_window(label)
                .filter(|window| window.is_visible().ok().unwrap_or(false))
        })
        .or_else(|| crate::chat::popout::first_visible_popout(app))
}

/// Windows：让本进程退出 EcoQoS 执行速度节流，使其在无窗口/后台空闲时仍保持正常调度，
/// 避免全局热键的 WM_HOTKEY 消息泵被饿死。ControlMask=EXECUTION_SPEED + StateMask=0 表示
/// "由本进程接管该项节流并关闭它"。best-effort：失败静默忽略。
#[cfg(target_os = "windows")]
fn disable_process_power_throttling() {
    use ::windows::Win32::System::Threading::{
        GetCurrentProcess, ProcessPowerThrottling, SetProcessInformation,
        PROCESS_POWER_THROTTLING_CURRENT_VERSION, PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
        PROCESS_POWER_THROTTLING_STATE,
    };
    let state = PROCESS_POWER_THROTTLING_STATE {
        Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
        ControlMask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
        StateMask: 0,
    };
    unsafe {
        let _ = SetProcessInformation(
            GetCurrentProcess(),
            ProcessPowerThrottling,
            &state as *const _ as *const core::ffi::c_void,
            std::mem::size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
        );
    }
}

// Generate once: on macOS this macro embeds a process-wide Info.plist symbol.
// The identity regression test must use the same context as the application.
fn app_context() -> tauri::Context<tauri::Wry> {
    tauri::generate_context!()
}

/// 应用入口函数
/// 初始化 Tauri Builder，加载插件，配置窗口事件处理，设置全局状态、热键和托盘
#[cfg_attr(target_os = "android", tauri::mobile_entry_point)]
pub fn run() {
    let context = app_context();
    assert_eq!(
        context.config().identifier,
        app_data::APP_IDENTIFIER,
        "Tauri and headless storage identities must match"
    );

    assert!(
        !cfg!(debug_assertions) || app_data::is_dev(),
        "Debug GUI builds must use the Dev identity: run npm run dev or pass --config src-tauri/tauri.dev.conf.json"
    );

    // GUI launches don't always inherit the *current* user PATH, so packaged
    // builds can't find user-installed CLIs. Enrich the process PATH once,
    // before any window creation or CLI probing. macOS: login-shell PATH is not
    // inherited from Finder/Dock. Windows: explorer's PATH is a stale login-time
    // snapshot, so read the current value from the registry, then also run the
    // user's PowerShell profile to pick up version-manager dirs (fnm/nvm) that
    // never touch the registry. No-op on Linux. Locale is the other half of
    // the same gap (see ensure_utf8_locale below). See `path_env` module docs.
    #[cfg(target_os = "macos")]
    path_env::enrich_path_macos();
    #[cfg(target_os = "windows")]
    path_env::enrich_path_windows();
    // Same GUI-launch gap as PATH: Finder/Dock often hands us no LANG, so libc
    // stays in C and BSD `ls` prints `?` for CJK filenames. One process-wide
    // fix is inherited by the dock PTY, run_command, MCP, and CLI probes.
    path_env::ensure_utf8_locale();

    tauri::Builder::default()
        .register_uri_scheme_protocol("kivio-preview", |context, request| html_preview::serve(context.app_handle(), request))
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .on_page_load(|webview, payload| {
            if webview.label() == "chat" && payload.event() == tauri::webview::PageLoadEvent::Started {
                if let Some(voice) = webview.app_handle().try_state::<voice_codex::VoiceCodexService>() {
                    voice.cancel();
                }
            }
        })
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::CloseRequested { api, .. } => {
                if window.label() == "chat" {
                    let keep_alive = window
                        .app_handle()
                        .state::<AppState>()
                        .settings_read()
                        .keep_chat_window_alive;
                    if keep_alive {
                        api.prevent_close();
                        chat::notification_viewing::clear_window(window.label());
                        #[cfg(not(target_os = "android"))]
                        hide_chat_window(window.app_handle(), window);
                    }
                    return;
                }
                if crate::chat::popout::is_popout_label(window.label()) {
                    // 弹出窗关即销毁，不走主聊天窗 keep-alive。
                    return;
                }
                if window.label() == "lens" || window.label() == "translate" {
                    api.prevent_close();
                    // Windows：原生关闭（Alt+F4 等 WM_CLOSE）也要走完整清理 + destroy，回收内存，
                    // 不留隐藏僵尸 overlay（active_overlay_window 只认可见窗，hide 掉的再也销毁不到）。
                    #[cfg(target_os = "windows")]
                    {
                        let _ = lens_commands::lens_close(window.app_handle().clone());
                    }
                    // macOS：换回原类后可安全 destroy（见 windows::destroy_overlay_window），
                    // 原生关闭也走完整清理 + destroy 回收内存，不留隐藏僵尸 overlay。
                    #[cfg(not(target_os = "windows"))]
                    {
                        let _ = lens_commands::lens_close(window.app_handle().clone());
                    }
                    return;
                }
                // 翻译浮层若仍收到默认 CloseRequested，必须拦截并走安全销毁：先恢复
                // TaoWindow 原类，再 destroy WebView/NSPanel，同时把前台交还给打开它之前的 App。
                #[cfg(target_os = "macos")]
                if window.label() == windows::TRANSLATOR_WINDOW_LABEL {
                    api.prevent_close();
                    let handle = window.app_handle();
                    let st = handle.state::<AppState>();
                    restore_previous_frontmost_app(handle, st.frontmost_apps().translator());
                    if let Some(webview_window) = handle.get_webview_window(windows::TRANSLATOR_WINDOW_LABEL) {
                        windows::destroy_overlay_window(&webview_window);
                    }
                    return;
                }
            }
            tauri::WindowEvent::Focused(false) => {
                chat::notification_viewing::clear_window(window.label());
            }
            tauri::WindowEvent::Focused(true) =>
            {
                #[cfg(target_os = "macos")]
                if window.label() == "lens" || window.label() == "translate" {
                    if let Some(webview_window) =
                        window.app_handle().get_webview_window(window.label())
                    {
                        ensure_overlay_panel(&webview_window);
                    }
                }
            }
            tauri::WindowEvent::Destroyed => {
                let label = window.label();
                if let Some(flows) = window.app_handle().try_state::<connectors::OAuthFlows>() {
                    flows.cancel_window(label);
                }
                chat::notification_viewing::clear_window(label);
                if crate::chat::popout::is_popout_label(label) {
                    crate::chat::popout::on_popout_destroyed(window.app_handle(), label);
                } else if label == "chat" {
                    window.app_handle().state::<voice_codex::VoiceCodexService>().cancel();
                    crate::chat::protocol::unsubscribe_label(
                        &window.app_handle().state::<AppState>(),
                        "chat",
                    );
                }
                #[cfg(target_os = "macos")]
                if label == "chat" || crate::chat::popout::is_popout_label(label) {
                    crate::chat::popout::sync_macos_activation_policy(window.app_handle());
                }
            }
            _ => {}
        })
        .setup(|app| {
            backup::recover_at_startup(&app.path().app_data_dir()?)
                .map_err(std::io::Error::other)?;
            extensions::resume(app.handle().clone());
            let launched_from_autostart = std::env::args().any(|arg| arg == AUTOSTART_ARG);

            // Windows：退出后台执行速度节流（EcoQoS）。无可见窗口时进程会被 Win11 当后台空闲
            // 进程节流,饿死全局热键的 WM_HOTKEY 消息泵 → 热键失灵（托盘点击是 shell 唤醒故仍
            // 可用）。退出 EXECUTION_SPEED 节流后,即便无窗口空闲也保持正常调度,热键消息泵持续工作。
            #[cfg(target_os = "windows")]
            disable_process_power_throttling();

            #[cfg(target_os = "macos")]
            {
                if launched_from_autostart {
                    let _ = app
                        .handle()
                        .set_activation_policy(tauri::ActivationPolicy::Accessory);
                } else {
                    set_macos_regular_activation_policy(app.handle());
                }
            }

            // 清理上次崩溃 / 强杀 / 旧版本遗留的截图 PNG（24h 之前的，避免误删并发实例的活文件）
            // These legacy temp prefixes are shared with installed production builds.
            // A Dev startup must not clean up another installation's files.
            if !app_data::is_dev() {
                cleanup_orphan_temp_files();
                cleanup_stale_sandbox_exports();
            }
            // 会话副产物：空/孤儿工作区目录、已删会话残留的附件目录。只碰 Kivio 自己造的
            // `conv_*` 目录，非空的孤儿工作区只报数不删（里面是用户产物）。
            chat::gc::sweep_conversation_side_artifacts(app.handle());

            #[cfg(target_os = "android")]
            {
                let workspace = app.path().app_data_dir()?.join("workspace");
                std::fs::create_dir_all(&workspace)?;
                std::env::set_var("KIVIO_APP_WORKSPACE", &workspace);
            }

            // 周期性回收闲置的持久外部 CLI **进程**（10 分钟无活动即丢弃 → actor 关闭子进程）。
            // 原生会话 id 仍落在 disk 上，下一轮（或重新打开这条对话）必须 resume，不是开新会话。
            {
                let sweeper = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    loop {
                        tokio::time::sleep(std::time::Duration::from_secs(120)).await;
                        // 启动竞态加固：AppState 可能尚未 manage（极早期 tick），此时跳过本轮，
                        // 120s 后再试，避免 state() panic。
                        let Some(state) = sweeper.try_state::<AppState>() else {
                            continue;
                        };
                        state.external_live_sessions().sweep_idle(
                            crate::external_agents::session::live::LIVE_SESSION_IDLE_TTL,
                        );
                    }
                });
            }

            let mut settings = load_settings(&app.handle());
            external_agents::overrides::sync_from_settings(&settings);
            // 旧版本随应用附带的内置专家已全部下线：启动时清理残留条目，并清空集里
            // 指向已不存在助手的默认助手。用户自建专家不受影响；清理幂等，旧备份恢复后
            // 再次出现的条目也会在下次启动被移除。
            if let Err(err) = chat::storage::remove_builtin_assistants(&app.handle()) {
                eprintln!("Failed to remove built-in assistants: {err}");
            }

            // 开机自启带 `--from-autostart` 时不弹窗；用户显式打开「启动后最小化到托盘」时
            // 任何启动路径都不弹（含参数丢失的自启、开始菜单快捷方式）。
            let skip_chat_on_launch =
                launched_from_autostart || settings.launch_minimized_to_tray;
            #[cfg(target_os = "macos")]
            if skip_chat_on_launch {
                let _ = app
                    .handle()
                    .set_activation_policy(tauri::ActivationPolicy::Accessory);
            }
            let usage_dir = usage::usage_dir(&app.handle()).unwrap_or_else(|err| {
                eprintln!("Failed to initialize usage ledger dir: {err}");
                std::env::temp_dir().join("kivio-usage")
            });

            let offline_models =
                offline_models::OfflineModelManager::new(&app.handle(), build_http_client());
            app.manage(AppState::base(
                settings,
                usage_dir,
                build_http_client(),
                #[cfg(target_os = "macos")]
                macos_ocr::MacOcrClient::new(&app.handle()),
                offline_models.clone(),
                rapidocr::RapidOcrClient::new(offline_models),
            ));
            app.manage(chat::repository::ConversationRepository::default());
            app.manage(scheduled_tasks::ScheduledTasks::load(
                app.path()
                    .app_data_dir()
                    .map_err(|err| format!("app_data_dir unavailable: {err}"))?
                    .join("scheduled_tasks"),
            )?);
            app.manage(chat::media_station::MediaStation::default());
            app.manage(connectors::OAuthFlows::default());
            app.manage(voice_codex::VoiceCodexService::default());

            // 崩溃残留的中断草稿日志:按每个 message_id 的最后一行合并回会话文件后删除。
            // setup 返回前完成草稿恢复，确保 UI run 不与日志恢复/删除并发。
            // 必须放在 ConversationRepository manage 之后：恢复路径要取仓库状态，
            // 曾在 manage 之前 spawn，有草稿残留且调度赶巧时直接 state() panic、启动即崩。
            {
                let handle = app.handle().clone();
                // Recovery only awaits filesystem/repository work, never the UI event loop.
                tauri::async_runtime::block_on(chat::draft_journal::recover_orphan_drafts(&handle));
                tauri::async_runtime::spawn(async move {
                    if let Err(error) = chat::goal::pause_unfinished_after_restart(&handle).await {
                        eprintln!("Failed to pause unfinished Goals after restart: {error}");
                    }
                });
            }
            // Dock services stay on desktop. Android has no PTY and no workspace watcher.
            #[cfg(not(target_os = "android"))]
            {
                app.manage(std::sync::Arc::new(dock::watch::WorkspaceWatchService::new(
                    app.handle().clone(),
                )));
                app.manage(std::sync::Arc::new(dock::terminal::TerminalService::new(
                    app.handle().clone(),
                )));
            }

            crate::chat::sub_agent::delivery::install(app.handle());

            crate::scheduled_tasks::spawn_scheduler(app.handle().clone());

            // 启动后 5s 静默检查更新（settings.auto_check_update 控制）
            // 发现新版 → emit "update-available" 事件，前端 Settings 打开时会展示提示
            // 失败 / 限流 / 网络问题全部静默，不打扰用户
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(Duration::from_secs(5)).await;
                // 启动竞态加固：若 AppState 尚未 manage（异常早期），直接跳过本次更新检查。
                let Some(state) = app_handle.try_state::<AppState>() else {
                    return;
                };
                if !state.settings_read().auto_check_update {
                    return;
                }
                if let Ok(value) = check_github_latest_release(state).await {
                    if value
                        .get("available")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false)
                    {
                        let _ = app_handle.emit("update-available", value);
                    }
                }
            });

            // MCP 持久连接空闲回收 + HTTP 保活：每 60s 扫描连接池。stdio 空闲会话
            // Drop 杀子进程；活着的 HTTP 会话不收，改发 ping（失败则按握手配置重连，
            // 含 OAuth 刷新）。
            {
                let app_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let mut ticker = tokio::time::interval(Duration::from_secs(60));
                    loop {
                        ticker.tick().await;
                        // 启动竞态加固：AppState 可能尚未 manage（极早期 tick），跳过本轮，60s 后再试。
                        let Some(state) = app_handle.try_state::<AppState>() else {
                            continue;
                        };
                        let idle_timeout = state.mcp_idle_timeout();
                        let evicted = state.mcp_reap_idle(idle_timeout).await;
                        for (server_id, _) in evicted {
                            let _ = app_handle.emit(
                                "mcp-server-state",
                                serde_json::json!({
                                    "serverId": server_id,
                                    "state": { "kind": "disconnected" },
                                }),
                            );
                        }
                        state.mcp_keepalive_http(Some(&app_handle)).await;
                    }
                });
            }

            // 无头测试通道（probe）——仅 debug 构建。轮询 <app_data>/chat_probe/request.json，
            // 走与聊天窗口相同的生成路径并把结果写 result.json，供自动化真实验证工具调用。
            #[cfg(debug_assertions)]
            {
                let app_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    crate::chat::probe::run_probe_watcher(app_handle).await;
                });
            }

            // MCP connects when generation discovers tools or the user explicitly
            // enables/tests a server. Tray startup must not launch tool processes.

            // 手动启动默认打开聊天窗口；自启 / 「启动后最小化到托盘」则只留托盘常驻。
            // Android creates the main window from tauri.android.conf.json.
            #[cfg(not(target_os = "android"))]
            if !skip_chat_on_launch {
                let app_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    if let Err(err) = open_chat_window(&app_handle) {
                        eprintln!("Failed to open chat on launch: {err}");
                    }
                });
            }
            #[cfg(target_os = "android")]
            {
                let _ = skip_chat_on_launch;
            }
            #[cfg(target_os = "macos")]
            macos_hang_watchdog::start(app.handle());
            Ok(())
        })
        .invoke_handler(|invoke| {
            if extensions::owns_command(invoke.message.command()) {
                return extensions::invoke(invoke);
            }
            let handler: fn(tauri::ipc::Invoke<tauri::Wry>) -> bool = tauri::generate_handler![
            extensions::extensions_list,
            extensions::extensions_set_enabled,
            html_preview::create_html_preview,
            html_preview::release_html_preview,
            provider_oauth::provider_oauth_start,
            provider_oauth::provider_oauth_poll,
            provider_oauth::provider_oauth_cancel,
            provider_oauth::provider_oauth_disconnect,
            provider_oauth::usage::provider_oauth_usage,
            provider_oauth::account::provider_oauth_account,
            chat::media_station::media_station_list,
            chat::media_station::media_station_start,
            chat::media_station::media_station_cancel,
            chat::media_station::media_station_delete,
            chat::media_station::media_station_resume,
            chat::media_station::media_station_read,
            chat::media_station::media_station_export,
            chat::media_station::media_station_reference,
            commands::get_settings,
            windows::chat_window_apply_mica,
            windows::chat_window_set_material_appearance,
            windows::chat_window_set_opaque,
            windows::chat_traffic_light_center_y,
            chat::notification_viewing::chat_report_notification_view,
            windows::chat_remember_last_route,
            fonts::list_system_fonts,
            commands::get_default_prompt_templates,
            commands::save_settings,
            commands::set_favorite_models,
            backup::webdav::webdav_load,
            backup::webdav::webdav_save,
            backup::webdav::webdav_test,
            backup::webdav::webdav_list,
            backup::webdav::webdav_upload,
            backup::webdav::webdav_download,
            backup::webdav::webdav_release,
            backup::webdav::webdav_delete,
            backup::export_full_backup,
            backup::import_full_backup,
            backup::backup_interface,
            backup::backup_restore_pending,
            backup::cancel_restore,
            backup::restart_after_restore,
            commands::export_settings,
            commands::import_settings,
            commands::open_settings_window,
            commands::open_external,
            commands::open_local_file,
            commands::open_data_url_file,
            commands::open_html_preview,
            html_preview::open_html_render,
            commands::fetch_models,
            commands::test_provider_connection,
            commands::test_web_search,
            commands::get_permission_status,
            commands::open_permission_settings,
            updates::check_github_latest_release,
            chat::model_database::get_model_database,
            chat::model_database::update_model_database,
            usage::usage_get_stats,
            usage::usage_get_conversation_cost,
            usage::usage_clear,
            chat::commands::interaction::get_request_debug_records,
            chat::commands::interaction::clear_request_debug_records,
            chat::protocol::chat_sync_state,
            chat::protocol::chat_protocol_subscribe,
            // Chat 模块命令
            chat::commands::catalog::chat_get_conversations,
            chat::commands::interaction::chat_list_background_tasks,
            chat::commands::interaction::chat_clear_finished_background_tasks,
            chat::commands::interaction::chat_stop_external_background_task,
            chat::commands::interaction::chat_kill_background_command,
            chat::commands::catalog::chat_search_conversations,
            chat::commands::catalog::chat_query_conversations,
            chat::commands::catalog::chat_get_conversation,
            chat::commands::catalog::chat_get_conversation_window,
            chat::commands::catalog::chat_get_conversation_page,
            chat::commands::catalog::chat_get_conversation_revision,
            chat::export::chat_export_conversation_markdown,
            chat::export::chat_export_set_backup,
            chat::export::chat_import_set_backup,
            chat::commands::catalog::chat_create_conversation,
            chat::commands::catalog::chat_import_external_conversation,
            chat::commands::catalog::chat_create_builder_conversation,
            chat::commands::catalog::chat_get_assistants,
            chat::commands::catalog::chat_create_assistant,
            chat::commands::catalog::chat_update_assistant,
            chat::commands::catalog::chat_duplicate_assistant,
            chat::commands::catalog::chat_delete_assistant,
            chat::commands::catalog::chat_get_projects,
            chat::commands::catalog::chat_reorder_projects,
            chat::commands::catalog::chat_get_conversation_pins,
            chat::commands::catalog::chat_set_conversation_pins,
            chat::commands::catalog::chat_create_project,
            chat::commands::catalog::chat_update_project,
            chat::commands::catalog::chat_delete_project,
            chat::commands::catalog::chat_project_open_folder,
            chat::commands::catalog::chat_get_sets,
            chat::commands::catalog::chat_reorder_sets,
            chat::commands::catalog::chat_create_set,
            chat::commands::catalog::chat_update_set,
            chat::commands::catalog::chat_delete_set,
            chat::commands::context::chat_get_context_stats,
            chat::commands::context::chat_compress_context,
            chat::commands::context::chat_clear_context,
            chat::commands::interaction::chat_take_external_sends,
            chat::commands::interaction::chat_ack_external_send,
            chat::commands::interaction::chat_renew_external_sends,
            chat::commands::interaction::chat_release_external_sends,
            chat::commands::interaction::chat_set_agent_plan_mode,
            chat::commands::interaction::chat_execute_agent_plan,
            chat::goal::chat_get_goal,
            chat::goal::chat_start_goal,
            chat::goal::chat_edit_goal,
            chat::goal::chat_pause_goal,
            chat::goal::chat_resume_goal,
            chat::goal::chat_cancel_goal,
            chat::goal::chat_set_goal_user_queue_pending,
            chat::commands::send::chat_continue_goal,
            chat::commands::send::chat_send_message,
            chat::commands::interaction::chat_cancel_stream,
            chat::sub_agent::commands::chat_subagent_snapshot,
            chat::sub_agent::commands::chat_subagent_waiting,
            chat::sub_agent::commands::chat_subagent_roles,
            chat::sub_agent::commands::chat_subagent_open,
            chat::sub_agent::commands::chat_subagent_send,
            chat::sub_agent::commands::chat_subagent_kill,
            chat::sub_agent::commands::chat_subagent_park,
            chat::sub_agent::commands::chat_subagent_transcript,
            chat::sub_agent::commands::chat_subagent_activity,
            chat::commands::interaction::chat_confirm_tool_call,
            chat::commands::interaction::chat_respond_session_consent,
            chat::commands::interaction::chat_submit_user_choice,
            chat::commands::interaction::chat_steer_message,
            chat::commands::interaction::chat_follow_up_message,
            chat::commands::attachments::chat_read_attachment,
            chat::artifacts::chat_artifacts_list,
            chat::artifacts::chat_artifact_action,
            chat::commands::attachments::chat_open_attachment,
            chat::commands::attachments::chat_reveal_attachment,
            chat::commands::attachments::chat_open_generated_artifact,
            chat::commands::attachments::chat_reveal_generated_artifact,
            chat::commands::attachments::chat_save_pasted_image,
            chat::commands::attachments::chat_save_pasted_attachment,
            chat::commands::attachments::chat_inspect_attachment_paths,
            chat::commands::attachments::chat_read_clipboard_files,
            chat::commands::attachments::chat_read_clipboard,
            chat::commands::attachments::chat_write_clipboard_text,
            chat::commands::mutations::chat_delete_conversation,
            chat::commands::mutations::chat_update_conversation,
            chat::commands::title::chat_regenerate_title,
            chat::commands::prompt_optimize::chat_optimize_prompt,
            chat::commands::mutations::chat_bulk_update_conversations,
            chat::commands::mutations::chat_bulk_delete_conversations,
            chat::commands::reasoning::chat_thinking_capabilities_for_model,
            chat::commands::mutations::chat_update_message,
            chat::commands::mutations::chat_delete_message,
            chat::commands::mutations::chat_set_group_selection,
            chat::commands::mutations::chat_get_branches,
            chat::commands::mutations::chat_switch_branch,
            chat::commands::mutations::chat_reply_with_model,
            chat::commands::mutations::chat_regenerate_message,
            chat::commands::mutations::chat_rewind_to_message,
            chat::commands::mutations::chat_fork_conversation,
            chat::memory::chat_memory_get,
            chat::memory::chat_memory_save,
            chat::memory::chat_memory_open_folder,
            mcp::registry::chat_mcp_list_tools,
            mcp::registry::chat_mcp_test_server,
            mcp::registry::chat_mcp_import_json,
            mcp::registry::chat_mcp_server_status,
            mcp::registry::chat_mcp_list_tool_defs,
            mcp::registry::chat_mcp_reload_server,
            mcp::registry::chat_mcp_warmup,
            connectors::connector_oauth_connect,
            connectors::connector_oauth_cancel,
            market::market_snapshot,
            market::market_install,
            market::market_uninstall,
            market::market_set_enabled,
            plugins::plugins_list,
            plugins::packages::plugin_packages_list,
            plugins::packages::details::plugin_packages_describe,
            plugins::marketplaces::plugin_marketplaces_describe,
            plugins::marketplaces::plugin_marketplaces_list,
            plugins::marketplaces::plugin_marketplaces_add,
            plugins::marketplaces::plugin_marketplaces_refresh,
            plugins::marketplaces::plugin_marketplaces_remove,
            plugins::marketplaces::plugin_marketplaces_install,
            plugins::packages::plugin_packages_import,
            plugins::packages::plugin_packages_set_enabled,
            plugins::packages::plugin_packages_remove,
            plugins::packages::workflow_hooks_get,
            plugins::packages::workflow_hooks_save,
            plugins::plugins_list_cached,
            plugins::plugins_install_brief,
            plugins::plugins_run_official_install,
            plugins::plugins_set_enabled,
            plugins::plugins_uninstall,
            scheduled_tasks::commands::scheduled_tasks_snapshot,
            scheduled_tasks::commands::scheduled_task_save,
            scheduled_tasks::commands::scheduled_task_delete,
            scheduled_tasks::commands::scheduled_task_set_enabled,
            scheduled_tasks::commands::scheduled_task_run_now,
            scheduled_tasks::commands::scheduled_task_runs,
            scheduled_tasks::commands::scheduled_task_preview,
            scheduled_tasks::commands::scheduled_task_run_delete,
            chat::commands::catalog::chat_session_skills,
            skills::chat_skills_list,
            skills::chat_skills_read,
            skills::chat_skills_import,
            skills::chat_skills_uninstall,
            skills::chat_skills_install_from_url,
            skills::chat_skills_open_folder,
        ];
            handler(invoke)
        })
        .build(context)
        .expect("error while building tauri application")
        .run(|app_handle, event| match event {
            tauri::RunEvent::ExitRequested { api, code, .. } => {
                if code.is_none() {
                    api.prevent_exit();
                } else {
                    // 真正退出：同步排干 MCP 连接池，杀掉所有持久子进程，避免孤儿进程。
                    let state: State<AppState> = app_handle.state();
                    let voice: State<voice_codex::VoiceCodexService> = app_handle.state();
                    if tauri::async_runtime::block_on(voice.shutdown()).is_err() {
                        eprintln!("Voice cleanup has not completed; keeping the application alive.");
                        backup::RESTART_REQUESTED.store(false, std::sync::atomic::Ordering::SeqCst);
                        api.prevent_exit();
                        return;
                    }
                    tauri::async_runtime::block_on(extensions::shutdown());
                    // 自动化先于 MCP：运行中的图可能正跑 agent loop（依赖 MCP/供应商）或
                    // 命令节点（Child 靠 kill_on_drop 收尸）。先标记取消、限时等收尾，
                    // 此时运行时还活着，select! 的取消分支才来得及 drop 掉 Child。
                    let cancelled = crate::automation::cancel_all_runs(app_handle);
                    if cancelled > 0 {
                        // 同上：timeout 必须在 async 块里构造。
                        let finished = tauri::async_runtime::block_on(async {
                            tokio::time::timeout(
                                std::time::Duration::from_secs(3),
                                crate::automation::wait_runs_finished(app_handle),
                            )
                            .await
                        });
                        if finished.is_err() {
                            eprintln!("Automation runs did not finish in time on exit.");
                        }
                    }
                    // 带超时：一个卡在握手里的 server 会占着会话锁不放，没有这层
                    // 上限的话退出钩子会永久阻塞在主线程上 —— 表现是「点关闭没反应、
                    // 进程不退」，连带其余所有 MCP 子进程全留在系统里。
                    //
                    // ⚠️ `timeout(..)` **必须在 async 块里构造**，不能当参数传给 block_on。
                    // 参数在进入运行时之前就求值了，而 `tokio::time::Sleep` 构造时就要求时间
                    // 驱动在场，否则 panic「there is no reactor running」。这条 panic 会让
                    // 下面整段退出清理（外部 CLI 会话 / 后台命令进程组 / 插件预览）全部不执行
                    // —— 表现是每次退出漏一批孤儿子进程，外加退出码 101。
                    let drained = tauri::async_runtime::block_on(async {
                        tokio::time::timeout(
                            std::time::Duration::from_secs(3),
                            state.mcp_disconnect_all(),
                        )
                        .await
                    });
                    if drained.is_err() {
                        // 超时说明某个会话锁拿不到（多半卡在握手里）。优雅关停这条路已经走不通，
                        // 按 pid 杀进程树兜底 —— 否则那句「falling back to process kill」就是空话，
                        // stdio 子进程（及它自己拉起的孙子）会留在系统里。
                        let killed = state.kill_mcp_children_now();
                        eprintln!(
                            "MCP disconnect timed out on exit; killed {killed} stdio child process tree(s)."
                        );
                    }
                    // 外部 CLI 会话：必须**同步**等它们关完。只 clear 掉 sender 是不够的
                    // —— actor 要等下一次被 poll 才会走 close()，而运行时马上就随进程走了，
                    // `kill_on_drop` 也因此不会触发（Child 在那个永不 drop 的帧里）。
                    // 结果是每次退出留下一批 CLI 进程，各自还挂着自己拉起的 MCP 子进程。
                    // 同上：timeout 必须在 async 块里构造。
                    let closed = tauri::async_runtime::block_on(async {
                        tokio::time::timeout(
                            std::time::Duration::from_secs(3),
                            state.external_live_sessions().close_all(),
                        )
                        .await
                    });
                    if closed.is_err() {
                        eprintln!("External CLI sessions did not close in time on exit.");
                    }
                    // 杀掉所有跟踪中的后台 run_command 进程组（跨 turn 存活，只在这里或
                    // 显式 kill_background 才清理），删除其 per-job 日志，避免孤儿进程/文件。
                    let killed = state.background_commands_handle().kill_all();
                    if killed > 0 {
                        eprintln!("Killed {killed} background command process group(s) on exit.");
                    }
                    #[cfg(target_os = "macos")]
                    state.macos_ocr.shutdown();
                    // OfficeCLI live preview (`officecli watch`) 等插件附属进程
                    crate::plugins::stop_all_previews();
                }
            }
            // 桌宠只在最终退出时拆掉。ExitRequested 在用户关窗（code 为空）
            // 和子进程清理未完成时都会 prevent_exit，那种路径进程还活着。
            tauri::RunEvent::Exit => {
                desktop_pet::shutdown(app_handle);
                // Plugins (single-instance lock) have already handled Exit; relaunch applies the restore.
                if backup::RESTART_REQUESTED.swap(false, std::sync::atomic::Ordering::SeqCst) {
                    app_handle.restart();
                }
            }
            #[cfg(target_os = "macos")]
            tauri::RunEvent::Reopen {
                has_visible_windows,
                ..
            } => {
                if !has_visible_windows {
                    if let Err(err) = open_chat_window(app_handle) {
                        eprintln!("Failed to open chat on dock reopen: {err}");
                    }
                } else if let Some(window) = first_visible_user_window(app_handle) {
                    // 已有可见窗口：只把它带到前台，绝不调 open_chat_window——那会把停在
                    // #chat/settings 的用户重置回 #chat，丢失填到一半的配置。
                    let _ = app_handle.set_activation_policy(tauri::ActivationPolicy::Regular);
                    if window.is_minimized().ok().unwrap_or(false) {
                        let _ = window.unminimize();
                    }
                    let _ = window.show();
                    let _ = window.set_focus();
                } else if let Err(err) = open_chat_window(app_handle) {
                    eprintln!("Failed to open chat on dock reopen: {err}");
                }
            }
            _ => {}
        });
}
