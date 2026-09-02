mod model;
mod net;
mod npm;
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

/// 后台拉起系统程序（资源管理器/浏览器），不等待其退出
fn spawn_open(program: &str, arg: &str) -> Result<(), String> {
    std::process::Command::new(program)
        .arg(arg)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
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
            spawn_open("explorer", &format!("/select,{p}"))
        } else {
            spawn_open("explorer", p)
        }
    }
    #[cfg(target_os = "macos")]
    {
        spawn_open("open", p)
    }
    #[cfg(target_os = "linux")]
    {
        spawn_open("xdg-open", p)
    }
}

/// 写系统剪贴板：WebView2 的 navigator.clipboard 在部分环境会静默挂起，
/// 统一走后端保证成败都有明确返回
#[tauri::command]
fn copy_text(text: String) -> Result<(), String> {
    arboard::Clipboard::new()
        .and_then(|mut c| c.set_text(text))
        .map_err(|e| format!("写入剪贴板失败: {e}"))
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
        spawn_open("explorer", u)
    }
    #[cfg(target_os = "macos")]
    {
        spawn_open("open", u)
    }
    #[cfg(target_os = "linux")]
    {
        spawn_open("xdg-open", u)
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(net::AppState::default())
        .manage(npm::NpmState::default())
        .invoke_handler(tauri::generate_handler![
            load_data,
            save_data,
            check_item,
            app_info,
            check_self_update,
            net::download_file,
            net::pause_download,
            net::cancel_download,
            net::list_downloads,
            vscode::list_vsix,
            vscode::read_installed_extensions,
            vscode::check_vscode_updates,
            npm::npm_detect_root,
            npm::scan_npm,
            npm::check_npm_updates,
            npm::npm_upgrade,
            npm::npm_cancel_upgrade,
            open_path,
            copy_text,
            open_url
        ])
        .run(tauri::generate_context!())
        .expect("Aurora 启动失败");
}
