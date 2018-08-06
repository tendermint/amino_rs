# Rust Amino

This is a work in progress implementation of the Amino serialization for Tendermint/Cosmos in the Rust Language. 
For details on amino, see: https://github.com/tendermint/go-amino\.

It is based on the Protocol Buffers implementation [prost!](https://github.com/danburkert/prost) by @danburkert.

Like prost! for protobuf, it uses Rust's type-directed metaprogramming to add rudimentary support for amino's 
[registered types](https://github.com/tendermint/go-amino/#registering-types). 

[Registered types](https://github.com/tendermint/go-amino/#registering-types) can be annotated via 
`#[aminoName="registered/name/goes/here"]` to derive encoding and decoding. 
As amino allows to register type aliases of primitive types (e.g. 
[bytes](https://github.com/tendermint/tendermint/blob/013b9cef642f875634c614019ab13b17570778ad/crypto/ed25519/ed25519.go#L40-L41) via 
[ed25519.Pubkey](https://github.com/tendermint/tendermint/blob/013b9cef642f875634c614019ab13b17570778ad/crypto/encoding/amino/amino.go#L26-L27)), you can also annotate fields. 

Below you can find a complete example which uses both, a registered type (or message) and a registered scalar type 
(`bytes` or `Vec<u8>`):
```rust
#[amino_name = "tendermint/socketpv/PubKeyMsg"]
pub struct PubKeyMsg {
    #[prost(bytes, tag = "1", amino_name = "tendermint/PubKeyEd25519")]
    pub_key_ed25519: Vec<u8>,
}
```

