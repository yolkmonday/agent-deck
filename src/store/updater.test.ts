import { describe, expect, test } from "bun:test";
import { bannerVisible, initialUpdaterState as s0, reduce } from "@/store/updater";

describe("updater reducer", () => {
  test("check → found shows banner", () => {
    const s = reduce(reduce(s0, { type: "check", manual: false }), { type: "found", version: "0.3.1" });
    expect(s.status).toBe("available");
    expect(bannerVisible(s)).toBe(true);
  });
  test("check → none", () => {
    expect(reduce(reduce(s0, { type: "check", manual: true }), { type: "none" }).status).toBe("none");
  });
  test("auto-check failure is silent", () => {
    const s = reduce(reduce(s0, { type: "check", manual: false }), { type: "failed", error: "offline" });
    expect(s.status).toBe("idle");
    expect(s.error).toBeNull();
  });
  test("manual-check failure is shown", () => {
    const s = reduce(reduce(s0, { type: "check", manual: true }), { type: "failed", error: "404" });
    expect(s.status).toBe("error");
    expect(s.error).toBe("404");
  });
  test("dismissed version stays hidden on re-check", () => {
    let s = reduce(reduce(s0, { type: "check", manual: false }), { type: "found", version: "0.3.1" });
    s = reduce(s, { type: "dismiss" });
    expect(bannerVisible(s)).toBe(false);
    s = reduce(reduce(s, { type: "check", manual: false }), { type: "found", version: "0.3.1" });
    expect(bannerVisible(s)).toBe(false);
  });
  test("a newer version after dismiss shows again", () => {
    let s = reduce(reduce(s0, { type: "check", manual: false }), { type: "found", version: "0.3.1" });
    s = reduce(s, { type: "dismiss" });
    s = reduce(reduce(s, { type: "check", manual: false }), { type: "found", version: "0.3.2" });
    expect(bannerVisible(s)).toBe(true);
  });
  test("progress moves to downloading and clamps", () => {
    let s = reduce(s0, { type: "found", version: "0.3.1" });
    s = reduce(s, { type: "progress", progress: 1.7 });
    expect(s.status).toBe("downloading");
    expect(s.progress).toBe(1);
    expect(bannerVisible(s)).toBe(true);
  });
  test("a background check does not interrupt a download", () => {
    let s = reduce(reduce(s0, { type: "found", version: "0.3.1" }), { type: "progress", progress: 0.4 });
    s = reduce(s, { type: "check", manual: false });
    expect(s.status).toBe("downloading");
  });
  test("install-failed from downloading shows the error and hides the banner", () => {
    let s = reduce(reduce(s0, { type: "found", version: "0.3.1" }), { type: "progress", progress: 0.4 });
    s = reduce(s, { type: "install-failed", error: "disk full" });
    expect(s.status).toBe("error");
    expect(s.error).toBe("disk full");
    expect(s.manual).toBe(true);
    expect(bannerVisible(s)).toBe(false);
  });
});
