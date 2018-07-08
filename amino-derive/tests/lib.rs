#[macro_use]
extern crate amino_derive;

#[macro_use]
extern crate prost_derive;

extern crate prost;

use prost::Message;

use std::fmt;

#[derive(Clone, PartialEq, Amino)]
pub struct Signature{
    #[amino(bytes)]
    pub s: Vec<u8>,
}

#[derive(Clone, PartialEq, Amino)]
struct Heartbeat {
    #[amino(bytes, tag="1")]
    pub validator_address: Vec<u8>,
    #[amino(int64)]
    validator_index: i64,
    #[amino(int64)]
    height: i64,
    #[amino(int64)]
    round: i64,
    #[amino(int64)]
    sequence: i64,
    #[amino(message)]
    signature: Option<Signature>,
}


#[derive(Clone, PartialEq, Message)]
struct Heartbeat2 {
    #[prost(bytes, tag="1")]
    pub validator_address: Vec<u8>,
    #[prost(int64)]
    validator_index: i64,
    #[prost(int64)]
    height: i64,
    #[prost(int64)]
    round: i64,
    #[prost(int64)]
    sequence: i64,
}



#[test]
fn prost_test() {
    let addr = vec![
        0xa3, 0xb2, 0xcc, 0xdd, 0x71, 0x86, 0xf1, 0x68, 0x5f, 0x21, 0xf2, 0x48, 0x2a, 0xf4,
        0xfb, 0x34, 0x46, 0xa8, 0x4b, 0x35,
    ];

    let hb = Heartbeat {
        validator_address: addr,
        validator_index: 1,
        height: 15,
        round: 10,
        sequence: 30,
        signature: None,
    };
    let hbp = Heartbeat2 {
        validator_address: vec![],
        validator_index: 1,
        height: 15,
        round: 10,
        sequence: 30,
    };

    //println!("enc len ={}", hbp.encoded_len());

    println!("enc len ={}", hb.encoded_len());
    let mut buf = vec![];
    let enc = hb.encode(&mut buf).unwrap();
    println!("{:x?}", buf);
}
