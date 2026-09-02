/**
 * 浏览器预览模式的模拟后端：无 Tauri 时提供假数据，
 * 便于纯前端调试 UI。打包后不会进入执行路径。
 */
import { compareVersion } from "./types";
import type {
  AppInfo,
  CheckOutcome,
  Config,
  DownloadProgress,
  NpmCheck,
  NpmInfo,
  NpmRef,
  NpmUpgradeProgress,
  SelfUpdateInfo,
  SoftItem,
  Settings,
  VsixCheck,
  VsixInfo,
  VsixRef,
} from "./types";

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
      vscodeDir: "C:\\Users\\demo\\Downloads\\vscode",
      npmGlobalRoot: "",
      npmRegistry: "https://registry.npmjs.org",
      autoCheckSelf: true,
    },
    vscodeChecks: [],
    npmChecks: [],
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
    return { version: "", releaseUrl: "", assets: [], suggested: 0, hasUpdate: null, notes: "", error: "模拟数据中无此软件" };
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
    notes: "",
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

export function mockAppInfo(): AppInfo {
  return { version: "0.3.0", repo: "YJLTF/Aurora" };
}

/** 模拟自更新：0.3.0 → 0.4.0 */
export async function mockSelfUpdate(_settings: Settings): Promise<SelfUpdateInfo> {
  await wait(600);
  return {
    currentVersion: "0.3.0",
    latestVersion: "0.4.0",
    hasUpdate: true,
    releaseUrl: "https://github.com/YJLTF/Aurora/releases/tag/v0.4.0",
    notes: "### v0.4.0（模拟数据）\n- 新增 NPM 全局包更新检查\n- 界面细节优化",
    assets: [
      { name: "Aurora_0.4.0_x64-setup.exe", url: "https://example.com/Aurora_0.4.0_x64-setup.exe", size: 8_800_000 },
      { name: "Aurora_0.4.0_arm64-setup.exe", url: "https://example.com/Aurora_0.4.0_arm64-setup.exe", size: 8_100_000 },
    ],
    suggested: 0,
    error: "",
  };
}

const MOCK_VSIX: VsixInfo[] = [
  { id: "dbaeumer.vscode-eslint", version: "3.0.10", target: "", fileName: "dbaeumer.vscode-eslint-3.0.10.vsix", dir: "C:\\Users\\demo\\Downloads\\vscode\\前端" },
  { id: "ms-ceintl.vscode-language-pack-zh-hans", version: "1.131.2026082318", target: "", fileName: "ms-ceintl.vscode-language-pack-zh-hans-1.131.2026082318.vsix", dir: "C:\\Users\\demo\\Downloads\\vscode" },
  { id: "ms-python.python", version: "2026.4.0", target: "win32-x64", fileName: "ms-python.python-2026.4.0-win32-x64.vsix", dir: "C:\\Users\\demo\\Downloads\\vscode\\python" },
  { id: "ms-python.vscode-pylance", version: "2026.3.1", target: "", fileName: "ms-python.vscode-pylance-2026.3.1.vsix", dir: "C:\\Users\\demo\\Downloads\\vscode\\python" },
  { id: "redhat.java", version: "1.55.0", target: "win32-x64", fileName: "redhat.java-1.55.0-win32-x64.vsix", dir: "C:\\Users\\demo\\Downloads\\vscode\\Extension Pack for Java" },
  { id: "vmware.vscode-boot-dev-pack", version: "0.2.1", target: "", fileName: "vmware.vscode-boot-dev-pack-0.2.1.vsix", dir: "C:\\Users\\demo\\Downloads\\vscode\\Spring Boot Extension Pack" },
];

export function mockListVsix(): VsixInfo[] {
  return MOCK_VSIX;
}

export function mockInstalledExtensions(): Record<string, string> {
  return {
    "dbaeumer.vscode-eslint": "3.0.10",
    "ms-ceintl.vscode-language-pack-zh-hans": "1.131.2026082318",
    "ms-python.python": "2026.4.0",
    "ms-python.vscode-pylance": "2026.3.1",
    "redhat.java": "1.55.0",
    "vmware.vscode-boot-dev-pack": "0.2.1",
  };
}

const VSIX_LATEST: Record<string, string> = {
  "dbaeumer.vscode-eslint": "3.0.13",
  "ms-ceintl.vscode-language-pack-zh-hans": "1.135.2026090112",
  "ms-python.python": "2026.4.0",
  "ms-python.vscode-pylance": "2026.4.2",
  "redhat.java": "1.55.0",
  "vmware.vscode-boot-dev-pack": "0.2.1",
};

export async function mockVscodeChecks(items: VsixRef[]): Promise<VsixCheck[]> {
  await wait(500 + Math.random() * 500);
  const now = Date.now();
  return items.map((it) => {
    const latest = VSIX_LATEST[it.id.toLowerCase()] ?? "";
    return {
      id: it.id,
      localVersion: it.localVersion,
      latestVersion: latest,
      downloadUrl: latest ? `https://example.com/${it.id}-${latest}.vsix` : "",
      hasUpdate: latest ? compareVersion(latest, it.localVersion) > 0 : false,
      checkedAt: now,
      error: latest ? "" : "模拟数据中无此插件",
    };
  });
}

const MOCK_NPM_ROOT = "C:\\Users\\demo\\AppData\\Roaming\\npm\\node_modules";

export function mockNpmRoot(): string {
  return MOCK_NPM_ROOT;
}

const MOCK_NPM: NpmInfo[] = [
  { name: "@anthropic-ai/claude-code", version: "1.0.0", dir: `${MOCK_NPM_ROOT}\\@anthropic-ai\\claude-code` },
  { name: "@types/node", version: "22.5.0", dir: `${MOCK_NPM_ROOT}\\@types\\node` },
  { name: "eslint", version: "9.9.0", dir: `${MOCK_NPM_ROOT}\\eslint` },
  { name: "npm-check-updates", version: "17.0.3", dir: `${MOCK_NPM_ROOT}\\npm-check-updates` },
  { name: "pnpm", version: "9.6.0", dir: `${MOCK_NPM_ROOT}\\pnpm` },
  { name: "typescript", version: "5.5.4", dir: `${MOCK_NPM_ROOT}\\typescript` },
];

export function mockScanNpm(): NpmInfo[] {
  return MOCK_NPM;
}

const NPM_LATEST: Record<string, string> = {
  "@anthropic-ai/claude-code": "1.0.12",
  "@types/node": "22.5.1",
  eslint: "9.9.1",
  "npm-check-updates": "17.1.0",
  pnpm: "9.6.0",
  typescript: "5.9.2",
};

export async function mockNpmChecks(items: NpmRef[]): Promise<NpmCheck[]> {
  await wait(400 + Math.random() * 500);
  const now = Date.now();
  return items.map((it) => {
    const latest = NPM_LATEST[it.name.toLowerCase()] ?? "";
    return {
      name: it.name,
      localVersion: it.localVersion,
      latestVersion: latest,
      hasUpdate: latest ? compareVersion(latest, it.localVersion) > 0 : false,
      checkedAt: now,
      error: latest ? "" : "模拟数据中无此包",
    };
  });
}

/** 模拟升级的取消标记（包名小写） */
const NPM_UPGRADE_CANCELLED = new Set<string>();

export function mockNpmCancelUpgrade(name: string): void {
  NPM_UPGRADE_CANCELLED.add(name.toLowerCase());
}

export async function mockNpmUpgrade(
  name: string,
  emit: (p: NpmUpgradeProgress) => void,
): Promise<void> {
  const key = name.toLowerCase();
  NPM_UPGRADE_CANCELLED.delete(key);
  emit({ name, status: "preparing", output: "", error: "", localVersion: "" });
  const steps = ["⠋ idealTree:dev 依赖计算", "reify:提取 tar 包内容", "run @latest postinstall", "link 全局 bin 链接"];
  for (const step of steps) {
    await wait(450);
    if (NPM_UPGRADE_CANCELLED.has(key)) {
      emit({ name, status: "cancelled", output: "", error: "", localVersion: "" });
      return;
    }
    emit({ name, status: "progressing", output: step, error: "", localVersion: "" });
  }
  await wait(400);
  if (NPM_UPGRADE_CANCELLED.has(key)) {
    emit({ name, status: "cancelled", output: "", error: "", localVersion: "" });
    return;
  }
  const latest = NPM_LATEST[key] ?? "";
  emit({
    name,
    status: "done",
    output: `added 1 package in ${(2 + Math.random() * 3).toFixed(0)}s`,
    error: "",
    localVersion: latest,
  });
}
