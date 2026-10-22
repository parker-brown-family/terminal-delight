#!/bin/sh
set -eu
terminal-delight restricted call '{"op":"status"}' > /work/status.json
terminal-delight restricted call '{"op":"note","text":"Allowed offline build completed"}' > /work/note.json
terminal-delight restricted call '{"op":"note","text":"Forged target","pid":1}' > /work/forged.json || true
terminal-delight restricted call '{"op":"send","recipient":"standard","text":"execute"}' > /work/delegate.json || true
terminal-delight restricted call '{"op":"request_access","reason":"Need external packages"}' > /work/request.json
: > /work/ready
while [ ! -e /work/policy-go ]; do sleep 0.05; done
sleep 1.05
terminal-delight restricted call '{"op":"note","text":"Policy must still apply"}' > /work/policy-refused.json || true
: > /work/policy-done
while [ ! -e /work/go ]; do sleep 0.05; done
terminal-delight restricted call '{"op":"note","text":"After revocation"}' > /work/revoked.json || true
: > /work/done
