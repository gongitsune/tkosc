pub use tkosc_macros::{OscPack, OscUnpack};

pub trait OscPack {
    fn pack(&self, address: &str, buf: &mut Vec<u8>);
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnpackError {
    InvalidMessage,
    InvalidAddress,
    InvalidTypeTag,
    TagCountMismatch {
        expected: usize,
        found: usize,
    },
    TagMismatch {
        field: &'static str,
        expected: &'static str,
        found: char,
    },
    UnexpectedEof {
        field: &'static str,
    },
    InvalidString {
        field: &'static str,
    },
    InvalidBlob {
        field: &'static str,
    },
}

pub trait OscUnpack: Sized {
    fn unpack(address: &str, type_tag: &[u8], data: &[u8]) -> Result<Self, UnpackError>;
}

/// 4バイトアラインメント後のバイト数。マクロ展開から `osc_pack::padded_len` として参照される。
#[inline(always)]
pub const fn padded_len(n: usize) -> usize {
    (n + 3) & !3
}

/// OSC 文字列エンコード (null終端 + 4バイトアライン)
#[inline(always)]
pub fn encode_osc_string(s: &str, buf: &mut Vec<u8>) {
    buf.extend_from_slice(s.as_bytes());
    buf.push(0u8);
    let pad = (4 - buf.len() % 4) % 4;
    buf.extend_from_slice(&[0u8; 3][..pad]);
}

/// OSC blob エンコード (4バイト長 + データ + 4バイトアライン)
#[inline(always)]
pub fn encode_osc_blob(data: &[u8], buf: &mut Vec<u8>) {
    buf.extend_from_slice(&(data.len() as u32).to_be_bytes());
    buf.extend_from_slice(data);
    let pad = padded_len(data.len()) - data.len();
    buf.extend_from_slice(&[0u8; 3][..pad]);
}

/// OSC 文字列デコード (null終端 + 4バイトアライン)
/// 成功時は (文字列, 残りのスライス) を返す
#[inline(always)]
pub fn decode_osc_string(data: &[u8]) -> Option<(String, &[u8])> {
    // null終端を探す
    let null_pos = data.iter().position(|&b| b == 0)?;
    let s = std::str::from_utf8(&data[..null_pos]).ok()?;

    // パディングを含めた次の位置
    let padded = padded_len(null_pos + 1);
    if data.len() < padded {
        return None;
    }

    Some((s.to_string(), &data[padded..]))
}

/// OSC blob デコード (4バイト長 + データ + 4バイトアライン)
/// 成功時は (データ, 残りのスライス) を返す
#[inline(always)]
pub fn decode_osc_blob(data: &[u8]) -> Option<(Vec<u8>, &[u8])> {
    if data.len() < 4 {
        return None;
    }

    let len = u32::from_be_bytes(data[..4].try_into().unwrap()) as usize;
    let padded = padded_len(len);

    if data.len() < 4 + padded {
        return None;
    }

    let blob = data[4..4 + len].to_vec();
    Some((blob, &data[4 + padded..]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as tkosc;

    #[test]
    fn padded_len_cases() {
        assert_eq!(padded_len(0), 0);
        assert_eq!(padded_len(1), 4);
        assert_eq!(padded_len(3), 4);
        assert_eq!(padded_len(4), 4);
        assert_eq!(padded_len(5), 8);
    }

    #[test]
    fn encode_osc_string_alignment() {
        let mut buf = Vec::new();
        encode_osc_string("/foo", &mut buf);
        assert_eq!(buf, b"/foo\0\0\0\0"); // 4+1=5 → pad3 → 8
        buf.clear();
        encode_osc_string("abc", &mut buf);
        assert_eq!(buf, b"abc\0"); // 3+1=4 → pad0 → 4
    }

    #[test]
    fn encode_osc_blob_padding() {
        let mut buf = Vec::new();
        encode_osc_blob(&[1, 2, 3], &mut buf);
        // len(4) + 3bytes + pad1 = 8
        assert_eq!(buf, &[0, 0, 0, 3, 1, 2, 3, 0]);
    }

    #[derive(OscPack)]
    struct SynthParams {
        freq: f32,
        gain: f32,
        note: i32,
        label: String,
    }

    #[test]
    fn pack_synth_params_into_reused_buf() {
        let p = SynthParams {
            freq: 440.0_f32,
            gain: 0.8_f32,
            note: 69,
            label: "A4".to_string(),
        };

        // バッファ使い回しパターン
        let mut buf = Vec::with_capacity(64);
        p.pack("/synth/note", &mut buf);

        // address "/synth/note\0" = 12バイト
        assert_eq!(&buf[0..12], b"/synth/note\0");
        // type tag ",ffis\0\0\0" = 8バイト (bool なし → static literal)
        assert_eq!(&buf[12..20], b",ffis\0\0\0");
        // f32 440.0
        assert_eq!(f32::from_be_bytes(buf[20..24].try_into().unwrap()), 440.0);
        // f32 0.8
        assert!((f32::from_be_bytes(buf[24..28].try_into().unwrap()) - 0.8).abs() < 1e-7);
        // i32 69
        assert_eq!(i32::from_be_bytes(buf[28..32].try_into().unwrap()), 69);
        // String "A4\0\0"
        assert_eq!(&buf[32..36], b"A4\0\0");

        // 2回目: clear して同じバッファを再利用 (アロケートなし)
        buf.clear();
        p.pack("/synth/note", &mut buf);
        assert_eq!(buf.len(), 36);
    }

    #[derive(OscPack)]
    struct WithBool {
        value: i32,
        active: bool,
    }

    #[test]
    fn pack_bool_runtime_tag() {
        let mut buf = Vec::new();
        WithBool {
            value: 1,
            active: true,
        }
        .pack("/x", &mut buf);
        // type tag は ",iT\0" = 4バイト (実行時生成だが with_capacity 済み)
        let tag_start = padded_len("/x".len() + 1); // 4
        assert_eq!(&buf[tag_start..tag_start + 4], b",iT\0");

        buf.clear();
        WithBool {
            value: 1,
            active: false,
        }
        .pack("/x", &mut buf);
        assert_eq!(&buf[tag_start..tag_start + 4], b",iF\0");
    }

    #[test]
    fn decode_osc_string_basic() {
        let data = b"hello\0\0\0more";
        let (s, rest) = decode_osc_string(data).unwrap();
        assert_eq!(s, "hello");
        assert_eq!(rest, b"more");

        let data = b"abc\0rest";
        let (s, rest) = decode_osc_string(data).unwrap();
        assert_eq!(s, "abc");
        assert_eq!(rest, b"rest");
    }

    #[test]
    fn decode_osc_blob_basic() {
        let data = [0, 0, 0, 3, 1, 2, 3, 0, b'm', b'o', b'r', b'e'];
        let (blob, rest) = decode_osc_blob(&data).unwrap();
        assert_eq!(blob, &[1, 2, 3]);
        assert_eq!(rest, b"more");
    }

    #[derive(OscPack, OscUnpack, Debug, PartialEq)]
    struct UnpackTest {
        freq: f32,
        gain: f32,
        note: i32,
        label: String,
    }

    #[test]
    fn pack_and_unpack_roundtrip() {
        let original = UnpackTest {
            freq: 440.0,
            gain: 0.8,
            note: 69,
            label: "A4".to_string(),
        };

        let mut buf = Vec::new();
        original.pack("/synth/note", &mut buf);

        // OSCメッセージをパース
        // address
        let (addr, rest) = decode_osc_string(&buf).unwrap();
        assert_eq!(addr, "/synth/note");

        // type tag
        let (tag_str, rest) = decode_osc_string(rest).unwrap();
        assert_eq!(tag_str, ",ffis");
        let type_tag = &tag_str.as_bytes()[1..]; // カンマを除く

        // unpack
        let decoded = UnpackTest::unpack(&addr, type_tag, rest).unwrap();
        assert_eq!(decoded, original);
    }

    #[derive(OscPack, OscUnpack, Debug, PartialEq)]
    struct WithBoolUnpack {
        value: i32,
        active: bool,
    }

    #[test]
    fn unpack_bool_true() {
        let mut buf = Vec::new();
        WithBoolUnpack {
            value: 42,
            active: true,
        }
        .pack("/test", &mut buf);

        let (addr, rest) = decode_osc_string(&buf).unwrap();
        let (tag_str, rest) = decode_osc_string(rest).unwrap();
        let type_tag = &tag_str.as_bytes()[1..];

        let decoded = WithBoolUnpack::unpack(&addr, type_tag, rest).unwrap();
        assert_eq!(decoded.value, 42);
        assert_eq!(decoded.active, true);
    }

    #[test]
    fn unpack_bool_false() {
        let mut buf = Vec::new();
        WithBoolUnpack {
            value: 42,
            active: false,
        }
        .pack("/test", &mut buf);

        let (addr, rest) = decode_osc_string(&buf).unwrap();
        let (tag_str, rest) = decode_osc_string(rest).unwrap();
        let type_tag = &tag_str.as_bytes()[1..];

        let decoded = WithBoolUnpack::unpack(&addr, type_tag, rest).unwrap();
        assert_eq!(decoded.value, 42);
        assert_eq!(decoded.active, false);
    }

    #[derive(OscPack, OscUnpack, Debug, PartialEq)]
    struct WithBlob {
        id: i32,
        data: Vec<u8>,
    }

    #[test]
    fn unpack_blob() {
        let original = WithBlob {
            id: 100,
            data: vec![1, 2, 3, 4, 5],
        };

        let mut buf = Vec::new();
        original.pack("/blob", &mut buf);

        let (addr, rest) = decode_osc_string(&buf).unwrap();
        let (tag_str, rest) = decode_osc_string(rest).unwrap();
        let type_tag = &tag_str.as_bytes()[1..];

        let decoded = WithBlob::unpack(&addr, type_tag, rest).unwrap();
        assert_eq!(decoded, original);
    }

    #[derive(OscPack, OscUnpack, Debug, PartialEq)]
    struct AllTypes {
        i: i32,
        f: f32,
        h: i64,
        d: f64,
        b: bool,
        s: String,
        blob: Vec<u8>,
    }

    #[test]
    fn unpack_all_types() {
        let original = AllTypes {
            i: -42,
            f: 3.14,
            h: 9223372036854775807,
            d: 2.718281828,
            b: true,
            s: "test".to_string(),
            blob: vec![0xde, 0xad, 0xbe, 0xef],
        };

        let mut buf = Vec::new();
        original.pack("/all", &mut buf);

        let (addr, rest) = decode_osc_string(&buf).unwrap();
        let (tag_str, rest) = decode_osc_string(rest).unwrap();
        let type_tag = &tag_str.as_bytes()[1..];

        let decoded = AllTypes::unpack(&addr, type_tag, rest).unwrap();
        assert_eq!(decoded, original);
    }
}
