default: fmt lint unused-deps deny test
alias d := debug

project_root := `git rev-parse --show-toplevel`

build:
    cargo build --release

lint:
    cargo clippy --all-targets --all-features -- -D warnings

test:
    cargo nextest run

miri:
    # only run if test prefixed with "miri"
    MIRIFLAGS='-Zmiri-tree-borrows -Zmiri-ignore-leaks' cargo miri nextest run miri

fmt:
    cargo fmt

unused-deps:
    cargo machete

deny:
    cargo deny check

debug:
    #!/usr/bin/env -S parallel --shebang --ungroup --jobs 3
    hyperion-proxy
    cargo watch -x build -s 'touch {{project_root}}/.trigger'
    cargo watch --no-vcs-ignores -w {{project_root}}/.trigger -s './target/debug/infection'

run:
    cargo run --release -- -t