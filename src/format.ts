export const value = (n: number | null | undefined, digits = 1) =>
  n == null || !Number.isFinite(n)
    ? "N/A"
    : n.toLocaleString("en-US", { minimumFractionDigits: digits, maximumFractionDigits: digits });
export const time = (n: number | null | undefined, full = false) =>
  n == null
    ? "N/A"
    : new Date(n).toLocaleString(
        "zh-CN",
        full
          ? {
              month: "2-digit",
              day: "2-digit",
              hour: "2-digit",
              minute: "2-digit",
              second: "2-digit",
            }
          : { hour: "2-digit", minute: "2-digit" },
      );
export const status = (s: string) =>
  ({
    completed: "已完成",
    running: "进行中",
    aborted: "已中止",
    failed: "失败",
    incomplete: "信息不足",
  })[s] ?? s;
