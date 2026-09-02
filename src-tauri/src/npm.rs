//! npm 全局包扫描与 registry 更新检查。
//!
//! 全局位置来自 `npm root -g`（可在设置中手动指定目录兜底，规避 GUI 进程
//! PATH 缺失问题），包清单读取各包目录下 package.json 的 name/version；
//! 最新版本查询 registry 的 `/-/package/<name>/dist-tags` 端点取 `latest`。

use crate::model::{NpmCheck, NpmInfo, NpmRef, Settings};
use crate::version::compare;
use futures_util::stream::{self, StreamExt};
use serde_json::Value;
use std::path::{Path, PathBuf};

/// registry 并发请求数上限（registry 无批量接口，逐包查询）
const CHECK_CONCURRENCY: usize = 6;

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

#[cfg(test)]
mod tests {
    use super::{encode_name, normalize_registry, parse_latest, scan_root};
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
