#![doc(html_root_url = "https://docs.rs/prost-derive/0.4.0")]
// The `quote!` macro requires deep recursion.
#![recursion_limit = "4096"]

extern crate itertools;
extern crate proc_macro;
extern crate proc_macro2;
extern crate sha2;
extern crate syn;

#[macro_use]
extern crate failure;
#[macro_use]
extern crate quote;

use failure::Error;
use itertools::Itertools;
use proc_macro::TokenStream;
use proc_macro2::Span;
use syn::punctuated::Punctuated;
use syn::{
    Data,
    DataEnum,
    DataStruct,
    DeriveInput,
    Expr,
    Fields,
    FieldsNamed,
    FieldsUnnamed,
    Ident,
    Variant,
};
use sha2::{Digest, Sha256};

mod field;

use field::Field;

fn try_message(input: TokenStream) -> Result<TokenStream, Error> {
    let input: DeriveInput = syn::parse(input)?;

    let top_level_attrs: Vec<syn::Attribute> = input.attrs;
    let amino_name_attrs: Vec<syn::Attribute> = top_level_attrs.
        into_iter().
        filter(|a| a.
            path.segments.first().unwrap().
            value().ident == "amino_name").collect();
    if amino_name_attrs.len() > 1 {
        bail!("got more than one registered amino_name");
    }
    let is_registered = amino_name_attrs.len() == 1;
    // TODO(ismail): move this into separate function!
    let amino_name: Option<String> = {
        match amino_name_attrs.first() {
            Some(att) => {
                let tts = att.tts.clone().into_iter().collect::<Vec<_>>();
// for example:
//                [
//                    Punct {
//                        op: '=',
//                        spacing: Alone
//                    },
//                    Literal {
//                        lit: "tendermint/socketpv/SignHeartbeatMsg"
//                    }
//                ]
                if tts.len() != 2 {
                    None
                } else {
                    let lit = &tts[1];
                    match lit {
                        // TODO: this leaves the quotes too:
                        proc_macro2::TokenTree::Literal(ref l) => Some(l.to_string()),
                        _ => None,
                    }
                }
            }
            None => None,
        }
    };

    let prefix: Option<Vec<u8>> = {
        match amino_name {
            Some(mut reg) => {
                assert_eq!(reg.remove(0), '"');
                let s = reg.len() - 1;
                assert_eq!(reg.remove(s), '"');
                let (_dis, pre) = compute_disfix(&reg[..]);

                Some(pre)
            }
            None => None,
        }
    };

    let comp_prefix = match prefix {
        Some(p) => {
            quote! {
                // add prefix bytes for registered types:
                let pre = vec![#(#p),*];
                buf.put(pre);
            }
        }
        None => quote!(),
    };


    let ident = input.ident;

    let variant_data = match input.data {
        Data::Struct(variant_data) => variant_data,
        Data::Enum(..) => bail!("Message can not be derived for an enum"),
        Data::Union(..) => bail!("Message can not be derived for a union"),
    };

    if !input.generics.params.is_empty() ||
        input.generics.where_clause.is_some() {
        bail!("Message may not be derived for generic type");
    }

    let fields = match variant_data {
        DataStruct { fields: Fields::Named(FieldsNamed { named: fields, .. }), .. } |
        DataStruct { fields: Fields::Unnamed(FieldsUnnamed { unnamed: fields, .. }), .. } => {
            fields.into_iter().collect()
        }
        DataStruct { fields: Fields::Unit, .. } => Vec::new(),
    };

    let mut next_tag: u32 = 0;
    let mut fields = fields.into_iter()
        .enumerate()
        .flat_map(|(idx, field)| {
            let field_ident = field.ident
                .unwrap_or_else(|| Ident::new(&idx.to_string(), Span::call_site()));
            match Field::new(field.attrs, Some(next_tag)) {
                Ok(Some(field)) => {
                    next_tag = field.tags().iter().max().map(|t| t + 1).unwrap_or(next_tag);
                    Some(Ok((field_ident, field)))
                }
                Ok(None) => None,
                Err(err) => Some(Err(err.context(format!("invalid message field {}.{}",
                                                         ident, field_ident)))),
            }
        })
        .collect::<Result<Vec<(Ident, Field)>, failure::Context<String>>>()?;

    // We want Debug to be in declaration order
    let unsorted_fields = fields.clone();

    // Sort the fields by tag number so that fields will be encoded in tag order.
    // TODO: This encodes oneof fields in the position of their lowest tag,
    // regardless of the currently occupied variant, is that consequential?
    // See: https://developers.google.com/protocol-buffers/docs/encoding#order
    fields.sort_by_key(|&(_, ref field)| field.tags().into_iter().min().unwrap());
    let fields = fields;

    let mut tags = fields.iter().flat_map(|&(_, ref field)| field.tags()).collect::<Vec<_>>();
    let num_tags = tags.len();
    tags.sort();
    tags.dedup();
    if tags.len() != num_tags {
        bail!("message {} has fields with duplicate tags", ident);
    }

    // Put impls in a special module, so that 'extern crate' can be used.
    let module = Ident::new(&format!("{}_MESSAGE", ident), Span::call_site());

    let encoded_len = fields.iter()
        .map(|&(ref field_ident, ref field)| {
            field.encoded_len(quote!(self.#field_ident))
        });
    let encoded_len2 = encoded_len.clone();

    let encode = fields.iter()
        .map(|&(ref field_ident, ref field)| {
            field.encode(quote!(self.#field_ident))
        });

    let merge = fields.iter().map(|&(ref field_ident, ref field)| {
        let merge = field.merge(quote!(self.#field_ident));
        let tags = field.tags().into_iter().map(|tag| quote!(#tag)).intersperse(quote!(|));
        quote!(#(#tags)* => #merge.map_err(|mut error| {
            error.push(STRUCT_NAME, stringify!(#field_ident));
            error
        }),)
    });

    let struct_name = if fields.is_empty() {
        quote!()
    } else {
        quote!(const STRUCT_NAME: &'static str = stringify!(#ident);)
    };

    // TODO
    let is_struct = true;

    let clear = fields.iter()
        .map(|&(ref field_ident, ref field)| {
            field.clear(quote!(self.#field_ident))
        });

    let default = fields.iter()
        .map(|&(ref field_ident, ref field)| {
            let value = field.default();
            quote!(#field_ident: #value,)
        });

    let methods = fields.iter()
        .flat_map(|&(ref field_ident, ref field)| field.methods(field_ident))
        .collect::<Vec<_>>();
    let methods = if methods.is_empty() {
        quote!()
    } else {
        quote! {
            #[allow(dead_code)]
            impl #ident {
                #(#methods)*
            }
        }
    };

    let debugs = unsorted_fields.iter()
        .map(|&(ref field_ident, ref field)| {
            let wrapper = field.debug(quote!(self.#field_ident));
            let call = if is_struct {
                quote!(builder.field(stringify!(#field_ident), &wrapper))
            } else {
                quote!(builder.field(&wrapper))
            };
            quote! {
                                         let builder = {
                                             let wrapper = #wrapper;
                                             #call
                                         };
                                    }
        });
    let debug_builder = if is_struct {
        quote!(f.debug_struct(stringify!(#ident)))
    } else {
        quote!(f.debug_tuple(stringify!(#ident)))
    };

    let expanded = quote! {
        #[allow(non_snake_case, unused_attributes)]
        mod #module {
            extern crate prost as _prost;
            extern crate bytes as _bytes;

            use super::*;

            impl _prost::Message for #ident {
                #[allow(unused_variables)]
                fn encode_raw<B>(&self, buf: &mut B) where B: _bytes::BufMut {
                    if #is_registered {
                        // TODO: in go-amino this only get length-prefixed if MarhsalBinary is used
                        // opposed to MarshalBinaryBare
                        let len = 4 #(+ #encoded_len2)*;
                        _prost::encoding::encode_varint(len as u64, buf);
                    } else {
                        // not length prefixed!
                    }
                    #comp_prefix
                    #(#encode)*
                }

                #[allow(unused_variables)]
                fn merge_field<B>(&mut self, buf: &mut B) -> ::std::result::Result<(), _prost::DecodeError>
                where B: _bytes::Buf {
                    #struct_name
                    if #is_registered {
                        // skip some bytes: varint(total_len) || prefix_bytes
                        // prefix (4) + total_encoded_len:
                        let _full_len = _prost::encoding::decode_varint(buf)?;
                        buf.advance(4);
                    }
                    if buf.remaining() > 0 {
                        let (tag, wire_type) = _prost::encoding::decode_key(buf)?;
                        match tag {
                            #(#merge)*
                            _ => _prost::encoding::skip_field(wire_type, buf),
                        }
                    } else {
                        Ok(())
                    }

                }

                #[inline]
                fn encoded_len(&self) -> usize {
                    let len = 0 #(+ #encoded_len)*;
                    if #is_registered {
                        4 + len
                    } else {
                        len
                    }
                }

                fn clear(&mut self) {
                    #(#clear;)*
                }
            }

            impl Default for #ident {
                fn default() -> #ident {
                    #ident {
                        #(#default)*
                    }
                }
            }

            impl ::std::fmt::Debug for #ident {
                fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
                    let mut builder = #debug_builder;
                    #(#debugs;)*
                    builder.finish()
                }
            }

            #methods
        };
    };
    Ok(expanded.into())
}

#[proc_macro_derive(Message, attributes(prost, amino_name, aminoDisamb))]
pub fn message(input: TokenStream) -> TokenStream {
    try_message(input).unwrap()
}


fn try_enumeration(input: TokenStream) -> Result<TokenStream, Error> {
    let input: DeriveInput = syn::parse(input)?;
    let ident = input.ident;

    if !input.generics.params.is_empty() ||
        input.generics.where_clause.is_some() {
        bail!("Message may not be derived for generic type");
    }

    let punctuated_variants = match input.data {
        Data::Enum(DataEnum { variants, .. }) => variants,
        Data::Struct(_) => bail!("Enumeration can not be derived for a struct"),
        Data::Union(..) => bail!("Enumeration can not be derived for a union"),
    };

    // Map the variants into 'fields'.
    let mut variants: Vec<(Ident, Expr)> = Vec::new();
    for Variant { ident, fields, discriminant, .. } in punctuated_variants {
        match fields {
            Fields::Unit => (),
            Fields::Named(_) | Fields::Unnamed(_) => bail!("Enumeration variants may not have fields"),
        }

        match discriminant {
            Some((_, expr)) => variants.push((ident, expr)),
            None => bail!("Enumeration variants must have a disriminant"),
        }
    }

    if variants.is_empty() {
        panic!("Enumeration must have at least one variant");
    }

    let default = variants[0].0.clone();

    // Put impls in a special module, so that 'extern crate' can be used.
    let module = Ident::new(&format!("{}_ENUMERATION", ident), Span::call_site());
    let is_valid = variants.iter().map(|&(_, ref value)| quote!(#value => true));
    let from = variants.iter().map(|&(ref variant, ref value)| quote!(#value => ::std::option::Option::Some(#ident::#variant)));

    let is_valid_doc = format!("Returns `true` if `value` is a variant of `{}`.", ident);
    let from_i32_doc = format!("Converts an `i32` to a `{}`, or `None` if `value` is not a valid variant.", ident);

    let expanded = quote! {
        #[allow(non_snake_case, unused_attributes)]
        mod #module {
            use super::*;

            impl #ident {

                #[doc=#is_valid_doc]
                pub fn is_valid(value: i32) -> bool {
                    match value {
                        #(#is_valid,)*
                        _ => false,
                    }
                }

                #[doc=#from_i32_doc]
                pub fn from_i32(value: i32) -> ::std::option::Option<#ident> {
                    match value {
                        #(#from,)*
                        _ => ::std::option::Option::None,
                    }
                }
            }

            impl ::std::default::Default for #ident {
                fn default() -> #ident {
                    #ident::#default
                }
            }

            impl ::std::convert::From<#ident> for i32 {
                fn from(value: #ident) -> i32 {
                    value as i32
                }
            }
        };
    };

    Ok(expanded.into())
}

#[proc_macro_derive(Enumeration, attributes(prost))]
pub fn enumeration(input: TokenStream) -> TokenStream {
    try_enumeration(input).unwrap()
}

fn try_oneof(input: TokenStream) -> Result<TokenStream, Error> {
    let input: DeriveInput = syn::parse(input)?;

    let ident = input.ident;

    let variants = match input.data {
        Data::Enum(DataEnum { variants, .. }) => variants,
        Data::Struct(..) => bail!("Oneof can not be derived for a struct"),
        Data::Union(..) => bail!("Oneof can not be derived for a union"),
    };

    if !input.generics.params.is_empty() ||
        input.generics.where_clause.is_some() {
        bail!("Message may not be derived for generic type");
    }

    // Map the variants into 'fields'.
    let mut fields: Vec<(Ident, Field)> = Vec::new();
    for Variant { attrs, ident: variant_ident, fields: variant_fields, .. } in variants {
        let variant_fields = match variant_fields {
            Fields::Unit => Punctuated::new(),
            Fields::Named(FieldsNamed { named: fields, .. }) |
            Fields::Unnamed(FieldsUnnamed { unnamed: fields, .. }) => fields,
        };
        if variant_fields.len() != 1 {
            bail!("Oneof enum variants must have a single field");
        }
        match Field::new_oneof(attrs)? {
            Some(field) => fields.push((variant_ident, field)),
            None => bail!("invalid oneof variant: oneof variants may not be ignored"),
        }
    }

    let mut tags = fields.iter().flat_map(|&(ref variant_ident, ref field)| -> Result<u32, Error> {
        if field.tags().len() > 1 {
            bail!("invalid oneof variant {}::{}: oneof variants may only have a single tag",
                  ident, variant_ident);
        }
        Ok(field.tags()[0])
    }).collect::<Vec<_>>();
    tags.sort();
    tags.dedup();
    if tags.len() != fields.len() {
        panic!("invalid oneof {}: variants have duplicate tags", ident);
    }

    // Put impls in a special module, so that 'extern crate' can be used.
    let module = Ident::new(&format!("{}_ONEOF", ident), Span::call_site());

    let encode = fields.iter().map(|&(ref variant_ident, ref field)| {
        let encode = field.encode(quote!(*value));
        quote!(#ident::#variant_ident(ref value) => { #encode })
    });

    let merge = fields.iter().map(|&(ref variant_ident, ref field)| {
        let tag = field.tags()[0];
        let merge = field.merge(quote!(value));
        quote! {
            #tag => {
                let mut value = ::std::default::Default::default();
                #merge.map(|_| *field = ::std::option::Option::Some(#ident::#variant_ident(value)))
            }
        }
    });

    let encoded_len = fields.iter().map(|&(ref variant_ident, ref field)| {
        let encoded_len = field.encoded_len(quote!(*value));
        quote!(#ident::#variant_ident(ref value) => #encoded_len)
    });

    let debug = fields.iter().map(|&(ref variant_ident, ref field)| {
        let wrapper = field.debug(quote!(*value));
        quote!(#ident::#variant_ident(ref value) => {
            let wrapper = #wrapper;
            f.debug_tuple(stringify!(#variant_ident))
                .field(&wrapper)
                .finish()
        })
    });

    let expanded = quote! {
        #[allow(non_snake_case, unused_attributes)]
        mod #module {
            extern crate bytes as _bytes;
            extern crate prost as _prost;
            use super::*;

            impl #ident {
                pub fn encode<B>(&self, buf: &mut B) where B: _bytes::BufMut {
                    match *self {
                        #(#encode,)*
                    }
                }

                pub fn merge<B>(field: &mut ::std::option::Option<#ident>,
                                tag: u32,
                                wire_type: _prost::encoding::WireType,
                                buf: &mut B)
                                -> ::std::result::Result<(), _prost::DecodeError>
                where B: _bytes::Buf {
                    match tag {
                        #(#merge,)*
                        _ => unreachable!(concat!("invalid ", stringify!(#ident), " tag: {}"), tag),
                    }
                }

                #[inline]
                pub fn encoded_len(&self) -> usize {
                    match *self {
                        #(#encoded_len,)*
                    }
                }
            }

            impl ::std::fmt::Debug for #ident {
                fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
                    match *self {
                        #(#debug,)*
                    }
                }
            }
        };
    };

    Ok(expanded.into())
}

#[proc_macro_derive(Oneof, attributes(prost))]
pub fn oneof(input: TokenStream) -> TokenStream {
    try_oneof(input).unwrap()
}

fn compute_disfix(identity: &str) -> (Vec<u8>, Vec<u8>) {
    let mut sh = Sha256::default();
    sh.input(identity.as_bytes());
    let output = sh.result();

    let disamb_bytes = output
        .iter()
        .filter(|&x| *x != 0x00)
        .cloned()
        .take(3)
        .collect();

    let prefix_bytes: Vec<u8> = output
        .iter()
        .filter(|&x| *x != 0x00)
        .skip(3)
        .filter(|&x| *x != 0x00)
        .cloned()
        .take(4)
        .collect();
    return (disamb_bytes, prefix_bytes);
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fmt;

    use std::error;

    #[test]
    fn compare_to_go_amino() {
        // test vectors generated via:
        // type Test struct {}
        // cdc.RegisterConcrete(Test{}, "test", nil)
        // dis, pre := amino.NameToDisfix("test")
        let want_disfix = vec![0x9f, 0x86, 0xd0];
        let want_prefix = vec![0x81, 0x88, 0x4c, 0x7d];
        let (disam, prefix) = compute_disfix("test");
        assert_eq!(disam, want_disfix);
        assert_eq!(prefix, want_prefix);
        {
            let want_disfix = vec![0x85, 0x6a, 0x57];
            let want_prefix = vec![0xbf, 0x58, 0xca, 0xef];
            let (disam, prefix) = compute_disfix("tendermint/socketpv/SignHeartbeatMsg");
            assert_eq!(disam, want_disfix);
            assert_eq!(prefix, want_prefix);
        }
    }
}