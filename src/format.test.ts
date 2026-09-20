import { describe, it, expect } from "vitest";
import { value, status } from "./format";
import { percentile } from "./api/client";
describe("metric presentation", () => {
  it("keeps missing evidence distinct from zero", () => {
    expect(value(null)).toBe("N/A");
    expect(value(0)).toBe("0.0");
    expect(value(NaN)).toBe("N/A");
  });
  it("uses exact interpolated quantiles", () => {
    expect(percentile([4, 1, 3, 2], 0.5)).toBe(2.5);
    expect(percentile([], 0.95)).toBeNull();
  });
  it("does not label incomplete work as completed", () => {
    expect(status("incomplete")).toBe("信息不足");
    expect(status("failed")).toBe("失败");
  });
});
