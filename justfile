pgo_profile := justfile_directory() + "/pgo/vexel.profdata"

# --- Build ---

# Build the workspace; pass "release" for the PGO-optimized distribution binary or "wasm" for the WASM package with rayon thread pool support
build profile="dev":
    {{ if profile == "release" { "RUSTFLAGS='-Cprofile-use=" + pgo_profile + "' cargo build --release" } else if profile == "wasm" { "wasm-pack build vexel --target web --release --out-dir ../target/wasm-pkg -- -Z build-std=panic_abort,std" } else if profile == "dev" { "cargo build" } else { error("unknown profile '" + profile + "', expected dev, release or wasm") } }}

# Regenerate PGO profile data by decoding the whole test image corpus
pgo-train:
    rm -rf target/pgo-data
    mkdir -p target/pgo-data pgo
    RUSTFLAGS="-Cprofile-generate={{justfile_directory()}}/target/pgo-data" CARGO_TARGET_DIR=target/pgo cargo build --release
    find vexel/tests/images -type f -print0 | xargs -0 -P 8 -I{} timeout 120 target/pgo/release/vexel --void "{}" || true
    "$(rustc --print sysroot)/lib/rustlib/x86_64-unknown-linux-gnu/bin/llvm-profdata" merge -o {{pgo_profile}} target/pgo-data/*.profraw

# --- Run ---

# Run vexel binary
vexel *args:
    target/debug/vexel {{args}}

# Open image in GUI viewer
gui path loglevel="":
    cargo run --package vexel-cli -- --gui {{ if loglevel != "" { "--log-level " + loglevel + " " } else { "" } }}"{{path}}"

# Print image info
info path loglevel="":
    cargo run --package vexel-cli -- --info {{ if loglevel != "" { "--log-level " + loglevel + " " } else { "" } }}"{{path}}"

# Decode image and save as JPEG XL
save path loglevel="":
    cargo run --package vexel-cli -- --format jxl {{ if loglevel != "" { "--log-level " + loglevel + " " } else { "" } }}"{{path}}"

# --- Test ---

# Run all tests or a specific test with output captured
test name="" loglevel="":
    cargo test --package vexel --test main {{ if name != "" { "\"" + name + "\"" } else { "" } }} -- --nocapture {{ if name != "" { "--exact" } else { "" } }} {{ if loglevel != "" { "--log-level " + loglevel } else { "" } }}

# Run benchmarks, optionally filtered by name
bench *args="":
    cargo bench --package vexel --bench decode {{ if args != "" { "-- " + args } else { "" } }}

# Fuzz a specific format
fuzz format seconds="" jobs="":
    #!/usr/bin/env bash
    set -euo pipefail
    target=decode_{{replace(format, "-", "_")}}
    seeds=$PWD/target/fuzz-seeds/$target
    source=vexel/tests/images/{{format}}
    max_len=65536
    case "{{format}}" in
        any) source=vexel/tests/images ;;
        parallel) source=vexel/tests/images; max_len=524288 ;;
    esac
    rm -rf "$seeds" "$seeds.dict"
    mkdir -p "$seeds"
    find "$source" -type f -size -$((max_len + 1))c -print0 | while IFS= read -r -d '' file; do
        name=${file#vexel/tests/images/}
        ln -f "$file" "$seeds/${name//\//_}" 2>/dev/null || cp "$file" "$seeds/${name//\//_}"
    done
    case "{{format}}" in
        any|parallel) sort -u vexel/fuzz/dicts/*.dict > "$seeds.dict" ;;
        *) if [ -f vexel/fuzz/dicts/{{format}}.dict ]; then cp vexel/fuzz/dicts/{{format}}.dict "$seeds.dict"; fi ;;
    esac
    args=(-max_len=$max_len -timeout=30 -report_slow_units=30 -rss_limit_mb=2048)
    if [ -f "$seeds.dict" ]; then args+=(-dict="$seeds.dict"); fi
    if [ -n "{{seconds}}" ]; then args+=(-max_total_time={{seconds}}); fi
    if [ -n "{{jobs}}" ]; then args+=(-fork={{jobs}} -ignore_crashes=1); fi
    mkdir -p vexel/fuzz/corpus/$target
    cd vexel && cargo fuzz run "$target" "fuzz/corpus/$target" "$seeds" -- "${args[@]}"

# --- Code quality ---

# Format the whole workspace
fmt:
    cargo fmt --all

# Lint the whole workspace with clippy
clippy:
    cargo clippy --workspace --all-targets

# Check formatting and lint every supported configuration (native, wasm32, aarch64; with and without rayon) with warnings denied
check-all:
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo clippy --package vexel --all-targets --no-default-features -- -D warnings
    cargo clippy --package vexel --lib --target wasm32-unknown-unknown -Z build-std=panic_abort,std -- -D warnings
    cargo clippy --package vexel --lib --target wasm32-unknown-unknown -Z build-std=panic_abort,std --no-default-features -- -D warnings
    cargo clippy --package vexel --lib --target aarch64-unknown-linux-gnu -Z build-std=std -- -D warnings
    cargo clippy --package vexel --lib --target aarch64-unknown-linux-gnu -Z build-std=std --no-default-features -- -D warnings

# --- Maintenance ---

# Delete build artifacts and incremental caches not used in the last N days
sweep days="3":
    cargo sweep --time {{days}} {{justfile_directory()}}
    cargo sweep --time {{days}} {{justfile_directory()}}/vexel/fuzz
    find target vexel/fuzz/target -regextype posix-extended -type d -regex '.*/incremental/[^/]+' -mmin +$(({{days}} * 1440)) -prune -exec rm -rf {} +

# --- Profiling ---

# Profile image decoding with samply
profile path:
    samply record target/debug/vexel --void "{{path}}"

# Profile memory usage with heaptrack
heaptrack path:
    heaptrack target/debug/vexel --void "{{path}}"
