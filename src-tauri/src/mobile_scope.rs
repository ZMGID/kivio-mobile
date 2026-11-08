#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn mobile_scope_cuts_shipped_surface() {
        let src = include_str!("lib.rs");
        let start = src
            .find("tauri::generate_handler![")
            .expect("invoke handler");
        let rest = &src[start..];
        let end = rest.find("];").expect("handler terminator");
        let handler = &rest[..end];

        for absent in [
            "desktop_pet::",
            "voice_live::",
            "voice_codex::",
            "lens_commands::",
            "rapidocr_",
            "replace_translation_",
            "translate_text",
            "chat::popout::",
            "external_agents::",
            "computer_control::",
            "notes::",
            "automation::commands::",
            "knowledge_base::",
            "dock::",
            "download_update_asset",
            "install_update_and_quit",
            "chat_cli_import_scan",
        ] {
            assert!(
                !handler.contains(absent),
                "shipped handler still registers {absent}"
            );
        }
        for present in [
            "chat_send_message",
            "scheduled_task_save",
            "chat_skills_list",
            "plugins_list",
            "chat_mcp_list_tools",
            "provider_oauth_start",
            "chat_memory_get",
            "chat_get_projects",
            "media_station_list",
            "open_external",
        ] {
            assert!(
                handler.contains(present),
                "shipped handler is missing {present}"
            );
        }

        let names: Vec<&str> = crate::mcp::native_registry::NATIVE_TOOLS
            .iter()
            .map(|entry| entry.name)
            .collect();
        for absent in [
            "knowledge_search",
            "automation_list",
            "automation_get",
            "automation_upsert",
            "automation_set_enabled",
            "automation_run",
            "automation_runs",
            "automation_delete",
        ] {
            assert!(!names.contains(&absent), "native tool still shipped: {absent}");
        }
        for present in [
            "web_search",
            "web_fetch",
            "read",
            "write",
            "edit",
            "bash",
            "bash_output",
            "kill_background",
            "ls",
            "grep",
            "glob",
            "kivio_inspect",
            "kivio_configure",
            "memory_read",
            "memory_modify",
        ] {
            assert!(names.contains(&present), "native tool missing: {present}");
        }

        assert_eq!(
            crate::android_platform::android_shell_program(),
            "/system/bin/sh"
        );
        assert_eq!(
            crate::android_platform::system_browser_action(),
            "android.intent.action.VIEW"
        );

        let root = std::env::temp_dir().join(format!(
            "kivio-mobile-scope-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("inside")).unwrap();
        let file = root.join("inside").join("note.txt");
        fs::write(&file, b"ok").unwrap();
        let roots = vec![root.clone()];
        assert!(crate::android_platform::confine_to_granted_roots(&file, &roots).is_ok());
        let sibling: PathBuf = root
            .parent()
            .unwrap()
            .join(format!("kivio-mobile-scope-sibling-{}", std::process::id()));
        assert!(crate::android_platform::confine_to_granted_roots(&sibling, &roots).is_err());
        let escape = root.join("inside").join("..").join("..").join("nope");
        assert!(crate::android_platform::confine_to_granted_roots(&escape, &roots).is_err());
        assert!(crate::android_platform::confine_to_granted_roots(&file, &[]).is_err());
        let _ = fs::remove_dir_all(&root);
    }
}
