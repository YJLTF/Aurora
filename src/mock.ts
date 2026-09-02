/**
 * 浏览器预览模式的模拟后端：无 Tauri 时提供假数据，
 * 便于纯前端调试 UI。打包后不会进入执行路径。
 */
import { compareVersion } from "./types";
import type { CheckOutcome, Config, DownloadProgress, SoftItem } from "./types";

const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));

const gh = (
  id: string,
  name: string,
  icon: string,
  repo: string,
  homepage: string,
): SoftItem => ({
  id,
  name,
  icon,
  source: { type: "github", repo },
  homepage,
  notes: "",
  installedVersion: "",
  latestVersion: "",
  releaseUrl: "",
  assets: [],
  suggested: 0,
  checkedAt: 0,
  lastError: "",
});

export function mockConfig(): Config {
  return {
    settings: {
      downloadDir: "C:\\Users\\demo\\Downloads",
      githubApiBase: "https://api.github.com",
      downloadProxy: "",
      githubToken: "",
    },
    items: [
      gh("cherry-studio", "Cherry Studio", "🍒", "CherryHQ/cherry-studio", "https://cherryai.com.cn/download"),
      {
        ...gh("siyuan", "思源笔记", "📝", "-", "https://b3log.org/siyuan/download.html"),
        source: {
          type: "html",
          checkUrl: "https://b3log.org/siyuan/download.html",
          versionRegex: "siyuan-([0-9][0-9.]*)-win\\.exe",
          downloadTemplate: "https://release.liuyun.io/siyuan/siyuan-{version}-win.exe",
        },
      },
      {
        ...gh("vscode", "Visual Studio Code", "📘", "-", "https://code.visualstudio.com/Download"),
        installedVersion: "1.132.0",
        source: {
          type: "html",
          checkUrl: "https://update.code.visualstudio.com/api/releases/stable",
          versionRegex: '"([0-9]+\\.[0-9]+\\.[0-9]+)"',
          downloadTemplate: "https://update.code.visualstudio.com/{version}/win32-x64-user/stable",
        },
      },
      { ...gh("drawio", "drawio-desktop", "📐", "jgraph/drawio-desktop", "-"), installedVersion: "26.0.1" },
      gh("openchamber", "openchamber", "🛰️", "openchamber/openchamber", "-"),
      gh("opencode", "opencode", "🤖", "anomalyco/opencode", "-"),
      gh("snow-shot", "snow-shot", "❄️", "mg-chao/snow-shot", "-"),
      { ...gh("wezterm", "wezterm", "🖥️", "wezterm/wezterm", "-"), installedVersion: "20240203-110809-5046fc22" },
      gh("electerm", "electerm", "⌨️", "electerm/electerm", "-"),
      gh("hoppscotch", "hoppscotch", "🚀", "hoppscotch/releases", "-"),
      gh("cc-switch", "cc-switch", "🔀", "farion1231/cc-switch", "-"),
    ],
  };
}

const FAKE: Record<string, { version: string; assets: [string, number][] }> = {
  "cherry-studio": { version: "1.5.6", assets: [["Cherry-Studio-1.5.6-setup.exe", 118_400_000], ["Cherry-Studio-1.5.6-arm64-setup.exe", 112_000_000], ["sha256.txt", 120]] },
  siyuan: { version: "3.8.2", assets: [["siyuan-3.8.2-win.exe", 96_300_000]] },
  vscode: { version: "1.135.0", assets: [["VSCode-1.135.0.exe", 101_500_000]] },
  drawio: { version: "26.2.3", assets: [["drawio-x86_64-26.2.3-windows-installer.exe", 78_100_000], ["drawio-arm64-26.2.3-windows-installer.exe", 74_000_000], ["drawio-26.2.3.zip", 90_000_000]] },
  openchamber: { version: "0.3.19", assets: [["openchamber-0.3.19-win-x64.zip", 24_000_000], ["openchamber-0.3.19-linux.tar.gz", 22_000_000]] },
  opencode: { version: "0.12.3", assets: [["opencode-windows-x64.zip", 31_000_000], ["opencode-macos-arm64.zip", 29_000_000]] },
  "snow-shot": { version: "1.10.1", assets: [["snow-shot_1.10.1_x64-setup.exe", 52_000_000], ["snow-shot_1.10.1_arm64-setup.exe", 49_000_000]] },
  wezterm: { version: "20250601-073000-e693f822", assets: [["wezterm-20250601-073000-e693f822-x86_64-pc-windows-msvc-setup.exe", 62_000_000], ["wezterm-20250601-073000-e693f822-x86_64-pc-windows-msvc.zip", 71_000_000]] },
  electerm: { version: "2.14.8", assets: [["electerm-2.14.8-win-x64-installer.exe", 108_000_000], ["electerm-2.14.8-mac-arm64.dmg", 120_000_000]] },
  hoppscotch: { version: "25.7.0", assets: [["Hoppscotch_win_x64.exe", 94_000_000], ["Hoppscotch_mac.dmg", 98_000_000]] },
  "cc-switch": { version: "1.7.2", assets: [["cc-switch_1.7.2_x64-setup.exe", 9_800_000], ["cc-switch_1.7.2_arm64-setup.exe", 9_200_000], ["latest.json", 300]] },
};

export async function mockCheck(item: SoftItem): Promise<CheckOutcome> {
  await wait(400 + Math.random() * 900);
  const fake = FAKE[item.id];
  if (!fake) {
    return { version: "", releaseUrl: "", assets: [], suggested: 0, hasUpdate: null, error: "模拟数据中无此软件" };
  }
  let suggested = 0;
  let best = -Infinity;
  fake.assets.forEach(([name], i) => {
    let s = 0;
    const n = name.toLowerCase();
    if (n.includes("win")) s += 40;
    if (n.includes("x64") || n.includes("amd64")) s += 25;
    if (n.includes("setup") || n.includes("installer") || n.endsWith(".exe")) s += 20;
    if (n.includes("arm64") || n.includes("mac") || n.includes("linux") || n.endsWith(".txt") || n.endsWith(".json")) s -= 60;
    if (s > best) { best = s; suggested = i; }
  });
  const hasUpdate =
    item.installedVersion && compareVersion(fake.version, item.installedVersion) > 0
      ? true
      : item.installedVersion
        ? false
        : null;
  return {
    version: fake.version,
    releaseUrl:
      item.homepage === "-"
        ? `https://github.com/${(item.source as { repo: string }).repo}/releases`
        : item.homepage,
    assets: fake.assets.map(([name, size]) => ({ name, url: `https://example.com/${name}`, size })),
    suggested,
    hasUpdate,
    error: "",
  };
}

export async function mockDownload(
  args: { itemId: string; url: string; fileName: string },
  emit: (p: DownloadProgress) => void,
): Promise<string> {
  const total = 58_000_000;
  let received = 0;
  const name = args.fileName || "setup.exe";
  while (received < total) {
    await wait(80);
    received = Math.min(total, received + 3_400_000);
    emit({ itemId: args.itemId, fileName: name, received, total, status: "progressing", path: "", error: "" });
  }
  const path = `C:\\Users\\demo\\Downloads\\${name}`;
  emit({ itemId: args.itemId, fileName: name, received, total, status: "done", path, error: "" });
  return path;
}

/** 预置的“下载目录”文件列表：含版本命名与无版本历史文件两种样例 */
export function mockDownloadList(): string[] {
  return [
    "Cherry-Studio-1.5.6-setup.exe",
    "siyuan-3.8.2-win.exe",
    "Hoppscotch_win_x64.exe",
    "desktop.ini",
    "readme.txt",
  ];
}
