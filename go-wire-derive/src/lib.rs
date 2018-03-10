extern crate itertools;
extern crate proc_macro2;
extern crate proc_macro;
extern crate syn;
extern crate sha2;

#[macro_use]
extern crate failure;
#[macro_use]
extern crate quote;

use sha2::{Sha256, Digest};
use failure::Error;
use itertools::Itertools;
use proc_macro::TokenStream;
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

fn compute_disfix(identity: &str)->(Vec<u8>, Vec<u8>) {
    let mut sh = Sha256::default();
    sh.input(identity.as_bytes());
    let output =  sh.result();
    let disamb_bytes = output.iter().filter(|&x| *x!= 0x00).cloned().take(3).collect();
    let mut prefix_bytes:Vec<u8> = output.iter().filter(|&x| *x!= 0x00).skip(3).filter(|&x| *x!= 0x00).cloned().take(4).collect();
    prefix_bytes[3] &= 0xF8;
    return (disamb_bytes,prefix_bytes);
}


fn try_message(input: TokenStream) -> Result<TokenStream, Error> {
    let input: DeriveInput = syn::parse(input)?;
    let (dis_bytes,pre_bytes) = compute_disfix(input.ident.as_ref());
    println!("Disfix Bytes {:?}",String::from_utf8((dis_bytes)));
    println!("Prefix Bytes {:?}",String::from_utf8((pre_bytes)));

    unimplemented!();
}

#[proc_macro_derive(Wire, attributes(prost))]
pub fn message(input: TokenStream) -> TokenStream {
    try_message(input).unwrap()
}

