# TKOSC

This is a simple OSC implementation for personal use.


## Features

- **Macro-based automatic implementation**: Converts structures automatically using #[derive(OscPack, OscUnpack)].
- **Compile-time optimization**: Size calculation for fixed-length fields is performed entirely at compile time.
- **Zero-copy design**: Eliminates unnecessary allocations through buffer reuse.
- **Type safety**: Ensures secure serialization and deserialization utilizing Rust's type system.

## Compatible types

| Rust Type | OSC Tag | description |
|--------|---------|------|
| `i32` | `i` | 32bit int |
| `f32` | `f` | 32bit float |
| `i64` | `h` | 64bit int |
| `f64` | `d` | 64bit double |
| `bool` | `T/F` | boolean |
| `String` | `s` | string |
| `Vec<u8>` | `b` | binary data (blob) |

## Usage

```rust
use tkosc::{OscPack, OscUnpack};

#[derive(OscPack, OscUnpack, Debug, PartialEq)]
struct SynthParams {
    freq: f32,
    gain: f32,
    note: i32,
    label: String,
}

fn main() {
    // Pack
    let params = SynthParams {
        freq: 440.0,
        gain: 0.8,
        note: 69,
        label: "A4".to_string(),
    };

    let mut buf = Vec::new();
    params.pack("/synth/note", &mut buf);

    // Unpack
    let (address, rest) = tkosc::decode_osc_string(&buf).unwrap();
    let (type_tag_str, rest) = tkosc::decode_osc_string(rest).unwrap();
    let type_tag = &type_tag_str.as_bytes()[1..];

    let decoded = SynthParams::unpack(&address, type_tag, rest).unwrap();
    assert_eq!(decoded, params);
}
```

## Performance

### Reuse buffer

```rust
let mut buf = Vec::with_capacity(64);
for params in params_list {
    buf.clear();
    params.pack("/synth/note", &mut buf);
    send_osc_message(&buf);
}
```

### Calculate size when compile time

The size of fixed field (i32, f32, i64, f64) calculated at compile time.

```rust
#[derive(OscPack)]
struct Fixed {
    a: i32,  // 4byte
    b: f32,  // 4byte
    c: i64,  // 8byte
}
```

## LICENSE

MIT
