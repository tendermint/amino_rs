use bytes::{
    Buf,
    BufMut,
    BigEndian
};
use std::cmp::min;  
use DecodeError;
use std::io::Cursor;

use sha2::{Sha256, Digest};

#[derive(PartialEq)]
pub enum Typ3Byte{
    // Typ3 types
	Typ3_Varint,
	Typ3_8Byte, 
	Typ3_ByteLength, 
	Typ3_Struct,
	Typ3_StructTerm, 
	Typ3_4Byte,
	Typ3_List,
	Typ3_Interface,
	// Typ4 bit
	Typ4_Pointer,
    Invalid
}

pub fn typ3_to_byte(typ3: Typ3Byte)->u8{
    match typ3{
    // Typ3 types
	Typ3Byte::Typ3_Varint => 0,
	Typ3Byte::Typ3_8Byte => 1, 
	Typ3Byte::Typ3_ByteLength => 2, 
	Typ3Byte::Typ3_Struct => 3,
	Typ3Byte::Typ3_StructTerm => 4, 
	Typ3Byte::Typ3_4Byte => 5, 
	Typ3Byte::Typ3_List => 6,
	Typ3Byte::Typ3_Interface => 7,
	// Typ4 bit
	Typ3Byte::Typ4_Pointer => 8,
    Typ3Byte::Invalid => panic!("Should not use an invalid Typ3")
    }
}

pub fn byte_to_type3(data: u8)->Typ3Byte{
    match data{
        0 => Typ3Byte::Typ3_Varint,
        1 => Typ3Byte::Typ3_8Byte,
        2 => Typ3Byte::Typ3_ByteLength,
        3 => Typ3Byte::Typ3_Struct,
        4 => Typ3Byte::Typ3_StructTerm,
        5 => Typ3Byte::Typ3_4Byte,
        6 => Typ3Byte::Typ3_List,
        7 => Typ3Byte::Typ3_Interface,
        8 => Typ3Byte::Typ4_Pointer,
        _ => Typ3Byte::Invalid
    }
} 

pub fn encode_field_number_typ3<B>(field_number: u32, typ:Typ3Byte, buf: &mut B) where B:BufMut{
	// Pack Typ3 and field number.
	let value = ((field_number as u8) << 3) | typ3_to_byte(typ);
    buf.put_u8(value);
}

pub fn decode_field_number_typ3<B>( buf: &mut B) ->Result<(u32,Typ3Byte),DecodeError> where B:Buf{
    let value = decode_uvarint(buf)?;
    let typ3 = byte_to_type3(value as u8 & 0x07);
    let field_number = value >>3;
    return Ok((field_number as u32, typ3))
}

pub fn compute_disfix(identity: &str)->(Vec<u8>, Vec<u8>) {
    let mut sh = Sha256::default();
    sh.input(identity.as_bytes());
    let output =  sh.result();
    let disamb_bytes = output.iter().skip_while(|&x| *x== 0x00).cloned().take(3).collect();
    let mut prefix_bytes:Vec<u8> = output.iter().skip_while (|&x| *x== 0x00).skip(3).skip_while(|&x| *x== 0x00).cloned().take(4).collect();
    prefix_bytes[3] &= 0xF8;
    return (disamb_bytes,prefix_bytes);
}

#[cfg(test)]
mod disfix_tests {
    use super::*;
    #[test]
    fn check_examples() {
        let want_disfix = vec![0x9f, 0x86, 0xd0];
        let want_prefix = vec![0x81, 0x88, 0x4c, 0x78];

        let (disfix , prefix) = compute_disfix("test"); 

        assert_eq!(want_disfix, disfix);
        assert_eq!(want_prefix, prefix);
    }
}

pub fn encode_varint<B>(value: i64, buf: &mut B) where B: BufMut {
    let mut ux = (value as u64) << 1;
    if value < 0 {
        ux = !ux;
    }
    encode_uvarint(ux, buf)
}

/// Encodes an integer value into LEB128 variable length format, and writes it to the buffer.
/// The buffer must have enough remaining space (maximum 10 bytes).
#[inline]
pub fn encode_uvarint<B>(mut value: u64, buf: &mut B) where B: BufMut {
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

pub fn decode_varint<B>(buf: &mut B) -> Result<i64, DecodeError> where B: Buf {
    let val = decode_uvarint(buf)?;
    let x = (val >> 1) as i64;
    if val & 1_u64 != 0{
        return Ok(!x)
    }
    Ok(x)
}

/// Decodes a LEB128-encoded variable length integer from the buffer.
pub fn decode_uvarint<B>(buf: &mut B) -> Result<u64, DecodeError> where B: Buf {
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
    decode_uvarint_slow(buf)
}

/// Decodes a LEB128-encoded variable length integer from the buffer, advancing the buffer as
/// necessary.
#[inline(never)]
fn decode_uvarint_slow<B>(buf: &mut B) -> Result<u64, DecodeError> where B: Buf {
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

pub fn encode_int8<B>(num:i8, buf:&mut B) where B:BufMut{
    encode_varint(num as i64, buf)
}

pub fn encode_int16<B>(num:i16, buf:&mut B) where B:BufMut{
    encode_varint(num as i64, buf)
}
pub fn encode_int32<B>(num:i32, buf:&mut B) where B:BufMut{
    buf.put_u32::<BigEndian>(num  as u32);
}
pub fn encode_int64<B>(num:i64, buf:&mut B) where B:BufMut{
    buf.put_u64::<BigEndian>(num as u64);
}


pub fn decode_int8<B>(buf: &mut B)-> Result<i8, DecodeError> where B: Buf {  
    Ok(decode_varint(buf)? as i8)
}

pub fn decode_int16<B>(buf: &mut B)-> Result<i16, DecodeError> where B: Buf {  
    Ok(decode_varint(buf)? as i16)
}

pub fn decode_int32<B>(buf: &mut B)-> Result<i32, DecodeError> where B: Buf {

    let x = B::get_u32::<BigEndian>(buf);
    Ok(x as i32)
}

pub fn decode_int64<B>(buf: &mut B)-> Result<i64, DecodeError> where B: Buf {

    let x = B::get_u64::<BigEndian>(buf);   
    Ok(x as i64)
}


pub mod amino_string {
    use super::*;

    pub fn encode<B>(value: &str,
                     buf: &mut B) where B: BufMut {
        encode_uvarint(value.len() as u64, buf);
        buf.put_slice(value.as_bytes());
    }

    pub fn decode<B>(buf: &mut B)->Result<String,DecodeError> where B: Buf {
             let len = decode_uvarint(buf)?;
             let mut dst = vec![];
             dst.resize(len as usize,0);
             buf.copy_to_slice(&mut dst);
             if dst.len() != len as usize{
                 Err(DecodeError::new(format!("invalid string length have {} want {}",len as usize, dst.len())))?
             }
             Ok(String::from_utf8(dst).map_err(|_| {
                DecodeError::new("invalid string value: data is not UTF-8 encoded")
            })?)
        }
}

pub mod amino_bytes {
    use super::*;
    pub fn encode<B>(value: &[u8], buf: &mut B) where B: BufMut {
        encode_uvarint(value.len() as u64, buf);
        buf.put_slice(value);
    }
    pub fn decode<B>(buf: &mut B)->Result<Vec<u8>,DecodeError> where B: Buf {
             let len = decode_uvarint(buf)?;
             let mut dst = vec!();
             dst.resize(len as usize,0);
             buf.copy_to_slice(&mut dst);
             if dst.len() != len as usize{
                 Err(DecodeError::new( format!("invalid byte length have {} want {}",len as usize, dst.len())))?
             }
             Ok(dst)
        }
}

pub mod amino_time {
    use super::*;
    use chrono::{DateTime,NaiveDateTime, Utc};
    pub fn encode<B>(value: DateTime<Utc>, buf: &mut B) where B: BufMut{
        let mut epoch = value.timestamp() as u64;
        let nanos = value.timestamp_subsec_nanos() as u64;
        encode_field_number_typ3(1,Typ3Byte::Typ3_8Byte, buf);
        encode_uvarint(epoch, buf);
        encode_field_number_typ3(2, Typ3Byte::Typ3_4Byte, buf);
        encode_uvarint(nanos, buf);
        buf.put_u8(0x04)

    }
    pub fn decode<B>(buf: &mut B)-> Result<DateTime<Utc>,DecodeError> where B:Buf{

     {
        let (field_number, typ3) = decode_field_number_typ3(buf)?;
        if field_number != 1{
            return Err(DecodeError::new("Field number in time struct is not 1"))
        }
        if typ3 != Typ3Byte::Typ3_8Byte{
            return Err(DecodeError::new("Invalid Typ3 bytes"))
        }
    }
        let epoch = decode_uvarint(buf)? as i64;

     {
        let (field_number, typ3) = decode_field_number_typ3(buf)?;
        if field_number != 2{
            return Err(DecodeError::new("Field number in time struct is not 2"))
        }
        if typ3 != Typ3Byte::Typ3_4Byte{
            return Err(DecodeError::new("Invalid Typ3 bytes"))
        }
     }
        let nanos = decode_uvarint(buf)? as u32;

        Ok(DateTime::<Utc>::from_utc(NaiveDateTime::from_timestamp(epoch,nanos),Utc))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_encdec_neg_int32() {
        let want = -1;
        let mut buf = Vec::with_capacity(4);
        encode_int32(want, &mut buf);

        let mut buf = Cursor::new(buf);
        let got_res = decode_int32(&mut buf);
        match got_res {
            Ok(got) => assert_eq!(got, want),
            Err(e) => panic!("Couldn't decode int32"),
        }
    }

    #[test]
    fn check_encdec_neg_int64() {
        let want = -1 as i64;
        let mut buf = Vec::with_capacity(8);
        encode_int64(want, &mut buf);

        let mut buf = Cursor::new(buf);
        let got_res = decode_int64(&mut buf);
        match got_res {
            Ok(got) => assert_eq!(got, want),
            Err(e) => panic!("Couldn't decode int32"),
        }
    }
}