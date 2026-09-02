/**
 * 共享下载队列（单例）：软件雷达、Aurora 自更新与 VSCode 插件下载共用。
 * 统一维护进度表、传输中集合与断点续传参数缓存，
 * 进度事件由 App.vue 订阅一次后喂给 handle()。
 */
import { reactive } from "vue";
import { api, type DownloadArgs } from "./api";
import type { DownloadProgress } from "./types";

type Notify = (text: string, kind: "ok" | "err" | "info") => void;

/** 下载失败提示（命令返回错误时没有进度事件，走这里弹 toast） */
let notify: Notify | null = null;

/** 进行中/已暂停/已失败的下载进度表；done 与 cancelled 后移除。
 *  用 reactive 而非 ref：模板里 dlStore.downloads[key] 可直接索引。 */
const downloads = reactive<Record<string, DownloadProgress>>({});
/** 正在网络传输中的下载（不含 暂停/失败 状态），用于行内按钮禁用 */
const active = reactive(new Set<string>());
/** 暂停/失败后「继续/重试」用的原参数缓存（后端据此从 .part 续传） */
const argsOf = new Map<string, DownloadArgs>();
/** 已入队且命令尚未返回的下载，防止同一条重复发起 */
const inFlight = new Set<string>();

/** 下载进度事件入口：App.vue 的全局订阅唯一调用 */
function handle(p: DownloadProgress) {
  if (p.status === "progressing" || p.status === "paused") {
    downloads[p.itemId] = p;
  } else {
    delete downloads[p.itemId];
  }
  // 暂停/结束都退出传输中状态，行内主按钮恢复可用
  if (p.status === "progressing") active.add(p.itemId);
  else active.delete(p.itemId);
}

function markError(args: DownloadArgs, msg: string) {
  const prev = downloads[args.itemId];
  downloads[args.itemId] = {
    itemId: args.itemId,
    fileName: prev?.fileName || args.fileName,
    received: prev?.received ?? 0,
    total: prev?.total ?? 0,
    status: "error",
    path: "",
    error: msg,
  };
  notify?.(`下载失败：${msg}`, "err");
}

/** 发起下载；同一条传输中时不重复入队（暂停/失败状态允许重新入队续传） */
async function queue(args: DownloadArgs): Promise<void> {
  if (inFlight.has(args.itemId)) return;
  inFlight.add(args.itemId);
  argsOf.set(args.itemId, args);
  active.add(args.itemId);
  try {
    await api.download(args);
    // 正常返回即完成，done 事件已清理状态
  } catch (e) {
    // 后端用中文错误串区分终态：取消→清理，暂停→保留进度供「继续」
    const msg = String(e);
    active.delete(args.itemId);
    if (msg.includes("取消")) {
      delete downloads[args.itemId];
    } else if (msg.includes("暂停")) {
      // 进度事件已置为 paused，保留状态
    } else {
      markError(args, msg);
    }
  } finally {
    inFlight.delete(args.itemId);
  }
}

/** 继续/重试：用缓存的原始参数重新入队，后端从 .part 断点续传 */
function resume(itemId: string) {
  const args = argsOf.get(itemId);
  if (args) void queue(args);
}

function pause(itemId: string) {
  void api.pause(itemId);
}

function cancel(itemId: string) {
  void api.cancel(itemId);
}

/** 条目被删除等场景下清空其下载痕迹 */
function drop(itemId: string) {
  delete downloads[itemId];
  argsOf.delete(itemId);
}

export const dlStore = {
  downloads,
  active,
  setNotify(fn: Notify | null) {
    notify = fn;
  },
  handle,
  queue,
  resume,
  pause,
  cancel,
  drop,
};
