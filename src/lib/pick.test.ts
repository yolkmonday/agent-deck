import { beforeEach, describe, expect, mock, test } from "bun:test";

type OpenOptions = {
  directory?: boolean;
  multiple?: boolean;
  title?: string;
  defaultPath?: string;
};

let openResult: unknown = null;
let openError: Error | null = null;
let openCalls: OpenOptions[] = [];

mock.module("@tauri-apps/plugin-dialog", () => ({
  open: (options: OpenOptions) => {
    openCalls.push(options);
    return openError === null ? Promise.resolve(openResult) : Promise.reject(openError);
  },
}));

const { pickFolder } = await import("@/lib/pick");

beforeEach(() => {
  openResult = null;
  openError = null;
  openCalls = [];
});

describe("pickFolder", () => {
  test("returns_the_selected_path", async () => {
    openResult = "/Users/yolk/Dev/noor";
    expect(await pickFolder()).toBe("/Users/yolk/Dev/noor");
  });

  test("returns_null_on_cancel", async () => {
    openResult = null;
    expect(await pickFolder()).toBeNull();
  });

  test("takes_the_first_entry_of_an_array", async () => {
    openResult = ["/a", "/b"];
    expect(await pickFolder()).toBe("/a");
  });

  test("treats_an_empty_array_as_cancel", async () => {
    openResult = [];
    expect(await pickFolder()).toBeNull();
  });

  test("treats_an_empty_string_as_cancel", async () => {
    openResult = "";
    expect(await pickFolder()).toBeNull();
  });

  test("passes_title_and_default_path_through", async () => {
    openResult = "/Users/yolk/Dev/noor";
    await pickFolder({ title: "Pilih folder project", defaultPath: "/Users/yolk/Dev" });
    expect(openCalls).toHaveLength(1);
    expect(openCalls[0]).toEqual({
      directory: true,
      multiple: false,
      title: "Pilih folder project",
      defaultPath: "/Users/yolk/Dev",
    });
  });
});

test("rejects_with_a_readable_message_when_the_plugin_fails", async () => {
  openError = new Error("dialog tidak tersedia");
  expect(pickFolder()).rejects.toThrow("dialog tidak tersedia");
});
