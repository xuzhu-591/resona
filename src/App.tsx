import { useCallback, useEffect, useRef, useState } from "react";
import {
  GearSix,
  ArrowRight,
  CaretRight,
  X,
  Copy,
  FolderOpen,
  ArrowClockwise,
  MagnifyingGlass,
  Check,
  WarningCircle,
  Clock,
  Waveform,
} from "@phosphor-icons/react";
import * as Switch from "@radix-ui/react-switch";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { api, desktop } from "./api/client";
import type { Bootstrap, Dashboard, Filters, Settings, Turn, TurnPage } from "./api/types";
import { Chart } from "./Charts";
import { value, time, status } from "./format";

const defaultFilters: Filters = { range: "today", providers: [], model: null };
function Badge({ provider }: { provider: string }) {
  return <span className={`badge ${provider}`}>{provider === "codex" ? "CX" : "CC"}</span>;
}
function Brand() {
  return (
    <div className="brand">
      <Waveform size={27} weight="bold" />
      <span>Resona</span>
    </div>
  );
}
function Toggle({
  checked,
  onChange,
  label,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  label: string;
}) {
  return (
    <Switch.Root className="switch" checked={checked} onCheckedChange={onChange} aria-label={label}>
      <Switch.Thumb className="switch-thumb" />
    </Switch.Root>
  );
}
function Empty({ text = "这个时间范围还没有记录" }: { text?: string }) {
  return (
    <div className="empty">
      <Waveform size={30} />
      <span>{text}</span>
      <small>完成一次对话后，回响会出现在这里</small>
    </div>
  );
}
export default function App() {
  const initial = new URLSearchParams(location.search).get("view") ?? "overview";
  const [view, setView] = useState(initial),
    [settingsTab, setSettingsTab] = useState("general");
  const [boot, setBoot] = useState<Bootstrap | null>(null),
    [dash, setDash] = useState<Dashboard | null>(null);
  const [filters, setFilters] = useState<Filters>(defaultFilters),
    [metric, setMetric] = useState<"ttft" | "tps">("ttft");
  const [selected, setSelected] = useState<Turn | null>(null),
    [page, setPage] = useState<TurnPage | null>(null);
  const [cursor, setCursor] = useState<string | null>(null),
    [history, setHistory] = useState<(string | null)[]>([]);
  const [sort, setSort] = useState("recent"),
    [stateFilter, setStateFilter] = useState("all"),
    [search, setSearch] = useState("");
  const [error, setError] = useState(""),
    [toast, setToast] = useState(""),
    [busy, setBusy] = useState(false),
    [refresh, setRefresh] = useState(0),
    [clockMinute, setClockMinute] = useState(() => Math.floor(Date.now() / 60000));
  const notify = useCallback((s: string) => {
    setToast(s);
    setTimeout(() => setToast(""), 2500);
  }, []);
  const initialized = useRef(false);
  const reload = useCallback(async () => {
    try {
      const b = await api.bootstrap();
      setBoot(b);
      if (!initialized.current) {
        initialized.current = true;
        setFilters((current) => ({ ...current, range: b.settings.defaultRange }));
      }
    } catch (e) {
      setError(String(e));
    }
  }, []);
  useEffect(() => {
    void reload();
    const visibleReload = async () => {
      if (!document.hidden && (!desktop || (await invoke<boolean>("window_visible_v1")))) {
        setClockMinute(Math.floor(Date.now() / 60000));
        void reload();
      }
    };
    const id = setInterval(visibleReload, 3000);
    document.addEventListener("visibilitychange", visibleReload);
    addEventListener("focus", visibleReload);
    return () => {
      clearInterval(id);
      document.removeEventListener("visibilitychange", visibleReload);
      removeEventListener("focus", visibleReload);
    };
  }, [reload]);
  useEffect(() => {
    const theme = boot?.settings.theme ?? "dark";
    const mq = matchMedia("(prefers-color-scheme: dark)");
    const apply = () => {
      document.documentElement.dataset.theme =
        theme === "system" ? (mq.matches ? "dark" : "light") : theme;
    };
    apply();
    mq.addEventListener("change", apply);
    return () => mq.removeEventListener("change", apply);
  }, [boot?.settings.theme]);
  useEffect(() => {
    let valid = true;
    if (view === "settings") return;
    setBusy(true);
    const f = filters.range === "all" ? { ...filters, range: "7d" } : filters;
    api
      .dashboard(f)
      .then((d) => {
        if (valid) {
          setDash(d);
          setError("");
        }
      })
      .catch((e) => {
        if (valid) setError(String(e));
      })
      .finally(() => {
        if (valid) setBusy(false);
      });
    return () => {
      valid = false;
    };
  }, [filters, boot?.dataRevision, view, refresh, clockMinute]);
  useEffect(() => {
    if (view !== "turns") return;
    let valid = true;
    setBusy(true);
    api
      .list({ filters, status: stateFilter, search, sort, pageSize: 20, cursor })
      .then((p) => {
        if (valid) {
          setPage(p);
          setError("");
        }
      })
      .catch((e) => {
        if (valid) setError(String(e));
      })
      .finally(() => {
        if (valid) setBusy(false);
      });
    return () => {
      valid = false;
    };
  }, [filters, stateFilter, search, sort, cursor, view, refresh]);
  useEffect(() => {
    if (!desktop) return;
    const p = listen<{ destination: string; context?: { filters?: Filters; turn?: Turn } }>(
      "resona://navigate/v1",
      (e) => {
        setView(e.payload.destination);
        if (e.payload.context?.filters) setFilters(e.payload.context.filters);
        if (e.payload.context?.turn) setSelected(e.payload.context.turn);
        setCursor(null);
        setHistory([]);
      },
    );
    return () => {
      void p.then((f) => f());
    };
  }, []);
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        if (selected) setSelected(null);
        else if (view === "popover" && desktop) void invoke("hide_popover_v1");
      }
    };
    addEventListener("keydown", key);
    return () => removeEventListener("keydown", key);
  }, [selected, view]);
  const selectedProvider = selected?.provider;
  const selectedKey = selected?.turnKey;
  useEffect(() => {
    if (!selectedProvider || !selectedKey) return;
    let valid = true;
    void api
      .turn(selectedProvider, selectedKey)
      .then((turn) => {
        if (valid) setSelected(turn);
      })
      .catch((e) => {
        if (valid) setError(String(e));
      });
    return () => {
      valid = false;
    };
  }, [selectedProvider, selectedKey, boot?.dataRevision]);
  const navigate = useCallback(
    (destination: string, context?: { filters?: Filters; turn?: Turn }) => {
      if (desktop) {
        void invoke("navigate_v1", { destination, context }).catch((e) => setError(String(e)));
      } else {
        setView(destination);
        if (context?.filters) setFilters(context.filters);
        if (context?.turn) setSelected(context.turn);
        setCursor(null);
        setHistory([]);
      }
    },
    [],
  );
  const openTurn = useCallback(
    (t: Turn) => {
      if (view === "popover")
        navigate("turns", { filters: { range: "all", providers: [] }, turn: t });
      else {
        setSelected(t);
        setView("turns");
      }
    },
    [view, navigate],
  );
  const updateFilters = (patch: Partial<Filters>) => {
    setFilters((f) => ({ ...f, ...patch }));
    setCursor(null);
    setHistory([]);
  };
  const save = async (patch: Partial<Settings>) => {
    if (!boot) return;
    setBusy(true);
    try {
      const s = await api.save({ ...boot.settings, ...patch });
      setBoot({ ...boot, settings: s });
      notify("已保存");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };
  const summary = dash?.summary;
  const recent =
    boot?.recent.find(
      (t) =>
        t.status === "completed" &&
        t.identityStatus === "verified" &&
        t.threadKind === "primary" &&
        t.recordSource === "parsed",
    ) ?? null;
  const rangeControl = (all = false) => (
    <div className="range-controls">
      <div className="segmented">
        {[
          ["today", "今天"],
          ["24h", "24 小时"],
          ["7d", "7 天"],
          ...(all ? [["all", "全部"]] : []),
        ].map(([key, label]) => (
          <button
            key={key}
            className={filters.range === key ? "active" : ""}
            onClick={() => updateFilters({ range: key })}
          >
            {label}
          </button>
        ))}
        {view !== "popover" && (
          <button
            className={filters.range === "custom" ? "active" : ""}
            onClick={() =>
              updateFilters({
                range: "custom",
                startDate:
                  filters.startDate ??
                  new Date(Date.now() - 6 * 86400000).toLocaleDateString("en-CA"),
                endDate: filters.endDate ?? new Date().toLocaleDateString("en-CA"),
              })
            }
          >
            自定义
          </button>
        )}
      </div>
      {filters.range === "custom" && (
        <div className="date-range">
          <input
            aria-label="开始日期"
            type="date"
            value={filters.startDate ?? ""}
            onChange={(e) => updateFilters({ startDate: e.target.value })}
          />
          <span>至</span>
          <input
            aria-label="结束日期"
            type="date"
            value={filters.endDate ?? ""}
            onChange={(e) => updateFilters({ endDate: e.target.value })}
          />
        </div>
      )}
    </div>
  );
  const sourceSelect = (
    <select
      aria-label="来源筛选"
      value={filters.providers[0] ?? ""}
      onChange={(e) => updateFilters({ providers: e.target.value ? [e.target.value] : [] })}
    >
      <option value="">全部来源</option>
      <option value="codex">Codex</option>
      <option value="claude">Claude Code</option>
    </select>
  );
  const distribution = (kind: "ttft" | "tps") => (
    <section className="distribution">
      <div className="section-heading">
        <h3>{kind === "ttft" ? "首次响应分布" : "端到端速度分布"}</h3>
        <span>
          {kind === "ttft"
            ? `中位数 ${value(summary?.ttftP50)} s · p95 ${value(summary?.ttftP95)} s`
            : `p5 ${value(summary?.tpsP5)} · 中位数 ${value(summary?.tpsP50)} tok/s`}
        </span>
      </div>
      {dash && summary && summary.completedCount > 0 ? (
        <Chart
          turns={dash.points}
          metric={kind}
          bins={kind === "ttft" ? dash.ttftBins : dash.tpsBins}
          summary={summary}
          onTurn={openTurn}
        />
      ) : (
        <Empty text="暂无有效样本" />
      )}
      <div className="chart-caption">
        <span>
          {kind === "ttft" ? (summary?.ttftValidCount ?? "N/A") : (summary?.tpsValidCount ?? "N/A")}{" "}
          个有效样本
          {(kind === "ttft" ? dash?.ttftBins.length : dash?.tpsBins.length) ? " · 分箱计数" : ""}
        </span>
        <span>{kind === "ttft" ? "首次响应 (s)" : "端到端速度 (tok/s)"}</span>
      </div>
    </section>
  );
  return (
    <main
      className={`app ${view === "popover" ? "popover" : ""} ${view === "settings" ? "settings-window" : ""} ${view === "turns" ? "turns-window" : ""} ${selected && view === "turns" ? "has-detail" : ""}`}
    >
      {view !== "popover" && (
        <header className="app-header">
          <Brand />
          <nav>
            {view === "settings" ? (
              <h3>设置</h3>
            ) : (
              <div className="segmented">
                <button
                  className={view === "overview" ? "active" : ""}
                  onClick={() => {
                    setView("overview");
                    if (filters.range === "all") updateFilters({ range: "7d" });
                  }}
                >
                  表现概览
                </button>
                <button
                  className={view === "turns" ? "active" : ""}
                  onClick={() => setView("turns")}
                >
                  轮次记录
                </button>
              </div>
            )}
          </nav>
          <button
            className="icon-button"
            aria-label={view === "settings" ? "返回概览" : "打开设置"}
            onClick={() => navigate(view === "settings" ? "overview" : "settings")}
          >
            <GearSix size={22} />
          </button>
        </header>
      )}
      {error && (
        <div className="notice error" role="alert">
          <WarningCircle size={16} />
          <span>{error.includes("STALE_CURSOR") ? "记录已更新，请刷新列表" : error}</span>
          <button
            onClick={() => {
              setCursor(null);
              setHistory([]);
              setRefresh((n) => n + 1);
              setError("");
            }}
          >
            重试
          </button>
        </div>
      )}
      {boot?.scanning && (
        <div className="notice">
          <ArrowClockwise className="spin" size={15} />
          <span>正在整理本地历史，统计尚不完整</span>
        </div>
      )}
      {view === "popover" && (
        <>
          <header className="popover-header">
            <div>
              <Brand />
              <p>感知每一次回响</p>
            </div>
            <button
              className="icon-button"
              aria-label="打开设置"
              onClick={() => navigate("settings")}
            >
              <GearSix size={23} />
            </button>
          </header>
          <section className="latest">
            <div className="section-heading">
              <h2>最近完成</h2>
              <span>{time(recent?.completedAtMs)}</span>
            </div>
            {recent ? (
              <>
                <div className="latest-model">
                  <Badge provider={recent.provider} />
                  <span>
                    {recent.provider === "codex" ? "cx" : "cc"} · {recent.model ?? "N/A"}
                  </span>
                </div>
                <div className="hero-metrics">
                  <div>
                    <strong>
                      {value(recent.ttftMs === null ? null : recent.ttftMs / 1000)}
                      <small> s</small>
                    </strong>
                    <span>首次响应</span>
                  </div>
                  <div>
                    <strong>
                      {value(recent.tps)}
                      <small> tok/s</small>
                    </strong>
                    <span>端到端速度</span>
                  </div>
                </div>
              </>
            ) : (
              <Empty text="等待第一轮完成" />
            )}
            {boot && boot.active.length > 0 && (
              <div className="activity">
                <Clock size={14} />
                {boot.active.length} 轮进行中或等待更新
              </div>
            )}
          </section>
          <section className="popover-summary">
            <div className="section-heading">
              <h2>表现概览</h2>
              {rangeControl()}
            </div>
            <div className="summary-line">
              <strong>{summary?.completedCount ?? "N/A"} 轮完成</strong>
              <span className="legend cx">cx {summary?.codexCount ?? "N/A"}</span>
              <span className="legend cc">cc {summary?.claudeCount ?? "N/A"}</span>
              {sourceSelect}
            </div>
          </section>
          {distribution("ttft")}
          {distribution("tps")}
          <section className="recent">
            <div className="section-heading">
              <h2>最近动态</h2>
              <button
                className="text-button"
                onClick={() => navigate("turns", { filters: { range: "all", providers: [] } })}
              >
                查看更多
                <CaretRight />
              </button>
            </div>
            {boot?.recent.slice(0, 2).map((t) => (
              <button
                className="recent-row"
                key={t.provider + t.turnKey}
                onClick={() => openTurn(t)}
              >
                <Badge provider={t.provider} />
                <div>
                  <strong>
                    {t.provider === "codex" ? "cx" : "cc"} · {t.model ?? "N/A"}
                  </strong>
                  <small>
                    {status(t.status)}
                    {t.hasTool ? " · 含工具调用" : ""}
                  </small>
                </div>
                <div className="recent-values">
                  <time>{time(t.completedAtMs)}</time>
                  <span>
                    {value(t.ttftMs === null ? null : t.ttftMs / 1000)} s　 {value(t.tps)} tok/s
                  </span>
                </div>
              </button>
            ))}
          </section>
          <footer className="popover-footer">
            <small>
              {desktop ? "本地统计 · " + time(boot?.sources[0]?.lastSuccessAtMs) : "演示数据"}
            </small>
            <button className="gold-button" onClick={() => navigate("overview", { filters })}>
              查看详情
              <ArrowRight />
            </button>
          </footer>
        </>
      )}
      {(view === "overview" || view === "turns") && (
        <>
          <section className="page-heading">
            <div>
              <h1>{view === "overview" ? "表现概览" : "轮次记录"}</h1>
              <p>{view === "overview" ? "看清每一次等待与输出的节奏" : "每一次交互，都有迹可循"}</p>
            </div>
            <div className="filters">
              {rangeControl(view === "turns")}
              <div className="filter-row">
                {sourceSelect}
                <select
                  aria-label="模型筛选"
                  value={filters.model ?? ""}
                  onChange={(e) => updateFilters({ model: e.target.value || null })}
                >
                  <option value="">全部模型</option>
                  {Array.from(
                    new Set(dash?.models.flatMap((m) => (m.model ? [m.model] : [])) ?? []),
                  ).map((m) => (
                    <option key={m}>{m}</option>
                  ))}
                </select>
              </div>
            </div>
          </section>
          {view === "overview" ? (
            <>
              <div className="metrics-strip">
                <div>
                  <strong>{summary?.completedCount ?? "N/A"}</strong> 轮完成
                </div>
                <div>
                  首次响应中位数{" "}
                  <strong>
                    {value(summary?.ttftP50)}
                    <small> s</small>
                  </strong>
                </div>
                <div>
                  速度中位数{" "}
                  <strong>
                    {value(summary?.tpsP50)}
                    <small> tok/s</small>
                  </strong>
                </div>
                <div>
                  <span className="legend cx">cx {summary?.codexCount ?? "N/A"}</span>
                  <span className="legend cc">cc {summary?.claudeCount ?? "N/A"}</span>
                </div>
              </div>
              <section className="trend-section">
                <div className="section-heading">
                  <h2>响应趋势{dash?.trendBuckets.length ? " · 分时段中位数" : ""}</h2>
                  <div className="segmented">
                    <button
                      className={metric === "ttft" ? "active" : ""}
                      onClick={() => setMetric("ttft")}
                    >
                      首次响应
                    </button>
                    <button
                      className={metric === "tps" ? "active" : ""}
                      onClick={() => setMetric("tps")}
                    >
                      端到端速度
                    </button>
                  </div>
                  <span>
                    <span className="legend cx">cx</span>
                    <span className="legend cc">cc</span>
                  </span>
                </div>
                {dash && summary && summary.completedCount > 0 ? (
                  <Chart
                    turns={dash.points}
                    metric={metric}
                    summary={summary}
                    trend
                    buckets={dash.trendBuckets}
                    onTurn={openTurn}
                  />
                ) : (
                  <Empty />
                )}
              </section>
              <div className="distribution-grid">
                {distribution("ttft")}
                {distribution("tps")}
              </div>
              <section className="model-section">
                <div className="section-heading">
                  <h2>模型表现</h2>
                  <button className="gold-button" onClick={() => setView("turns")}>
                    查看轮次
                    <ArrowRight />
                  </button>
                </div>
                <table>
                  <thead>
                    <tr>
                      <th>来源 / 模型</th>
                      <th>轮次</th>
                      <th>首次响应 p50 / p95</th>
                      <th>速度 p50 / p5</th>
                    </tr>
                  </thead>
                  <tbody>
                    {dash?.models.map((m) => (
                      <tr
                        key={m.provider + (m.model ?? "")}
                        onClick={() => {
                          updateFilters({ providers: [m.provider], model: m.model });
                          setView("turns");
                        }}
                      >
                        <td>
                          <Badge provider={m.provider} />
                          <button className="text-button">{m.model ?? "N/A"}</button>
                        </td>
                        <td>{m.summary.completedCount}</td>
                        <td>
                          {value(m.summary.ttftP50)} s / {value(m.summary.ttftP95)} s
                        </td>
                        <td>
                          {value(m.summary.tpsP50)} / {value(m.summary.tpsP5)} tok/s
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </section>
            </>
          ) : (
            <section className="records">
              <div className="record-toolbar">
                <label className="search">
                  <MagnifyingGlass />
                  <input
                    placeholder="搜索 thread / turn ID"
                    value={search}
                    onChange={(e) => {
                      setSearch(e.target.value);
                      setCursor(null);
                      setHistory([]);
                    }}
                  />
                </label>
                <select
                  aria-label="状态筛选"
                  value={stateFilter}
                  onChange={(e) => {
                    setStateFilter(e.target.value);
                    setCursor(null);
                    setHistory([]);
                  }}
                >
                  <option value="all">全部状态</option>
                  <option value="completed">已完成</option>
                  <option value="aborted">已中止</option>
                  <option value="failed">失败</option>
                  <option value="incomplete">信息不足</option>
                </select>
                <select
                  aria-label="排序方式"
                  value={sort}
                  onChange={(e) => {
                    setSort(e.target.value);
                    setCursor(null);
                    setHistory([]);
                  }}
                >
                  <option value="recent">最近完成优先</option>
                  <option value="ttft">首次响应最慢</option>
                  <option value="tps">输出速度最慢</option>
                </select>
                <button
                  className="text-button"
                  onClick={() => {
                    setCursor(null);
                    setHistory([]);
                    setRefresh((n) => n + 1);
                  }}
                >
                  <ArrowClockwise />
                  刷新
                </button>
              </div>
              {boot && page && boot.dataRevision !== page.dataRevision && (
                <div className="notice">有更新的记录，点击刷新查看</div>
              )}
              <div className="table-scroll">
                <table>
                  <thead>
                    <tr>
                      <th>来源 / 模型</th>
                      <th>完成时间</th>
                      <th>首次响应</th>
                      <th>端到端速度</th>
                      <th>状态</th>
                      <th />
                    </tr>
                  </thead>
                  <tbody>
                    {page?.items.map((t) => (
                      <tr
                        key={t.provider + t.turnKey}
                        className={selected?.turnKey === t.turnKey ? "selected" : ""}
                        onClick={() => setSelected(t)}
                      >
                        <td>
                          <Badge provider={t.provider} />
                          {t.model ?? "N/A"}
                        </td>
                        <td>{time(t.completedAtMs, true)}</td>
                        <td>{value(t.ttftMs === null ? null : t.ttftMs / 1000)} s</td>
                        <td>{value(t.tps)} tok/s</td>
                        <td>
                          <span className={`status ${t.status}`}>{status(t.status)}</span>
                        </td>
                        <td>
                          <button className="icon-button" aria-label={`查看轮次 ${t.nativeTurnId}`}>
                            <CaretRight />
                          </button>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
              {!page?.items.length && !busy && <Empty />}
              <div className="pagination">
                <small>
                  共 {page?.total ?? 0} 条 · 第 {history.length + 1} 页
                </small>
                <div>
                  <button
                    disabled={!history.length}
                    onClick={() => {
                      setCursor(history.at(-1) ?? null);
                      setHistory((h) => h.slice(0, -1));
                    }}
                  >
                    上一页
                  </button>
                  <button
                    disabled={!page?.nextCursor}
                    onClick={() => {
                      setHistory((h) => [...h, cursor]);
                      setCursor(page?.nextCursor ?? null);
                    }}
                  >
                    下一页
                  </button>
                </div>
              </div>
            </section>
          )}
          <footer className="page-footer">
            <span>
              {desktop ? "仅保存在这台 Mac" : "演示数据"}
              {summary?.excludedCount ? ` · ${summary.excludedCount} 条未计入可信统计` : ""}
            </span>
            <span>{busy ? "更新中…" : `更新于 ${time(dash?.asOfMs)}`}</span>
          </footer>
        </>
      )}
      {view === "settings" && boot && (
        <div className="settings-layout">
          <aside className="settings-nav">
            <button
              className={settingsTab === "general" ? "active" : ""}
              onClick={() => setSettingsTab("general")}
            >
              <GearSix />
              通用
            </button>
            <button
              className={settingsTab === "sources" ? "active" : ""}
              onClick={() => setSettingsTab("sources")}
            >
              <FolderOpen />
              数据来源
            </button>
            <div className="settings-version">
              <img src="/resona-icon.png" width="48" />
              <strong>Resona</strong>
              <small>版本 {boot.version}</small>
            </div>
          </aside>
          <section className="settings-content">
            {settingsTab === "general" ? (
              <>
                <h1>通用</h1>
                <p>让回响融入你的工作节奏</p>
                <h3>启动</h3>
                <div className="setting-row">
                  <div>
                    <strong>登录时启动</strong>
                    <small>开机后在菜单栏安静运行</small>
                  </div>
                  <Toggle
                    checked={boot.settings.launchAtLogin}
                    onChange={(v) => void save({ launchAtLogin: v })}
                    label="登录时启动"
                  />
                </div>
                <h3>菜单栏</h3>
                <div className="menu-preview">
                  <Waveform size={20} />
                  {boot.settings.showProvider ? "cx · " : ""}
                  {boot.settings.showModel ? "gpt-5.6-sol · " : ""}
                  {boot.settings.menuMetric === "tps"
                    ? "21.2 tok/s"
                    : boot.settings.menuMetric === "both"
                      ? "7.8s · 21.2 tok/s"
                      : "7.8s"}
                  <small>显示样式预览</small>
                </div>
                <div className="setting-row">
                  <strong>显示指标</strong>
                  <select
                    aria-label="菜单栏指标"
                    value={boot.settings.menuMetric}
                    onChange={(e) => void save({ menuMetric: e.target.value })}
                  >
                    <option value="ttft">首次响应</option>
                    <option value="tps">端到端速度</option>
                    <option value="both">两项指标</option>
                  </select>
                </div>
                <div className="setting-row">
                  <strong>显示来源</strong>
                  <Toggle
                    checked={boot.settings.showProvider}
                    onChange={(v) => void save({ showProvider: v })}
                    label="显示来源"
                  />
                </div>
                <div className="setting-row">
                  <strong>显示模型</strong>
                  <Toggle
                    checked={boot.settings.showModel}
                    onChange={(v) => void save({ showModel: v })}
                    label="显示模型"
                  />
                </div>
                <h3>外观与默认范围</h3>
                <div className="setting-row">
                  <strong>主题</strong>
                  <select
                    aria-label="主题"
                    value={boot.settings.theme}
                    onChange={(e) => void save({ theme: e.target.value })}
                  >
                    <option value="system">跟随系统</option>
                    <option value="dark">深色</option>
                    <option value="light">浅色</option>
                  </select>
                </div>
                <div className="setting-row">
                  <strong>默认统计范围</strong>
                  <select
                    aria-label="默认统计范围"
                    value={boot.settings.defaultRange}
                    onChange={(e) => void save({ defaultRange: e.target.value })}
                  >
                    <option value="today">今天</option>
                    <option value="24h">24 小时</option>
                    <option value="7d">7 天</option>
                  </select>
                </div>
              </>
            ) : (
              <>
                <h1>数据来源</h1>
                <p>从本地会话中感知表现，无需 API Key</p>
                {["codex", "claude"].map((provider) => (
                  <section className="source-card" key={provider}>
                    <div className="section-heading">
                      <h2>
                        <Badge provider={provider} />
                        {provider === "codex" ? "Codex" : "Claude Code"}
                      </h2>
                      <Toggle
                        checked={
                          provider === "codex"
                            ? boot.settings.codexEnabled
                            : boot.settings.claudeEnabled
                        }
                        onChange={(v) =>
                          void save(
                            provider === "codex" ? { codexEnabled: v } : { claudeEnabled: v },
                          )
                        }
                        label={`${provider} 采集`}
                      />
                    </div>
                    <code>
                      {provider === "codex"
                        ? boot.settings.codexHome
                        : boot.settings.claudeProjects}
                    </code>
                    {boot.sources
                      .filter((s) => s.provider === provider)
                      .map((s) => (
                        <div key={s.kind} className="source-status">
                          <span>
                            {s.kind === "archive"
                              ? "归档目录"
                              : s.kind === "active"
                                ? "活跃目录"
                                : "会话目录"}
                            <code>{s.path}</code>
                          </span>
                          <small>
                            {s.state === "ready"
                              ? `可读取 · ${s.files} 个文件`
                              : s.state === "missing"
                                ? "未发现目录"
                                : s.state === "paused"
                                  ? "已暂停"
                                  : `部分记录异常 · ${s.errorFiles} 个文件`}
                          </small>
                        </div>
                      ))}
                    <div className="source-actions">
                      <button
                        onClick={() =>
                          void api
                            .select(provider)
                            .then((s) => {
                              if (s) setBoot({ ...boot, settings: s });
                            })
                            .catch((e) => setError(String(e)))
                        }
                      >
                        选择{provider === "codex" ? " home" : ""}目录
                      </button>
                      <button
                        onClick={() =>
                          void api
                            .open(provider === "codex" ? "codexHome" : "claudeProjects")
                            .catch((e) => setError(String(e)))
                        }
                      >
                        打开目录
                      </button>
                      <button
                        onClick={() =>
                          void api
                            .check()
                            .then(() => notify("已开始检查读取"))
                            .catch((e) => setError(String(e)))
                        }
                      >
                        <ArrowClockwise />
                        检查读取
                      </button>
                    </div>
                  </section>
                ))}
                <p className="source-note">关闭来源会暂停新增采集，已统计的历史记录仍保留。</p>
                <div className="storage-row">
                  <div>
                    <h3>本地统计数据</h3>
                    <code>{boot.storageDirectory}</code>
                    <small>固定位置 · 仅保存在这台 Mac</small>
                  </div>
                  <button
                    onClick={() => void api.open("storage").catch((e) => setError(String(e)))}
                  >
                    <FolderOpen />
                    打开
                  </button>
                </div>
              </>
            )}
          </section>
        </div>
      )}
      {selected && view === "turns" && (
        <div className="drawer-backdrop" onClick={() => setSelected(null)}>
          <aside className="turn-drawer" onClick={(e) => e.stopPropagation()}>
            <div className="section-heading">
              <h2>轮次详情</h2>
              <button
                className="icon-button"
                aria-label="关闭详情"
                onClick={() => setSelected(null)}
              >
                <X size={23} />
              </button>
            </div>
            <div className="detail-model">
              <Badge provider={selected.provider} />
              <div>
                <strong>{selected.model ?? "N/A"}</strong>
                <small>
                  {status(selected.status)}
                  {selected.hasTool ? " · 含工具调用" : ""}
                </small>
              </div>
            </div>
            <div className="drawer-metrics">
              <div>
                <strong>
                  {value(selected.ttftMs === null ? null : selected.ttftMs / 1000)}
                  <small> s</small>
                </strong>
                <span>首次响应</span>
              </div>
              <div>
                <strong>
                  {value(selected.tps)}
                  <small> tok/s</small>
                </strong>
                <span>端到端速度</span>
              </div>
            </div>
            {selected.qualityCode && (
              <div className="notice">
                {(
                  {
                    LEGACY_UNVERIFIED: "旧版导入记录，尚未通过原始日志核验",
                    IDENTITY_CONFLICT: "同一轮的来源证据不一致，暂不计入统计",
                    LINEAGE_INCOMPLETE: "继承的历史记录尚不完整",
                    TOKEN_EVIDENCE_INCOMPLETE: "输出量证据不完整，速度显示为 N/A",
                    TOKEN_COUNTER_REGRESSION: "输出计数发生回退，速度显示为 N/A",
                  } as Record<string, string>
                )[selected.qualityCode] ?? "部分来源信息尚无法确认"}
              </div>
            )}
            <dl>
              <dt>开始时间</dt>
              <dd>{time(selected.startedAtMs, true)}</dd>
              <dt>完成时间</dt>
              <dd>{time(selected.completedAtMs, true)}</dd>
              <dt>轮次总耗时</dt>
              <dd>{value(selected.durationMs === null ? null : selected.durationMs / 1000)} s</dd>
              <dt>输出 token</dt>
              <dd>{value(selected.outputTokens, 0)}</dd>
              <dt>首次响应依据</dt>
              <dd>
                {selected.ttftSource === "native"
                  ? "原生指标"
                  : selected.ttftSource === "assistant_event"
                    ? "助手事件估算"
                    : "N/A"}
              </dd>
              <dt>身份校验</dt>
              <dd>
                {selected.identityStatus === "verified"
                  ? "已确认"
                  : selected.identityStatus === "conflict"
                    ? "存在冲突"
                    : "待确认"}
              </dd>
            </dl>
            <h3>轮次标识</h3>
            {(["thread", "turn"] as const).map((field) => (
              <div className="identifier" key={field}>
                <small>{field === "thread" ? "Thread ID" : "Turn ID"}</small>
                <code>
                  {(field === "thread" ? selected.ownerThreadId : selected.nativeTurnId) ?? "N/A"}
                </code>
                <button
                  className="icon-button"
                  aria-label={`复制 ${field} ID`}
                  onClick={() =>
                    void api
                      .copy(selected, field)
                      .then(() => notify("已复制完整 ID"))
                      .catch((e) => setError(String(e)))
                  }
                >
                  <Copy />
                </button>
              </div>
            ))}
            <details>
              <summary>指标如何计算</summary>
              <p>
                首次响应优先使用源工具的原生指标，缺失时以首个可确认的助手事件估算。端到端速度 =
                输出 token ÷ 整轮耗时，包含等待、思考和工具过程。证据不足显示 N/A。
              </p>
            </details>
            <p className="local-note">仅展示统计信息，不保存对话正文。</p>
          </aside>
        </div>
      )}
      {toast && (
        <div className="toast" role="status">
          <Check />
          {toast}
        </div>
      )}
    </main>
  );
}
