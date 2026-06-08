# Open image in GUI viewer
gui path:
    cargo run --package vexel-cli --release -- --gui "{{path}}"

# Print image info
info path:
    cargo run --package vexel-cli --release -- --info "{{path}}"

# Decode image and save as WebP
save path:
    cargo run --package vexel-cli --release -- --format webp "{{path}}"

# Run a specific test with output captured
test name:
    cargo test --package vexel --release "{{name}}" -- --nocapture

# Generate or verify AVIF reference images
convert *args:
    cd vexel/tests && python3 generate_references.py {{args}}
