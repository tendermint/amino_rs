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

mod field;
use field::Field;

use itertools::Itertools;
use proc_macro::TokenStream;
use proc_macro2::Span;
use sha2::{Digest, Sha256};
use syn::punctuated::Punctuated;
use syn::{
    Data, DataEnum, DataStruct, DeriveInput, Expr, Fields, FieldsNamed, FieldsUnnamed, Ident,
    Variant,
};

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
    let mut prefix_bytes: Vec<u8> = output
        .iter()
        .filter(|&x| *x != 0x00)
        .skip(3)
        .filter(|&x| *x != 0x00)
        .cloned()
        .take(4)
        .collect();
    prefix_bytes[3] &= 0xF8;
    return (disamb_bytes, prefix_bytes);
}

fn try_aminify(input: TokenStream) -> Result<TokenStream, Error> {
    let input: DeriveInput = syn::parse(input)?;

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
        DataStruct { fields: Fields::Unnamed(FieldsUnnamed { unnamed: fields, .. }), ..} => {
            fields.into_iter().collect()
        },
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
                // TODO add prefix / disfix bytes here
                #[allow(unused_variables)]
                fn encode_raw<B>(&self, buf: &mut B) where B: _bytes::BufMut {
                    buf.put("TODO: prefix bytes instead");
                    #(#encode)*
                }

                #[allow(unused_variables)]
                fn merge_field<B>(&mut self, buf: &mut B) -> ::std::result::Result<(), _prost::DecodeError>
                where B: _bytes::Buf {
                    #struct_name
                    let (tag, wire_type) = _prost::encoding::decode_key(buf)?;
                    match tag {
                        #(#merge)*
                        _ => _prost::encoding::skip_field(wire_type, buf),
                    }
                }

                #[inline]
                fn encoded_len(&self) -> usize {
                    0 #(+ #encoded_len)*
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

#[proc_macro_derive(Amino, attributes(amino, AminoName))]
pub fn amininfy(input: TokenStream) -> TokenStream {
    try_aminify(input).unwrap()
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
        let want_prefix = vec![0x81, 0x88, 0x4c, 0x78];
        let (disam, prefix) = compute_disfix("test");
        assert_eq!(disam, want_disfix);
        assert_eq!(prefix, want_prefix);
    }
}