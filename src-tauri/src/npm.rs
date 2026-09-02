//! npm 全局包扫描与 registry 更新检查。
//!
//! 全局位置来自 `npm root -g`（可在设置中手动指定目录兜底，规避 GUI 进程
//! PATH 缺失问题），包清单读取各包目录下 package.json 的 name/version；
//! 最新版本查询 registry 的 `/-/package/<name>/dist-tags` 端点取 `latest`。

use crate::model::{NpmCheck, NpmInfo, NpmRef, Settings};
use crate::version::compare;
use futures_util::stream::{self, StreamExt};
use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::io::BufRead;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, State};

/// registry 并发请求数上限（registry 无批量接口，逐包查询）
const CHECK_CONCURRENCY: usize = 6;

/// npm 升级进度事件名（与 download-progress 同级的全局唯一订阅事件）
pub const UPGRADE_EVENT: &str = "npm-upgrade-progress";

/// npm 升级进度（preparing/progressing/done/error/cancelled；done 回填升级后版本）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NpmUpgradeProgress {
    pub name: String,
    pub status: String,
    /// 最近一行 npm 输出（error 时为错误摘要）
    pub output: String,
    pub error: String,
    /// done 时回填升级后的本地版本（重读 package.json，读不到为空）
    pub local_version: String,
}

/// npm 升级运行态：活动子进程与取消标记
#[derive(Default)]
pub struct NpmState {
    /// 进行中的升级子进程（包名 → OS PID），用于取消
    pub children: std::sync::Mutex<HashMap<String, u32>>,
    /// 取消标记（包名），升级结束时消费
    pub cancels: std::sync::Mutex<HashSet<String>>,
}

/// 全局串行化升级：并发 `npm install -g` 会在全局目录与缓存上竞争
static UPGRADE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// scoped 包名的 URL 编码：@scope/name → @scope%2Fname
fn encode_name(name: &str) -> String {
    name.replace('/', "%2F")
}

/// registry 基址规整：空白回退官方源，缺协议补 https，去尾部斜杠
fn normalize_registry(registry: &str) -> String {
    let r = registry.trim().trim_end_matches('/');
    if r.is_empty() {
        return crate::model::DEFAULT_NPM_REGISTRY.to_string();
    }
    if r.contains("://") {
        r.to_string()
    } else {
        format!("https://{r}")
    }
}

/// 从 dist-tags JSON 提取 latest
fn parse_latest(text: &str) -> Option<String> {
    let v: Value = serde_json::from_str(text).ok()?;
    v.get("latest")?
        .as_str()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// 读取目录下 package.json 的（name, version）；文件缺失/损坏/字段缺失返回 None
fn read_pkg(dir: &Path) -> Option<(String, String)> {
    let text = std::fs::read_to_string(dir.join("package.json")).ok()?;
    let v: Value = serde_json::from_str(&text).ok()?;
    let name = v.get("name")?.as_str()?.trim();
    let version = v.get("version")?.as_str()?.trim();
    if name.is_empty() || version.is_empty() {
        return None;
    }
    Some((name.to_string(), version.to_string()))
}

/// 目录名是否为扫描噪声（.bin、.package-lock、_npx、_cacache 等）
fn is_noise(name: &str) -> bool {
    name.starts_with('.') || name.starts_with('_')
}

/// 扫描全局 node_modules：直系子目录各为一个包，`@scope` 进一层；
/// 本地版本一律以 package.json 为准（天然规避 npm 别名包）。目录不可读时返回空表。
fn scan_root(root: &Path) -> Vec<NpmInfo> {
    let mut out = Vec::new();
    let Ok(top) = std::fs::read_dir(root) else {
        return out;
    };
    for entry in top.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let base = entry.file_name().to_string_lossy().to_string();
        if is_noise(&base) {
            continue;
        }
        if base.starts_with('@') {
            let Ok(scope) = std::fs::read_dir(&path) else {
                continue;
            };
            for se in scope.flatten() {
                let spath = se.path();
                if !spath.is_dir() {
                    continue;
                }
                let sub = se.file_name().to_string_lossy().to_string();
                if is_noise(&sub) {
                    continue;
                }
                if let Some((name, version)) = read_pkg(&spath) {
                    out.push(NpmInfo {
                        name,
                        version,
                        dir: spath.to_string_lossy().to_string(),
                    });
                }
            }
        } else if let Some((name, version)) = read_pkg(&path) {
            out.push(NpmInfo {
                name,
                version,
                dir: path.to_string_lossy().to_string(),
            });
        }
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

/// 扫描 npm 全局目录中的包
#[tauri::command]
pub async fn scan_npm(root: String) -> Result<Vec<NpmInfo>, String> {
    let root = PathBuf::from(root.trim());
    // 目录遍历放在工作线程，避免包多时阻塞主线程
    tokio::task::spawn_blocking(move || Ok(scan_root(&root)))
        .await
        .map_err(|e| format!("扫描线程失败: {e}"))?
}

/// 执行 `npm root -g`；GUI 进程直接 spawn npm 会因 npm.cmd 失败，
/// Windows 下经 cmd /C 调用并隐藏控制台窗口
fn run_npm_root() -> Result<std::process::Output, String> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("cmd")
            .args(["/C", "npm", "root", "-g"])
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
            .output()
            .map_err(|e| format!("未检测到 npm（{e}），请安装 Node.js 或在设置中手动填写全局目录"))
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::process::Command::new("npm")
            .args(["root", "-g"])
            .output()
            .map_err(|e| format!("未检测到 npm（{e}），请安装 Node.js 或在设置中手动填写全局目录"))
    }
}

fn detect_root_blocking(manual: &str) -> Result<String, String> {
    if !manual.is_empty() {
        return if Path::new(manual).is_dir() {
            Ok(manual.to_string())
        } else {
            Err("设置的全局目录不存在，请检查路径".into())
        };
    }
    let out = run_npm_root()?;
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if !out.status.success() || text.is_empty() {
        return Err("npm root -g 执行失败，请确认已安装 Node.js，或在设置中手动填写全局目录".into());
    }
    if !Path::new(&text).is_dir() {
        return Err(format!("npm 返回的全局目录不存在：{text}"));
    }
    Ok(text)
}

/// 解析 npm 全局目录：设置中手动指定的目录优先，否则执行 `npm root -g`
#[tauri::command]
pub async fn npm_detect_root(manual: String) -> Result<String, String> {
    let manual = manual.trim().to_string();
    tokio::task::spawn_blocking(move || detect_root_blocking(&manual))
        .await
        .map_err(|e| format!("探测线程失败: {e}"))?
}

/// 查询单个包的 dist-tags.latest
async fn fetch_latest(
    client: &reqwest::Client,
    registry: &str,
    name: &str,
) -> Result<String, String> {
    let url = format!("{registry}/-/package/{}/dist-tags", encode_name(name));
    let resp = client
        .get(&url)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| format!("请求 registry 失败: {e}"))?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| format!("读取响应失败: {e}"))?;
    if status == reqwest::StatusCode::NOT_FOUND {
        return Err("registry 未收录该包".into());
    }
    if !status.is_success() {
        return Err(format!("registry 返回 {status}"));
    }
    parse_latest(&text).ok_or_else(|| "响应缺少 latest 标签".into())
}

/// 批量检查 npm 全局包更新：逐包查 dist-tags，限并发，结果与入参同序
#[tauri::command]
pub async fn check_npm_updates(
    items: Vec<NpmRef>,
    settings: Settings,
) -> Result<Vec<NpmCheck>, String> {
    if items.is_empty() {
        return Ok(vec![]);
    }
    let registry = normalize_registry(&settings.npm_registry);
    let client = crate::net::client_with_ipv4_pref(&crate::net::host_of(&registry));
    // 直接构造 future 列表（不用闭包映射）：owned 值进 async block，
    // 规避 HRTB 闭包返回 async block 的 rustc 泛型化限制
    let mut futs = Vec::with_capacity(items.len());
    for it in &items {
        let name = it.name.clone();
        let client = client.clone();
        let registry = registry.clone();
        futs.push(async move { fetch_latest(&client, &registry, &name).await });
    }
    let latest: Vec<Result<String, String>> = stream::iter(futs)
        .buffered(CHECK_CONCURRENCY)
        .collect()
        .await;
    let mut out = Vec::with_capacity(items.len());
    for (it, res) in items.iter().zip(latest) {
        let mut chk = NpmCheck {
            name: it.name.clone(),
            local_version: it.local_version.clone(),
            latest_version: String::new(),
            has_update: false,
            checked_at: 0,
            error: String::new(),
        };
        match res {
            Ok(latest) => {
                chk.latest_version = latest.clone();
                chk.has_update = !latest.is_empty()
                    && !chk.local_version.trim().is_empty()
                    && compare(latest.trim(), chk.local_version.trim())
                        == std::cmp::Ordering::Greater;
            }
            Err(e) => chk.error = e,
        }
        out.push(chk);
    }
    Ok(out)
}

/// npm 包名白名单字符：防止拼进 shell 命令的注入风险
fn valid_pkg_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 214
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '@' | '/' | '.' | '_' | '-'))
}

/// 构造 `npm install -g <pkg> --no-fund --no-audit --loglevel=info`（GUI 进程须规避 npm.cmd 与窗口闪烁；
/// 管道模式下 npm 默认几乎静默，info 级让安装过程有逐行日志可展示）
fn spawn_upgrade(name: &str) -> Result<std::process::Child, String> {
    let pkg = format!("{name}@latest");
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("cmd")
            .args([
                "/C", "npm", "install", "-g", &pkg, "--no-fund", "--no-audit", "--loglevel=info",
            ])
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .stdin(std::process::Stdio::null())
            .spawn()
            .map_err(|e| format!("启动 npm 失败（{e}），请确认已安装 Node.js"))
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::process::Command::new("npm")
            .args([
                "install", "-g", &pkg, "--no-fund", "--no-audit", "--loglevel=info",
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .stdin(std::process::Stdio::null())
            .spawn()
            .map_err(|e| format!("启动 npm 失败（{e}），请确认已安装 Node.js"))
    }
}

/// 起一个读线程把 npm 输出逐行转发为 progressing 事件
fn spawn_reader<R: std::io::Read + Send + 'static>(
    pipe: R,
    app: AppHandle,
    name: String,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        for line in std::io::BufReader::new(pipe).lines().map_while(Result::ok) {
            let t = line.trim();
            if t.is_empty() {
                continue;
            }
            let _ = app.emit(
                UPGRADE_EVENT,
                NpmUpgradeProgress {
                    name: name.clone(),
                    status: "progressing".into(),
                    output: t.to_string(),
                    error: String::new(),
                    local_version: String::new(),
                },
            );
        }
    })
}

/// 升级完成后重读该包 package.json，取升级后的本地版本
fn installed_version_after_upgrade(manual_root: &str, name: &str) -> String {
    let Ok(root) = detect_root_blocking(manual_root) else {
        return String::new();
    };
    // "@scope/name" 逐段下钻，普通名只有一段
    let mut p = PathBuf::from(root);
    for seg in name.split('/') {
        p.push(seg);
    }
    read_pkg(&p).map(|(_, v)| v).unwrap_or_default()
}

/// 执行 `npm install -g <name>@latest`：输出经 npm-upgrade-progress 逐行推送，
/// 结束时推 done/error/cancelled。升级全程全局串行，避免 -g 目录竞争。
#[tauri::command]
pub async fn npm_upgrade(
    app: AppHandle,
    state: State<'_, NpmState>,
    name: String,
    manual_root: String,
) -> Result<(), String> {
    let name = name.trim().to_string();
    if !valid_pkg_name(&name) {
        return Err(format!("非法的包名：{name}"));
    }
    let manual_root = manual_root.trim().to_string();
    let _guard = UPGRADE_LOCK.lock().await;
    state.cancels.lock().unwrap().remove(&name);

    // spawn 失败直接 Err 走命令拒绝路径，前端本地标错
    let mut child = spawn_upgrade(&name)?;
    state
        .children
        .lock()
        .unwrap()
        .insert(name.clone(), child.id());
    let _ = app.emit(
        UPGRADE_EVENT,
        NpmUpgradeProgress {
            name: name.clone(),
            status: "preparing".into(),
            output: String::new(),
            error: String::new(),
            local_version: String::new(),
        },
    );
    // npm 管道模式日志稀疏：先推一条已启动，避免前端长时间停留在「正在准备」
    let _ = app.emit(
        UPGRADE_EVENT,
        NpmUpgradeProgress {
            name: name.clone(),
            status: "progressing".into(),
            output: format!("npm install -g {name}@latest 已启动"),
            error: String::new(),
            local_version: String::new(),
        },
    );

    // 进程等待是阻塞调用，放工作线程；读线程先于终态事件排空管道
    let app2 = app.clone();
    let name2 = name.clone();
    let result = tokio::task::spawn_blocking(move || {
        let mut readers = Vec::new();
        if let Some(out) = child.stdout.take() {
            readers.push(spawn_reader(out, app2.clone(), name2.clone()));
        }
        if let Some(err) = child.stderr.take() {
            // npm 的进度日志主要走 stderr，同样转发
            readers.push(spawn_reader(err, app2, name2));
        }
        let status = child.wait();
        for r in readers {
            let _ = r.join();
        }
        status.map(|s| s.success()).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("升级线程失败: {e}"))?;

    state.children.lock().unwrap().remove(&name);
    let cancelled = state.cancels.lock().unwrap().remove(&name);
    let emit_final = |status: &str, error: &str, local_version: &str| {
        let _ = app.emit(
            UPGRADE_EVENT,
            NpmUpgradeProgress {
                name: name.clone(),
                status: status.to_string(),
                output: String::new(),
                error: error.to_string(),
                local_version: local_version.to_string(),
            },
        );
    };
    match result {
        Ok(true) => {
            let local_version = installed_version_after_upgrade(&manual_root, &name);
            emit_final("done", "", &local_version);
        }
        Ok(false) => {
            if cancelled {
                emit_final("cancelled", "", "");
            } else {
                emit_final("error", "npm 退出码非 0，详见输出", "");
            }
        }
        Err(e) => emit_final("error", &e, ""),
    }
    Ok(())
}

/// 取消进行中的升级：杀整个进程树（cmd → node 子进程）
#[tauri::command]
pub async fn npm_cancel_upgrade(state: State<'_, NpmState>, name: String) -> Result<(), String> {
    let name = name.trim().to_string();
    state.cancels.lock().unwrap().insert(name.clone());
    let pid = state.children.lock().unwrap().remove(&name);
    if let Some(pid) = pid {
        let _ = tokio::task::spawn_blocking(move || kill_tree(pid)).await;
    }
    Ok(())
}

fn kill_tree(pid: u32) {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .creation_flags(0x0800_0000)
            .output();
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = std::process::Command::new("kill")
            .arg("-9")
            .arg(pid.to_string())
            .output();
    }
}

#[cfg(test)]
mod tests {
    use super::{encode_name, normalize_registry, parse_latest, scan_root, valid_pkg_name};
    use std::fs;
    use std::path::Path;

    /// 建一个一次性扫描样例目录：普通包、scope 包、噪声与坏文件
    fn setup_sample() -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("aurora-npm-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let pkg = |p: std::path::PathBuf, name: &str, ver: &str| {
            fs::create_dir_all(&p).unwrap();
            fs::write(
                p.join("package.json"),
                format!(r#"{{ "name": "{name}", "version": "{ver}" }}"#),
            )
            .unwrap();
        };
        pkg(root.join("typescript"), "typescript", "5.5.4");
        pkg(root.join("@types").join("node"), "@types/node", "22.5.0");
        pkg(root.join("_npx").join("cached"), "noise", "1.0.0");
        pkg(root.join(".bin"), "noise", "1.0.0");
        // 无 package.json 的目录、损坏的 package.json、散落文件
        fs::create_dir_all(root.join("empty-dir")).unwrap();
        fs::create_dir_all(root.join("broken")).unwrap();
        fs::write(root.join("broken").join("package.json"), "{oops").unwrap();
        fs::write(root.join("loose.txt"), "x").unwrap();
        root
    }

    #[test]
    fn scans_scope_and_skips_noise() {
        let root = setup_sample();
        let out = scan_root(&root);
        assert!(out.iter().all(|i| Path::new(&i.dir).is_dir()));
        let _ = fs::remove_dir_all(&root);
        let names: Vec<&str> = out.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["@types/node", "typescript"]);
        assert_eq!(out[0].version, "22.5.0");
    }

    #[test]
    fn encodes_scoped_name() {
        assert_eq!(encode_name("@types/node"), "@types%2Fnode");
        assert_eq!(encode_name("typescript"), "typescript");
    }

    #[test]
    fn validates_pkg_names() {
        assert!(valid_pkg_name("typescript"));
        assert!(valid_pkg_name("@types/node"));
        assert!(valid_pkg_name("npm-check-updates"));
        assert!(!valid_pkg_name(""));
        assert!(!valid_pkg_name("a;rm -rf"));
        assert!(!valid_pkg_name("a b"));
        assert!(!valid_pkg_name("$(calc)"));
    }

    #[test]
    fn parses_dist_tags() {
        let v = parse_latest(r#"{"latest":"2.0.1","next":"2.1.0-rc.1"}"#).unwrap();
        assert_eq!(v, "2.0.1");
        assert_eq!(parse_latest(r#"{"next":"1.0.0"}"#), None);
        assert_eq!(parse_latest("not-json"), None);
    }

    #[test]
    fn normalizes_registry() {
        assert_eq!(normalize_registry(""), "https://registry.npmjs.org");
        assert_eq!(
            normalize_registry("registry.npmmirror.com"),
            "https://registry.npmmirror.com"
        );
        assert_eq!(
            normalize_registry(" https://reg.example.com/ "),
            "https://reg.example.com"
        );
    }
}
