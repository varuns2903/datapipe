use criterion::{black_box, criterion_group, criterion_main, Criterion};
use datapipe::io::read_json_stream;
use datapipe::pipeline::Pipeline;
use datapipe::stages::*;
use std::io::Cursor;
use datapipe::expr::parse;

fn generate_mock_data(count: usize) -> Vec<u8> {
    let mut data = Vec::new();
    for i in 0..count {
        let json = format!(r#"{{"name":"User{}","age":{},"status":200}}"#, i, i % 100);
        data.extend_from_slice(json.as_bytes());
        data.push(b'\n');
    }
    data
}

fn bench_filter_pipeline(c: &mut Criterion) {
    let data = generate_mock_data(10_000);

    c.bench_function("pipeline_filter_10k", |b| {
        b.iter(|| {
            let cursor = Cursor::new(black_box(&data));
            let records = Box::new(read_json_stream(cursor));
            
            let mut pipeline = Pipeline::new();
            pipeline.add_stage(Box::new(FilterStage { ast: parse(".age > 50").unwrap() }));
            pipeline.add_stage(Box::new(CountStage));
            
            let result = pipeline.process(records);
            for rec in result {
                black_box(rec.unwrap());
            }
        });
    });
}

criterion_group!(benches, bench_filter_pipeline);
criterion_main!(benches);
