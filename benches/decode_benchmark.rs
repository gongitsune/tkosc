use criterion::{Criterion, black_box, criterion_group, criterion_main};
use tkosc::{OscPack, OscUnpack};

#[derive(OscPack, OscUnpack)]
struct LargeMessage<'a> {
    id: i32,
    timestamp: i64,
    data: &'a [u8],
    label: &'a str,
    value: f64,
}

fn benchmark_decode(c: &mut Criterion) {
    // 大きなメッセージを準備
    let large_data = vec![0u8; 1024 * 64]; // 64KB
    let msg = LargeMessage {
        id: 12345,
        timestamp: 1234567890,
        data: &large_data,
        label: "test_message_with_long_label_name",
        value: 3.14159265359,
    };

    let mut buf = Vec::new();
    msg.pack("/bench/test", &mut buf);

    c.bench_function("decode_large_message_zerocopy", |b| {
        b.iter(|| {
            let (addr, rest) = tkosc::decode_osc_string(black_box(&buf)).unwrap();
            let (tag_str, rest) = tkosc::decode_osc_string(rest).unwrap();
            let type_tag = &tag_str.as_bytes()[1..];

            let decoded = LargeMessage::unpack(&addr, type_tag, rest).unwrap();
            black_box(decoded);
        })
    });
}

criterion_group!(benches, benchmark_decode);
criterion_main!(benches);
