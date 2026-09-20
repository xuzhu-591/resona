import { useEffect, useRef } from "react";
import * as echarts from "echarts/core";
import { ScatterChart, BarChart, LineChart } from "echarts/charts";
import { GridComponent, TooltipComponent, MarkLineComponent } from "echarts/components";
import { CanvasRenderer } from "echarts/renderers";
import type { Turn, Summary, DistributionBin, TrendBucket } from "./api/types";
import { value, time } from "./format";
echarts.use([
  ScatterChart,
  BarChart,
  LineChart,
  GridComponent,
  TooltipComponent,
  MarkLineComponent,
  CanvasRenderer,
]);
export function Chart({
  turns,
  metric,
  summary,
  trend = false,
  bins = [],
  buckets = [],
  onTurn,
}: {
  turns: Turn[];
  metric: "ttft" | "tps";
  summary: Summary;
  trend?: boolean;
  bins?: DistributionBin[];
  buckets?: TrendBucket[];
  onTurn?: (t: Turn) => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!ref.current) return;
    const chart = echarts.init(ref.current, undefined, { renderer: "canvas" });
    const val = (t: Turn) =>
      metric === "ttft" ? (t.ttftMs === null ? null : t.ttftMs / 1000) : t.tps;
    const valid = turns.filter((t) => val(t) !== null);
    const marks =
      metric === "ttft"
        ? [
            ["中位数", summary.ttftP50],
            ["p95", summary.ttftP95],
          ]
        : [
            ["p5", summary.tpsP5],
            ["中位数", summary.tpsP50],
          ];
    const histogram = !trend && bins.length > 0;
    const bucketed = trend && buckets.length > 0;
    chart.setOption({
      animation: false,
      grid: { left: trend ? 38 : 12, right: 18, top: trend ? 15 : 34, bottom: 26 },
      textStyle: { fontFamily: "-apple-system,BlinkMacSystemFont,sans-serif" },
      tooltip: {
        trigger: "item",
        backgroundColor: "#252b2e",
        borderColor: "#485052",
        textStyle: { color: "#eee9df" },
        formatter: (p: {
          data?: { turn?: Turn; bin?: DistributionBin; bucket?: TrendBucket };
          value?: unknown;
        }) => {
          if (p.data?.bin) {
            const b = p.data.bin;
            return `${value(b.lower)}–${value(b.upper)} ${metric === "ttft" ? "s" : "tok/s"}<br/>cx ${b.codexCount} · cc ${b.claudeCount}`;
          }
          if (p.data?.bucket) {
            const b = p.data.bucket;
            return `${time(b.startAtMs)}–${time(b.endExclusiveMs)} · ${b.provider}<br/>${b.summary.completedCount} 轮 · p50 ${value(metric === "ttft" ? b.summary.ttftP50 : b.summary.tpsP50)}`;
          }
          const t = p.data?.turn;
          return t
            ? `${time(t.completedAtMs)} · ${t.provider === "codex" ? "cx" : "cc"}<br/>${metric === "ttft" ? "首次响应" : "端到端速度"} ${value(val(t))} ${metric === "ttft" ? "s" : "tok/s"}`
            : "";
        },
      },
      xAxis: {
        type: histogram ? "category" : trend ? "time" : "value",
        data: histogram ? bins.map((b) => value((b.lower + b.upper) / 2)) : undefined,
        min: trend || histogram ? undefined : 0,
        axisLine: { show: true, lineStyle: { color: "#42484b" } },
        axisTick: { show: !trend },
        axisLabel: { color: "#9ba4ac", fontSize: 11 },
        splitLine: { show: trend, lineStyle: { color: "#2d3337", type: "dashed" } },
      },
      yAxis: {
        type: "value",
        show: trend,
        min: 0,
        max: trend || histogram ? undefined : 1,
        splitLine: { show: trend, lineStyle: { color: "#2d3337", type: "dashed" } },
        axisLabel: { color: "#9ba4ac" },
      },
      series: ["codex", "claude"].map((provider, index) => ({
        type: histogram ? "bar" : bucketed ? "line" : "scatter",
        stack: histogram ? "samples" : undefined,
        name: provider,
        symbol: provider === "codex" ? "circle" : "diamond",
        symbolSize: trend ? 7 : 6,
        itemStyle: { color: provider === "codex" ? "#66cfbe" : "#f0c57e", opacity: 0.9 },
        data: histogram
          ? bins.map((bin) => ({
              value: provider === "codex" ? bin.codexCount : bin.claudeCount,
              bin,
            }))
          : bucketed
            ? buckets
                .filter((b) => b.provider === provider)
                .map((bucket) => ({
                  value: [
                    (bucket.startAtMs + bucket.endExclusiveMs) / 2,
                    metric === "ttft" ? bucket.summary.ttftP50 : bucket.summary.tpsP50,
                  ],
                  bucket,
                }))
            : valid
                .filter((t) => t.provider === provider)
                .map((t, i) => ({
                  value: trend ? [t.completedAtMs, val(t)] : [val(t), 0.12 + (i % 5) * 0.08],
                  turn: t,
                })),
        markLine:
          !trend && !histogram && index === 0
            ? {
                silent: true,
                symbol: "none",
                label: { show: false },
                lineStyle: { color: "#a6adb4", type: "dashed", width: 1 },
                data: marks
                  .filter(([, n]) => n !== null)
                  .map(([label, n]) => ({ name: label, xAxis: n })),
              }
            : undefined,
      })),
    });
    chart.on("click", (p) => {
      const data = p.data as { turn?: Turn };
      if (data.turn) onTurn?.(data.turn);
    });
    const observer = new ResizeObserver(() => chart.resize());
    observer.observe(ref.current);
    return () => {
      observer.disconnect();
      chart.dispose();
    };
  }, [turns, metric, summary, trend, onTurn, bins, buckets]);
  return (
    <div
      ref={ref}
      className={trend ? "chart trend-chart" : "chart"}
      role="img"
      aria-label={trend ? "响应趋势" : metric === "ttft" ? "首次响应分布" : "端到端速度分布"}
    />
  );
}
