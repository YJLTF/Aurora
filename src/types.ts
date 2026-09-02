export type Source =
  | { type: "github"; repo: string }
  | { type: "html"; checkUrl: string; versionRegex: string; downloadTemplate: string };

export interface Asset {
  name: string;
  url: string;
  size: number;
}

export interface SoftItem {
  id: string;
  name: string;
  icon: string;
  source: Source;
  homepage: string;
  notes: string;
  /** 本地已安装版本 */
  installedVersion: string;
  /** 最近检测到的最新版本 */
  latestVersion: string;
  releaseUrl: string;
  assets: Asset[];
  suggested: number;
  /** 最近检查时间（epoch 毫秒，0 = 从未） */
  checkedAt: number;
  lastError: string;
}

export interface Settings {
  downloadDir: string;
  githubApiBase: string;
  downloadProxy: string;
  githubToken: string;
  /** VSCode 离线 vsix 备份目录 */
  vscodeDir: string;
  /** npm 全局目录，留空 = 自动执行 npm root -g 探测 */
  npmGlobalRoot: string;
  /** npm registry 源（默认官方源，可切 npmmirror） */
  npmRegistry: string;
  /** 启动时自动检查 Aurora 自身更新 */
  autoCheckSelf: boolean;
}

export interface AppInfo {
  version: string;
  repo: string;
}

export interface SelfUpdateInfo {
  currentVersion: string;
  latestVersion: string;
  hasUpdate: boolean;
  releaseUrl: string;
  notes: string;
  assets: Asset[];
  suggested: number;
  error: string;
}

export interface VsixInfo {
  id: string;
  version: string;
  /** 目标平台后缀（win32-x64 等），通用包为空 */
  target: string;
  fileName: string;
  dir: string;
}

export interface VsixRef {
  id: string;
  target: string;
  localVersion: string;
}

export interface VsixCheck {
  id: string;
  localVersion: string;
  latestVersion: string;
  downloadUrl: string;
  hasUpdate: boolean;
  /** 检查时间（epoch 毫秒），用于持久化恢复 */
  checkedAt: number;
  error: string;
}

/** npm 全局目录中的一个包 */
export interface NpmInfo {
  /** 包名（scoped 形如 @types/node） */
  name: string;
  version: string;
  /** 包目录绝对路径 */
  dir: string;
}

/** 参与检查的 npm 包 */
export interface NpmRef {
  name: string;
  localVersion: string;
}

/** 单个 npm 全局包的 registry 检查结果 */
export interface NpmCheck {
  name: string;
  localVersion: string;
  latestVersion: string;
  hasUpdate: boolean;
  /** 检查时间（epoch 毫秒），用于持久化恢复 */
  checkedAt: number;
  error: string;
}

/** npm 升级进度（npm-upgrade-progress 事件载荷） */
export interface NpmUpgradeProgress {
  name: string;
  status: "preparing" | "progressing" | "done" | "error" | "cancelled";
  /** 最近一行 npm 输出（error 时为错误摘要） */
  output: string;
  error: string;
  /** done 时回填升级后的本地版本（读不到为空） */
  localVersion: string;
}

export interface Config {
  settings: Settings;
  items: SoftItem[];
  /** VSCode 插件最近一次检查结果（跨会话恢复） */
  vscodeChecks: VsixCheck[];
  /** npm 全局包最近一次检查结果（跨会话恢复） */
  npmChecks: NpmCheck[];
}

export interface CheckOutcome {
  version: string;
  releaseUrl: string;
  assets: Asset[];
  suggested: number;
  /** Some(true)=有更新；未登记本地版本时为 null */
  hasUpdate: boolean | null;
  /** Release 说明文本，来源无说明时为空 */
  notes: string;
  error: string;
}

export interface DownloadProgress {
  itemId: string;
  fileName: string;
  received: number;
  total: number;
  status: "progressing" | "paused" | "done" | "error" | "cancelled";
  path: string;
  error: string;
}

export type ItemStatus =
  | "idle"
  | "checking"
  | "uptodate"
  | "update"
  | "untracked"
  | "error";

/** 宽松版本比较：a < b 返回 -1，等于返回 0，a > b 返回 1 */
export function compareVersion(a: string, b: string): number {
  const ta = tokenize(a.trim().replace(/^[vV]/, ""));
  const tb = tokenize(b.trim().replace(/^[vV]/, ""));
  const n = Math.max(ta.length, tb.length);
  for (let i = 0; i < n; i++) {
    const x = ta[i];
    const y = tb[i];
    if (x === undefined && y === undefined) return 0;
    // 预发布约定：多出来的字母段（如 -beta）视为更旧
    if (x === undefined) return typeof y === "number" ? (y === 0 ? 0 : -1) : 1;
    if (y === undefined) return typeof x === "number" ? (x === 0 ? 0 : 1) : -1;
    if (typeof x === "number" && typeof y === "number") {
      if (x !== y) return x < y ? -1 : 1;
    } else if (typeof x === "string" && typeof y === "string") {
      if (x !== y) return x < y ? -1 : 1;
    } else if (typeof x === "number") {
      return 1;
    } else {
      return -1;
    }
  }
  return 0;
}

function tokenize(s: string): (number | string)[] {
  const segs: (number | string)[] = [];
  let cur = "";
  let kind = "";
  const flush = () => {
    if (!cur) return;
    if (kind === "d") segs.push(parseInt(cur, 10));
    else if (kind === "a") segs.push(cur.toLowerCase());
    cur = "";
  };
  for (const c of s) {
    const k = /[0-9]/.test(c) ? "d" : /[a-zA-Z]/.test(c) ? "a" : "";
    if (!cur && !k) continue;
    if (cur && k !== kind) flush();
    if (!k) continue;
    kind = k;
    cur += c;
  }
  flush();
  return segs;
}
