import { invoke, isTauri } from "@tauri-apps/api/core";
import type {
  Bootstrap,
  Dashboard,
  Filters,
  ListRequest,
  Settings,
  Turn,
  TurnPage,
  Summary,
} from "./types";
export const desktop = isTauri();
const now = Date.now();
export const demoTurns: Turn[] = Array.from({ length: 48 }, (_, i) => {
  const cc = i % 3 === 1;
  const tt = 1.5 + ((i * 13) % 71) / 5;
  const duration = i === 0 ? 120 : 60 + ((i * 17) % 160);
  const out = Math.round((12 + ((i * 7) % 38)) * duration);
  return {
    provider: cc ? "claude" : "codex",
    turnKey: `turn:demo-${i}`,
    nativeTurnId: `demo-turn-${String(i).padStart(4, "0")}`,
    ownerThreadId: `demo-thread-${i % 7}`,
    model: cc ? "claude-sonnet-4.5" : "gpt-5.6-sol",
    status: "completed",
    threadKind: "primary",
    identityStatus: "verified",
    recordSource: "parsed",
    startedAtMs: now - i * 12 * 60000 - duration * 1000,
    completedAtMs: now - i * 12 * 60000,
    firstAssistantAtMs: now - i * 12 * 60000 - duration * 1000 + tt * 1000,
    durationMs: duration * 1000,
    ttftMs: i === 0 ? 7800 : tt * 1000,
    ttftSource: "native",
    outputTokens: i === 0 ? 2544 : out,
    tokenSource: "turn_usage",
    tps: i === 0 ? 21.2 : out / duration,
    hasTool: i % 2 === 0,
    qualityCode: null,
  };
});
let settings: Settings = {
  revision: 0,
  launchAtLogin: false,
  menuMetric: "ttft",
  showProvider: true,
  showModel: false,
  theme: "dark",
  defaultRange: "today",
  codexHome: "~/.codex",
  claudeProjects: "~/.claude/projects",
  codexEnabled: true,
  claudeEnabled: true,
};
const source = [
  {
    provider: "codex",
    kind: "active",
    path: "~/.codex/sessions",
    enabled: true,
    state: "ready",
    files: 24,
    errorFiles: 0,
    lastSuccessAtMs: now,
  },
  {
    provider: "codex",
    kind: "archive",
    path: "~/.codex/archived_sessions",
    enabled: true,
    state: "ready",
    files: 86,
    errorFiles: 0,
    lastSuccessAtMs: now,
  },
  {
    provider: "claude",
    kind: "projects",
    path: "~/.claude/projects",
    enabled: true,
    state: "ready",
    files: 16,
    errorFiles: 0,
    lastSuccessAtMs: now,
  },
];
export const percentile = (v: number[], q: number): number | null => {
  if (!v.length) return null;
  const a = [...v].sort((a, b) => a - b),
    p = (a.length - 1) * q,
    l = a[Math.floor(p)]!,
    h = a[Math.ceil(p)]!;
  return l + (h - l) * (p - Math.floor(p));
};
function summary(t: Turn[]): Summary {
  const tt = t.flatMap((t) => (t.ttftMs === null ? [] : [t.ttftMs / 1000])),
    tp = t.flatMap((t) => (t.tps === null ? [] : [t.tps]));
  return {
    completedCount: t.length,
    ttftValidCount: tt.length,
    tpsValidCount: tp.length,
    excludedCount: 0,
    ttftP50: percentile(tt, 0.5),
    ttftP95: percentile(tt, 0.95),
    tpsP50: percentile(tp, 0.5),
    tpsP5: percentile(tp, 0.05),
    codexCount: t.filter((t) => t.provider === "codex").length,
    claudeCount: t.filter((t) => t.provider === "claude").length,
  };
}
function filtered(f: Filters) {
  const end =
    f.range === "custom" && f.endDate
      ? new Date(`${f.endDate}T00:00:00`).setDate(new Date(`${f.endDate}T00:00:00`).getDate() + 1)
      : Date.now() + 1;
  const start =
    f.range === "custom" && f.startDate
      ? new Date(`${f.startDate}T00:00:00`).getTime()
      : f.range === "today"
        ? new Date().setHours(0, 0, 0, 0)
        : f.range === "24h"
          ? Date.now() - 86400000
          : f.range === "7d"
            ? Date.now() - 7 * 86400000
            : 0;
  return demoTurns.filter(
    (t) =>
      t.completedAtMs !== null &&
      t.completedAtMs >= start &&
      t.completedAtMs < end &&
      (!f.providers.length || f.providers.includes(t.provider)) &&
      (!f.model || t.model === f.model),
  );
}
export const api = {
  bootstrap: () =>
    desktop
      ? invoke<Bootstrap>("bootstrap_v1")
      : Promise.resolve({
          settings,
          storageDirectory: "~/.resona/",
          sources: source,
          dataRevision: 1,
          scanning: false,
          recent: demoTurns.slice(0, 6),
          active: [],
          version: "0.1.0",
          error: null,
        }),
  dashboard: (filters: Filters) =>
    desktop
      ? invoke<Dashboard>("get_dashboard_v1", { filters })
      : Promise.resolve(
          (() => {
            const points = filtered(filters);
            return {
              dataRevision: 1,
              asOfMs: now,
              startAtMs: now - 86400000,
              endExclusiveMs: now + 1,
              summary: summary(points),
              models: ["codex", "claude"].map((provider) => ({
                provider,
                model: provider === "codex" ? "gpt-5.6-sol" : "claude-sonnet-4.5",
                summary: summary(points.filter((t) => t.provider === provider)),
              })),
              points,
              totalPoints: points.length,
              ttftBins: [],
              tpsBins: [],
              trendBuckets: [],
              sources: source,
              scanning: false,
            };
          })(),
        ),
  list: (request: ListRequest) =>
    desktop
      ? invoke<TurnPage>("list_turns_v1", { request })
      : Promise.resolve(
          (() => {
            const rows = filtered(request.filters).filter(
              (t) =>
                (!request.status || request.status === "all" || t.status === request.status) &&
                (!request.search ||
                  t.nativeTurnId.includes(request.search) ||
                  (t.ownerThreadId?.includes(request.search) ?? false)),
            );
            if (request.sort === "ttft") rows.sort((a, b) => (b.ttftMs ?? -1) - (a.ttftMs ?? -1));
            if (request.sort === "tps")
              rows.sort((a, b) => (a.tps ?? Infinity) - (b.tps ?? Infinity));
            const start = Number(request.cursor ?? 0);
            return {
              dataRevision: 1,
              total: rows.length,
              items: rows.slice(start, start + request.pageSize),
              nextCursor:
                start + request.pageSize < rows.length ? String(start + request.pageSize) : null,
            };
          })(),
        ),
  turn: (provider: string, turnKey: string) =>
    desktop
      ? invoke<Turn>("get_turn_v1", { provider, turnKey })
      : Promise.resolve(demoTurns.find((t) => t.provider === provider && t.turnKey === turnKey)!),
  save: (next: Settings) =>
    desktop
      ? invoke<Settings>("patch_settings_v1", { settings: next })
      : Promise.resolve((settings = { ...next, revision: next.revision + 1 })),
  select: (provider: string) =>
    desktop
      ? invoke<Settings | null>("select_source_directory_v1", { provider })
      : Promise.resolve(null),
  check: () => (desktop ? invoke<void>("check_source_v1") : Promise.resolve()),
  open: (target: string) =>
    desktop ? invoke<void>("open_directory_v1", { target }) : Promise.resolve(),
  copy: (t: Turn, field: string) =>
    desktop
      ? invoke<void>("copy_identifier_v1", { provider: t.provider, turnKey: t.turnKey, field })
      : navigator.clipboard.writeText(
          (field === "thread" ? t.ownerThreadId : t.nativeTurnId) ?? "N/A",
        ),
  quit: () => (desktop ? invoke<void>("quit_v1") : Promise.resolve()),
};
