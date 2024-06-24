default: debug

# runs all CI checks.
ci: fmt unused-deps deny lint test doc-once

project_root := `git rev-parse --show-toplevel`
arch := `uname -m`
fds := "8192"

# builds in release mode
build:
    cargo build --release

# cargo clippy
lint:
    cargo clippy --all-targets --all-features -- -D warnings

lint-fix:
    cargo clippy --fix --all-targets --all-features --allow-dirty --allow-staged -- -D warnings

# cargo nextest
test:
    cargo nextest run

# cargo miri
miri:
    # only run if test prefixed with "miri"
    MIRIFLAGS='-Zmiri-tree-borrows -Zmiri-ignore-leaks' cargo miri nextest run miri

# cargo fmt
fmt:
    cargo fmt

proxy:
    ulimit -Sn {{fds}} && cargo run --bin hyperion-proxy --release

infection:
    cargo run --bin infection --release -- -t

# cargo machete
unused-deps:
    cargo machete

# cargo deny
deny:
    cargo deny check

# run in debug mode with tracy; auto-restarts on changes
debug:
    #!/usr/bin/env -S parallel --shebang --ungroup --jobs 3
    RUST_BACKTRACE=full RUN_MODE=debug-{{arch}} cargo watch --postpone --no-vcs-ignores -w {{project_root}}/.trigger -s './target/debug/infection -t'
    RUST_BACKTRACE=full ulimit -Sn {{fds}} && cargo run --bin hyperion-proxy --release
    cargo watch --why -w '{{project_root}}/crates/hyperion' -w '{{project_root}}/events/infection' -s 'cargo build -p infection' -s 'touch {{project_root}}/.trigger'

# run in release mode with tracy; auto-restarts on changes
release:
    #!/usr/bin/env -S parallel --shebang --ungroup --jobs 3
    RUN_MODE=release-{{arch}} cargo watch --postpone --no-vcs-ignores -w {{project_root}}/.trigger -s './target/release/infection -t'
    ulimit -Sn {{fds}} && cargo run --bin hyperion-proxy --release
    cargo watch --why -w '{{project_root}}/crates/hyperion' -w '{{project_root}}/events/infection' -s 'cargo build --release -p infection' -s 'touch {{project_root}}/.trigger'

# run a given number of bots to connect to hyperion
bots count='1000':
    cargo install -q --git https://github.com/andrewgazelka/rust-mc-bot --branch optimize
    ulimit -Sn {{fds}} && rust-mc-bot 127.0.0.1:25566 {{count}}

# run in release mode with tracy
run:
    cargo run --release -- -t

doc-once:
    cargo doc --workspace --no-deps --all-features

doc:
    cargo watch -x 'doc --workspace --no-deps --all-features'

# Run the data extractor and save generated data to `/extracted`.
extract:
    mkdir -p extractor/run
    echo 'eula=true' > extractor/run/eula.txt
    cd extractor && sh gradlew runServer
    cp extractor/run/extractor_output/* extracted/
