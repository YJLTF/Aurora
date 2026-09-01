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
}

export interface Config {
  settings: Settings;
  items: SoftItem[];
}

export interface CheckOutcome {
  version: string;
  releaseUrl: string;
  assets: Asset[];
  suggested: number;
  /** Some(true)=有更新；未登记本地版本时为 null */
  hasUpdate: boolean | null;
  error: string;
}

export interface DownloadProgress {
  itemId: string;
  fileName: string;
  received: number;
  total: number;
  status: "progressing" | "done" | "error" | "cancelled";
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
