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

# Generate or verify AVIF reference images
convert *args:
    cd vexel/tests && python3 generate_references.py {{args}}
