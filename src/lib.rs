#![doc(html_root_url = "https://docs.rs/prost/0.4.0")]

pub extern crate bytes;
#[cfg(feature = "prost-derive")]
#[doc(hidden)]
#[doc(hidden)]
pub use bytes;

#[cfg(test)]
#[macro_use]
extern crate quickcheck;

pub mod error;
mod message;
mod types;

#[doc(hidden)]
pub mod encoding;

pub use error::{DecodeError, EncodeError};
pub use message::Message;

use bytes::{Buf, BufMut};

use encoding::{decode_varint, encode_varint, encoded_len_varint};

/// Encodes a length delimiter to the buffer.
///
/// See [Message.encode_length_delimited] for more info.
///
/// An error will be returned if the buffer does not have sufficient capacity to encode the
/// delimiter.
pub fn encode_length_delimiter<B>(length: usize, buf: &mut B) -> Result<(), EncodeError>
where
    B: BufMut,
{
    let length = length as u64;
    let required = encoded_len_varint(length);
    let remaining = buf.remaining_mut();
    if required > remaining {
        return Err(EncodeError::new(required, remaining));
    }
    encode_varint(length, buf);
    Ok(())
}

/// Returns the encoded length of a length delimiter.
///
/// Applications may use this method to ensure sufficient buffer capacity before calling
/// `encode_length_delimiter`. The returned size will be between 1 and 10, inclusive.
pub fn length_delimiter_len(length: usize) -> usize {
    encoded_len_varint(length as u64)
}

/// Decodes a length delimiter from the buffer.
///
/// This method allows the length delimiter to be decoded independently of the message, when the
/// message is encoded with [Message.encode_length_delimited].
///
/// An error may be returned in two cases:
///
///  * If the supplied buffer contains fewer than 10 bytes, then an error indicates that more
///    input is required to decode the full delimiter.
///  * If the supplied buffer contains more than 10 bytes, then the buffer contains an invalid
///    delimiter, and typically the buffer should be considered corrupt.
pub fn decode_length_delimiter<B>(mut buf: B) -> Result<usize, DecodeError>
where
    B: Buf,
{
    let length = decode_varint(&mut buf)?;
    if length > usize::max_value() as u64 {
        return Err(DecodeError::new(
            "length delimiter exceeds maximum usize value",
        ));
    }
    Ok(length as usize)
}
