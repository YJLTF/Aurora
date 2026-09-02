import type { CheckOutcome, Config, DownloadProgress, SoftItem } from "./types";
import { mockConfig, mockCheck, mockDownload, mockDownloadList } from "./mock";

const isTauri =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function call<T>(cmd: string, args?: object): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args as never);
}

export interface DownloadArgs {
  itemId: string;
  url: string;
  fileName: string;
  destDir: string;
  proxyPrefix: string;
}

type ProgressHandler = (p: DownloadProgress) => void;
const mockHandlers = new Set<ProgressHandler>();

export const api = {
  isTauri,

  load(): Promise<Config> {
    return isTauri ? call<Config>("load_data") : Promise.resolve(mockConfig());
  },

  save(config: Config): Promise<void> {
    return isTauri ? call("save_data", { config }) : Promise.resolve();
  },

  check(item: SoftItem, config: Config): Promise<CheckOutcome> {
    if (!isTauri) return mockCheck(item);
    return call<CheckOutcome>("check_item", { item, settings: config.settings });
  },

  download(args: DownloadArgs): Promise<string> {
    if (!isTauri) return mockDownload(args, (p) => mockHandlers.forEach((h) => h(p)));
    return call<string>("download_file", args);
  },

  /** 列出下载目录中的文件名（目录不存在时返回空） */
  listDownloads(destDir: string): Promise<string[]> {
    if (!isTauri) return Promise.resolve(mockDownloadList());
    return call<string[]>("list_downloads", { destDir });
  },

  cancel(itemId: string): Promise<void> {
    if (!isTauri) return Promise.resolve();
    return call("cancel_download", { itemId });
  },

  open(path: string, reveal = false): Promise<void> {
    if (!isTauri) {
      console.info("[mock] open", path, reveal);
      return Promise.resolve();
    }
    return call("open_path", { path, reveal });
  },

  openUrl(url: string): Promise<void> {
    if (!isTauri) {
      window.open(url, "_blank");
      return Promise.resolve();
    }
    return call("open_url", { url });
  },

  /** 订阅下载进度，返回取消订阅函数 */
  async onProgress(handler: ProgressHandler): Promise<() => void> {
    if (isTauri) {
      const { listen } = await import("@tauri-apps/api/event");
      const un = await listen<DownloadProgress>("download-progress", (e) =>
        handler(e.payload),
      );
      return un;
    }
    mockHandlers.add(handler);
    return () => mockHandlers.delete(handler);
  },
};
