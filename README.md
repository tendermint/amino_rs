# Rust Amino

This is a work in progress implementation of the Amino serialization for Tendermint/Cosmos in the Rust Language. 
For details on amino, see: https://github.com/tendermint/go-amino.

It is based on the Protocol Buffers implementation [prost!](https://github.com/danburkert/prost) by [@danburkert](https://github.com/danburkert).

Like prost! for protobuf, it uses Rust's type-directed metaprogramming to add support for amino's 
[registered types](https://github.com/tendermint/go-amino/#registering-types). 

[Registered types](https://github.com/tendermint/go-amino/#registering-types) can be annotated via 
`#[aminoName="registered/name/goes/here"]` to derive encoding and decoding. 
As amino allows to register type aliases of primitive types (e.g. 
[bytes](https://github.com/tendermint/tendermint/blob/013b9cef642f875634c614019ab13b17570778ad/crypto/ed25519/ed25519.go#L40-L41) via 
[ed25519.Pubkey](https://github.com/tendermint/tendermint/blob/013b9cef642f875634c614019ab13b17570778ad/crypto/encoding/amino/amino.go#L26-L27)), you can also annotate fields. 

You can find a complete example which uses both, a registered type (or message) and a registered scalar type 
(`bytes` or `Vec<u8>`) in the [kms repository](https://github.com/tendermint/kms/blob/9344e3411676ff4e27e139b2033697fe48a7e87a/src/types/ed25519msg.rs#L8-L13).
