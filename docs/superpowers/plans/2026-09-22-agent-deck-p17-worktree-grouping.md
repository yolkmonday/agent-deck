# Agent Deck P17: Group worktree sessions under their parent project

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development or superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax.

**Goal:** A session running in a git worktree sits under its main project, so `noor` and `noor-oc-feat-1` read as one project rather than two unrelated cards.

**Architecture:** A `repo` module resolves any directory to its main repository root by reading the worktree's `.git` file — no shelling out to git. The result becomes a `group` on every session, and the Live board renders one section per group.

**Depends on:** P16 (which also touches `Session` and `SessionCard`).

## How a worktree is detected — verified on this machine

In a normal checkout, `<cwd>/.git` is a **directory**.
In a linked worktree, `<cwd>/.git` is a **file** whose single line is:

```
gitdir: /Users/yolk/Dev/agent-deck/.git/worktrees/agent-deck-oc-p16
```

So the main repository root is that path with `/.git/worktrees/<name>` removed. This is exact and
costs one file read. **Do not infer the parent from the folder name.** A convention like
`<repo>-oc-<slug>` is our habit, not a rule, and guessing would group unrelated projects that happen
to share a prefix.

## Global Constraints

Same as previous phases. Plus:

- Resolution must be cached per directory. The 1-second loop must not read `.git` for every session
  on every tick; cache by path, and only re-check when the path is new.
- A directory that is not a git repository at all is its own group. Never fail, never guess.
- UI text Indonesian, code and commit messages English. No `Co-Authored-By`. Never push.

## Command API (the contract)

`Session` gains:

```ts
group: string;            // display name of the main project, e.g. "noor"
groupRoot: string;        // absolute path of the main repo root, the stable grouping key
isWorktree: boolean;      // true when this cwd is a linked worktree
worktreeName: string | null;  // e.g. "agent-deck-oc-p16", null when not a worktree
```

A non-repo directory gets `group = project`, `groupRoot = cwd`, `isWorktree = false`.

---

### Task 1: Repo root resolution

**Files:** Create `src-tauri/crates/collector/src/repo.rs`; modify `src-tauri/crates/collector/src/lib.rs`

**Interfaces:**
- `struct RepoInfo { pub root: PathBuf, pub is_worktree: bool, pub worktree_name: Option<String> }`
- `fn resolve(cwd: &Path) -> RepoInfo` with these rules, in order:
  1. `<cwd>/.git` is a directory → `{ root: cwd, is_worktree: false, worktree_name: None }`
  2. `<cwd>/.git` is a file whose content starts with `gitdir:` → take the path after the prefix,
     trim whitespace; if it contains `/.git/worktrees/<name>`, the root is everything before
     `/.git/worktrees`, `is_worktree` is true and `worktree_name` is `<name>`; if it does not match
     that shape, fall through to rule 4
  3. neither exists → walk up the parent directories, repeating rules 1 and 2, stopping at the
     filesystem root
  4. nothing found → `{ root: cwd, is_worktree: false, worktree_name: None }`
- `fn display_name(root: &Path, home: &str) -> String` — the root's last segment, or `~` when the
  root is the home directory. Reuse the logic in `live::project_name` rather than duplicating it.

- [ ] **Step 1: Write the failing tests** (all with `tempfile`, building real directory shapes):
1. `plain_repo_is_its_own_root`: a dir containing a `.git` DIRECTORY resolves to itself, not a worktree.
2. `worktree_resolves_to_the_main_repo`: `.git` is a file containing `gitdir: /tmp/x/main/.git/worktrees/feat-1` → root `/tmp/x/main`, `is_worktree` true, name `feat-1`.
3. `worktree_file_with_trailing_newline_and_spaces_parses`: the same content with `\n` and padding still parses.
4. `gitdir_without_the_worktrees_segment_is_not_a_worktree`: `gitdir: /tmp/x/main/.git` falls through to rule 4 rather than mis-grouping.
5. `subdirectory_of_a_repo_walks_up`: `<repo>/src/deep` resolves to `<repo>`.
6. `subdirectory_of_a_worktree_walks_up_to_the_main_repo`.
7. `a_non_repo_directory_is_its_own_root`.
8. `resolution_stops_at_the_filesystem_root_and_does_not_loop`: a temp dir with no `.git` anywhere returns itself and terminates.
- [ ] **Step 2: Run to verify it fails.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run to verify it passes** — 8 tests pass.
- [ ] **Step 5: Commit** — `git commit -m "feat(collector): resolve a worktree to its main repository"`

---

### Task 2: Put the group on the session

**Files:** Modify `src-tauri/crates/collector/src/model.rs`, `src-tauri/crates/collector/src/live.rs`

**Interfaces:**
- `Session` gains the four fields from the Command API (serde camelCase). `same_content` compares them.
- `LiveCollector` gains `repo_cache: HashMap<String /*cwd*/, RepoInfo>`; `resolve` is called only on
  a cache miss.
- Both the Claude and the opencode session builders fill the fields.

- [ ] **Step 1: Write the failing tests** in `live.rs`:
1. `a_worktree_session_is_grouped_under_the_main_repo`: two sessions, one in `<repo>` and one in `<repo>-wt` set up as a real worktree fixture; both get the same `group_root`, and the second has `is_worktree: true`.
2. `a_plain_session_groups_under_itself`.
3. `repo_resolution_is_cached`: assert `resolve` is not re-run on a second snapshot — expose a counter on a test double, or assert via a `Cell` counter injected for the test.
- [ ] **Step 2: Run to verify it fails.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run to verify it passes**, and every pre-existing collector test still green.
- [ ] **Step 5: Commit** — `git commit -m "feat(collector): group sessions by their main repository"`

---

### Task 3: Render groups on the Live board

**Files:** Modify `src/lib/types.ts`, `src/pages/LivePage.tsx`; create `src/lib/grouping.ts` and `src/lib/grouping.test.ts`

**Interfaces:**
- `src/lib/grouping.ts` exports
  `groupSessions(sessions: Session[]): { key: string; label: string; sessions: Session[] }[]`:
  - groups by `groupRoot`
  - a group's `label` is its `group`
  - sessions inside a group keep the incoming order (waiting first, then most recent), and within a
    group the non-worktree session comes before the worktree ones
  - groups are ordered by their best session: a group containing a waiting session first, then by the
    newest `updatedAtMs`
- `LivePage` renders one section per group. A group with a single session and no worktree renders
  exactly as today — **no heading**, so the common case does not gain visual noise. A group with more
  than one session gets a heading: the project name, then `· N sesi`, and `lucide:folder-git-2` when
  any member is a worktree.

**Two distinct icons, because they are two different things.** Both are already in the offline
lucide bundle and were verified present:

| Thing | Icon | Where |
|---|---|---|
| Git branch | `lucide:git-branch` | next to `session.branch` on the card |
| Worktree | `lucide:folder-git-2` | next to `worktreeName`, and on a group heading that contains one |

A worktree is a separate *folder* of the same repository, which is exactly what `folder-git-2`
depicts; reusing the branch icon for both would hide the distinction the user asked to see.

- The card's model/branch line becomes: model, then `lucide:git-branch` + branch name when a branch
  is known, then `lucide:folder-git-2` + `worktreeName` as a chip when `isWorktree` is true.
- Both icons are 12 px, in `text-fg-3`, and never replace the text label.

- [ ] **Step 1: Write the failing tests** in `src/lib/grouping.test.ts`:
1. `groups_by_group_root_not_by_name`: two sessions with the same `groupRoot` but different `project` end up in one group.
2. `main_checkout_sorts_before_its_worktrees`.
3. `a_group_with_a_waiting_session_comes_first`.
4. `groups_without_waiting_sort_by_newest_activity`.
5. `a_single_session_still_yields_one_group`.
- [ ] **Step 2: Run to verify it fails** — `bun test`.
- [ ] **Step 3: Implement the helper.**
- [ ] **Step 4: Run to verify it passes.**
- [ ] **Step 5:** Render the sections in `LivePage`.
- [ ] **Step 6:** Verify — `bunx tsc --noEmit`, `bun test`, `bun run build` all clean.
- [ ] **Step 7: Commit** — `git commit -m "feat(ui): group live sessions by project"`

---

## Done when

- The whole Rust suite passes with 11 new tests; `bun test` passes with 5 new tests.
- `bunx tsc --noEmit`, `bun run build`, `cargo check --workspace` all clean.
- One commit per task, no `Co-Authored-By`, nothing pushed.

## Self-Review

- **Coverage:** worktrees group under their main project, with the relationship shown rather than
  implied.
- **Why not match on the folder name:** `<repo>-oc-<slug>` is a convention this workflow happens to
  use; two unrelated projects sharing a prefix would be merged by a name rule. Reading `.git` is
  exact, and test 4 pins the case where the file exists but is not a worktree pointer.
- **Restraint:** a single-session group renders with no heading. Grouping should only appear when
  there is something to group, or the board gains a row of headings that say nothing.
