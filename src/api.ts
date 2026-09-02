import type {
  AppInfo,
  CheckOutcome,
  Config,
  DownloadProgress,
  SelfUpdateInfo,
  SoftItem,
  Settings,
  VsixCheck,
  VsixInfo,
  VsixRef,
} from "./types";
import {
  mockConfig,
  mockCheck,
  mockDownload,
  mockDownloadList,
  mockAppInfo,
  mockSelfUpdate,
  mockListVsix,
  mockInstalledExtensions,
  mockVscodeChecks,
} from "./mock";

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
  /** 目标 CDN 在本机可能有半通 IPv6 时传 true（VSCode 插件下载） */
  preferIpv4?: boolean;
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

  /** 当前应用版本与自更新仓库 */
  appInfo(): Promise<AppInfo> {
    if (!isTauri) return Promise.resolve(mockAppInfo());
    return call<AppInfo>("app_info");
  },

  /** 检查 Aurora 自身的更新 */
  checkSelfUpdate(settings: Settings): Promise<SelfUpdateInfo> {
    if (!isTauri) return mockSelfUpdate(settings);
    return call<SelfUpdateInfo>("check_self_update", { settings });
  },

  /** 递归扫描 VSCode 备份目录中的 .vsix 文件 */
  listVsix(dir: string): Promise<VsixInfo[]> {
    if (!isTauri) return Promise.resolve(mockListVsix());
    return call<VsixInfo[]>("list_vsix", { dir });
  },

  /** 本机 VSCode 已安装扩展版本表（id 小写 → 版本），读不到为空表 */
  readInstalledExtensions(): Promise<Record<string, string>> {
    if (!isTauri) return Promise.resolve(mockInstalledExtensions());
    return call<Record<string, string>>("read_installed_extensions");
  },

  /** 批量检查 VSCode 插件的 Marketplace 更新 */
  checkVscodeUpdates(items: VsixRef[]): Promise<VsixCheck[]> {
    if (!isTauri) return Promise.resolve(mockVscodeChecks(items));
    return call<VsixCheck[]>("check_vscode_updates", { items });
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

  /** 暂停下载：保留 .part 分片，重新 download 即断点续传 */
  pause(itemId: string): Promise<void> {
    if (!isTauri) return Promise.resolve();
    return call("pause_download", { itemId });
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
