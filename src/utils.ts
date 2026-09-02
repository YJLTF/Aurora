export function fmtSize(n: number): string {
  if (!n || n <= 0) return "";
  const units = ["B", "KB", "MB", "GB"];
  let v = n;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  const s = v >= 100 || i === 0 ? Math.round(v).toString() : v.toFixed(1);
  return `${s} ${units[i]}`;
}

export function timeAgo(ms: number): string {
  if (!ms) return "";
  const diff = Date.now() - ms;
  if (diff < 60_000) return "刚刚";
  if (diff < 3_600_000) return `${Math.floor(diff / 60_000)} 分钟前`;
  const d = new Date(ms);
  const now = new Date();
  const hm = `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
  if (d.toDateString() === now.toDateString()) return `今天 ${hm}`;
  const yest = new Date(now.getTime() - 86_400_000);
  if (d.toDateString() === yest.toDateString()) return `昨天 ${hm}`;
  return `${d.getMonth() + 1}月${d.getDate()}日 ${hm}`;
}

export function slugify(name: string): string {
  const s = name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return s || "item";
}

/** 去掉扩展名的文件主干 */
function stemOf(name: string): string {
  return name.replace(/\.[A-Za-z0-9]{1,6}$/, "");
}

/** 主干中是否出现独立、边界干净的版本串（避免 2.0.1 误命中 2.0.10） */
export function containsVersion(stem: string, version: string): boolean {
  const s = stem.toLowerCase();
  const v = version.toLowerCase();
  if (!s || !v) return false;
  let idx = s.indexOf(v);
  while (idx >= 0) {
    const before = idx > 0 ? s[idx - 1] : "";
    const after = idx + v.length < s.length ? s[idx + v.length] : "";
    if (!/[0-9.]/.test(before) && !/[0-9.]/.test(after)) return true;
    idx = s.indexOf(v, idx + 1);
  }
  return false;
}

/** 拆词并把版本段抽掉后的文件名骨架，用于跨命名风格比对 */
function skeletonKey(name: string, versionSegs: string[]): string {
  const stem = stemOf(name)
    .toLowerCase()
    .replace(/\s*\(\d+\)$/, "");
  const segs = stem.split(/[^a-z0-9]+/).filter(Boolean);
  if (!versionSegs.length) return segs.join("-");
  const kept: string[] = [];
  for (let i = 0; i < segs.length; ) {
    let j = 0;
    while (j < versionSegs.length && segs[i + j] === versionSegs[j]) j++;
    if (j === versionSegs.length) {
      i += versionSegs.length;
      continue;
    }
    kept.push(segs[i++]);
  }
  return kept.join("-");
}

/**
 * 从已下载文件列表里找出最新版本的安装包：
 * 1) 文件名包含版本号；2) 去掉版本后的骨架与安装包名一致（兼容不含版本的历史文件）。
 */
export function matchDownloaded(
  files: string[],
  version: string,
  assetNames: string[],
): string {
  if (!files.length) return "";
  const v = version.trim();
  if (v) {
    const hit = files.find((f) => containsVersion(stemOf(f), v));
    if (hit) return hit;
  }
  const vsegs = v ? v.toLowerCase().split(/[^a-z0-9]+/).filter(Boolean) : [];
  for (const a of assetNames) {
    if (!a) continue;
    const target = skeletonKey(a, vsegs);
    if (!target) continue;
    const hit = files.find((f) => skeletonKey(f, vsegs) === target);
    if (hit) return hit;
  }
  return "";
}

/** 按目录风格拼接完整路径 */
export function joinPath(dir: string, name: string): string {
  if (dir.endsWith("\\") || dir.endsWith("/")) return dir + name;
  return dir + (dir.includes("\\") ? "\\" : "/") + name;
}
