# TKOSC

高性能なOSC (Open Sound Control) パック/アンパックライブラリ

## 特徴

- **マクロベースの自動実装**: `#[derive(OscPack, OscUnpack)]`で構造体の変換を自動生成
- **コンパイル時最適化**: 固定長フィールドのサイズ計算は完全にコンパイル時に畳み込み
- **ゼロコピー設計**: バッファの再利用により不要なアロケーションを排除
- **型安全**: Rustの型システムによる安全なシリアライゼーション/デシリアライゼーション

## 対応型

| Rust型 | OSCタグ | 説明 |
|--------|---------|------|
| `i32` | `i` | 32ビット整数 |
| `f32` | `f` | 32ビット浮動小数点数 |
| `i64` | `h` | 64ビット整数 |
| `f64` | `d` | 64ビット浮動小数点数 |
| `bool` | `T/F` | 真偽値 |
| `String` | `s` | 文字列 |
| `Vec<u8>` | `b` | バイナリデータ (blob) |

## 使用例

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
    // パック
    let params = SynthParams {
        freq: 440.0,
        gain: 0.8,
        note: 69,
        label: "A4".to_string(),
    };

    let mut buf = Vec::new();
    params.pack("/synth/note", &mut buf);

    // アンパック
    let (address, rest) = tkosc::decode_osc_string(&buf).unwrap();
    let (type_tag_str, rest) = tkosc::decode_osc_string(rest).unwrap();
    let type_tag = &type_tag_str.as_bytes()[1..]; // カンマを除く

    let decoded = SynthParams::unpack(&address, type_tag, rest).unwrap();
    assert_eq!(decoded, params);
}
```

## パフォーマンス最適化

### バッファの再利用

```rust
let mut buf = Vec::with_capacity(64);
for params in params_list {
    buf.clear(); // バッファをクリアして再利用
    params.pack("/synth/note", &mut buf);
    send_osc_message(&buf);
}
```

### コンパイル時サイズ計算

固定長フィールド（i32, f32, i64, f64）のサイズはコンパイル時に計算されます。

```rust
#[derive(OscPack)]
struct Fixed {
    a: i32,  // 4バイト
    b: f32,  // 4バイト
    c: i64,  // 8バイト
}
// 合計16バイトはコンパイル時定数として展開
```

## ライセンス

MIT
