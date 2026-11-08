//! Android host seams. Pure helpers run on every target; JNI is Android-only
//! and no-ops when the process has no Java context (host unit tests).

use std::path::{Component, Path, PathBuf};

pub fn android_shell_program() -> &'static str {
    "/system/bin/sh"
}

pub fn system_browser_action() -> &'static str {
    "android.intent.action.VIEW"
}

pub fn next_alarm_trigger_ms(now_ms: u64, wait_secs: u64) -> u64 {
    now_ms.saturating_add(wait_secs.saturating_mul(1000))
}

/// `KIVIO_APP_WORKSPACE` plus optional `KIVIO_GRANTED_ROOTS` (semicolon-separated).
pub fn granted_workspace_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(path) = non_empty_env_path("KIVIO_APP_WORKSPACE") {
        roots.push(path);
    }
    if let Ok(extra) = std::env::var("KIVIO_GRANTED_ROOTS") {
        for part in extra.split(';') {
            let trimmed = part.trim();
            if !trimmed.is_empty() {
                roots.push(PathBuf::from(trimmed));
            }
        }
    }
    roots
}

fn non_empty_env_path(key: &str) -> Option<PathBuf> {
    let value = std::env::var(key).ok()?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(PathBuf::from(trimmed))
    }
}

/// Lexical confinement. No symlink resolution. Empty `roots` is an error.
/// A path is accepted when it equals a root or is a descendant after `..` is resolved.
pub fn confine_to_granted_roots(path: &Path, roots: &[PathBuf]) -> Result<PathBuf, String> {
    if roots.is_empty() {
        return Err("no granted workspace roots".to_string());
    }
    let normalized = normalize_lexically(path)?;
    for root in roots {
        let root = normalize_lexically(root)?;
        if normalized == root || normalized.starts_with(&root) {
            return Ok(normalized);
        }
    }
    Err(format!(
        "path is outside granted workspace roots: {}",
        normalized.display()
    ))
}

fn normalize_lexically(path: &Path) -> Result<PathBuf, String> {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => out.push(prefix.as_os_str()),
            Component::RootDir => out.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                let popped_root = out
                    .components()
                    .all(|part| matches!(part, Component::Prefix(_) | Component::RootDir));
                if popped_root || !out.pop() {
                    return Err("path escapes via ..".to_string());
                }
            }
            Component::Normal(part) => out.push(part),
        }
    }
    if out.as_os_str().is_empty() {
        return Err("path is empty".to_string());
    }
    Ok(out)
}

/// Arms an inexact while-idle alarm. Non-Android and missing JNI context succeed without work.
pub fn arm_scheduled_task(trigger_ms: u64) -> Result<(), String> {
    #[cfg(not(target_os = "android"))]
    {
        let _ = trigger_ms;
        Ok(())
    }
    #[cfg(target_os = "android")]
    {
        match try_android_context() {
            Some(ctx) => arm_scheduled_task_jni(&ctx, trigger_ms),
            None => Ok(()),
        }
    }
}

/// ACTION_VIEW for an http(s) URL. Only called on Android.
#[cfg(target_os = "android")]
pub fn open_http_url(url: &str) -> Result<(), String> {
    let Some(ctx) = try_android_context() else {
        return Err("Android JNI context is unavailable".to_string());
    };
    open_http_url_jni(&ctx, url)
}

#[cfg(target_os = "android")]
fn try_android_context() -> Option<ndk_context::AndroidContext> {
    let ctx = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ndk_context::android_context()
    }))
    .ok()?;
    if ctx.vm().is_null() || ctx.context().is_null() {
        None
    } else {
        Some(ctx)
    }
}

#[cfg(target_os = "android")]
fn with_jni<T>(
    ctx: &ndk_context::AndroidContext,
    body: impl FnOnce(&mut jni::JNIEnv, &jni::objects::JObject) -> Result<T, String>,
) -> Result<T, String> {
    unsafe {
        let vm = jni::JavaVM::from_raw(ctx.vm() as *mut jni::sys::JavaVM)
            .map_err(|err| err.to_string())?;
        let mut env = vm.attach_current_thread().map_err(|err| err.to_string())?;
        let context = std::mem::ManuallyDrop::new(jni::objects::JObject::from_raw(
            ctx.context() as jni::sys::jobject,
        ));
        body(&mut env, &context)
    }
}

#[cfg(target_os = "android")]
fn arm_scheduled_task_jni(
    ctx: &ndk_context::AndroidContext,
    trigger_ms: u64,
) -> Result<(), String> {
    // AlarmManager.RTC_WAKEUP = 0. setAndAllowWhileIdle is inexact and does not
    // need SCHEDULE_EXACT_ALARM. PendingIntent.FLAG_UPDATE_CURRENT | FLAG_IMMUTABLE.
    const RTC_WAKEUP: i32 = 0;
    const PENDING_FLAGS: i32 = 0x0800_0000 | 0x0400_0000;
    with_jni(ctx, |env, context| {
        let service_name = env
            .new_string("alarm")
            .map_err(|err| err.to_string())?;
        let alarm_manager = env
            .call_method(
                context,
                "getSystemService",
                "(Ljava/lang/String;)Ljava/lang/Object;",
                &[jni::objects::JValue::Object(service_name.as_ref())],
            )
            .map_err(|err| err.to_string())?
            .l()
            .map_err(|err| err.to_string())?;
        if alarm_manager.as_raw().is_null() {
            return Ok(());
        }
        let intent_class = env
            .find_class("android/content/Intent")
            .map_err(|err| err.to_string())?;
        let intent = env
            .new_object(&intent_class, "()V", &[])
            .map_err(|err| err.to_string())?;
        let package_name = env
            .new_string("com.zmair.kivio")
            .map_err(|err| err.to_string())?;
        let class_name = env
            .new_string("com.zmair.kivio.TaskAlarm")
            .map_err(|err| err.to_string())?;
        env.call_method(
            &intent,
            "setClassName",
            "(Ljava/lang/String;Ljava/lang/String;)Landroid/content/Intent;",
            &[
                jni::objects::JValue::Object(package_name.as_ref()),
                jni::objects::JValue::Object(class_name.as_ref()),
            ],
        )
        .map_err(|err| err.to_string())?;
        let action = env
            .new_string("com.zmair.kivio.TASK_ALARM")
            .map_err(|err| err.to_string())?;
        env.call_method(
            &intent,
            "setAction",
            "(Ljava/lang/String;)Landroid/content/Intent;",
            &[jni::objects::JValue::Object(action.as_ref())],
        )
        .map_err(|err| err.to_string())?;
        let pending_class = env
            .find_class("android/app/PendingIntent")
            .map_err(|err| err.to_string())?;
        let pending = env
            .call_static_method(
                &pending_class,
                "getBroadcast",
                "(Landroid/content/Context;ILandroid/content/Intent;I)Landroid/app/PendingIntent;",
                &[
                    jni::objects::JValue::Object(context),
                    jni::objects::JValue::Int(0),
                    jni::objects::JValue::Object(&intent),
                    jni::objects::JValue::Int(PENDING_FLAGS),
                ],
            )
            .map_err(|err| err.to_string())?
            .l()
            .map_err(|err| err.to_string())?;
        env.call_method(
            &alarm_manager,
            "setAndAllowWhileIdle",
            "(IJLandroid/app/PendingIntent;)V",
            &[
                jni::objects::JValue::Int(RTC_WAKEUP),
                jni::objects::JValue::Long(i64::try_from(trigger_ms).unwrap_or(i64::MAX)),
                jni::objects::JValue::Object(&pending),
            ],
        )
        .map_err(|err| err.to_string())?;
        Ok(())
    })
}

#[cfg(target_os = "android")]
fn open_http_url_jni(ctx: &ndk_context::AndroidContext, url: &str) -> Result<(), String> {
    const FLAG_ACTIVITY_NEW_TASK: i32 = 0x1000_0000;
    with_jni(ctx, |env, context| {
        let action = env
            .new_string(system_browser_action())
            .map_err(|err| err.to_string())?;
        let intent_class = env
            .find_class("android/content/Intent")
            .map_err(|err| err.to_string())?;
        let intent = env
            .new_object(
                &intent_class,
                "(Ljava/lang/String;)V",
                &[jni::objects::JValue::Object(action.as_ref())],
            )
            .map_err(|err| err.to_string())?;
        let uri_class = env
            .find_class("android/net/Uri")
            .map_err(|err| err.to_string())?;
        let url = env.new_string(url).map_err(|err| err.to_string())?;
        let uri = env
            .call_static_method(
                &uri_class,
                "parse",
                "(Ljava/lang/String;)Landroid/net/Uri;",
                &[jni::objects::JValue::Object(url.as_ref())],
            )
            .map_err(|err| err.to_string())?
            .l()
            .map_err(|err| err.to_string())?;
        env.call_method(
            &intent,
            "setData",
            "(Landroid/net/Uri;)Landroid/content/Intent;",
            &[jni::objects::JValue::Object(uri.as_ref())],
        )
        .map_err(|err| err.to_string())?;
        env.call_method(
            &intent,
            "addFlags",
            "(I)Landroid/content/Intent;",
            &[jni::objects::JValue::Int(FLAG_ACTIVITY_NEW_TASK)],
        )
        .map_err(|err| err.to_string())?;
        env.call_method(
            context,
            "startActivity",
            "(Landroid/content/Intent;)V",
            &[jni::objects::JValue::Object(&intent)],
        )
        .map_err(|err| err.to_string())?;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn shell_browser_and_alarm_math() {
        assert_eq!(android_shell_program(), "/system/bin/sh");
        assert_eq!(system_browser_action(), "android.intent.action.VIEW");
        assert_eq!(next_alarm_trigger_ms(1_000, 2), 3_000);
        assert_eq!(next_alarm_trigger_ms(u64::MAX, 5), u64::MAX);
        assert!(arm_scheduled_task(next_alarm_trigger_ms(0, 1)).is_ok());
    }

    #[test]
    fn confine_to_granted_roots_accepts_file_and_rejects_escape() {
        let root = std::env::temp_dir().join(format!(
            "kivio-confine-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("nested")).unwrap();
        let file = root.join("nested").join("note.txt");
        fs::write(&file, b"ok").unwrap();
        let roots = vec![root.clone()];

        let confined = confine_to_granted_roots(&file, &roots).unwrap();
        assert!(confined.ends_with("note.txt"));
        assert!(confine_to_granted_roots(&root, &roots).is_ok());

        let sibling = root
            .parent()
            .unwrap()
            .join(format!("kivio-confine-sibling-{}", std::process::id()));
        assert!(confine_to_granted_roots(&sibling, &roots).is_err());

        let escape = root.join("nested").join("..").join("..").join("outside.txt");
        assert!(confine_to_granted_roots(&escape, &roots).is_err());
        assert!(confine_to_granted_roots(&file, &[]).is_err());

        let _ = fs::remove_dir_all(&root);
    }
}
