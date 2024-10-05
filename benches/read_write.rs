use std::io::{Read, Write};

use criterion::{black_box, criterion_group, criterion_main, Criterion};

use mockpipe::MockPipe;

fn benchmark_loopback_write(c: &mut Criterion) {
    let mut pipe = MockPipe::loopback(1024);

    c.bench_function("loopback_write_hello", |b| {
        b.iter(|| {
            pipe.write_all(black_box(b"hello")).unwrap();
            pipe.clear_write(); // Clear to reset state for next iteration
        })
    });

    c.bench_function("loopback_write_large", |b| {
        let data = vec![0u8; 1024];
        b.iter(|| {
            pipe.write_all(black_box(&data)).unwrap();
            pipe.clear_write();
        })
    });
}

fn benchmark_loopback_read(c: &mut Criterion) {
    let mut pipe = MockPipe::loopback(1024);

    c.bench_function("loopback_read_hello", |b| {
        b.iter(|| {
            pipe.write_all(b"hello").unwrap();
            let mut buffer = [0u8; 5];
            pipe.read_exact(black_box(&mut buffer)).unwrap();
            pipe.clear_read();
        })
    });

    c.bench_function("loopback_read_large", |b| {
        let mut buffer = vec![0u8; 1024];
        b.iter(|| {
            pipe.write_all(&vec![0u8; 1024]).unwrap();
            pipe.read_exact(black_box(&mut buffer)).unwrap();
            pipe.clear_read();
        })
    });
}

fn benchmark_pair_write(c: &mut Criterion) {
    let (mut pipe1, _pipe2) = MockPipe::pair(1024);

    c.bench_function("pair_write_hello", |b| {
        b.iter(|| {
            pipe1.write_all(black_box(b"hello")).unwrap();
            pipe1.clear_write();
        })
    });

    c.bench_function("pair_write_large", |b| {
        let data = vec![0u8; 1024];
        b.iter(|| {
            pipe1.write_all(black_box(&data)).unwrap();
            pipe1.clear_write();
        })
    });
}

fn benchmark_pair_read(c: &mut Criterion) {
    let (mut pipe1, mut pipe2) = MockPipe::pair(1024);

    c.bench_function("pair_read_hello", |b| {
        b.iter(|| {
            pipe1.write_all(b"hello").unwrap();
            let mut buffer = [0u8; 5];
            pipe2.read_exact(black_box(&mut buffer)).unwrap();
            pipe2.clear_read();
        })
    });

    c.bench_function("pair_read_large", |b| {
        let mut buffer = vec![0u8; 1024];
        b.iter(|| {
            pipe1.write_all(&vec![0u8; 1024]).unwrap();
            pipe2.read_exact(black_box(&mut buffer)).unwrap();
            pipe2.clear_read();
        })
    });
}

criterion_group!(
    benches,
    benchmark_loopback_write,
    benchmark_loopback_read,
    benchmark_pair_write,
    benchmark_pair_read
);
criterion_main!(benches);
