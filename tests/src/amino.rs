extern crate prost_amino as prost;
use prost::Message;

#[test]
fn amino() {
    #[derive(Clone, PartialEq, Message)]
    pub struct PartsSetHeader {
        #[prost(sint64, tag = "1")]
        total: i64,
        #[prost(bytes, tag = "2")]
        hash: Vec<u8>,
    }

    #[derive(Clone, PartialEq, Message)]
    pub struct BlockID {
        #[prost(bytes, tag = "1")]
        hash: Vec<u8>,
        #[prost(message, tag = "2")]
        parts_header: Option<PartsSetHeader>,
    }

    #[derive(Clone, PartialEq, Message)]
    struct Heartbeat {
        #[prost(bytes, tag = "1")]
        pub validator_address: Vec<u8>,
        #[prost(sint64)]
        validator_index: i64,
        #[prost(sint64)]
        height: i64,
        #[prost(sint64)]
        round: i64,
        #[prost(sint64)]
        sequence: i64,
        #[prost(message)]
        signature: Option<Vec<u8>>,
    }

    #[derive(Clone, PartialEq, Message)]
    #[amino_name = "tendermint/socketpv/SignHeartbeatMsg"]
    struct SignHeartbeatMsg {
        #[prost(message, tag = "1")]
        heartbeat: Option<Heartbeat>,
    }
    let addr = vec![
        0xa3, 0xb2, 0xcc, 0xdd, 0x71, 0x86, 0xf1, 0x68, 0x5f, 0x21, 0xf2, 0x48, 0x2a, 0xf4, 0xfb,
        0x34, 0x46, 0xa8, 0x4b, 0x35,
    ];

    let hb = Heartbeat {
        validator_address: addr,
        validator_index: 1,
        height: 15,
        round: 10,
        sequence: 30,
        signature: None,
    };

    let hb_msg = SignHeartbeatMsg {
        heartbeat: Some(hb),
    };

    let mut buf = vec![];
    let _enc = hb_msg.encode(&mut buf).unwrap();
    let want = vec![
        0x24, 0xbf, 0x58, 0xca, 0xef, 0xa, 0x1e, 0xa, 0x14, 0xa3, 0xb2, 0xcc, 0xdd, 0x71, 0x86,
        0xf1, 0x68, 0x5f, 0x21, 0xf2, 0x48, 0x2a, 0xf4, 0xfb, 0x34, 0x46, 0xa8, 0x4b, 0x35, 0x10,
        0x2, 0x18, 0x1e, 0x20, 0x14, 0x28, 0x3c,
    ];
    assert_eq!(want, buf);

    let hb2 = SignHeartbeatMsg::decode(want);
    assert_eq!(hb_msg, hb2.unwrap());
}
