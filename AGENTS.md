# AGENTS.md — Aurora 开发指南

面向在本仓库工作的 AI 代理与新成员。项目为 **Aurora · 软件更新雷达**：集中管理常用软件，一键检测最新版本、对比本地版本、下载升级包；附带 Aurora 自更新与 VSCode 插件（.vsix 备份）更新检查。技术栈 **Vue 3 `<script setup>` + TypeScript strict + Vite 7 + Tauri 2（Rust）**，无路由、无 pinia、无 UI 库，全局样式集中在 `src/style.css`。

## 常用命令

```bash
npm run tauri dev        # 桌面端开发调试（vite + cargo run）
npm run dev              # 纯前端 UI 调试（http://localhost:5173，自动切换 mock 数据）
npm run build            # vue-tsc 类型检查 + vite 产物构建（CI 级验证）
npm run tauri build      # 打包 NSIS 安装程序（src-tauri/target/release/bundle/）
cd src-tauri && cargo check && cargo test   # Rust 检查与单元测试（7 个）
```

前端无独立测试框架；`src-tauri/src/version.rs` 与 `vscode.rs` 内嵌 `#[cfg(test)]` 单元测试。

## 架构地图

```
src/App.vue            壳层：顶栏（视图切换/设置/扫描/检查全部）、状态栏、设置与自更新弹窗
src/components/
  RadarPanel.vue       软件雷达视图：清单/筛选/单项与全部检查/下载/已下载识别/编辑与选包弹窗
  VscodePanel.vue      VSCode 插件视图：扫描/检查/下载
  SoftRow.vue 等       行组件与各弹窗；DlProgress（进度行）、AssetRadioList（安装包单选）为复用基础组件
src/download.ts        共享下载队列单例 dlStore：进度表(reactive)、传输中 Set、续传参数缓存、queue/resume/pause/cancel/drop
src/api.ts             全部 Tauri invoke 封装；isTauri 为 false 时走 src/mock.ts（浏览器预览）
src/types.ts           前端类型 + compareVersion（与 Rust version.rs 语义一致，两份实现需同步改动）
src-tauri/src/lib.rs   命令注册（generate_handler!）、配置读写（原子 tmp+rename）、open_path/open_url
src-tauri/src/model.rs 数据模型（serde camelCase）、score_asset Windows 附件评分、seed_config 预置清单
src-tauri/src/net.rs   GitHub/HTML 检测、自更新、download_file（Range 断点续传/暂停/取消/自动重试）、list_downloads
src-tauri/src/vscode.rs .vsix 文件名解析、递归扫描、VS Marketplace extensionquery 批量检查
```

**视图面板模式**：两个面板均 `v-show` 常驻 + `defineExpose` 供顶栏直调（RadarPanel: `openAdd/checkAll/handleDone/busy`；VscodePanel: `scan/check/busy`），数据经 props 传入、事件冒泡（`notify`→toast、`persist`→防抖保存、`stats`→状态栏计数）。清单数组由 RadarPanel 原地增删改（push/splice/索引赋值），配置持久化始终由 App 收口。

配置文件：`%APPDATA%/com.aurora.updater/aurora.json`（settings + items + vscodeChecks），前端 250ms 防抖后 `save_data`。

## 关键设计约定

- **前后端字段映射**：Rust 结构体一律 `#[serde(rename_all = "camelCase")]`，与 `src/types.ts` 手写接口一一对应；新增字段两处都要加，可选字段用 `#[serde(default)]` 保证旧配置兼容。
- **下载生命周期**（net.rs `pump`）：临时分片为 `<目标目录>/<文件名>.part`；暂停/失败**保留**分片，取消**删除**，完成 rename 为最终名。续传靠 `Range: bytes=N-`（206 追加 / 200 覆盖 / 416 删分片重来）；瞬时错误自动重试 2 次（退避 `attempt*800ms`），错误串用中文哨兵值（`下载已取消`/`下载已暂停`），前端 `download.ts` 按关键字分流。
- **进度事件**：后端只 emit `download-progress`（progressing/paused/done/cancelled；error 仅经命令 Err 返回）。App.vue `onMounted` 里**全局唯一订阅**，喂给 `dlStore.handle()`；组件不要各自再订阅。
- **IPv4 优先**：`net::client_with_ipv4_pref(host)` 为 marketplace.visualstudio.com 与 vsix CDN 固定 IPv4 解析——国内环境 IPv6 半通（TCP 可连但数据黑洞）。新增直连国内不畅的域名时复用该函数，`DownloadArgs.preferIpv4` 控制下载路径。
- **VSCode 页**：清单来自递归扫描 `.vsix` 文件名（解析规则见 `vscode.rs::parse_stem`，含平台后缀剥离）；检查结果由前端写入 `config.vscodeChecks` 持久化，重扫描时按新本地版本重算 `hasUpdate`。面板随视图常驻（`v-show`），`defineExpose({ scan, check, busy })` 供顶栏调用。
- **浏览器预览**：任何新命令都要在 `api.ts` 里同时写 Tauri 分支与 mock 分支，保证 `npm run dev` 可用。
- **界面语言**：全中文；深蓝主题变量见 `style.css` 顶部 `:root`；新 UI 优先复用既有 class（btn/pill/dl/chip…），不要引入组件库。

## 验证清单（提交前）

1. `npm run build`（含 vue-tsc strict 检查）必须零错误
2. `cd src-tauri && cargo check && cargo test` 必须通过
3. 涉及 UI 交互的改动：`npm run dev` 走一遍 mock 流程（检查全部 → 下载 → 暂停/继续/取消 → VSCode 页检查）

## 已知坑

- **vue-tsc 3.3 内联处理器推断**：组件 A 导入组件 B 后，A 在 `v-for` 中被使用的内联箭头处理器参数会失去上下文类型（TS7006）。解法：给参数写显式类型，如 `@download="(a: Asset | null) => ..."`。
- **vite HMR 状态残留**：改 `<script setup>` 结构后有时页面状态不刷新（尤其 defineExpose/组合式状态），整重启 `npm run dev` 即可；5173 端口被残留进程占用时先 `netstat -ano | findstr 5173` 找 PID 杀掉。
- **Marketplace 请求必须走 IPv4 优先客户端**，直连会在 IPv6 上挂起（表现为请求 30s 超时）。
- Windows 下下载重命名前必须先 drop 文件句柄（`pump` 中的 `drop(file)`），否则 remove/rename 报错。

## 提交规范

中文提交信息，前缀 `feat:` / `ui:` / `fix:`（参考 `git log`）。功能开发在 `feat/*` 分支进行。
