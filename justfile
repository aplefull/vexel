# Set perf_event_paranoid to allow samply without root
perf-allow:
    echo '1' | sudo tee /proc/sys/kernel/perf_event_paranoid

# Profile image decoding with samply
profile path:
    samply record target/release/vexel --void "{{path}}"

# Profile memory usage with heaptrack
heaptrack path:
    heaptrack target/release/vexel --void "{{path}}"

# Open image in GUI viewer
gui path loglevel="":
    cargo run --package vexel-cli --release -- --gui {{ if loglevel != "" { "--log-level " + loglevel + " " } else { "" } }}"{{path}}"

# Print image info
info path loglevel="":
    cargo run --package vexel-cli --release -- --info {{ if loglevel != "" { "--log-level " + loglevel + " " } else { "" } }}"{{path}}"

# Decode image and save as WebP
save path loglevel="":
    cargo run --package vexel-cli --release -- --format webp {{ if loglevel != "" { "--log-level " + loglevel + " " } else { "" } }}"{{path}}"

# Run all tests or a specific test with output captured
test name="" loglevel="":
    cargo test --package vexel --release {{ if name != "" { "\"" + name + "\"" } else { "" } }} -- --nocapture {{ if loglevel != "" { "--log-level " + loglevel } else { "" } }}

# Run benchmarks, optionally filtered by name
bench *args="":
    cargo bench --package vexel --bench decode {{ if args != "" { "-- " + args } else { "" } }}

# Decode all images in VEXEL_CORPUS, compare against ImageMagick, save CSV
corpus-bench:
    cargo test --package vexel --release corpus_bench -- --ignored --nocapture

# Analyze a corpus result CSV (--stats, --top-slow N, --mse-above X, --errors); defaults to latest
corpus-analyze file="" *args:
    python3 scripts/corpus_diff.py analyze {{ if file != "" { "\"" + file + "\" " } else { "" } }}{{args}}

# Diff two corpus result CSVs (--threshold PCT, --mse-delta X, --limit N); defaults to two latest
corpus-diff baseline="" compare="" *args:
    python3 scripts/corpus_diff.py diff {{ if baseline != "" { "\"" + baseline + "\" " } else { "" } }}{{ if compare != "" { "\"" + compare + "\" " } else { "" } }}{{args}}

# Generate or verify AVIF reference images
convert *args:
    python3 scripts/generate_references.py {{args}}

# Run vexel binary
vexel *args:
    target/release/vexel {{args}}
