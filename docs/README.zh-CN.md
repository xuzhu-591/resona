# Resona · 回响

一款常驻 macOS 菜单栏的本地 AI 交互统计工具，支持 Codex 与 Claude Code。

## 安装与使用

从 GitHub Releases 选择与你的 Mac 架构对应的 DMG，将 Resona 拖入 Applications 并打开。菜单栏点击图标可查看最近完成轮次、今天/24 小时/7 天的双分布和最近动态。

“查看详情”继承当前筛选；“查看更多”打开全局记录。详情中可按模型、来源、时间筛选，搜索 thread / turn ID，查看单轮指标依据。设置中可调整登录启动、菜单栏显示、主题及输入目录。

## 指标

| 指标 | 含义 |
| --- | --- |
| 首次响应 TTFT | 优先采用原生首 token 指标，缺失时使用可确认的助手事件估算 |
| 端到端速度 TPS | 输出 token 数除以整轮耗时，包含等待与工具过程 |
| N/A | 缺失或无法确认，不等同于零 |
| 未校验历史 | 从旧插件导入的记录，可查看但不进入可信分位统计 |

统计实际发生过的轮次。Codex 同一 thread 的多个 rollout 分别读取、按实际轮次归并；归档、压缩与复制不会增加样本，回退前已经完成的轮次仍保留。

## 本地数据

数据和设置固定存放 `~/.resona/monitor.sqlite3`，不可更改位置。Codex 选择 home 后，同时读取其中 `sessions/` 与 `archived_sessions/`；Claude 默认读取 `~/.claude/projects/`。来源关闭后保留历史，重新开启会补采。

仅存储统计事实与续读位置，不保存提示词、推理正文或工具输出，不上传遥测，不需要 API Key。关闭窗口不会退出；退出请使用菜单栏的“退出 Resona”。

首次整理历史时数据会逐步出现；单个文件读失败不会阻断其他来源。发布包的签名、公证和实机测试状态以相应 Release notes 为准。

[English README](../README.md) · [贡献指南](../CONTRIBUTING.md)
