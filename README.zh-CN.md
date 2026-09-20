# Resona · 回响

[English](README.md) | **简体中文**

[![CI](https://github.com/xuzhu-591/resona/actions/workflows/ci.yml/badge.svg)](https://github.com/xuzhu-591/resona/actions/workflows/ci.yml)

**感知每一次回响。**

Resona 是一款常驻 macOS 菜单栏的本地 AI 交互统计工具，支持 Codex 与 Claude Code。读取已有会话日志，不发送模型请求，不需要 API Key。

[贡献指南](CONTRIBUTING.md) · [更新日志](CHANGELOG.md) · [问题反馈](https://github.com/xuzhu-591/resona/issues)

![Resona 浮层：合成演示数据](assets/screenshot-popover.png)

## 能看到什么

- 最近完成轮次的首次响应时间与端到端输出速度。
- 统一的时间、来源筛选，响应趋势、两张分布图与模型分位数。
- 可检索的轮次记录、完整标识、指标依据与缺失数据说明。
- Codex 活跃及归档会话，支持同一 thread 的多个 rollout。
- Claude Code 主会话；已识别的子代理不计入主交互统计。
- 菜单栏显示、系统/深色/浅色主题、登录启动和本地数据来源设置。

| 指标 | 含义 |
| --- | --- |
| 首次响应 TTFT | 优先采用源工具的原生首 token 指标，缺失时使用可确认的助手事件估算 |
| 端到端速度 | 输出 token 数除以整轮耗时，包含等待与工具过程，不等同于模型纯生成速度 |
| N/A | 缺失或无法确认，不等同于零 |
| 未校验历史 | 从旧插件导入的记录，可查看；从原日志重新解析前不进入可信统计 |

## 安装

**正式签名下载包尚未发布**：Apple Developer ID 与公证配置仍待完成。目前可以按照下方步骤从源码构建并在本机运行。

目标系统为 macOS 13 及以上。正式安装包准备好后，[Releases](https://github.com/xuzhu-591/resona/releases) 将提供 Apple Silicon（aarch64）和 Intel（x64）两个版本。本地构建后，将生成的 `Resona.app` 拖入“应用程序”并打开。关闭窗口后应用继续常驻；停止采集请使用 **退出 Resona**。

源码构建和 CI 开发制品不带正式 Developer ID 签名。各制品的签名、公证与实际测试平台以 Release notes 为准。

## 快速使用

1. 左键点击菜单栏打开统计浮层，再次点击或按 Escape 收起；右键打开应用菜单。
2. 选择今天、24 小时或 7 天，两张分布图共享所选范围。
3. “查看详情”将当前筛选带入概览；“查看更多”打开轮次记录，可按来源、模型、时间筛选和搜索 thread / turn ID。
4. 使用自定义日志目录时，在“设置 → 数据来源”选择 Codex home 或 Claude projects 目录。Codex 的活跃和归档目录会一起读取。

首次整理历史时数据会逐步出现；单个文件读取失败会显示提示，不会阻断其他可读来源。

## 数据留在本机

| 数据 | 位置 |
| --- | --- |
| Resona 数据库与设置 | `~/.resona/monitor.sqlite3`，固定位置，不支持配置 |
| Codex 活跃会话 | `<Codex home>/sessions/` |
| Codex 归档会话 | `<Codex home>/archived_sessions/` |
| Claude Code 会话 | 默认 `~/.claude/projects/` |

Resona 不修改源日志，只存储统计事实和本地续读位置，不保存提示词、推理正文、命令或工具输出。关闭来源会暂停新增采集并保留历史；重新开启会继续补采。没有遥测和云端同步。

## 从源码构建

安装 Xcode Command Line Tools、Rust（rustup）、Node.js 与 pnpm。版本分别固定在 `rust-toolchain.toml`、`.node-version` 和 `package.json`。

```bash
git clone https://github.com/xuzhu-591/resona.git
cd resona
pnpm install --frozen-lockfile
pnpm dev
```

```bash
pnpm check
pnpm check:desktop    # 仅 macOS
pnpm build           # 本地 ad-hoc 签名的 app 和 DMG
```

核心库不依赖 WebView，可在 Linux 测试：

```bash
cargo test -p resona-core --locked
```

`pnpm dev:web` 使用明确标记的合成演示数据，便于开发界面。`pnpm dev` 启动真实桌面应用并使用固定的本地数据库。贡献代码和本地构建不需要 Apple 发布凭据。

## 兼容性

来源日志格式会随工具版本变化。无法确认的身份、缺失的继承前缀、冲突事实与不可信 token 计数会保留质量说明，不会被静默计入可信样本。

统计实际发生过的轮次。同一 thread 的多个 rollout 会按实际轮次归并；重复、归档或压缩表示不会增加样本。会话回退不会抹去已经发生的等待，废弃分支上实际完成的轮次仍保留在历史统计中。

Resona 是独立社区项目，并非 OpenAI 或 Anthropic 官方产品。

## 许可证

[MIT](LICENSE)，copyright © 2026 Xu Zhu。第三方依赖遵循各自许可证。
