# Aurora · 软件更新雷达

自用的常用软件更新检查工具：集中管理一批常用软件，一键检测最新版本、对比本地已装版本、直接下载升级包。基于 **Vue 3 + TypeScript + Vite + Tauri 2（Rust）**。

软件清单预置自 Edge 浏览器"软件更新"收藏夹（首次启动自动导入，可增删改）。

## 功能

- **两种检测方式**
  - GitHub Releases：通过 GitHub API 获取最新 Release 的版本号与附件列表（支持非"最新正式版"仓库的回退策略）
  - 页面 / 接口解析：对任意 URL 的响应文本做正则提取版本号，可配置 `{version}` 下载直链模板（VS Code、思源笔记等即用此方式）
- **一键检查全部**：并发检测，可更新的软件自动置顶，版本跃迁 `本地 → 最新` 一目了然
- **升级包下载**：自动按 Windows 相关性推荐安装包（x64/setup/exe 优先，排除 arm64/macOS/校验文件），也可手动挑选；实时进度、可取消、下载完成可直接打开文件所在目录
- **本地版本登记**：未登记时显示"待登记"；登记后自动判定 可更新 / 已最新；行内即可编辑，或一键"设为已装"
- **针对国内网络的设置**：GitHub API 镜像地址、下载加速前缀（仅作用于 github.com 直链）、可选 Token（避免 60 次/小时限流）

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

配置保存在 `%APPDATA%/com.aurora.updater/aurora.json`（软件清单 + 设置 + 最近检测结果），删除该文件可恢复预置清单。

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
src/                  Vue 3 前端（列表/编辑/设置/安装包选择组件）
src/api.ts            Tauri invoke 封装（浏览器预览时自动切换 mock）
src-tauri/src/
  lib.rs              Tauri 命令：load/save 配置、check、open_path/open_url
  model.rs            数据模型、Windows 附件评分、预置清单
  net.rs              GitHub/HTML 检测、流式下载（进度事件、取消、断点清理）
  version.rs          宽松版本比较（兼容日期版本号、预发布后缀）
scripts/gen_icons.py  图标生成脚本（PNG/ICO）
```
