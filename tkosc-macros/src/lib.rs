use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, GenericArgument, PathArguments, Type, parse_macro_input};

#[derive(Clone, Copy, PartialEq)]
enum OscTag {
    Int32,   // i  — 4バイト固定
    Float32, // f  — 4バイト固定
    Int64,   // h  — 8バイト固定
    Float64, // d  — 8バイト固定
    Bool,    // T/F — 引数バイトなし、実行時分岐
    Str,     // s  — 実行時可変
    Blob,    // b  — 実行時可変
}

impl OscTag {
    fn char(self) -> char {
        match self {
            Self::Int32 => 'i',
            Self::Float32 => 'f',
            Self::Int64 => 'h',
            Self::Float64 => 'd',
            Self::Bool => 'T',
            Self::Str => 's',
            Self::Blob => 'b',
        }
    }

    /// コンパイル時に確定する引数バイト数。可変長は None
    fn static_arg_bytes(self) -> Option<usize> {
        match self {
            Self::Int32 | Self::Float32 => Some(4),
            Self::Int64 | Self::Float64 => Some(8),
            Self::Bool => Some(0),
            Self::Str | Self::Blob => None,
        }
    }

    fn is_runtime(self) -> bool {
        self == Self::Bool
    }
}

fn parse_tag(ty: &Type) -> Option<OscTag> {
    let path = match ty {
        Type::Path(p) => &p.path,
        _ => return None,
    };
    let last = path.segments.last()?;
    match last.ident.to_string().as_str() {
        "i32" => Some(OscTag::Int32),
        "f32" => Some(OscTag::Float32),
        "i64" => Some(OscTag::Int64),
        "f64" => Some(OscTag::Float64),
        "bool" => Some(OscTag::Bool),
        "String" => Some(OscTag::Str),
        "Vec" => {
            if let PathArguments::AngleBracketed(args) = &last.arguments {
                if let Some(GenericArgument::Type(Type::Path(inner))) = args.args.first() {
                    if inner.path.is_ident("u8") {
                        return Some(OscTag::Blob);
                    }
                }
            }
            None
        }
        _ => None,
    }
}

const fn padded_len(n: usize) -> usize {
    (n + 3) & !3
}

#[proc_macro_derive(OscPack)]
pub fn derive_osc_pack(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    let name = &ast.ident;

    let named_fields = match &ast.data {
        Data::Struct(s) => match &s.fields {
            Fields::Named(f) => &f.named,
            _ => panic!("OscPack is only supported for structs with named fields"),
        },
        _ => panic!("OscPack is only supported for structs"),
    };

    struct FieldInfo<'a> {
        ident: &'a syn::Ident,
        tag: OscTag,
    }
    let fields: Vec<FieldInfo> = named_fields
        .iter()
        .map(|f| {
            let ident = f.ident.as_ref().expect("field name are required");
            let tag =
                parse_tag(&f.ty).unwrap_or_else(|| panic!("type of `{}` is not supported", ident));
            FieldInfo { ident, tag }
        })
        .collect();

    let has_runtime_tag = fields.iter().any(|f| f.tag.is_runtime());

    // ----------------------------------------------------------------
    // 1. type tag 文字列
    //    bool なし → &'static str リテラル (コンパイル時定数)
    //    bool あり → 実行時 String 構築、with_capacity で1回確保
    // ----------------------------------------------------------------
    let type_tag_code = if has_runtime_tag {
        let static_len = fields.len() + 1;
        let pushes = fields.iter().map(|f| {
            let ident = f.ident;
            if f.tag.is_runtime() {
                quote! { type_tag.push(if self.#ident { 'T' } else { 'F' }); }
            } else {
                let c = f.tag.char();
                quote! { type_tag.push(#c); }
            }
        });
        quote! {
            let mut type_tag = String::with_capacity(#static_len);
            type_tag.push(',');
            #(#pushes)*
            tkosc::encode_osc_string(&type_tag, buf);
        }
    } else {
        // bool なし: type tag をコンパイル時リテラルに展開
        let tag_str: String = std::iter::once(',')
            .chain(fields.iter().map(|f| f.tag.char()))
            .collect();
        quote! {
            tkosc::encode_osc_string(#tag_str, buf);
        }
    };

    // ----------------------------------------------------------------
    // 2. 引数エンコード
    // ----------------------------------------------------------------
    let encode_stmts = fields.iter().map(|f| {
        let ident = f.ident;
        match f.tag {
            OscTag::Int32 | OscTag::Float32 | OscTag::Int64 | OscTag::Float64 => quote! {
                buf.extend_from_slice(&self.#ident.to_be_bytes());
            },
            OscTag::Str => quote! {
                tkosc::encode_osc_string(&self.#ident, buf);
            },
            OscTag::Blob => quote! {
                tkosc::encode_osc_blob(&self.#ident, buf);
            },
            OscTag::Bool => quote! {},
        }
    });

    // ----------------------------------------------------------------
    // 3. reserve: 静的サイズはコンパイル時定数に畳み込み、動的のみ実行時加算
    // ----------------------------------------------------------------
    let static_arg_bytes: usize = fields.iter().filter_map(|f| f.tag.static_arg_bytes()).sum();

    // type tag のパディング済みサイズ
    // bool あり: 実行時長になるが文字数は fields.len()+2 で上界が決まるので同じ計算でOK
    let tag_str_padded = padded_len(fields.len() + 2); // カンマ + タグ文字 + null

    let dynamic_cap_exprs: Vec<_> = fields
        .iter()
        .filter_map(|f| {
            let ident = f.ident;
            match f.tag {
                OscTag::Str => Some(quote! {
                    tkosc::padded_len(self.#ident.len() + 1)
                }),
                OscTag::Blob => Some(quote! {
                    4 + tkosc::padded_len(self.#ident.len())
                }),
                _ => None,
            }
        })
        .collect();

    let reserve_expr = if dynamic_cap_exprs.is_empty() {
        // 全フィールドが固定長 → address 以外は完全にコンパイル時定数
        quote! {
            buf.reserve(tkosc::padded_len(address.len() + 1) + #tag_str_padded + #static_arg_bytes);
        }
    } else {
        quote! {
            let _dynamic: usize = 0usize #( + #dynamic_cap_exprs)*;
            buf.reserve(
                tkosc::padded_len(address.len() + 1)
                + #tag_str_padded
                + #static_arg_bytes
                + _dynamic,
            );
        }
    };

    // ----------------------------------------------------------------
    // 4. impl 組み立て
    // ----------------------------------------------------------------
    let expanded = quote! {
        impl tkosc::OscPack for #name {
            #[inline]
            fn pack(&self, address: &str, buf: &mut Vec<u8>) {
                #reserve_expr
                tkosc::encode_osc_string(address, buf);
                #type_tag_code
                #(#encode_stmts)*
            }
        }
    };

    expanded.into()
}

#[proc_macro_derive(OscUnpack)]
pub fn derive_osc_unpack(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    let name = &ast.ident;

    let named_fields = match &ast.data {
        Data::Struct(s) => match &s.fields {
            Fields::Named(f) => &f.named,
            _ => panic!("OscUnpack is only supported for structs with named fields"),
        },
        _ => panic!("OscUnpack is only supported for structs"),
    };

    struct FieldInfo<'a> {
        ident: &'a syn::Ident,
        tag: OscTag,
    }
    let fields: Vec<FieldInfo> = named_fields
        .iter()
        .map(|f| {
            let ident = f.ident.as_ref().expect("field name are required");
            let tag =
                parse_tag(&f.ty).unwrap_or_else(|| panic!("type of `{}` is not supported", ident));
            FieldInfo { ident, tag }
        })
        .collect();

    // ----------------------------------------------------------------
    // 1. type tag の検証コード
    // ----------------------------------------------------------------
    let tag_checks = fields.iter().enumerate().map(|(idx, f)| {
        let ident = f.ident;
        if f.tag.is_runtime() {
            // bool の場合は T/F のどちらか
            quote! {
                if type_tag[#idx] != b'T' && type_tag[#idx] != b'F' {
                    return Err(tkosc::UnpackError::TagMismatch {
                        field: stringify!(#ident),
                        expected: "T/F",
                        found: type_tag[#idx] as char,
                    });
                }
            }
        } else {
            let c = f.tag.char() as u8;
            quote! {
                if type_tag[#idx] != #c {
                    return Err(tkosc::UnpackError::TagMismatch {
                        field: stringify!(#ident),
                        expected: stringify!(#c),
                        found: type_tag[#idx] as char,
                    });
                }
            }
        }
    });

    // ----------------------------------------------------------------
    // 2. 引数デコード
    // ----------------------------------------------------------------
    let decode_stmts = fields.iter().enumerate().map(|(idx, f)| {
        let ident = f.ident;
        match f.tag {
            OscTag::Int32 => quote! {
                let #ident = {
                    if data.len() < 4 {
                        return Err(tkosc::UnpackError::UnexpectedEof {
                            field: stringify!(#ident),
                        });
                    }
                    let bytes: [u8; 4] = data[..4].try_into().unwrap();
                    data = &data[4..];
                    i32::from_be_bytes(bytes)
                };
            },
            OscTag::Float32 => quote! {
                let #ident = {
                    if data.len() < 4 {
                        return Err(tkosc::UnpackError::UnexpectedEof {
                            field: stringify!(#ident),
                        });
                    }
                    let bytes: [u8; 4] = data[..4].try_into().unwrap();
                    data = &data[4..];
                    f32::from_be_bytes(bytes)
                };
            },
            OscTag::Int64 => quote! {
                let #ident = {
                    if data.len() < 8 {
                        return Err(tkosc::UnpackError::UnexpectedEof {
                            field: stringify!(#ident),
                        });
                    }
                    let bytes: [u8; 8] = data[..8].try_into().unwrap();
                    data = &data[8..];
                    i64::from_be_bytes(bytes)
                };
            },
            OscTag::Float64 => quote! {
                let #ident = {
                    if data.len() < 8 {
                        return Err(tkosc::UnpackError::UnexpectedEof {
                            field: stringify!(#ident),
                        });
                    }
                    let bytes: [u8; 8] = data[..8].try_into().unwrap();
                    data = &data[8..];
                    f64::from_be_bytes(bytes)
                };
            },
            OscTag::Bool => quote! {
                let #ident = type_tag[#idx] == b'T';
            },
            OscTag::Str => quote! {
                let #ident = {
                    let (s, rest) = tkosc::decode_osc_string(data)
                        .ok_or_else(|| tkosc::UnpackError::InvalidString {
                            field: stringify!(#ident),
                        })?;
                    data = rest;
                    s
                };
            },
            OscTag::Blob => quote! {
                let #ident = {
                    let (b, rest) = tkosc::decode_osc_blob(data)
                        .ok_or_else(|| tkosc::UnpackError::InvalidBlob {
                            field: stringify!(#ident),
                        })?;
                    data = rest;
                    b
                };
            },
        }
    });

    let field_names = fields.iter().map(|f| f.ident);
    let fields_count = fields.len();

    // ----------------------------------------------------------------
    // 3. impl 組み立て
    // ----------------------------------------------------------------
    let expanded = quote! {
        impl tkosc::OscUnpack for #name {
            fn unpack(address: &str, type_tag: &[u8], mut data: &[u8]) -> Result<Self, tkosc::UnpackError> {
                // type tag の長さチェック
                if type_tag.len() != #fields_count {
                    return Err(tkosc::UnpackError::TagCountMismatch {
                        expected: #fields_count,
                        found: type_tag.len(),
                    });
                }

                // type tag の各要素チェック
                #(#tag_checks)*

                // 各フィールドのデコード
                #(#decode_stmts)*

                Ok(Self {
                    #(#field_names),*
                })
            }
        }
    };

    expanded.into()
}
