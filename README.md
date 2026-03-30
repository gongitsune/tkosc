# TKOSC

This is a simple OSC implementation for personal use.


## Features

- **Macro-based automatic implementation**: Converts structures automatically using #[derive(OscPack, OscUnpack)].
- **Compile-time optimization**: Size calculation for fixed-length fields is performed entirely at compile time.
- **Zero-copy decoding**: String and blob data are returned as references to the original byte slice, eliminating allocations.
- **Type safety**: Ensures secure serialization and deserialization utilizing Rust's type system.

## Compatible types

### Encoding (OscPack)
| Rust Type | OSC Tag | Description |
|--------|---------|------|
| `i32` | `i` | 32-bit integer |
| `f32` | `f` | 32-bit float |
| `i64` | `h` | 64-bit integer |
| `f64` | `d` | 64-bit double |
| `bool` | `T/F` | Boolean |
| `String`, `&str` | `s` | String |
| `Vec<u8>`, `&[u8]` | `b` | Binary data (blob) |

### Decoding (OscUnpack)
| Rust Type | OSC Tag | Description |
|--------|---------|------|
| `i32` | `i` | 32-bit integer |
| `f32` | `f` | 32-bit float |
| `i64` | `h` | 64-bit integer |
| `f64` | `d` | 64-bit double |
| `bool` | `T/F` | Boolean |
| `&'a str` | `s` | String reference (zero-copy) |
| `&'a [u8]` | `b` | Binary data reference (zero-copy) |

## Usage

### Basic encode/decode

```rust
use tkosc::{OscPack, OscUnpack};

// For encoding: owned or borrowed types
#[derive(OscPack)]
struct SynthParamsOwned {
    freq: f32,
    gain: f32,
    note: i32,
    label: String,
}

// For decoding: zero-copy borrowed types
#[derive(OscUnpack)]
struct SynthParams<'a> {
    freq: f32,
    gain: f32,
    note: i32,
    label: &'a str,
}

fn main() {
    // Pack
    let params = SynthParamsOwned {
        freq: 440.0,
        gain: 0.8,
        note: 69,
        label: "A4".to_string(),
    };

    let mut buf = Vec::new();
    params.pack("/synth/note", &mut buf);

    // Zero-copy unpack
    let (address, rest) = tkosc::decode_osc_string(&buf).unwrap();
    let (type_tag_str, rest) = tkosc::decode_osc_string(rest).unwrap();
    let type_tag = &type_tag_str.as_bytes()[1..];

    let decoded = SynthParams::unpack(&address, type_tag, rest).unwrap();
    // decoded.label is a reference to buf - no allocation!
    assert_eq!(decoded.label, "A4");
}
```

### Same struct for both pack/unpack

```rust
#[derive(OscPack, OscUnpack, Debug, PartialEq)]
struct Message<'a> {
    id: i32,
    data: &'a [u8],
    label: &'a str,
}

// Sender side
let msg = Message {
    id: 42,
    data: &[1, 2, 3, 4],
    label: "test",
};
let mut buf = Vec::new();
msg.pack("/msg", &mut buf);

// Receiver side (zero-copy)
let (addr, rest) = tkosc::decode_osc_string(&buf).unwrap();
let (tag_str, rest) = tkosc::decode_osc_string(rest).unwrap();
let type_tag = &tag_str.as_bytes()[1..];

let received = Message::unpack(&addr, type_tag, rest).unwrap();
assert_eq!(received, msg);
// received.data and received.label are references to buf!
```

## Performance Optimization

### 1. Buffer Reuse (Encoding)

```rust
let mut buf = Vec::with_capacity(64);
for params in params_list {
    buf.clear(); // Reuse buffer
    params.pack("/synth/note", &mut buf);
    send_osc_message(&buf);
}
```

### 2. Zero-Copy Decoding

During decoding, `&str` and `&[u8]` fields are returned as references to the original byte slice.
Benefits include:

- **No memory allocation**: No copying to `String` or `Vec<u8>`
- **Cache efficiency**: Fast access due to contiguous memory layout
- **Low latency**: OSC message processing can start immediately

```rust
// Message with large binary data
#[derive(OscUnpack)]
struct AudioData<'a> {
    samples: &'a [u8],  // Even megabytes of data - no copy!
}

// Buffer is kept externally
let buf: Vec<u8> = receive_osc_message();
let audio = AudioData::unpack(&addr, type_tag, data).unwrap();
// audio.samples points to buf - no memory copy!
```

### 3. Compile-Time Size Calculation

The size of fixed-length fields (i32, f32, i64, f64) is calculated at compile time.

```rust
#[derive(OscPack)]
struct Fixed {
    a: i32,  // 4 bytes
    b: f32,  // 4 bytes
    c: i64,  // 8 bytes
}
// Total 16 bytes is expanded as compile-time constant
```

## Design Philosophy

### Encoding
- Flexibility: Accepts both `String`/`&str` and `Vec<u8>`/`&[u8]`
- Single allocation: Pre-calculates buffer size and reserves once

### Decoding
- Performance: Complete zero-copy, references only
- Lifetimes: Decoded structs are bound to the lifetime of the original buffer

## LICENSE

MIT
