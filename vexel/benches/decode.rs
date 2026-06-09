use criterion::{criterion_group, criterion_main, Criterion};
use std::fs;

const IMAGES_PATH: &str = "benches/images";

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
        let full_path = format!("{}/{}", IMAGES_PATH, case.path);
        let data = fs::read(&full_path).unwrap_or_else(|_| panic!("Missing {full_path}"));

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
        let full_path = format!("{}/{}", IMAGES_PATH, case.path);
        let data = fs::read(&full_path).unwrap_or_else(|_| panic!("Missing {full_path}"));

        group.bench_function(case.name, |b| {
            b.iter(|| {
                let cursor = std::io::Cursor::new(&data);
                vexel::Vexel::new(cursor).unwrap().decode().unwrap()
            });
        });
    }

    group.finish();
}

criterion_group!(benches, bench_jpeg, bench_png);
criterion_main!(benches);
