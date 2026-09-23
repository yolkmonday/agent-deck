// Mirrors the `pub const ERR_*` strings in `src-tauri/src/projects.rs::validate`.
// Keep these character-for-character identical to the Rust side; a comment
// there points back here.
export const PROJECT_ERRORS = {
  nameEmpty: "name cannot be empty",
  nameTooLong: "name is too long",
  pathNotAbsolute: "path must be absolute",
  folderNotFound: "folder not found",
  folderAlreadyUsed: "a project with this folder already exists",
  profileUnknown: "unknown profile",
} as const;
