use criterion::{criterion_group, criterion_main, Criterion};
use std::fs;

const IMAGE_SEARCH_PATHS: &[&str] = &["benches/images", "tests/images"];

fn find_image(rel_path: &str) -> Vec<u8> {
    for base in IMAGE_SEARCH_PATHS {
        let full = format!("{}/{}", base, rel_path);
        if let Ok(data) = fs::read(&full) {
            return data;
        }
    }
    panic!("Missing image in any search path: {rel_path}");
}

struct BenchCase {
    name: &'static str,
    path: &'static str,
}

fn bench_jpeg(c: &mut Criterion) {
    let mut group = c.benchmark_group("jpeg");

    let cases = [
        BenchCase { name: "massimiliano_unsplash", path: "jpeg/massimiliano-morosinotto-3i5PHVp1Fkw-unsplash.jpg" },
        BenchCase { name: "cat_arithmetic", path: "jpeg/cat_arithmetic.jpg" },
    ];

    for case in &cases {
        let data = find_image(case.path);

        group.bench_function(case.name, |b| {
            b.iter(|| {
                let cursor = std::io::Cursor::new(&data);
                vexel::Vexel::new(cursor).unwrap().decode().unwrap()
            });
        });
    }

    group.finish();
}

fn bench_png(c: &mut Criterion) {
    let mut group = c.benchmark_group("png");

    let cases = [
        BenchCase { name: "7b00e7bc", path: "png/7b00e7bc5b225e9495861e17183db6da.png" },
    ];

    for case in &cases {
        let data = find_image(case.path);

        group.bench_function(case.name, |b| {
            b.iter(|| {
                let cursor = std::io::Cursor::new(&data);
                vexel::Vexel::new(cursor).unwrap().decode().unwrap()
            });
        });
    }

    group.finish();
}

fn bench_hdr(c: &mut Criterion) {
    let mut group = c.benchmark_group("hdr");

    let cases = [
        BenchCase { name: "venice_sunset_8k", path: "hdr/venice_sunset_8k.hdr" },
    ];

    for case in &cases {
        let data = find_image(case.path);

        group.bench_function(case.name, |b| {
            b.iter(|| {
                let cursor = std::io::Cursor::new(&data);
                vexel::Vexel::new(cursor).unwrap().decode().unwrap()
            });
        });
    }

    group.finish();
}

fn bench_netpbm(c: &mut Criterion) {
    let mut group = c.benchmark_group("netpbm");

    let cases = [
        BenchCase { name: "P3_16bit", path: "netpbm/P3_16bit.ppm" },
    ];

    for case in &cases {
        let data = find_image(case.path);

        group.bench_function(case.name, |b| {
            b.iter(|| {
                let cursor = std::io::Cursor::new(&data);
                vexel::Vexel::new(cursor).unwrap().decode().unwrap()
            });
        });
    }

    group.finish();
}

fn bench_tiff(c: &mut Criterion) {
    let mut group = c.benchmark_group("tiff");

    let cases = [
        BenchCase { name: "RAW_NIKON_D800_M", path: "tiff/RAW_NIKON_D800_M.tiff" },
    ];

    for case in &cases {
        let data = find_image(case.path);

        group.bench_function(case.name, |b| {
            b.iter(|| {
                let cursor = std::io::Cursor::new(&data);
                vexel::Vexel::new(cursor).unwrap().decode().unwrap()
            });
        });
    }

    group.finish();
}

criterion_group!(benches, bench_jpeg, bench_png, bench_hdr, bench_netpbm, bench_tiff);
criterion_main!(benches);
