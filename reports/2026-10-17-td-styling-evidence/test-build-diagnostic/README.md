# Excluded diagnostic

This five-pair run compared a normal release baseline against the binary emitted by cargo test --release. Cargo's integration-test build enables GPUI test-support through dev-dependency feature unification. It is not the same dependency graph as cargo build --release; the candidate was 59332456 bytes instead of the normal production build's 59043048 bytes. These observations are retained as an excluded diagnostic and must not support an improvement or regression claim. The final comparison uses two normal cargo build --release --locked binaries.
