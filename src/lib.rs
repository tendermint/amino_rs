#[macro_use]
extern crate go_wire_derive;



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

