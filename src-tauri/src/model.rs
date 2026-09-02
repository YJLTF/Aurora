use serde::{Deserialize, Serialize};

/// Aurora 自身发布仓库（用于自更新检查）
pub const SELF_REPO: &str = "YJLTF/Aurora";

fn default_true() -> bool {
    true
}

/// 全局设置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub download_dir: String,
    pub github_api_base: String,
    /// 下载加速前缀（仅对 github.com 链接生效），如 https://gh-proxy.com/
    pub download_proxy: String,
    /// 可选的 GitHub Token，避免 API 限流
    pub github_token: String,
    /// VSCode 离线 vsix 备份目录
    pub vscode_dir: String,
    /// 启动时自动检查 Aurora 自身更新
    #[serde(default = "default_true")]
    pub auto_check_self: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            download_dir: String::new(),
            github_api_base: "https://api.github.com".into(),
            download_proxy: String::new(),
            github_token: String::new(),
            vscode_dir: String::new(),
            auto_check_self: true,
        }
    }
}

/// 更新来源
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum Source {
    /// GitHub Releases：通过 GitHub API 获取最新版本与附件
    Github { repo: String },
    /// 任意页面/接口：用正则从响应文本中提取版本号
    Html {
        check_url: String,
        version_regex: String,
        /// 可选，支持 {version} 占位符的直链模板
        download_template: String,
    },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SoftwareItem {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub source: Source,
    pub homepage: String,
    pub notes: String,
    /// 本地已安装版本
    pub installed_version: String,
    /// 最近一次检测到的最新版本
    pub latest_version: String,
    pub release_url: String,
    pub assets: Vec<Asset>,
    /// 建议下载的附件下标（按 Windows 相关性打分）
    pub suggested: u32,
    /// 最近检查时间（epoch 毫秒，0 表示从未检查）
    pub checked_at: u64,
    pub last_error: String,
}

impl Default for SoftwareItem {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            icon: String::from("📦"),
            source: Source::Github { repo: String::new() },
            homepage: String::new(),
            notes: String::new(),
            installed_version: String::new(),
            latest_version: String::new(),
            release_url: String::new(),
            assets: Vec::new(),
            suggested: 0,
            checked_at: 0,
            last_error: String::new(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    pub settings: Settings,
    pub items: Vec<SoftwareItem>,
    /// VSCode 插件最近一次检查结果（跨会话恢复，重新扫描时按新本地版本重算）
    pub vscode_checks: Vec<VsixCheck>,
}

/// 单次检测结果
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckOutcome {
    pub version: String,
    pub release_url: String,
    pub assets: Vec<Asset>,
    pub suggested: u32,
    /// Some(true) 表示最新版本大于本地已登记版本；未登记本地版本时为 None
    pub has_update: Option<bool>,
    /// Release 说明文本（GitHub body），来源无说明时为空
    pub notes: String,
    pub error: String,
}

/// 应用自身信息
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub repo: String,
}

/// Aurora 自身更新检查结果
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelfUpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub has_update: bool,
    pub release_url: String,
    pub notes: String,
    pub assets: Vec<Asset>,
    pub suggested: u32,
    pub error: String,
}

/// 备份目录中的一个 .vsix 文件
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VsixInfo {
    /// publisher.extension
    pub id: String,
    pub version: String,
    /// 目标平台后缀（win32-x64 等），通用包为空
    pub target: String,
    pub file_name: String,
    /// 文件所在目录（绝对路径）
    pub dir: String,
}

/// 参与检查的插件（去重后的 ID + 平台 + 本地版本）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VsixRef {
    pub id: String,
    pub target: String,
    pub local_version: String,
}

/// 单个插件的 Marketplace 检查结果
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VsixCheck {
    pub id: String,
    pub local_version: String,
    pub latest_version: String,
    /// 最新版 vsix 下载直链
    pub download_url: String,
    pub has_update: bool,
    /// 检查时间（epoch 毫秒，由前端填写，用于跨会话恢复）
    #[serde(default)]
    pub checked_at: u64,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub item_id: String,
    pub file_name: String,
    pub received: u64,
    pub total: u64,
    /// progressing | done | error | cancelled
    pub status: String,
    pub path: String,
    pub error: String,
}

/// 按“Windows 安装包优先”给附件打分
pub fn score_asset(name: &str) -> i32 {
    let n = name.to_lowercase();
    const JUNK: [&str; 9] = [
        ".blockmap", ".json", ".yml", ".yaml", ".txt", ".sha256", ".sha512", ".sig", ".map",
    ];
    if JUNK.iter().any(|ext| n.ends_with(ext)) {
        return -1000;
    }
    let mut s = 0;
    if n.contains("win") {
        s += 40;
    }
    if n.contains("x64") || n.contains("amd64") || n.contains("x86_64") || n.contains("win64") {
        s += 25;
    }
    if n.contains("setup") || n.contains("installer") || n.ends_with(".exe") {
        s += 20;
    }
    if n.ends_with(".msi") {
        s += 15;
    }
    if n.ends_with(".zip") {
        s += 8;
    }
    if n.contains("arm64") || n.contains("aarch64") || n.contains("arm32") {
        s -= 35;
    }
    const NOT_WIN: [&str; 10] = [
        "macos", "darwin", "dmg", "appimage", ".deb", ".rpm", "linux", "android", ".apk", ".tar.gz",
    ];
    if NOT_WIN.iter().any(|k| n.contains(k)) {
        s -= 60;
    }
    s
}

/// 首次运行时预置的软件清单（来自用户 Edge“软件更新”收藏夹）
pub fn seed_config() -> Config {
    let gh = |id: &str, name: &str, icon: &str, repo: &str, homepage: &str| SoftwareItem {
        id: id.into(),
        name: name.into(),
        icon: icon.into(),
        source: Source::Github { repo: repo.into() },
        homepage: homepage.into(),
        ..Default::default()
    };
    Config {
        settings: Settings::default(),
        vscode_checks: vec![],
        items: vec![
            gh("cherry-studio", "Cherry Studio", "🍒", "CherryHQ/cherry-studio", "https://cherryai.com.cn/download"),
            SoftwareItem {
                id: "siyuan".into(),
                name: "思源笔记".into(),
                icon: "📝".into(),
                source: Source::Html {
                    check_url: "https://b3log.org/siyuan/download.html".into(),
                    version_regex: r#"siyuan-([0-9][0-9.]*)-win\.exe"#.into(),
                    download_template: "https://release.liuyun.io/siyuan/siyuan-{version}-win.exe".into(),
                },
                homepage: "https://b3log.org/siyuan/download.html".into(),
                ..Default::default()
            },
            SoftwareItem {
                id: "vscode".into(),
                name: "Visual Studio Code".into(),
                icon: "📘".into(),
                source: Source::Html {
                    check_url: "https://update.code.visualstudio.com/api/releases/stable".into(),
                    version_regex: r#""([0-9]+\.[0-9]+\.[0-9]+)""#.into(),
                    download_template: "https://update.code.visualstudio.com/{version}/win32-x64-user/stable".into(),
                },
                homepage: "https://code.visualstudio.com/Download".into(),
                ..Default::default()
            },
            gh("drawio", "drawio-desktop", "📐", "jgraph/drawio-desktop", "https://github.com/jgraph/drawio-desktop/releases"),
            gh("openchamber", "openchamber", "🛰️", "openchamber/openchamber", "https://github.com/openchamber/openchamber/releases"),
            gh("opencode", "opencode", "🤖", "anomalyco/opencode", "https://github.com/anomalyco/opencode/releases"),
            gh("snow-shot", "snow-shot", "❄️", "mg-chao/snow-shot", "https://github.com/mg-chao/snow-shot/releases"),
            gh("wezterm", "wezterm", "🖥️", "wezterm/wezterm", "https://github.com/wezterm/wezterm/releases"),
            gh("electerm", "electerm", "⌨️", "electerm/electerm", "https://github.com/electerm/electerm/releases"),
            gh("hoppscotch", "hoppscotch", "🚀", "hoppscotch/releases", "https://github.com/hoppscotch/releases/releases"),
            gh("cc-switch", "cc-switch", "🔀", "farion1231/cc-switch", "https://github.com/farion1231/cc-switch/releases"),
        ],
    }
}
