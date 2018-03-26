use bytes::{
    Buf,
    BufMut,
    LittleEndian,
};
use std::cmp::min;  
use DecodeError;

/// Encodes an integer value into LEB128 variable length format, and writes it to the buffer.
/// The buffer must have enough remaining space (maximum 10 bytes).
#[inline]
pub fn encode_varint<B>(mut value: u64, buf: &mut B) where B: BufMut {
    // Safety notes:
    //
    // - bytes_mut is unsafe because it may return an uninitialized slice.
    //   The use here is safe because the slice is only written to, never read from.
    //
    // - advance_mut is unsafe because it could cause uninitialized memory to be
    //   advanced over. The use here is safe since each byte which is advanced over
    //   has been written to in the previous loop iteration.
    let mut i;
    'outer: loop {
        i = 0;

        for byte in unsafe { buf.bytes_mut() } {
            i += 1;
            if value < 0x80 {
                *byte = value as u8;
                break 'outer;
            } else {
                *byte = ((value & 0x7F) | 0x80) as u8;
                value >>= 7;
            }
        }

        unsafe { buf.advance_mut(i); }
        debug_assert!(buf.has_remaining_mut());
    }

    unsafe { buf.advance_mut(i); }
}

/// Decodes a LEB128-encoded variable length integer from the buffer.
pub fn decode_varint<B>(buf: &mut B) -> Result<u64, DecodeError> where B: Buf {
    // NLL hack.
    'slow: loop {
        // Another NLL hack.
        let (value, advance) = {
            let bytes = buf.bytes();
            let len = bytes.len();
            if len == 0 {
                return Err(DecodeError::new("invalid varint"));
            }

            let byte = bytes[0];
            if byte < 0x80 {
                (u64::from(byte), 1)
            } else {
                break 'slow;
            }
        };

        buf.advance(advance);
        return Ok(value);
    }
    decode_varint_slow(buf)
}

/// Decodes a LEB128-encoded variable length integer from the buffer, advancing the buffer as
/// necessary.
#[inline(never)]
fn decode_varint_slow<B>(buf: &mut B) -> Result<u64, DecodeError> where B: Buf {
    let mut value = 0;
    for count in 0..min(10, buf.remaining()) {
        let byte = buf.get_u8();
        value |= u64::from(byte & 0x7F) << (count * 7);
        if byte <= 0x7F {
            return Ok(value);
        }
    }

    Err(DecodeError::new("invalid varint"))
}

pub mod string {
    use super::*;

    pub fn encode<B>(value: &String,
                     buf: &mut B) where B: BufMut {
        encode_varint(value.len() as u64, buf);
        buf.put_slice(value.as_bytes());
    }

    pub fn decode<B>(buf: &mut B)->Result<String,DecodeError> where B: Buf {
             let len = decode_varint(buf)?;
             let mut dst = vec![];
             dst.put(buf.take(len as usize).into_inner());
             if dst.len() != len as usize{
                  Err(DecodeError::new("invalid string length"))?
             }
             Ok(String::from_utf8(dst).map_err(|_| {
                DecodeError::new("invalid string value: data is not UTF-8 encoded")
            })?)
        }
}

pub mod bytes {
    use super::*;
    pub fn encode<B>(value: &Vec<u8>, buf: &mut B) where B: BufMut {
        encode_varint(value.len() as u64, buf);
        buf.put_slice(value);
    }
    pub fn decode<B>(buf: &mut B)->Result<Vec<u8>,DecodeError> where B: Buf {
             let len = decode_varint(buf)?;
             let mut dst = vec![];
             dst.put(buf.take(len as usize).into_inner());
             if dst.len() != len as usize{
                 Err(DecodeError::new("invalid byte length"))?
             }
             Ok(dst)
        }
}