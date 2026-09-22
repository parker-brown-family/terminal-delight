# Program Design: the project rail

## Files

- `app/src/engstate.rs` — new. Model, scan, derivations, and their tests.
  Lives apart from `main.rs` because everything in it is decidable without a
  window, and `main.rs` is thirty-six thousand lines that need a live gpui
  `Window` to exercise.
- `app/src/main.rs` — the `Workspace` fields, two sweeps, four methods, the
  corner (`place_name`, `eng_badge`, `eng_ink`), the middle (`render_ticker`,
  `render_pulse`), the `middle` choice in the top row, and the guards. The
  strip's group heading (`group_title`) and the `group_rename` buffer it was
  the only door into are removed; the tree renames a group through
  `bar_rename` already.
- `docs/plans/project-rail/` — this plan.

## Types & signatures

```rust
// engstate.rs — std only
pub struct Writer { tab: usize, tab_name: String, label: String, cwd: PathBuf }
pub struct ScanInput { project: Option<u32>, name: Option<String>,
                       mine: Vec<Writer>, others: Vec<(Option<String>, Writer)> }
pub struct Checkout { root, repo: String, branch: Option<String>, head: Option<String>,
                      dirty: Option<u32>, untracked: Option<u32>, delta: Option<(u32,u32)>,
                      ahead_behind: Option<(u32,u32)>, last_commit: Option<u64>, writers: Vec<Writer> }
pub struct RepoFacts { id, name, main_ref: Option<String>,
                       worktrees_on_disk: Option<u32>, pulse: Option<[u32; 12]> }
pub struct ProjectState { project, name, scanned_at: Instant, took: Duration,
                          repos: Vec<RepoFacts>, checkouts: Vec<Checkout>,
                          no_git: Vec<Writer>, foreign: Vec<Foreign>, visitors: Vec<Visitor> }
pub enum Tone { Plain, Muted, Good, Warn }
pub struct Segment { text, tone }                       // the badge
pub enum FrameKind { Dirty, Branches, Shared, Foreign, Visitors, NoGit, Repos, Worktrees, Pulse, Sentence }
pub struct Frame { kind, text, tone }                   // the ticker

pub fn scan(input: &ScanInput) -> ProjectState;          // blocking; background executor
pub fn repo_identity(origin: Option<&str>, common_dir: &Path) -> String;
impl ProjectState {
    pub fn badge(&self) -> Vec<Segment>;
    pub fn frames(&self) -> Vec<Frame>;                  // empty = healthy silence
    pub fn sentence(&self) -> Option<String>;
    pub fn dirty_checkouts(&self) -> Option<usize>;      // None when unmeasured, never 0
}

// main.rs
const ENG_STALE: Duration;                               // 20s
struct Workspace { eng: HashMap<Option<u32>, ProjectState>, eng_scanning: bool, eng_frame: usize, … }
fn eng_state(&self) -> Option<&ProjectState>;
fn eng_scan_request(&mut self, cx) -> Option<ScanInput>;
fn eng_input(&self, key: Option<u32>, cx) -> ScanInput;
fn apply_eng(&mut self, state, cx);
fn tick_eng_frame(&mut self, cx);
fn place_name(&self, width: Option<f32>, pt: f32, cx) -> Div;   // PROJECT + badge
fn eng_badge(&self, pt: f32, cx) -> Div;
fn eng_ink(&self, tone: Tone, th: &Theme) -> Hsla;               // the only place a tone is a colour
fn render_ticker(&self, scale: f32, cx) -> Div;
fn render_pulse(&self, p: &[u32; 12], scale: f32, cx) -> Div;
```

## Call stack

Scan: `constructor spawn → eng_scan_request → eng_input → [pool] engstate::scan →
git ×N → apply_eng → notify`. Render: `render → bezel_top → place_name →
eng_badge → eng_state → badge()` and `render → middle → render_ticker →
frames() / render_pulse`.

## Test plan

engstate (twelve, all real, against a temporary repository):

- `origins_spelled_three_ways_are_one_repository` — ssh alias, github ssh and https agree
- `a_repository_with_no_origin_is_its_own_directory` — and is named by its checkout
- `status_lines_are_counted_by_kind`, `numstat_sums_and_skips_binaries`
- `the_pulse_buckets_the_last_hour_newest_last` — an hour ago and the future are out
- `an_empty_input_scans_to_no_panes_and_says_so`
- `a_pane_outside_any_repository_is_no_git_not_zero_dirty` — `dirty_checkouts()` is `None`
- `two_worktrees_one_shared_is_read_from_the_filesystem` — badge `2 WT · 1 SHARED`, the dirty frame's exact text, the sentence
- `isolated_and_clean_is_a_tick_and_near_silence` — `2 WT ✓`, no dirty frame
- `a_pane_filed_here_but_working_elsewhere_is_foreign_and_a_visitor_there` — both directions, exact frame text
- `a_directory_that_has_gone_away_is_a_fact_not_a_panic`
- `the_checkouts_come_back_in_tab_order`

main.rs guards (source scans, since the render needs a window):

- `the_header_corner_carries_the_mark_and_the_projects_state` — the corner
  names the project uppercase and carries the badge, never the group; the
  middle is chosen by `self.left_bar` between the ticker and the strip
- the rename sweep still takes every buffer that exists (two now)

## Least confident decisions

1. **The tab strip leaves the top row when the tree is open.** Parker's
   words say the tabs up there are duplicate information, and the strip
   stays for a shut tree. But the `+` at the end of the strip goes with it;
   Ctrl+Shift+T and the tree's menus are the ways to make a tab.
2. **Writers are panes, not agents.** A shell pane sitting in a checkout
   counts as a writer, so a shell beside an agent in one worktree reads as
   `1 SHARED`. That is the safety reading (a shell can `git add -A` too), and
   it may read as noisy.
3. **Repository identity is the origin URL.** Two clones of one GitHub
   repository are one repository with two checkouts — the safety question is
   about directories, so that is right — but two unrelated repositories that
   happen to share a local bare origin would also merge, which nobody has.
4. **The badge counts only the PRIMARY repository's checkouts.** A project
   spanning two repositories shows `2 REPOS · 3 WT` where the 3 is the
   primary's; the other repository's checkouts appear in the repos frame.
5. **Twenty seconds stale, six seconds a frame, one scan at a time.** All
   three are guesses at a person's pace and are one constant each.
6. **Badges and frames are English only**, unlike the help modal's nine
   languages. The badges are jargon (`WT`, `SHARED`) like `CLAUDE`; the
   frames are derived sentences, and localising a derived sentence is a
   grammar problem this plan does not take on.
