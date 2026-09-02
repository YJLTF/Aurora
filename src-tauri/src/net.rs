use crate::model::{
    score_asset, Asset, CheckOutcome, DownloadProgress, SelfUpdateInfo, Settings, SoftwareItem,
    Source,
};
use crate::version::compare;
use futures_util::StreamExt;
use regex::Regex;
use serde_json::Value;
use std::collections::HashMap;
use std::net::ToSocketAddrs;
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, State};
use tokio::io::AsyncWriteExt;

/// 下载取消标记
#[derive(Default)]
pub struct AppState {
    pub cancels: std::sync::Mutex<HashMap<String, Arc<AtomicBool>>>,
    /// 下载暂停标记
    pub pauses: std::sync::Mutex<HashMap<String, Arc<AtomicBool>>>,
}

/// 瞬时错误自动重试次数（不含首次）
const MAX_AUTO_RETRIES: u32 = 2;

fn base_client_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        .user_agent(concat!(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Aurora/",
            env!("CARGO_PKG_VERSION")
        ))
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(15))
}

fn http_client() -> reqwest::Client {
    base_client_builder().build().unwrap_or_default()
}

/// 构建"指定域名优先走 IPv4"的客户端：本机 IPv6 半通（TCP 可连但数据黑洞）时兜底。
/// 解析不到 IPv4（纯 IPv6/代理场景）时退回常规构建，不影响原有通路。
pub(crate) fn client_with_ipv4_pref(host: &str) -> reqwest::Client {
    let builder = base_client_builder();
    let Ok(addrs) = (host, 443u16).to_socket_addrs() else {
        return builder.build().unwrap_or_default();
    };
    let v4: Vec<std::net::SocketAddr> = addrs.filter(|a| a.is_ipv4()).collect();
    if v4.is_empty() {
        return builder.build().unwrap_or_default();
    }
    builder.resolve_to_addrs(host, &v4).build().unwrap_or_default()
}

/// 从 URL 提取主机名（用于 IPv4 优先解析）
pub(crate) fn host_of(url: &str) -> String {
    let rest = url.split("://").nth(1).unwrap_or("");
    rest.split(['/']).next().unwrap_or("").to_string()
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
            notes: String::new(),
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
    let notes = v
        .get("body")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    Ok(CheckOutcome {
        version,
        release_url,
        assets,
        suggested,
        has_update: None,
        notes,
        error: String::new(),
    })
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
        notes: String::new(),
        error: String::new(),
    })
}

/// 检查 Aurora 自身的更新（GitHub Releases，仓库见 model::SELF_REPO）
pub async fn check_self_update(current_version: &str, settings: &Settings) -> SelfUpdateInfo {
    let mut info = SelfUpdateInfo {
        current_version: current_version.to_string(),
        latest_version: String::new(),
        has_update: false,
        release_url: String::new(),
        notes: String::new(),
        assets: vec![],
        suggested: 0,
        error: String::new(),
    };
    match check_github(crate::model::SELF_REPO, settings).await {
        Ok(o) => {
            info.latest_version = o.version.clone();
            info.release_url = o.release_url;
            info.notes = o.notes;
            info.assets = o.assets;
            info.suggested = o.suggested;
            info.has_update = !o.version.trim().is_empty()
                && compare(o.version.trim(), current_version.trim()) == std::cmp::Ordering::Greater;
        }
        Err(e) => info.error = e,
    }
    info
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
    // VSCode 插件下载传 true：目标 CDN 在本机可能有半通 IPv6
    prefer_ipv4: Option<bool>,
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
    let pause = Arc::new(AtomicBool::new(false));
    state
        .cancels
        .lock()
        .unwrap()
        .insert(item_id.clone(), cancel.clone());
    state
        .pauses
        .lock()
        .unwrap()
        .insert(item_id.clone(), pause.clone());

    let display_name = final_path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| safe_name.clone());
    let client = if prefer_ipv4.unwrap_or(false) {
        client_with_ipv4_pref(&host_of(&real_url))
    } else {
        http_client()
    };

    // 自动重试：网络中断等瞬时错误从 .part 断点续传，暂停/取消立即返回
    let mut attempt = 0u32;
    let result = loop {
        match pump(
            &app, &client, &real_url, &tmp_path, &final_path, &item_id, &display_name, &cancel,
            &pause,
        )
        .await
        {
            Ok(p) => break Ok(p),
            Err(e) if e == "下载已取消" || e == "下载已暂停" => break Err(e),
            Err(e) => {
                if attempt >= MAX_AUTO_RETRIES {
                    break Err(e);
                }
                attempt += 1;
                // 退避等待，期间设置的取消/暂停会在下一轮 pump 顶部生效
                let backoff = u64::from(attempt) * 800;
                let mut waited = 0u64;
                while waited < backoff
                    && !cancel.load(AtomicOrdering::Relaxed)
                    && !pause.load(AtomicOrdering::Relaxed)
                {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    waited += 100;
                }
            }
        }
    };

    state.cancels.lock().unwrap().remove(&item_id);
    state.pauses.lock().unwrap().remove(&item_id);
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
    pause: &AtomicBool,
) -> Result<String, String> {
    // 已有分片则从断点续传；分片越界（416，资源可能已更新）时删除分片从头下
    let mut part_size = part_file_size(tmp);
    let resp = loop {
        let mut req = client.get(url);
        if part_size > 0 {
            req = req.header(reqwest::header::RANGE, format!("bytes={part_size}-"));
        }
        let resp = match req.send().await {
            Ok(r) => r,
            Err(e) => return Err(format!("连接下载地址失败: {e}")),
        };
        let status = resp.status();
        if status == reqwest::StatusCode::RANGE_NOT_SATISFIABLE && part_size > 0 {
            let _ = tokio::fs::remove_file(tmp).await;
            part_size = 0;
            continue;
        }
        if !status.is_success() {
            return Err(format!("下载地址返回 {status}"));
        }
        break resp;
    };

    // 服务器不支持 Range 时会返回 200 全量，此时覆盖重写
    let resume = part_size > 0 && resp.status() == reqwest::StatusCode::PARTIAL_CONTENT;
    let base = if resume { part_size } else { 0 };
    let total = resp.content_length().map(|l| l + base).unwrap_or(0);
    let mut stream = resp.bytes_stream();
    let mut file = if resume {
        tokio::fs::OpenOptions::new().append(true).open(tmp).await
    } else {
        tokio::fs::File::create(tmp).await
    }
    .map_err(|e| format!("创建文件失败: {e}"))?;
    let mut received: u64 = base;
    let mut last_emit = Instant::now() - Duration::from_millis(500);

    loop {
        if cancel.load(AtomicOrdering::Relaxed) {
            drop(file);
            let _ = tokio::fs::remove_file(tmp).await;
            send_progress(app, item_id, name, received, total, "cancelled", "", "已取消");
            return Err("下载已取消".into());
        }
        // 暂停保留 .part 分片，供「继续」断点续传
        if pause.load(AtomicOrdering::Relaxed) {
            drop(file);
            let part = tmp.to_string_lossy().to_string();
            send_progress(app, item_id, name, received, total, "paused", &part, "已暂停");
            return Err("下载已暂停".into());
        }
        let chunk = match stream.next().await {
            Some(Ok(b)) => b,
            Some(Err(e)) => {
                drop(file);
                // 保留分片供自动重试/手动重试续传
                return Err(format!("下载中断: {e}"));
            }
            None => break,
        };
        if let Err(e) = file.write_all(&chunk).await {
            drop(file);
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

fn part_file_size(p: &std::path::Path) -> u64 {
    std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)
}

/// 暂停指定下载：保留 .part 分片，可再次调用 download_file 断点续传
#[tauri::command]
pub fn pause_download(state: State<'_, AppState>, item_id: String) {
    if let Some(flag) = state.pauses.lock().unwrap().get(&item_id) {
        flag.store(true, AtomicOrdering::Relaxed);
    }
}

#[tauri::command]
pub fn cancel_download(state: State<'_, AppState>, item_id: String) {
    if let Some(flag) = state.cancels.lock().unwrap().get(&item_id) {
        flag.store(true, AtomicOrdering::Relaxed);
    }
}

/// 列出下载目录中的文件名，供前端匹配“最新版本是否已下载”。
/// 目录不存在或不可读时返回空列表，不让检查流程报错。
/// async：目录遍历放在工作线程，避免大目录阻塞主线程。
#[tauri::command]
pub async fn list_downloads(dest_dir: String) -> Result<Vec<String>, String> {
    let dir = std::path::PathBuf::from(dest_dir.trim());
    let mut out = Vec::new();
    let rd = match std::fs::read_dir(&dir) {
        Ok(rd) => rd,
        Err(_) => return Ok(out),
    };
    for entry in rd.flatten() {
        let Ok(ft) = entry.file_type() else { continue };
        if !ft.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let lower = name.to_lowercase();
        if name.starts_with('.')
            || lower.ends_with(".part")
            || lower.ends_with(".tmp")
            || lower.ends_with(".crdownload")
        {
            continue;
        }
        out.push(name);
    }
    out.sort();
    Ok(out)
}
