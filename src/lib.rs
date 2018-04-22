#[macro_use]
extern crate bytes;
extern crate chrono;

mod encoding;
mod error;

pub use error::{DecodeError, EncodeError};
pub use encoding::*;
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

