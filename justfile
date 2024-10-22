default: debug

# runs all CI checks in parallel where possible
ci:
    #!/usr/bin/env bash
    just fmt & # can run independently
    just unused-deps & # can run independently
    just deny & # can run independently
    wait
    just lint # depends on compilation, but not on other tasks
    just test # run tests last since they depend on compilation
    just doc-once

project_root := `git rev-parse --show-toplevel`
arch := `uname -m`
fds := "32768"

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
    ulimit -Sn {{fds}} && cargo run --profile release-full --bin hyperion-proxy

nyc:
    cargo run --bin nyc --release

nyc-full:
    cargo run --bin nyc --profile release-full

# cargo machete
unused-deps:
    cargo machete

# cargo deny
deny:
    cargo deny check

# run in debug mode with tracy; auto-restarts on changes
debug:
    #!/usr/bin/env -S parallel --shebang --ungroup --jobs 3
    RUST_BACKTRACE=full RUN_MODE=debug-{{arch}} cargo watch --postpone --no-vcs-ignores -w {{project_root}}/.trigger-debug -s './target/debug/nyc'
    RUST_BACKTRACE=full ulimit -Sn {{fds}} && cargo run --bin hyperion-proxy --release
    cargo watch -w '{{project_root}}/crates/hyperion' -w '{{project_root}}/events/nyc' -s 'cargo check -p nyc && cargo build -p nyc' -s 'touch {{project_root}}/.trigger-debug'

# run in release mode with tracy; auto-restarts on changes
release:
    #!/usr/bin/env -S parallel --shebang --ungroup --jobs 3
    RUN_MODE=release-{{arch}} cargo watch --postpone --no-vcs-ignores -w {{project_root}}/.trigger-release -s './target/release/nyc'
    ulimit -Sn {{fds}} && cargo run --profile release-full --bin hyperion-proxy
    cargo watch -w '{{project_root}}/crates/hyperion' -w '{{project_root}}/events/nyc' -s 'cargo check -p nyc && cargo build --release -p nyc' -s 'touch {{project_root}}/.trigger-release'

release-full:
    #!/usr/bin/env -S parallel --shebang --ungroup --jobs 2
    RUN_MODE=release-f-{{arch}} cargo run --profile release-full -p nyc'
    ulimit -Sn {{fds}} && cargo run --bin hyperion-proxy --profile release-full

# run a given number of bots to connect to hyperion
bots count='1000':
    cargo install -q --git https://github.com/andrewgazelka/rust-mc-bot --branch optimize
    ulimit -Sn {{fds}} && rust-mc-bot 127.0.0.1:25565 {{count}} 3

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
