#[macro_use]
extern crate amino_derive;
extern crate bytes;

mod encoding;
mod error;

pub use error::{DecodeError, EncodeError};

#[cfg(test)]
mod tests {
    #[derive(Wire)]
    struct Foo(i32);

    #[derive(Wire)]
    struct Bar{
        baz: i32
    }
    
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}

