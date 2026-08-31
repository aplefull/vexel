pgo_profile := justfile_directory() + "/pgo/vexel.profdata"

# --- Build ---

# Build the workspace; pass "release" for the PGO-optimized distribution binary
build profile="dev":
    {{ if profile == "release" { "RUSTFLAGS='-Cprofile-use=" + pgo_profile + "' cargo build --release" } else { "cargo build" } }}

# Build WASM with rayon thread pool support
build-wasm:
    wasm-pack build vexel --target web --release --out-dir ../target/wasm-pkg -- -Z build-std=panic_abort,std

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
    cargo test --package vexel {{ if name != "" { "\"" + name + "\"" } else { "" } }} -- --nocapture {{ if name != "" { "--exact" } else { "" } }} {{ if loglevel != "" { "--log-level " + loglevel } else { "" } }}

# Run benchmarks, optionally filtered by name
bench *args="":
    cargo bench --package vexel --bench decode {{ if args != "" { "-- " + args } else { "" } }}

# Fuzz a specific format
fuzz format seconds="":
    mkdir -p vexel/fuzz/corpus/decode_{{replace(format, "-", "_")}}
    cd vexel && cargo fuzz run decode_{{replace(format, "-", "_")}} fuzz/corpus/decode_{{replace(format, "-", "_")}} tests/images/{{format}}/ {{ if seconds != "" { "-- -max_total_time=" + seconds } else { "" } }}

# --- Code quality ---

# Format the whole workspace
fmt:
    cargo fmt --all

# Lint the whole workspace with clippy
clippy:
    cargo clippy --workspace --all-targets

# --- Profiling ---

# Profile image decoding with samply
profile path:
    samply record target/debug/vexel --void "{{path}}"

# Profile memory usage with heaptrack
heaptrack path:
    heaptrack target/debug/vexel --void "{{path}}"

# --- Tools ---

# Generate a lossless JXL reference
jxl input output="":
    cjxl --lossless_jpeg=0 --distance=0 --effort=1 "{{input}}" "{{ if output != "" { output } else { without_extension(input) + ".jxl" } }}"
