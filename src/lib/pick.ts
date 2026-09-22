import { open } from "@tauri-apps/plugin-dialog";

export type PickFolderOptions = {
  title?: string;
  defaultPath?: string;
};

/// The plugin's return type differs across versions: a single path, a list of
/// them, or null. An empty array or an empty string means the user cancelled
/// just as much as null does, so all of them collapse to null.
const firstPath = (result: string | string[] | null): string | null => {
  const value = Array.isArray(result) ? result[0] : result;
  return typeof value === "string" && value !== "" ? value : null;
};

/// Opens the native folder picker. Resolves with the chosen absolute path, or
/// null when the user cancels. A plugin failure rejects with a readable message
/// so the calling component can show it inline instead of crashing.
export const pickFolder = async (opts: PickFolderOptions = {}): Promise<string | null> => {
  try {
    const result = await open({
      directory: true,
      multiple: false,
      title: opts.title,
      defaultPath: opts.defaultPath,
    });
    return firstPath(result as string | string[] | null);
  } catch (e) {
    throw new Error(e instanceof Error ? e.message : String(e));
  }
};
