#[macro_use]
extern crate bytes;
extern crate chrono;
extern crate sha2;

mod encoding;
mod error;

pub use error::{DecodeError, EncodeError};
pub use encoding::*;

pub trait Amino{
    fn serialize(self)->Vec<u8>;
    fn deserialize(self, &[u8]);
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

