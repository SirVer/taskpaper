use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::collections::HashMap;
use taskpaper::{db::Database, TaskpaperFile};

fn parse() {
    let db = Database::from_dir("Tasks").unwrap();
    let all_files = db.parse_all_files().unwrap();
}

fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("parse database", |b| b.iter(|| parse()));
}

criterion_group!(
    name = benches;
    config = Criterion::default().sample_size(10);
    targets = criterion_benchmark
);
criterion_main!(benches);
