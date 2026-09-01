use crate::model::{
    score_asset, Asset, CheckOutcome, DownloadProgress, Settings, SoftwareItem, Source,
};
use crate::version::compare;
use futures_util::StreamExt;
use regex::Regex;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, State};
use tokio::io::AsyncWriteExt;

/// 下载取消标记
#[derive(Default)]
pub struct AppState {
    pub cancels: std::sync::Mutex<HashMap<String, Arc<AtomicBool>>>,
}

fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) Aurora/0.1")
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(15))
        .build()
        .unwrap_or_default()
}

/// 检测单个软件的最新版本
pub async fn check_item(item: &SoftwareItem, settings: &Settings) -> CheckOutcome {
    let r = match &item.source {
        Source::Github { repo } => check_github(repo, settings).await,
        Source::Html { check_url, version_regex, download_template } => {
            check_html(check_url, version_regex, download_template).await
        }
    };
    match r {
        Ok(mut o) => {
            o.has_update = if item.installed_version.trim().is_empty() || o.version.is_empty() {
                None
            } else {
                Some(compare(&o.version, item.installed_version.trim()) == std::cmp::Ordering::Greater)
            };
            o
        }
        Err(e) => CheckOutcome {
            version: String::new(),
            release_url: String::new(),
            assets: vec![],
            suggested: 0,
            has_update: None,
            error: e,
        },
    }
}

type CheckResult = Result<CheckOutcome, String>;

async fn github_request(
    client: &reqwest::Client,
    settings: &Settings,
    url: &str,
) -> Result<Value, String> {
    let mut req = client
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28");
    let token = settings.github_token.trim();
    if !token.is_empty() {
        req = req.bearer_auth(token);
    }
    let resp = req.send().await.map_err(|e| format!("请求失败: {e}"))?;
    let status = resp.status();
    let body = resp.text().await.map_err(|e| format!("读取响应失败: {e}"))?;
    if !status.is_success() {
        let detail = if body.len() < 300 { format!(": {body}") } else { String::new() };
        return Err(format!("GitHub API 返回 {status}{detail}"));
    }
    serde_json::from_str(&body).map_err(|e| format!("解析 JSON 失败: {e}"))
}

fn outcome_from_release(v: &Value) -> CheckResult {
    let tag = v
        .get("tag_name")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if tag.is_empty() {
        return Err("Release 数据缺少 tag_name".into());
    }
    let version = tag.strip_prefix('v').unwrap_or(&tag).to_string();
    let release_url = v
        .get("html_url")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let mut assets: Vec<Asset> = Vec::new();
    if let Some(arr) = v.get("assets").and_then(|x| x.as_array()) {
        for a in arr {
            let name = a.get("name").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let url = a
                .get("browser_download_url")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            let size = a.get("size").and_then(|x| x.as_u64()).unwrap_or(0);
            if !name.is_empty() && !url.is_empty() {
                assets.push(Asset { name, url, size });
            }
        }
    }
    let suggested = assets
        .iter()
        .enumerate()
        .max_by_key(|(_, a)| score_asset(&a.name))
        .map(|(i, _)| i as u32)
        .unwrap_or(0);
    Ok(CheckOutcome { version, release_url, assets, suggested, has_update: None, error: String::new() })
}

async fn check_github(repo: &str, settings: &Settings) -> CheckResult {
    let repo = repo.trim().trim_matches('/').trim_end_matches(".git");
    // 容错：粘贴完整 releases 页面链接时提取 owner/name
    let repo = if let Some(idx) = repo.find("github.com/") {
        repo[idx + "github.com/".len()..].to_string()
    } else {
        repo.to_string()
    };
    let parts: Vec<&str> = repo
        .split(|c| c == '/' || c == '#' || c == '?')
        .filter(|s| !s.is_empty())
        .collect();
    if parts.is_empty() {
        return Err("仓库地址为空".into());
    }
    if parts.len() < 2 {
        return Err(format!("仓库格式应为 owner/name，收到的是 {repo:?}"));
    }
    let repo = format!("{}/{}", parts[0], parts[1]);

    let client = http_client();
    let base = settings.github_api_base.trim().trim_end_matches('/');
    let latest_url = format!("{base}/repos/{repo}/releases/latest");
    match github_request(&client, settings, &latest_url).await {
        Ok(v) => outcome_from_release(&v),
        Err(e) => {
            // 部分仓库没有“最新正式版”，退回按列表取第一条非草稿（优先非预发布）
            let list_url = format!("{base}/repos/{repo}/releases?per_page=20");
            let v = github_request(&client, settings, &list_url)
                .await
                .map_err(|_| e)?;
            let arr = v.as_array().ok_or("Release 列表格式异常")?;
            let is_draft = |r: &Value| r.get("draft").and_then(|d| d.as_bool()).unwrap_or(false);
            let is_pre = |r: &Value| {
                r.get("prerelease").and_then(|d| d.as_bool()).unwrap_or(false)
            };
            let pick = arr
                .iter()
                .find(|r| !is_draft(r) && !is_pre(r))
                .or_else(|| arr.iter().find(|r| !is_draft(r)))
                .ok_or("该仓库没有可用的 Release")?;
            outcome_from_release(pick)
        }
    }
}

async fn check_html(check_url: &str, version_regex: &str, download_template: &str) -> CheckResult {
    let check_url = check_url.trim();
    if check_url.is_empty() {
        return Err("检查地址为空".into());
    }
    let client = http_client();
    let resp = client
        .get(check_url)
        .send()
        .await
        .map_err(|e| format!("请求失败: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("页面返回 {}", resp.status()));
    }
    let body = resp.text().await.map_err(|e| format!("读取页面失败: {e}"))?;
    let re = Regex::new(version_regex).map_err(|e| format!("正则表达式无效: {e}"))?;
    let caps = re
        .captures(&body)
        .ok_or("页面中未匹配到版本号，请检查正则表达式")?;
    let m = caps
        .get(1)
        .or_else(|| caps.get(0))
        .ok_or("正则表达式缺少捕获组")?;
    let version = m.as_str().trim().to_string();

    let mut assets: Vec<Asset> = Vec::new();
    let tpl = download_template.trim();
    if !tpl.is_empty() {
        let url = tpl.replace("{version}", &version);
        let raw = url.rsplit('/').next().unwrap_or("下载包");
        let name = if raw.contains('.') { raw } else { "安装包" };
        assets.push(Asset { name: name.to_string(), url, size: 0 });
    }
    Ok(CheckOutcome {
        version,
        release_url: check_url.to_string(),
        assets,
        suggested: 0,
        has_update: None,
        error: String::new(),
    })
}

fn send_progress(
    app: &AppHandle,
    item_id: &str,
    file_name: &str,
    received: u64,
    total: u64,
    status: &str,
    path: &str,
    error: &str,
) {
    let _ = app.emit(
        "download-progress",
        DownloadProgress {
            item_id: item_id.to_string(),
            file_name: file_name.to_string(),
            received,
            total,
            status: status.to_string(),
            path: path.to_string(),
            error: error.to_string(),
        },
    );
}

fn sanitize_name(name: &str) -> String {
    let bad = ['<', '>', ':', '"', '|', '?', '*', '/', '\\'];
    let cleaned: String = name.chars().map(|c| if bad.contains(&c) { '_' } else { c }).collect();
    let cleaned = cleaned.trim().trim_end_matches('.').to_string();
    if cleaned.is_empty() { "download.bin".into() } else { cleaned }
}

#[tauri::command]
pub async fn download_file(
    app: AppHandle,
    state: State<'_, AppState>,
    item_id: String,
    url: String,
    file_name: String,
    dest_dir: String,
    proxy_prefix: String,
) -> Result<String, String> {
    let dest_dir = dest_dir.trim().to_string();
    if dest_dir.is_empty() {
        return Err("下载目录未设置，请先在设置中填写".into());
    }
    let dir = std::path::PathBuf::from(&dest_dir);
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| format!("创建下载目录失败: {e}"))?;

    // 国内网络可配置加速前缀，仅作用于 GitHub 直链
    let real_url = if !proxy_prefix.trim().is_empty() && url.starts_with("https://github.com/") {
        format!("{}{}", proxy_prefix.trim().trim_end_matches('/'), url)
    } else {
        url.clone()
    };

    let safe_name = sanitize_name(&file_name);
    let mut final_path = dir.join(&safe_name);
    let mut n = 1u32;
    while final_path.exists() {
        let stem = final_path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| safe_name.clone());
        let ext = final_path
            .extension()
            .map(|s| s.to_string_lossy().to_string());
        let name2 = match ext {
            Some(e) => format!("{stem} ({n}).{e}"),
            None => format!("{stem} ({n})"),
        };
        final_path = dir.join(name2);
        n += 1;
    }
    let tmp_path = final_path.with_extension("part");

    let cancel = Arc::new(AtomicBool::new(false));
    state
        .cancels
        .lock()
        .unwrap()
        .insert(item_id.clone(), cancel.clone());

    let display_name = final_path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| safe_name.clone());
    let client = http_client();
    let result = pump(
        &app, &client, &real_url, &tmp_path, &final_path, &item_id, &display_name, &cancel,
    )
    .await;

    state.cancels.lock().unwrap().remove(&item_id);
    result
}

async fn pump(
    app: &AppHandle,
    client: &reqwest::Client,
    url: &str,
    tmp: &std::path::Path,
    final_path: &std::path::Path,
    item_id: &str,
    name: &str,
    cancel: &AtomicBool,
) -> Result<String, String> {
    let resp = match client.get(url).send().await {
        Ok(r) => r,
        Err(e) => return Err(format!("连接下载地址失败: {e}")),
    };
    if !resp.status().is_success() {
        return Err(format!("下载地址返回 {}", resp.status()));
    }
    let total = resp.content_length().unwrap_or(0);
    let mut stream = resp.bytes_stream();
    let mut file = match tokio::fs::File::create(tmp).await {
        Ok(f) => f,
        Err(e) => return Err(format!("创建文件失败: {e}")),
    };
    let mut received: u64 = 0;
    let mut last_emit = Instant::now() - Duration::from_millis(500);

    loop {
        if cancel.load(AtomicOrdering::Relaxed) {
            drop(file);
            let _ = tokio::fs::remove_file(tmp).await;
            send_progress(app, item_id, name, received, total, "cancelled", "", "已取消");
            return Err("下载已取消".into());
        }
        let chunk = match stream.next().await {
            Some(Ok(b)) => b,
            Some(Err(e)) => {
                drop(file);
                let _ = tokio::fs::remove_file(tmp).await;
                return Err(format!("下载中断: {e}"));
            }
            None => break,
        };
        if let Err(e) = file.write_all(&chunk).await {
            drop(file);
            let _ = tokio::fs::remove_file(tmp).await;
            return Err(format!("写入文件失败: {e}"));
        }
        received += chunk.len() as u64;
        if last_emit.elapsed() >= Duration::from_millis(120) {
            send_progress(app, item_id, name, received, total, "progressing", "", "");
            last_emit = Instant::now();
        }
    }
    if let Err(e) = file.flush().await {
        drop(file);
        let _ = tokio::fs::remove_file(tmp).await;
        return Err(format!("写入文件失败: {e}"));
    }
    drop(file);
    tokio::fs::rename(tmp, final_path)
        .await
        .map_err(|e| format!("保存文件失败: {e}"))?;
    let final_str = final_path.to_string_lossy().to_string();
    send_progress(app, item_id, name, received, total, "done", &final_str, "");
    Ok(final_str)
}

#[tauri::command]
pub fn cancel_download(state: State<'_, AppState>, item_id: String) {
    if let Some(flag) = state.cancels.lock().unwrap().get(&item_id) {
        flag.store(true, AtomicOrdering::Relaxed);
    }
}
