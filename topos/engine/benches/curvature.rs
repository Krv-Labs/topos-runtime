use criterion::{black_box, criterion_group, criterion_main, Criterion};
use topos_engine::functors::curvature::{balanced_forman_curvature, WeightedEdge};

fn synthetic_graph(n: usize, e: usize) -> Vec<WeightedEdge> {
    let mut edges = Vec::with_capacity(e);
    for i in 0..n {
        edges.push(WeightedEdge::new(i, (i + 1) % n, 1.0));
    }
    let mut s = 1u64;
    while edges.len() < e {
        s = s.wrapping_mul(6364136223846793005).wrapping_add(1);
        let u = (s as usize) % n;
        s = s.wrapping_mul(6364136223846793005).wrapping_add(1);
        let v = (s as usize) % n;
        if u != v {
            edges.push(WeightedEdge::new(u, v, 1.0));
        }
    }
    edges
}

fn bench(c: &mut Criterion) {
    let edges = synthetic_graph(10_000, 50_000);
    c.bench_function("balanced_forman_10k_50k", |b| {
        b.iter(|| black_box(balanced_forman_curvature(black_box(&edges))))
    });
}

criterion_group!(benches, bench);
criterion_main!(benches);
