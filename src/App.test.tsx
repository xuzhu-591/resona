// @vitest-environment jsdom
import { afterEach, beforeEach, describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent, waitFor, cleanup } from "@testing-library/react";
import App from "./App";
import { api } from "./api/client";
vi.mock("./Charts", () => ({ Chart: () => <div data-testid="chart" /> }));
beforeEach(() => {
  window.history.replaceState({}, "", "/?view=overview");
  Object.defineProperty(window, "matchMedia", {
    value: () => ({ matches: true, addEventListener: vi.fn(), removeEventListener: vi.fn() }),
    writable: true,
  });
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});
describe("desktop workflows with synthetic IPC data", () => {
  it("uses the saved default range on first load", async () => {
    const b = await api.bootstrap();
    vi.spyOn(api, "bootstrap").mockResolvedValue({
      ...b,
      settings: { ...b.settings, defaultRange: "7d" },
    });
    const query = vi.spyOn(api, "dashboard");
    render(<App />);
    await waitFor(() =>
      expect(query).toHaveBeenCalledWith(expect.objectContaining({ range: "7d" })),
    );
    expect(screen.getAllByTestId("chart")).toHaveLength(3);
  });
  it("applies one range and source to both distributions", async () => {
    const query = vi.spyOn(api, "dashboard");
    render(<App />);
    await screen.findByRole("button", { name: "24 小时" });
    fireEvent.click(screen.getByRole("button", { name: "24 小时" }));
    fireEvent.change(screen.getByLabelText("来源筛选"), { target: { value: "codex" } });
    await waitFor(() =>
      expect(query).toHaveBeenLastCalledWith(
        expect.objectContaining({ range: "24h", providers: ["codex"] }),
      ),
    );
  });
  it("opens a turn, explains metrics and closes with Escape", async () => {
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: "轮次记录" }));
    const detail = await screen.findAllByRole("button", { name: /查看轮次 demo-turn/ });
    fireEvent.click(detail[0]!);
    await screen.findByRole("heading", { name: "轮次详情" });
    expect(screen.getByText("仅展示统计信息，不保存对话正文。")).toBeTruthy();
    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByRole("heading", { name: "轮次详情" })).toBeNull();
  });
  it("keeps the fixed storage and archive source visible", async () => {
    window.history.replaceState({}, "", "/?view=settings");
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "数据来源" }));
    expect(await screen.findByText("~/.resona/")).toBeTruthy();
    expect(screen.getByText(/archived_sessions/)).toBeTruthy();
    expect(screen.queryByLabelText(/存储位置/)).toBeNull();
  });
  it("saves segmented menu settings and updates the preview", async () => {
    window.history.replaceState({}, "", "/?view=settings");
    const save = vi.spyOn(api, "save");
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "两项都显示" }));
    await waitFor(() =>
      expect(save).toHaveBeenCalledWith(expect.objectContaining({ menuMetric: "both" })),
    );
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "两项都显示" }).getAttribute("aria-pressed")).toBe(
        "true",
      ),
    );
    expect(screen.getByText(/7.8s · 21.2 tok\/s/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "数据来源" }));
    fireEvent.click(screen.getByRole("button", { name: "通用" }));
    expect(screen.getByRole("button", { name: "两项都显示" }).getAttribute("aria-pressed")).toBe(
      "true",
    );
  });
  it("renders both popover distributions and passes its selected range to details", async () => {
    window.history.replaceState({}, "", "/?view=popover");
    const query = vi.spyOn(api, "dashboard");
    render(<App />);
    await waitFor(() => expect(screen.getAllByTestId("chart")).toHaveLength(2));
    fireEvent.click(screen.getByRole("button", { name: "7 天" }));
    fireEvent.click(screen.getByRole("button", { name: "查看详情" }));
    await screen.findByRole("heading", { name: "响应趋势" });
    await waitFor(() =>
      expect(query).toHaveBeenLastCalledWith(expect.objectContaining({ range: "7d" })),
    );
  });
  it("surfaces IPC failures instead of rendering a successful zero", async () => {
    vi.spyOn(api, "dashboard").mockRejectedValue(new Error("查询失败"));
    render(<App />);
    expect(await screen.findByText(/查询失败/)).toBeTruthy();
  });
});
