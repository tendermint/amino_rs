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

fn try_message(input: TokenStream) -> Result<TokenStream, Error> {
    let input: DeriveInput = syn::parse(input)?;
    let (dis_bytes, pre_bytes) = compute_disfix(input.ident.as_ref());

    if !input.generics.params.is_empty() || input.generics.where_clause.is_some() {
        bail!("Message may not be derived for generic type");
    }

    let variant_data = match input.data {
        Data::Struct(variant_data) => variant_data,
        Data::Enum(..) => bail!("Message can not be derived for an enum"),
        Data::Union(..) => bail!("Message can not be derived for a union"),
    };

    let fields = match variant_data {
        DataStruct {
            fields: Fields::Named(FieldsNamed { named: fields, .. }),
            ..
        }
        | DataStruct {
            fields:
                Fields::Unnamed(FieldsUnnamed {
                    unnamed: fields, ..
                }),
            ..
        } => fields.into_iter().collect(),
        DataStruct {
            fields: Fields::Unit,
            ..
        } => Vec::new(),
    };
    print!("{} \n", input.ident);
    print!("{:?} \n", fields);

    let module = Ident::from(format!("{}_WIRE", input.ident));

    let expanded = quote! {
        #[allow(non_snake_case, unused_attributes)]
        mod #module {

        };
    };
    Ok(expanded.into())
}

#[proc_macro_derive(Wire)]
pub fn message(input: TokenStream) -> TokenStream {
    try_message(input).unwrap()
}
