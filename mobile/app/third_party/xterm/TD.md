# xterm.dart 4.0.0, vendored

This is xterm.dart 4.0.0 from pub.dev (MIT, Copyright (c) 2020 xuty — see
`LICENSE`), copied verbatim into the phone app so one small change can sit on top
of it. Only `lib/`, `pubspec.yaml`, `LICENSE`, `README.md` and `CHANGELOG.md` were
copied; the upstream examples, tests and media were left behind.

## Why it is vendored

xterm's own keyboard connection (`lib/src/ui/custom_text_edit.dart`) tells the
phone's keyboard that the terminal takes text and nothing else, and it ignores
any content the keyboard commits. So tapping a picture in the Samsung or Gboard
clipboard panel does nothing. `TerminalView` builds that connection internally
and exposes no way to configure it, so the change has to be made here.

## What differs from upstream

The commit after the verbatim copy is the whole delta. `git log -p --
mobile/app/third_party/xterm` shows it.

- `CustomTextEdit` takes a `ContentInsertionConfiguration`, the same type a
  Flutter `TextField` takes. It puts the configuration's MIME types on the input
  connection and hands committed content to its callback.
- `TerminalView` takes the same parameter and passes it down.

To upgrade, copy the new release over this one, then re-apply that commit.
