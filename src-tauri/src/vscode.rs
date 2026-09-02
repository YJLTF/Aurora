//! VSCode 离线备份（.vsix）扫描与 Marketplace 更新检查。
//!
//! 插件列表与版本编码在备份文件名里（`publisher.extension-版本[-平台].vsix`，
//! 按扩展包分子文件夹存放），因此递归扫描文件名即可得到清单；
//! 最新版本通过 VS Marketplace 的 extensionquery 接口批量查询。

use crate::model::{Settings, VsixCheck, VsixInfo, VsixRef};
use crate::version::compare;
use regex::Regex;
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::OnceLock;

const MARKETPLACE_URL: &str =
    "https://marketplace.visualstudio.com/_apis/public/gallery/extensionquery";
const VSIX_ASSET: &str = "Microsoft.VisualStudio.Services.VSIXPackage";

fn stem_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^(.+?)-(\d[\w.]*)$").unwrap())
}

/// 剥离文件名尾部的目标平台后缀（win32-x64 / linux-arm64 / universal 等）
fn strip_target(stem: &str) -> (&str, &str) {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(
            r"(?i)-((?:win32|linux|darwin|alpine|android)-(?:x64|arm64|ia32|armhf|arm)|universal|web)$",
        )
        .unwrap()
    });
    match re.captures(stem) {
        Some(c) => {
            let whole = c.get(0).unwrap();
            (&stem[..whole.start()], whole.as_str().trim_start_matches('-'))
        }
        None => (stem, ""),
    }
}

/// 从去掉 .vsix 后缀的文件名解析插件 ID、版本与平台后缀。
/// 先剥平台后缀，再把最后一个数字开头的连字符段作为版本起点，
/// 因此 ID 本身含连字符（如 vscode-language-pack-zh-hans）也能正确切分。
fn parse_stem(stem: &str) -> Option<(String, String, String)> {
    let (stem, target) = strip_target(stem);
    let caps = stem_re().captures(stem)?;
    let id = caps.get(1).unwrap().as_str().to_string();
    let version = caps.get(2).unwrap().as_str().to_string();
    if !id.contains('.') {
        return None;
    }
    Some((id, version, target.to_string()))
}

/// 递归扫描目录下的 .vsix 文件；目录不存在或不可读时返回空列表
#[tauri::command]
pub fn list_vsix(dir: String) -> Result<Vec<VsixInfo>, String> {
    let root = Path::new(dir.trim());
    let mut out = Vec::new();
    if !root.is_dir() {
        return Ok(out);
    }
    let mut queue = VecDeque::from([root.to_path_buf()]);
    while let Some(d) = queue.pop_front() {
        let rd = match std::fs::read_dir(&d) {
            Ok(rd) => rd,
            Err(_) => continue,
        };
        for entry in rd.flatten() {
            let path = entry.path();
            let Ok(ft) = entry.file_type() else { continue };
            if ft.is_dir() {
                queue.push_back(path);
                continue;
            }
            if !ft.is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || !name.to_lowercase().ends_with(".vsix") {
                continue;
            }
            let Some((id, version, target)) = parse_stem(&name[..name.len() - 5]) else {
                continue;
            };
            out.push(VsixInfo {
                id,
                version,
                target,
                file_name: name,
                dir: d.to_string_lossy().to_string(),
            });
        }
    }
    out.sort_by(|a, b| a.id.cmp(&b.id).then(b.version.cmp(&a.version)));
    Ok(out)
}

/// 读取本机 VSCode 已安装扩展的版本表（id 小写 → 版本）；读不到时返回空表
#[tauri::command]
pub fn read_installed_extensions() -> Result<HashMap<String, String>, String> {
    let mut map = HashMap::new();
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_default();
    if home.is_empty() {
        return Ok(map);
    }
    let path = Path::new(&home)
        .join(".vscode")
        .join("extensions")
        .join("extensions.json");
    let Ok(text) = std::fs::read_to_string(path) else {
        return Ok(map);
    };
    let Ok(v) = serde_json::from_str::<Value>(&text) else {
        return Ok(map);
    };
    if let Some(arr) = v.as_array() {
        for e in arr {
            let id = e
                .get("identifier")
                .and_then(|i| i.get("id"))
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .trim()
                .to_lowercase();
            let ver = e
                .get("version")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if !id.is_empty() && !ver.is_empty() {
                map.insert(id, ver);
            }
        }
    }
    Ok(map)
}

/// 批量检查插件更新：一次 Marketplace 查询取全部最新版本与下载直链
#[tauri::command]
pub async fn check_vscode_updates(
    items: Vec<VsixRef>,
    settings: Settings,
) -> Result<Vec<VsixCheck>, String> {
    let _ = &settings;
    if items.is_empty() {
        return Ok(vec![]);
    }
    let ids: Vec<String> = items.iter().map(|i| i.id.clone()).collect();
    let latest = query_marketplace(&ids).await?;
    let mut out = Vec::with_capacity(items.len());
    for it in items {
        let mut chk = VsixCheck {
            id: it.id.clone(),
            local_version: it.local_version,
            latest_version: String::new(),
            download_url: String::new(),
            has_update: false,
            checked_at: 0,
            error: String::new(),
        };
        match latest.get(&it.id.to_lowercase()) {
            Some(ext) => match pick_latest(ext, &it.target) {
                Some((version, url)) => {
                    chk.latest_version = version.clone();
                    chk.download_url = url;
                    chk.has_update = !version.is_empty()
                        && !chk.local_version.trim().is_empty()
                        && compare(version.trim(), chk.local_version.trim())
                            == std::cmp::Ordering::Greater;
                }
                None => chk.error = "响应中缺少版本信息".into(),
            },
            None => chk.error = "Marketplace 未收录该插件".into(),
        }
        out.push(chk);
    }
    Ok(out)
}

/// 查询 Marketplace，返回 小写插件ID → 原始 extension 结果
async fn query_marketplace(ids: &[String]) -> Result<HashMap<String, Value>, String> {
    let client = crate::net::client_with_ipv4_pref("marketplace.visualstudio.com");
    let criteria: Vec<Value> = ids
        .iter()
        .map(|id| json!({ "filterType": 7, "value": id }))
        .collect();
    let body = json!({
        "filters": [{
            "criteria": criteria,
            "pageNumber": 1,
            "pageSize": ids.len().max(1),
            "sortBy": 0,
            "sortOrder": 0,
        }],
        "assetTypes": [VSIX_ASSET],
        // IncludeFiles | IncludeVersionProperties | IncludeAssetUri | IncludeStatistics | IncludeVersionTags
        "flags": 914,
    });
    let resp = client
        .post(MARKETPLACE_URL)
        .header("Accept", "application/json; api-version=3.0-preview.1")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("请求 Marketplace 失败: {e}"))?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| format!("读取响应失败: {e}"))?;
    if !status.is_success() {
        return Err(format!("Marketplace 返回 {status}"));
    }
    let v: Value =
        serde_json::from_str(&text).map_err(|e| format!("解析 Marketplace 响应失败: {e}"))?;
    let mut map = HashMap::new();
    let exts = v
        .pointer("/results/0/extensions")
        .and_then(|x| x.as_array())
        .ok_or("Marketplace 响应格式异常")?;
    for ext in exts {
        let id = format!(
            "{}.{}",
            ext.pointer("/publisher/publisherName")
                .and_then(|x| x.as_str())
                .unwrap_or(""),
            ext.get("extensionName")
                .and_then(|x| x.as_str())
                .unwrap_or("")
        );
        if id != "." {
            map.insert(id.to_lowercase(), ext.clone());
        }
    }
    Ok(map)
}

/// 从 extension 结果中按目标平台挑出最新版本与下载直链。
/// 匹配顺序：与本地备份一致的 targetPlatform → 通用包（无平台标记）→
/// win32-x64（本应用面向 Windows，无平台标记的旧备份也应优先拿 Windows 包）→ 列表第一条。
fn pick_latest(ext: &Value, want_target: &str) -> Option<(String, String)> {
    let publisher = ext
        .pointer("/publisher/publisherName")
        .and_then(|x| x.as_str())
        .unwrap_or("");
    let name = ext.get("extensionName").and_then(|x| x.as_str()).unwrap_or("");
    let versions = ext.get("versions")?.as_array()?;
    let target_of = |v: &Value| {
        v.get("targetPlatform")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string()
    };
    let url_of = |v: &Value, version: &str| {
        files_url(v).unwrap_or_else(|| fallback_url(publisher, name, version))
    };
    let find = |pred: &dyn Fn(&Value) -> bool| {
        versions
            .iter()
            .find(|v| pred(v))
            .and_then(|v| v.get("version")?.as_str().map(|ver| (ver.to_string(), url_of(v, ver))))
    };
    find(&|v: &Value| !want_target.is_empty() && target_of(v) == want_target)
        .or_else(|| find(&|v: &Value| target_of(v).is_empty() || target_of(v) == "universal"))
        .or_else(|| find(&|v: &Value| target_of(v) == "win32-x64"))
        .or_else(|| {
            versions
                .first()
                .and_then(|v| v.get("version")?.as_str().map(|ver| (ver.to_string(), url_of(v, ver))))
        })
}

fn files_url(version: &Value) -> Option<String> {
    version
        .get("files")?
        .as_array()?
        .iter()
        .find(|f| f.get("assetType").and_then(|a| a.as_str()) == Some(VSIX_ASSET))
        .and_then(|f| f.get("source"))
        .and_then(|s| s.as_str())
        .map(|s| s.to_string())
}

fn fallback_url(publisher: &str, name: &str, version: &str) -> String {
    format!(
        "https://marketplace.visualstudio.com/_apis/public/gallery/publishers/{publisher}/vsextensions/{name}/{version}/vspackage"
    )
}

#[cfg(test)]
mod tests {
    use super::parse_stem;

    #[test]
    fn parses_plain_vsix() {
        let (id, ver, target) = parse_stem("redhat.java-1.55.0").unwrap();
        assert_eq!((id.as_str(), ver.as_str(), target.as_str()), ("redhat.java", "1.55.0", ""));
    }

    #[test]
    fn parses_platform_suffix() {
        let (id, ver, target) = parse_stem("ms-python.python-2026.4.0-win32-x64").unwrap();
        assert_eq!(
            (id.as_str(), ver.as_str(), target.as_str()),
            ("ms-python.python", "2026.4.0", "win32-x64")
        );
    }

    #[test]
    fn parses_id_with_hyphens_and_timestamp() {
        let (id, ver, _) = parse_stem("ms-ceintl.vscode-language-pack-zh-hans-1.131.2026082318").unwrap();
        assert_eq!(id, "ms-ceintl.vscode-language-pack-zh-hans");
        assert_eq!(ver, "1.131.2026082318");
    }

    #[test]
    fn rejects_names_without_version() {
        assert!(parse_stem("eslint").is_none());
        assert!(parse_stem("no-version-here").is_none());
    }
}
