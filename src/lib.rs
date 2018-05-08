extern crate bytes;
extern crate chrono;
extern crate sha2;

mod encoding;
mod error;

pub use error::{DecodeError, EncodeError};
pub use encoding::*;

pub trait Amino:Sized{
    fn serialize(self)->Vec<u8>;
    fn deserialize(&[u8])->Result<Self, DecodeError>;
}

// #[cfg(test)]
// mod tests {
//     #[derive(Wire)]
//     struct Foo(i32);

//     #[derive(Wire)]
//     struct Bar{
//         baz: i32
//     }
    
//     #[test]
//     fn it_works() {
//         assert_eq!(2 + 2, 4);
//     }
// }

