use failure::Error;
use proc_macro2::TokenStream;
use quote::ToTokens;
use syn::Meta;

use field::{amino_name_attr, set_bool, set_option, tag_attr, word_attr, Label};

use super::compute_disfix;

#[derive(Clone, Debug)]
pub struct Field {
    pub label: Label,
    pub tag: u32,
    // this is to be able to de/encode registered type aliases:
    pub amino_prefix: Vec<u8>,
}

impl Field {
    pub fn new(attrs: &[Meta], inferred_tag: Option<u32>) -> Result<Option<Field>, Error> {
        let mut message = false;
        let mut label = None;
        let mut tag = None;
        let mut boxed = false;
        let mut amino_name = None;

        let mut unknown_attrs = Vec::new();

        for attr in attrs {
            if word_attr("message", attr) {
                set_bool(&mut message, "duplicate message attribute")?;
            } else if word_attr("boxed", attr) {
                set_bool(&mut boxed, "duplicate boxed attribute")?;
            } else if let Some(t) = tag_attr(attr)? {
                set_option(&mut tag, t, "duplicate tag attributes")?;
            } else if let Some(l) = Label::from_attr(attr) {
                set_option(&mut label, l, "duplicate label attributes")?;
            } else if let Some(n) = amino_name_attr(attr)? {
                set_option(&mut amino_name, n, "duplicate amino_name attributes")?;
            } else {
                unknown_attrs.push(attr);
            }
        }

        if !message {
            return Ok(None);
        }

        match unknown_attrs.len() {
            0 => (),
            1 => bail!(
                "unknown attribute for message field: {:?}",
                unknown_attrs[0]
            ),
            _ => bail!("unknown attributes for message field: {:?}", unknown_attrs),
        }

        let tag = match tag.or(inferred_tag) {
            Some(tag) => tag,
            None => bail!("message field is missing a tag attribute"),
        };

        let amino_prefix: Vec<u8> = match amino_name {
            Some(n) => {
                let (_dis, pre) = compute_disfix(n.as_str());
                pre
            }
            None => vec![],
        };

        Ok(Some(Field {
            label: label.unwrap_or(Label::Optional),
            tag: tag,
            amino_prefix: amino_prefix,
        }))
    }

    pub fn new_oneof(attrs: &[Meta]) -> Result<Option<Field>, Error> {
        if let Some(mut field) = Field::new(attrs, None)? {
            if let Some(attr) = attrs.iter().find(|attr| Label::from_attr(attr).is_some()) {
                bail!(
                    "invalid attribute for oneof field: {}",
                    attr.path().into_token_stream()
                );
            }
            field.label = Label::Required;
            Ok(Some(field))
        } else {
            Ok(None)
        }
    }

    pub fn encode(&self, ident: TokenStream) -> TokenStream {
        let tag = self.tag;
        let amino_prefix = &self.amino_prefix;
        match self.label {
            Label::Optional => quote! {
                if let Some(ref msg) = #ident {
                    ::prost::encoding::message::encode(#tag, msg, buf);
                }
            },
            Label::Required => quote! {
                let pre = vec![#(#amino_prefix),*];
                buf.put(pre.as_ref());
                ::prost::encoding::message::encode(#tag, &#ident, buf);
            },
            Label::Repeated => quote! {
                for msg in &#ident {
                    let pre = vec![#(#amino_prefix),*];
                    buf.put(pre.as_ref());
                    ::prost::encoding::message::encode(#tag, msg, buf);
                }
            },
        }
    }

    pub fn merge(&self, ident: TokenStream) -> TokenStream {
        match self.label {
            Label::Optional => quote! {
                _prost::encoding::message::merge(wire_type,
                                                 #ident.get_or_insert_with(Default::default),
                                                 buf)
            },
            Label::Required => quote! {
                _prost::encoding::message::merge(wire_type, &mut #ident, buf)
            },
            Label::Repeated => quote! {
                _prost::encoding::message::merge_repeated(wire_type, &mut #ident, buf)
            },
        }
    }

    pub fn encoded_len(&self, ident: TokenStream) -> TokenStream {
        let tag = self.tag;
        let pl: usize = self.amino_prefix.len();
        match self.label {
            Label::Optional => quote! {
                #ident.as_ref().map_or(0, |msg| _prost::encoding::message::encoded_len(#tag, msg) + #pl)
            },
            Label::Required => quote! {
                _prost::encoding::message::encoded_len(#tag, &#ident)
            },
            Label::Repeated => quote! {
                _prost::encoding::message::encoded_len_repeated(#tag, &#ident)
            },
        }
    }

    pub fn clear(&self, ident: TokenStream) -> TokenStream {
        match self.label {
            Label::Optional => quote!(#ident = ::std::option::Option::None),
            Label::Required => quote!(#ident.clear()),
            Label::Repeated => quote!(#ident.clear()),
        }
    }
}
