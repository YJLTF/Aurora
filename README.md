# Aurora · 软件更新雷达

自用的常用软件更新检查工具：集中管理一批常用软件，一键检测最新版本、对比本地已装版本、直接下载升级包。基于 **Vue 3 + TypeScript + Vite + Tauri 2（Rust）**。

软件清单预置自 Edge 浏览器"软件更新"收藏夹（首次启动自动导入，可增删改）。

## 功能

- **两种检测方式**
  - GitHub Releases：通过 GitHub API 获取最新 Release 的版本号与附件列表（支持非"最新正式版"仓库的回退策略）
  - 页面 / 接口解析：对任意 URL 的响应文本做正则提取版本号，可配置 `{version}` 下载直链模板（VS Code、思源笔记等即用此方式）
- **一键检查全部**：并发检测，可更新的软件自动置顶，版本跃迁 `本地 → 最新` 一目了然
- **Aurora 自更新检查**：顶栏切换旁的视图之外，左下角状态栏常显 `Aurora v当前版本`，点击即可检查自身更新（GitHub Releases，仓库 `YJLTF/Aurora`）；发现新版本时版本号旁出现琥珀色圆点，弹窗内展示更新说明与安装包列表，可直接下载到下载目录后手动运行升级（默认启动时静默检查一次，可在设置中关闭）
- **VSCode 插件更新检查**：顶栏切换到"VSCode 插件"视图，递归扫描备份目录（默认 `下载目录\vscode`，可设置）及其子文件夹中的 `.vsix` 文件，从文件名解析 `插件ID + 版本 + 平台后缀`；一键批量查询 VS Marketplace 最新版本，对比"备份版本 / 本机已装版本 / 最新版本"，可更新项一键下载新版 vsix 到原文件所在子文件夹（沿用原命名规则）；已装版本读取自 `~/.vscode/extensions/extensions.json`；检查结果持久化保存，重启后直接恢复（重新扫描时按最新本地版本重算可更新标记）
- **NPM 全局包更新检查**：顶栏切换到"NPM 全局包"视图，自动执行 `npm root -g` 定位全局 node_modules（探测失败可在设置手动指定），读取各包 `package.json` 得到本地版本（含 `@scope` 包，别名包也不歧义）；批量查询 registry 的 dist-tags 最新版（限并发、IPv4 优先）；可更新项支持**一键执行更新**（应用内 `npm install -g <包>@latest`，实时输出逐行展示、可取消、完成后自动回填版本）或复制升级命令，也可打开 npm 包页面；检查结果跨会话持久化，重扫描按新本地版本重算
- **升级包下载**：自动按 Windows 相关性推荐安装包（x64/setup/exe 优先，排除 arm64/macOS/校验文件），也可手动挑选；实时进度，支持 **暂停 / 继续（断点续传）/ 失败自动重试（2 次，指数退避）/ 取消**，下载完成可直接打开文件所在目录
  - 断点续传基于 HTTP Range：未完成的分片保存为 `<下载目录>/<文件名>.part`，暂停或失败时保留供续传，取消时删除，完成后自动改名为最终文件（GitHub 与 VS Marketplace CDN 均支持 Range；服务器不支持时自动从头下载）
- **已下载安装包识别**：检查时扫描下载目录，若最新版本的安装包已经下载过，行内显示"✓ 最新版本安装包已下载"并可一键在资源管理器中定位，避免重复下载
  - 文件名含版本号的按版本匹配；下载时若安装包文件名不含版本号会自动追加到扩展名前（如 `Hoppscotch_win_x64.exe` → `Hoppscotch_win_x64-25.7.0.exe`），确保日后能识别
  - 兼容历史无版本文件：按"去掉版本后的文件名骨架"匹配
- **本地版本登记**：未登记时显示"待登记"；登记后自动判定 可更新 / 已最新；行内即可编辑，或一键"设为已装"
- **针对国内网络的设置**：GitHub API 镜像地址、下载加速前缀（仅作用于 github.com 直链）、可选 Token（避免 60 次/小时限流）、npm Registry 镜像（可切 npmmirror，检测走 IPv4 优先）

## 运行

依赖：Node.js、Rust（cargo）、Tauri 2 所需的 WebView2（Windows 10/11 一般自带）。

```bash
npm install        # 本机 node 未带 npm 时，可用 .tools 里的独立 npm：node .tools/package/bin/npm-cli.js install
npm run tauri dev  # 开发调试
npm run tauri build  # 打包 NSIS 安装程序（输出在 src-tauri/target/release/bundle/）
```

纯前端 UI 调试（无 Tauri，自动切换为浏览器模拟数据）：

```bash
npm run dev        # 打开 http://localhost:5173
```

## 配置存储

配置保存在 `%APPDATA%/com.aurora.updater/aurora.json`（软件清单 + 设置 + 最近检测结果），删除该文件可恢复预置清单。设置项：下载目录、VSCode 备份目录、npm 全局目录、npm Registry、GitHub API 镜像、下载加速前缀、GitHub Token、启动时自动检查 Aurora 更新。

## 预置清单（来自收藏夹）

| 软件 | 检测方式 |
| --- | --- |
| Cherry Studio 🍒 | GitHub `CherryHQ/cherry-studio` |
| 思源笔记 📝 | 页面解析 `b3log.org/siyuan/download.html` |
| Visual Studio Code 📘 | 官方 update API + 直链模板 |
| drawio-desktop 📐 / openchamber 🛰️ / opencode 🤖 / snow-shot ❄️ / wezterm 🖥️ / electerm ⌨️ / hoppscotch 🚀 / cc-switch 🔀 | GitHub Releases |

## 添加新软件

点"＋ 添加软件"：

- **GitHub 仓库**：填 `owner/name` 即可
- **页面解析**：填检查地址 + 版本正则（第一个捕获组作为版本号），可选填 `{version}` 下载模板，例如：
  - 检查地址 `https://update.code.visualstudio.com/api/releases/stable`
  - 版本正则 `"([0-9]+\.[0-9]+\.[0-9]+)"`
  - 下载模板 `https://update.code.visualstudio.com/{version}/win32-x64-user/stable`

## 项目结构

```
src/                  Vue 3 前端
src/App.vue           壳层：顶栏 / 三视图面板 / 状态栏 / 设置与自更新弹窗
src/api.ts            Tauri invoke 封装（浏览器预览时自动切换 mock）
src/download.ts       共享下载队列：进度表、传输中集合、断点续传参数缓存，三处下载入口共用
src/components/
  RadarPanel.vue      软件雷达视图（清单/筛选/检查/下载/编辑与选包弹窗）
  VscodePanel.vue     VSCode 插件视图（扫描/检查/下载）
  NpmPanel.vue        NPM 全局包视图（探测与扫描/检查/复制升级命令/打开主页）
  DlProgress.vue      下载进度行（进度条/状态文案/暂停/继续/重试/取消）
  AssetRadioList.vue  安装包单选列表（选择弹窗与自更新弹窗共用）
src-tauri/src/
  lib.rs              Tauri 命令注册：load/save 配置、check、自更新、open_path/open_url
  model.rs            数据模型、Windows 附件评分、预置清单
  net.rs              GitHub/HTML 检测、Aurora 自更新、流式下载（进度事件、暂停/取消/断点续传）、下载目录扫描
  vscode.rs           .vsix 文件名解析与目录扫描、VS Marketplace 批量更新检查
  npm.rs              npm root -g 探测（Windows 经 cmd /C）、全局包扫描、registry dist-tags 批量检查（限并发）
  version.rs          宽松版本比较（兼容日期版本号、预发布后缀）
scripts/gen_icons.py  图标生成脚本（PNG/ICO）
```

## 更新日志

### v0.3.1

- **修复**：VSCode 插件下载完成后行内备份版本与状态不刷新——现下载完成自动重扫备份目录，新版文件即被识别并重算"已最新/可更新"
- **修复**：VSCode 插件「定位」按钮遇含空格的备份路径时 explorer 退开"文档"目录——`/select` 参数改用原样传参，引号只包路径

### v0.3.0

- **NPM 全局包更新检查**：自动执行 `npm root -g` 定位全局目录（可在设置手动指定兜底），扫描各包 `package.json` 得到本地版本（支持 `@scope` 包）；批量查询 registry 的 dist-tags 最新版（限 6 路并发、IPv4 优先），标记"可更新 / 已最新"；检查结果跨会话持久化，重扫描按新本地版本重算
- **一键执行更新**：可更新项在应用内执行 `npm install -g <包>@latest`，npm 输出逐行实时展示、可随时取消（杀进程树）、完成后自动重读版本并刷新"已最新"状态；同一时间串行执行，避免全局目录竞争；也可一键复制升级命令
- **npm 设置项**：npm 全局目录（GUI 环境探测失败时的手动兜底）、npm Registry 源（默认官方，国内可切 `registry.npmmirror.com`）
- **界面**：顶栏扩为 软件雷达 / VSCode 插件 / NPM 全局包 三视图，状态栏按视图计数并常显 npm 全局目录；设置面板新增两个 npm 字段；滚动条全应用统一为深蓝主题细样式

### v0.2.0

- **Aurora 自更新检查**：状态栏版本号一键检查 GitHub Releases，展示更新说明与安装包列表，下载后手动运行即可升级；支持启动时静默检查（可关闭）
- **VSCode 插件更新检查**：递归扫描备份目录中的 `.vsix`，从文件名解析插件与版本，批量查询 VS Marketplace 最新版与下载直链，对比备份/已装/最新三个版本，一键下载新版；检查结果跨会话持久化
- **下载体验升级**：断点续传（HTTP Range + `.part` 分片）、暂停/继续、失败自动重试（2 次退避）、随时取消；VS Marketplace 下载自动 IPv4 优先，规避国内 IPv6 半通问题
- **界面**：软件雷达 / VSCode 插件双视图统一布局；进度条按状态着色；toast 分类标识
- **结构**：共享下载队列 `download.ts`，进度行 / 安装包单选 / 双视图面板组件化，App.vue 瘦身为壳层
