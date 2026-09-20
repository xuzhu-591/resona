# 原型生成提示词

生成方式：内置 Image Gen。日期：2026-09-20。三张独立图片，共享以下产品约束，各叠加一个视觉方向。画面数据均为演示数据。

## 共享约束

```text
Create realistic, production-quality UI designs with clear hierarchy, strong typography, intentional imagery, and purposeful spacing. Use case: ui-mockup. This is a NEW independent macOS menu bar utility, built later with Tauri 2 + React/TypeScript + Rust + SQLite. It passively reads local Codex and Claude Code logs. User explicitly dislikes spreadsheet/report-like UI and wants beautiful visual presentation.
Target dimensions: 1024 x 1536 pixels, portrait image. Show ONE enlarged, pixel-sharp macOS menu bar popover as the primary screen, realistic logical panel width about 460 px and height 640 px at 2x scale, with a slim top macOS menu-bar strip and quiet wallpaper margins. This is deliberately a contained menu bar panel, not a website or admin dashboard. No browser frame, sidebar, desktop full window, phone, illustration, fake 3D device or comparison collage.
Simplified Chinese UI, crisp readable 14-16pt body typography (rendered at 2x), elegant tabular numerals. Product header text "Latency". Current date is 2026-09-20 Asia/Shanghai; use "今天" or chronological times 14:32, 14:21, 14:08 rather than other dates. All numbers are realistic MOCKS, add discrete footer "演示数据".
Focus hero use case: understand latest completed AI turn, then today's performance at a glance, then access details. Menu bar item "cx · 7.8s". Hero latest "cx · gpt-5.6-sol", "最近完成", "7.8 s" label "首次响应", companion "21.2 tok/s" label "端到端速度". Do not claim server health, raw generation speed, cost, remaining quota, or fabricated health scores.
Supporting today summary mock data: 48 completed turns, cx 32, cc 16; TTFT median 6.4s and p95 18.2s; throughput median 24.6 tok/s and p5 9.1 tok/s. Use ONLY what fits comfortably; not every value must appear. One beautiful evidence-bearing chart or visual grouping and one small recent activity section maximum. A source/model chip, "今天 / 24 小时 / 7 天" control, one "查看详情" primary action and subtle settings icon. No giant CTA. Actual idle gaps in time chart stay gaps, no connecting through a long period with no turns. No decorative unrelated sparkline.
Prioritize space, alignment, grouping, typography; then fine separators; minimal borders/shadow. No cards inside cards, excessive badges, feature inventory, rainbow KPIs, loud gradients, dense table, sidebar or fake data heatmap. Entire content must fit naturally with consistent padding and no clipping. The concept should feel like a premium small macOS utility.
```

## 轻盈浅色

```text
Direction name: "轻盈浅色". A quietly luxurious light macOS utility. Soft pearl/ivory panel with fine warm-gray separators, graphite text, one restrained jade/teal accent. Natural macOS material only at the edges, highly legible opaque content. Large editorial typographic hero with generous breathing room; latest response time visually leads at upper left and throughput sits smaller beside it. Below, a thin full-width TPS timeline using refined teal points/line and visible idle breaks, subtle vertical scale, underneath a single compact line of today's median and p95. Footer two tidy recent activity rows with tiny cx/cc source monograms, aligned numeric columns without table grid. Place one small rounded "查看详情" button bottom-right and unobtrusive settings icon. Top header at same surface level, today control small and quiet. Airy typography and considered balance matter more than ornamental cards. Draw this direction only.
```

## 深色仪表

```text
Direction name: "深色仪表". Premium restrained dark graphite instrument panel, not cyberpunk. Charcoal base, warm near-white numerals, subdued amber/brass accent with a little desaturated teal only to differentiate source. Information hierarchy radically different: latest completed response and throughput presented as an oversized horizontal typographic readout across the top third; small source/model label above. Main supporting area is a horizontal response-time distribution dot plot of today's turns with labeled median 6.4s and p95 18.2s ticks, not a speedometer or percentage ring and not a health score. It clearly labels "今天 · 首次响应分布" and 48 turns. Near bottom show two source performance rows "cx 32轮" and "cc 16轮", one small last-completed timestamp, and a discrete full-width footer action "查看详情". Excellent typographic contrast, subtle etched lines, no nested cards, neon, bloom, glowing gradients, many tiny metrics, or conventional analytics dashboard. Show realistic app UI, not a marketing poster. Draw this direction only.
```

## 紧凑时间线

```text
Direction name: "紧凑时间线". Distinct activity-first light cool porcelain panel, ink/navy text, restrained cornflower blue accent, slight pale blue tint in selected range chip. This is an elegant compact personal activity journal. Primary body is a vertical chronological timeline with three completed turn entries at 14:32, 14:21, 14:08, spacious asymmetrical typography and thin time rail. The latest 14:32 entry is expanded inline (not inside another rounded card): cx gpt-5.6-sol, 7.8s first response and 21.2 tok/s end-to-end speed, visually dominant. Next rows more compact: cc Sonnet at 14:21 with 5.2s and 28.6 tok/s; cx gpt-5.6-sol at 14:08 with 9.1s and 19.4 tok/s. Above timeline place a compact day overview "今天 48轮" with 32 cx and 16 cc represented by one thin two-segment activity bar, small median 6.4s text; filter selector aligned right. Bottom a light separator with button "查看详情" and settings. Make the timeline the main visualization rather than a line chart. Source identity and recency are immediately clear. No chat bubbles, actual conversation text, invented tasks or project names, table grid, dashboards, health indicators or many cards. Draw this direction only.
```

