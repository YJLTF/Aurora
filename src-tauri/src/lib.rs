mod model;
mod net;
mod version;
mod vscode;

use model::{AppInfo, CheckOutcome, Config, SelfUpdateInfo, Settings, SoftwareItem};
use tauri::{AppHandle, Manager};

fn config_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("aurora.json"))
}

fn normalize_settings(app: &AppHandle, s: &mut Settings) {
    if s.download_dir.trim().is_empty() {
        s.download_dir = app
            .path()
            .download_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
    }
    if s.github_api_base.trim().is_empty() {
        s.github_api_base = "https://api.github.com".into();
    }
    // VSCode 备份目录默认放在下载目录下
    if s.vscode_dir.trim().is_empty() && !s.download_dir.trim().is_empty() {
        s.vscode_dir = format!(
            "{}{}vscode",
            s.download_dir.trim_end_matches(['/', '\\']),
            std::path::MAIN_SEPARATOR
        );
    }
}

fn save_to(path: &std::path::Path, cfg: &Config) -> Result<(), String> {
    let json = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    // 先写临时文件再替换，避免写入中断损坏配置
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

/// 读取配置；首次运行返回预置清单并落盘
#[tauri::command]
fn load_data(app: AppHandle) -> Result<Config, String> {
    let path = config_path(&app)?;
    if path.exists() {
        let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let mut cfg: Config =
            serde_json::from_str(&text).map_err(|e| format!("配置文件解析失败: {e}"))?;
        normalize_settings(&app, &mut cfg.settings);
        Ok(cfg)
    } else {
        let mut cfg = model::seed_config();
        normalize_settings(&app, &mut cfg.settings);
        let _ = save_to(&path, &cfg);
        Ok(cfg)
    }
}

#[tauri::command]
fn save_data(app: AppHandle, config: Config) -> Result<(), String> {
    let path = config_path(&app)?;
    save_to(&path, &config)
}

/// 检测最新版本。结果通过 error 字段表达失败原因，方便前端逐项展示。
#[tauri::command]
async fn check_item(item: SoftwareItem, settings: Settings) -> Result<CheckOutcome, String> {
    Ok(net::check_item(&item, &settings).await)
}

/// 当前应用版本与自更新仓库
#[tauri::command]
fn app_info(app: AppHandle) -> AppInfo {
    AppInfo {
        version: app.package_info().version.to_string(),
        repo: model::SELF_REPO.into(),
    }
}

/// 检查 Aurora 自身的更新（GitHub Releases）
#[tauri::command]
async fn check_self_update(app: AppHandle, settings: Settings) -> SelfUpdateInfo {
    let current = app.package_info().version.to_string();
    net::check_self_update(&current, &settings).await
}

/// 用资源管理器打开目录；reveal 为 true 时定位到文件
#[tauri::command]
fn open_path(path: String, reveal: Option<bool>) -> Result<(), String> {
    let p = path.trim();
    if p.is_empty() {
        return Err("路径为空".into());
    }
    #[cfg(target_os = "windows")]
    {
        if reveal.unwrap_or(false) {
            std::process::Command::new("explorer")
                .arg(format!("/select,{p}"))
                .spawn()
                .map_err(|e| e.to_string())?;
        } else {
            std::process::Command::new("explorer")
                .arg(p)
                .spawn()
                .map_err(|e| e.to_string())?;
        }
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(p)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(p)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 用系统默认浏览器打开链接
#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    let u = url.trim();
    if !u.starts_with("http://") && !u.starts_with("https://") {
        return Err("仅支持 http(s) 链接".into());
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(u)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(u)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(u)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(net::AppState::default())
        .invoke_handler(tauri::generate_handler![
            load_data,
            save_data,
            check_item,
            app_info,
            check_self_update,
            net::download_file,
            net::cancel_download,
            net::list_downloads,
            vscode::list_vsix,
            vscode::read_installed_extensions,
            vscode::check_vscode_updates,
            open_path,
            open_url
        ])
        .run(tauri::generate_context!())
        .expect("Aurora 启动失败");
}
