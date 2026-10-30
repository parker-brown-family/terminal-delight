# Uninstall Review, Round 5

**3 findings: 0 critical, 2 high, 1 medium.** Reviewed `/home/parker/Work/td-uninstall-lab/terminal-delight`, including the added regression-test source. CONFIRMED means traced through source; no scenarios were executed. No builds, tests, TD execution, process signals or code edits. This report is the only file written.

The legacy-root overlap check, nonblocking manifest writer, MCP read/parse error propagation and partial-record wording address the round-4 cases. A static foreign or externally writable manifest is now rejected. The remaining findings concern AppImage identity and parsing, and the manifest reader's separate check and open.

## High

### H1. The mount check authenticates a filename, not the image file

`app/src/uninstall/plan.rs:905`; `app/src/uninstall/plan.rs:916`; `app/src/uninstall/plan.rs:1297`

Scenario: TD runs from a mounted `/home/user/apps/TerminalDelight.AppImage`. `APPDIR` still names that genuine mount, but `APPIMAGE` names a different valid image at `/home/user/archive/TerminalDelight.AppImage`. The mount check discards the image's directory and accepts a substring match on its basename. The genuine TD payload matches the worker, so all checks pass and the different file is scheduled for deletion without a hash. A shorter name such as `AppImage` also passes wherever it is a substring of the mount's type or source.

**CONFIRMED (traced).** The comparison never establishes that the selected path names the mounted file. AppImage's own issue documents filesystem types containing an image's basename, which cannot distinguish two files in different directories. [AppImage filesystem-type discussion](https://github.com/AppImage/type2-runtime/issues/83)

Suggested fix: verify the backing file's full identity, or require a trusted image hash/receipt; a basename or filesystem-label match must not authorize deletion.

### H2. Receipt validation and reading can select different files

`app/src/uninstall/apply.rs:598`; `app/src/uninstall/apply.rs:604`; `app/src/uninstall/roots.rs:70`

Scenario: runtime is unset and `TMPDIR` is a shared writable directory without the sticky bit, such as `/srv/shared/tmp`. A legitimate receipt owned by the uninstalling account passes `symlink_metadata` and the new mode check. Another account with directory write access then replaces that directory entry before `read_to_string` opens it. The replacement contains forged hash/name pairs for the uninstalling account's unrelated TD-like files. The reader consumes it without checking the opened descriptor's owner or mode, and the later target-UID check passes for those victim files.

**CONFIRMED (traced).** The roots permit this temp directory, and the reader performs two pathname lookups. Directory write access permits replacement even when the original file itself is not writable. This case requires a different account and a shared directory without sticky protection; it is not the previously accepted same-account ancestor-swap limitation.

Suggested fix: open the receipt with no-follow/nonblocking flags, validate owner/type/mode on that descriptor, and read from the same descriptor, as the writer now does for validation and writing.

## Medium

### M1. Escaped mount paths reject legitimate AppImages

`app/src/uninstall/plan.rs:911`; `app/src/uninstall/plan.rs:395`; `app/src/uninstall/apply.rs:875`

Scenario: TD's AppImage uses `TMPDIR=/home/user/Temporary Files`. Its real mount path contains a space, while `/proc/self/mountinfo` represents that space as `\040`. The parser constructs a PathBuf directly from the encoded field and compares it with the decoded canonical APPDIR. They differ, so `proven_appimage` returns None and drops both the image and payload from the plan's executable targets. A still-running TD window from that payload is then classified as another TD and blocks removal. Escaped characters in the filesystem type/source can also fail the filename comparison.

**CONFIRMED (traced).** Linux escapes mount paths and filesystem labels when producing mountinfo; the new parser does not decode them. The AppImage runtime uses TMPDIR when constructing its mount directory. [Linux mountinfo writer](https://raw.githubusercontent.com/torvalds/linux/master/fs/proc_namespace.c), [AppImage runtime](https://raw.githubusercontent.com/AppImage/type2-runtime/main/src/runtime/runtime.c)

Suggested fix: decode mountinfo's escaped fields before path comparisons, and cover spaces and backslashes in parser fixtures without requiring a live mount.
