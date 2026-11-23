<p align="center">
  <img src="assets/arca-logo.png" alt="Arca Logo" width="160" height="160">
</p>

# Arca

[English](README.md) | [简体中文](README-zh.md)

> 项目名 **Arca** 取自拉丁语「*arca*」——在历史上指用于保管贵重文献、账目与珍宝的宝匣、金库或方舟。在现代计算中，Arca 作为 Wayland 桌面环境中轻快、安全且以键盘操作为核心的现代文件容器而设计。

Arca 是一个专为 Wayland 构建的高性能文件管理器，也是 Tessera 桌面生态系统的核心原生组件。其视觉语言严格契合 Tessera 设计系统，同时完全支持在任何标准 Wayland 混成器（Compositor）上独立运行。Arca 原生基于 Optics 图形与 UI 技术栈构建（**optics-app** 应用外壳 · **optics-gfx** Vulkan 渲染器 · **optics-ui** 即时模式 UI · **optics-window** 平台运行时 · **optics-motion** 动力学动效）。

![Rust](https://img.shields.io/badge/rust-1.85%2B-informational)

---

## 核心特性

- **键盘优先的导航体验**：支持完整的后退/前进/上级目录历史栈、带 Tab 路径补全的可编辑地址栏（`Ctrl+L`），以及快速书签（主目录、XDG 用户目录、用户自定义固定项）。
- **三种视图形态**：自适应图标网格视图、详尽可排序列视图以及响应式 Miller 多列视图；借助严格的视口虚拟化渲染（Virtualized Rows），大型文件夹浏览始终保持极速且内存受控。
- **异步缩略图流水线**：高性能非阻塞图片与音频专辑封面（FLAC/MP3）缩略图提取，严格遵循 FreeDesktop 缩略图缓存规范，并通过 GPU 纹理即时渲染。
- **多标签页工作区**：每个标签页独立维护浏览路径、历史记录、选中状态与过滤条件，并支持平滑拖拽重排（`Ctrl+T`、`Ctrl+W`、`Ctrl+Tab`、`Ctrl+1..9`）。
- **Quick Look 快速预览**：按下 `Space`（空格键）即可唤起带 Optics `optics-motion` 弹簧动效的弹窗，支持文本摘录、图片尺寸与预览、音频内嵌专辑封面；`Space`、`Esc` 或点击弹窗外部均可关闭。
- **安全的文件操作**：双击或 `Enter` 使用默认应用打开文件，新建文件夹（`Ctrl+Shift+N`）自动重名规避，原地安全重命名（`Ctrl+E`/`F2`）绝不覆盖既有文件，遵循 FreeDesktop 规范的回收站/恢复/彻底删除（`Del`/`Shift+Del`），复制/剪切/粘贴（`Ctrl+C/X/V`）完整支持 `text/uri-list` 与 `x-special/gnome-copied-files`，支持应用内及跨应用拖放、多步撤销/重做以及右键上下文菜单；8 MiB 以上的传输在后台线程执行并在状态栏显示进度。
- **XDG 桌面 Portal 文件选择器**：原生支持桌面 Portal 文件选择器（`arca --chooser-prompt`），遵循契约 v6，支持 `OpenFile`、`SaveFile`、`SaveFiles`、`OpenDirectory`、多选、MIME/Glob 过滤、自定义选项控件以及文件覆盖防误触保护；响应在写出前会依据请求进行校验；同时提供独立的命令行文件选择工具（`--choose-file`、`--choose-files`、`--choose-dir`、`--save-file`）。
- **自适应主题**：实时跟随系统深色/浅色偏好，工具栏外观设置支持在「跟随系统 / 浅色 / 深色」之间自由切换。
- **持久化配置**：视图模式、隐藏文件显示、排序规则、主题以及书签统一存储于 `~/.config/arca/arca.conf`。

---

## 快速上手

### 1. 编译构建

Arca 通过官方 Rust 绑定链接 Optics C 库（`optics-app` / `optics-window` / `optics-ui` / `optics-gfx`）。Optics 同时支持静态与动态链接，由构建方决定（`OPTICS_STATIC=1`、`OPTICS_DYNAMIC=1`、可叠加的 `static` Cargo feature，或不设开关时的共享优先探测）；发布产物固定使用静态模式。Vulkan 加载器、Wayland 客户端与字体栈始终为动态链接：

```bash
cargo build --release
```

### 2. 启动图形文件管理器

```bash
# 在当前工作目录启动 Arca
./target/release/arca

# 或以 Miller 多列视图直接打开指定目录
./target/release/arca --miller ~/Documents
```

### 3. 作为独立 CLI 文件选择器运行

```bash
# 选择单个文件（将所选绝对路径打印至 stdout）
./target/release/arca --choose-file ~/Downloads

# 选择目录
./target/release/arca --choose-dir ~/Projects

# 保存文件并指定默认推荐文件名
./target/release/arca --save-file ~/Documents/report.pdf
```

### 4. 作为 XDG 桌面 Portal Prompter 运行

```bash
# 从 stdin 读取 Portal JSON 请求包，并将选取的响应 JSON 输出至 stdout
echo '{"title":"Select File","multiple":false,"directory":false}' | ./target/release/arca --chooser-prompt
```

---

## CLI 常用命令

| 命令 | 说明 |
| :--- | :--- |
| `arca [path]` | 在指定路径（或当前工作目录）打开交互式文件管理器 |
| `arca --grid [path]` | 以**网格视图**打开交互式文件管理器 |
| `arca --list [path]` | 以**列表视图**打开交互式文件管理器 |
| `arca --miller [path]` | 以 **Miller 多列视图**打开交互式文件管理器 |
| `arca --choose-file [path]` | 独立命令行单文件选择器（路径输出至 stdout） |
| `arca --choose-files [path]` | 独立命令行多文件选择器（多路径输出至 stdout） |
| `arca --choose-dir [path]` | 独立命令行目录选择器（目录路径输出至 stdout） |
| `arca --save-file <path>` | 独立命令行文件保存选择器（输出目标保存路径） |
| `arca --chooser-prompt` | 作为 XDG Desktop Portal FileChooser Prompter 运行（基于 stdio 的 JSON 协议） |
| `arca --help` | 查看命令行参数说明与完整用法 |
| `arca --version` | 查看 Arca 版本信息 |

---

## 常用快捷键

| 快捷键 | 动作 |
| :--- | :--- |
| `↑` / `↓` | 向上 / 向下移动选中项 |
| `←` / `→` | 在网格中横向遍历 / 在 Miller 列之间穿梭 |
| `Enter` | 打开选中目录或运行文件 |
| `Space` | 打开 / 关闭 Quick Look 快速预览弹窗 |
| `Backspace` / `Alt+↑` | 返回上级目录 |
| `Alt+←` / `Alt+→` | 浏览历史后退 / 前进 |
| `Ctrl+L` | 聚焦可编辑地址栏（输入路径后按 `Enter` 跳转） |
| `Ctrl+F` | 聚焦实时过滤搜索框 |
| `Ctrl+R` | 刷新当前目录列表 |
| `Ctrl+H` | 显示 / 隐藏以点开头的隐藏文件 |
| `Ctrl+Shift+N` | 新建文件夹并进入原地命名模式 |
| `Ctrl+E` | 对选中的文件/文件夹进行原地重命名 |
| `Del` | 将选中项移入 FreeDesktop 回收站 |
| `Ctrl+C` / `Ctrl+X` / `Ctrl+V` | 复制 / 剪切 / 粘贴选中的文件 |
| `Ctrl+T` / `Ctrl+W` | 新建 / 关闭标签页 |
| `Ctrl+Tab` / `Ctrl+Shift+Tab` | 切换至下一个 / 上一个标签页 |
| `Ctrl+1` … `Ctrl+9` | 按序号直接跳转到指定标签页 |
| `Esc` | 关闭弹窗、取消原地重命名或退出程序 |

---

## 架构拓扑

Arca 按照向心架构（Concentric Architecture）组织为四个层次分明的 Cargo 工作区 Crate：

| Crate | 层次 | 职责与子系统边界 |
| :--- | :--- | :--- |
| **`crates/arca`** | 宿主入口层 | 轻量宿主二进制：CLI 参数解析、独立选择器分发、Wayland 事件循环、Portal Prompter 生命周期管理。 |
| **`crates/arca-ui`** | 视口表现层 | 基于 Optics（`optics-app`/`optics-ui`/`optics-window`/`optics-gfx`/`optics-motion`）的渲染与交互层：视口虚拟化列表/网格/Miller视图、标签页、Quick Look 动效、1 MiB Frame Arena 内存预算管控。 |
| **`crates/arca-engine`** | 领域引擎层 | 核心业务逻辑：目录模型、导航历史栈、书签、安全文件操作（防覆盖重命名/新建）、配置文件序列化、缩略图缓存计算。纯无头设计，零 GUI 依赖。 |
| **`crates/arca-xdg`** | 标准协议层 | 纯标准库实现（纯 `std`，零外部依赖）：XDG 基础目录、用户目录、RFC 8089 文件 URI、共享 MIME 类型嗅探、桌面入口项（.desktop）与 `mimeapps.list` 解析、FreeDesktop 回收站 v1.0 规范、RFC 1321 MD5 摘要。 |

---

## 相关文档

- **[快速上手 (Getting Started)](docs/tutorials/01-getting-started.md)**：从零启动 Arca、浏览目录并使用标签页的实操教程。
- **[操作指南 (How-To Guides)](docs/how-to/index.md)**：文件管理操作、外观定制、Quick Look 预览以及 Portal 文件选择器集成指南。
- **[CLI 参数参考 (CLI Reference)](docs/reference/cli.md)**：命令行参数、标志位与独立选择器模式详解。
- **[配置规范参考 (Configuration Reference)](docs/reference/configuration.md)**：`~/.config/arca/arca.conf` 的键值语法与默认值。
- **[快捷键一览 (Keyboard Shortcuts)](docs/reference/keyboard-shortcuts.md)**：完整的键盘操作与快捷键映射。
- **[Portal Prompter 协议参考 (Portal Protocol)](docs/reference/portal-protocol.md)**：Prompter 进程契约与 stdio JSON 请求/响应 Schema。
- **[架构与虚拟化渲染 (Architecture & Rendering)](docs/explanation/architecture-and-rendering.md)**：即时模式 UI、1 MiB 内存 Arena 预算与视口虚拟化设计。
- **[子系统架构蓝图 (Subsystem Architecture)](docs/dev/subsystems.md)**：工作区拓扑、Crate 边界与架构不变量约束的权威蓝图。
- **[工程规范总览 (Engineering Standards)](docs/dev/standards/README.md)**：架构边界、不变量治理、ADR 准则与 PR 门禁。
- **[架构决策记录 (ADR)](docs/adr/index.md)**：不可变的核心技术决策演进、权衡考量与否定知识记录。
