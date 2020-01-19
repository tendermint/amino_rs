#![doc(html_root_url = "https://docs.rs/prost-build/0.4.0")]

//! `prost-build` compiles `.proto` files into Rust.
//!
//! `prost-build` is designed to be used for build-time code generation as part of a Cargo
//! build-script.
//!
//! ## Example
//!
//! Let's create a small crate, `snazzy`, that defines a collection of
//! snazzy new items in a protobuf file.
//!
//! ```bash
//! $ cargo new snazzy && cd snazzy
//! ```
//!
//! First, add `prost-build`, `prost` and its public dependencies to `Cargo.toml`
//! (see [crates.io](https://crates.io/crates/prost) for the current versions):
//!
//! ```toml
//! [dependencies]
//! bytes = <bytes-version>
//! prost = <prost-version>
//! prost-amino-derive = <prost-version>
//!
//! [build-dependencies]
//! prost-build = <prost-version>
//! ```
//!
//! Next, add `src/items.proto` to the project:
//!
//! ```proto
//! syntax = "proto3";
//!
//! package snazzy.items;
//!
//! // A snazzy new shirt!
//! message Shirt {
//! enum Size {
//!     SMALL = 0;
//!     MEDIUM = 1;
//!     LARGE = 2;
//! }
//!
//! string color = 1;
//! Size size = 2;
//! }
//! ```
//!
//! To generate Rust code from `items.proto`, we use `prost-build` in the crate's
//! `build.rs` build-script:
//!
//! ```rust,no_run
//! extern crate prost_build;
//!
//! fn main() {
//!     prost_build::compile_protos(&["src/items.proto"],
//!                                 &["src/"]).unwrap();
//! }
//! ```
//!
//! And finally, in `lib.rs`, include the generated code:
//!
//! ```rust,ignore
//! extern crate prost_amino;
//! #[macro_use]
//! extern crate prost_derive;
//!
//! // Include the `items` module, which is generated from items.proto.
//! pub mod items {
//!     include!(concat!(env!("OUT_DIR"), "/snazzy.items.rs"));
//! }
//!
//! pub fn create_large_shirt(color: String) -> items::Shirt {
//!     let mut shirt = items::Shirt::default();
//!     shirt.color = color;
//!     shirt.set_size(items::shirt::Size::Large);
//!     shirt
//! }
//! ```
//!
//! That's it! Run `cargo doc` to see documentation for the generated code. The full
//! example project can be found on [GitHub](https://github.com/danburkert/snazzy).
//!
//! ## Sourcing `protoc`
//!
//! `prost-build` depends on the Protocol Buffers compiler, `protoc`, to parse `.proto` files into
//! a representation that can be transformed into Rust. If set, `prost-build` uses the `PROTOC` and
//! `PROTOC_INCLUDE` environment variables for locating `protoc` and the Protobuf includes
//! directory. For example, on a macOS system where Protobuf is installed with Homebrew, set the
//! environment to:
//!
//! ```bash
//! PROTOC=/usr/local/bin/protoc
//! PROTOC_INCLUDE=/usr/local/include
//! ```
//!
//! and in a typical Linux installation:
//!
//! ```bash
//! PROTOC=/usr/bin/protoc
//! PROTOC_INCLUDE=/usr/include
//! ```
//!
//! If `PROTOC` is not found in the environment, then a pre-compiled `protoc` binary bundled in
//! the prost-build crate is used. Pre-compiled `protoc` binaries exist for Linux, macOS, and
//! Windows systems. If no pre-compiled `protoc` is available for the host platform, then the
//! `protoc` or `protoc.exe` binary on the `PATH` is used. If `protoc` is not available in any of
//! these fallback locations, then the build fails.
//!
//! If `PROTOC_INCLUDE` is not found in the environment, then the Protobuf include directory bundled
//! in the prost-build crate is be used.

extern crate heck;
extern crate itertools;
extern crate multimap;
extern crate petgraph;
extern crate prost_amino as prost;
extern crate prost_types;
extern crate tempdir;

#[macro_use]
extern crate log;

mod ast;
mod code_generator;
mod ident;
mod message_graph;

use std::collections::HashMap;
use std::default;
use std::env;
use std::fs;
use std::io::{Error, ErrorKind, Read, Result, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use prost::Message;
use prost_types::{FileDescriptorProto, FileDescriptorSet};

pub use ast::{Comments, Method, Service};
use code_generator::{module, CodeGenerator};
use message_graph::MessageGraph;

type Module = Vec<String>;

/// A service generator takes a service descriptor and generates Rust code.
///
/// `ServiceGenerator` can be used to generate application-specific interfaces
/// or implementations for Protobuf service definitions.
///
/// Service generators are registered with a code generator using the
/// `Config::service_generator` method.
///
/// A viable scenario is that an RPC framework provides a service generator. It generates a trait
/// describing methods of the service and some glue code to call the methods of the trait, defining
/// details like how errors are handled or if it is asynchronous. Then the user provides an
/// implementation of the generated trait in the application code and plugs it into the framework.
///
/// Such framework isn't part of Prost at present.
pub trait ServiceGenerator {
    /// Generates a Rust interface or implementation for a service, writing the
    /// result to `buf`.
    fn generate(&mut self, service: Service, buf: &mut String);

    /// Finalizes the generation process.
    ///
    /// In case there's something that needs to be output at the end of the generation process, it
    /// goes here. Similar to [`generate`](#method.generate), the output should be appended to
    /// `buf`.
    ///
    /// An example can be a module or other thing that needs to appear just once, not for each
    /// service generated.
    ///
    /// This still can be called multiple times in a lifetime of the service generator, because it
    /// is called once per `.proto` file.
    ///
    /// The default implementation is empty and does nothing.
    fn finalize(&mut self, _buf: &mut String) {}
}

/// Configuration options for Protobuf code generation.
///
/// This configuration builder can be used to set non-default code generation options.
pub struct Config {
    service_generator: Option<Box<ServiceGenerator>>,
    btree_map: Vec<String>,
    type_attributes: Vec<(String, String)>,
    field_attributes: Vec<(String, String)>,
    prost_types: bool,
    strip_enum_prefix: bool,
}

impl Config {
    /// Creates a new code generator configuration with default options.
    pub fn new() -> Config {
        Config::default()
    }

    /// Configure the code generator to generate Rust [`BTreeMap`][1] fields for Protobuf
    /// [`map`][2] type fields.
    ///
    /// # Arguments
    ///
    /// **`paths`** - paths to specific fields, messages, or packages which should use a Rust
    /// `BTreeMap` for Protobuf `map` fields. Paths are specified in terms of the Protobuf type
    /// name (not the generated Rust type name). Paths with a leading `.` are treated as fully
    /// qualified names. Paths without a leading `.` are treated as relative, and are suffix
    /// matched on the fully qualified field name. If a Protobuf map field matches any of the
    /// paths, a Rust `BTreeMap` field is generated instead of the default [`HashMap`][3].
    ///
    /// The matching is done on the Protobuf names, before converting to Rust-friendly casing
    /// standards.
    ///
    /// # Examples
    ///
    /// ```
    /// # let mut config = prost_build::Config::new();
    /// // Match a specific field in a message type.
    /// config.btree_map(&[".my_messages.MyMessageType.my_map_field"]);
    ///
    /// // Match all map fields in a message type.
    /// config.btree_map(&[".my_messages.MyMessageType"]);
    ///
    /// // Match all map fields in a package.
    /// config.btree_map(&[".my_messages"]);
    ///
    /// // Match all map fields.
    /// config.btree_map(&["."]);
    ///
    /// // Match all map fields in a nested message.
    /// config.btree_map(&[".my_messages.MyMessageType.MyNestedMessageType"]);
    ///
    /// // Match all fields named 'my_map_field'.
    /// config.btree_map(&["my_map_field"]);
    ///
    /// // Match all fields named 'my_map_field' in messages named 'MyMessageType', regardless of
    /// // package or nesting.
    /// config.btree_map(&["MyMessageType.my_map_field"]);
    ///
    /// // Match all fields named 'my_map_field', and all fields in the 'foo.bar' package.
    /// config.btree_map(&["my_map_field", ".foo.bar"]);
    /// ```
    ///
    /// [1]: https://doc.rust-lang.org/std/collections/struct.BTreeMap.html
    /// [2]: https://developers.google.com/protocol-buffers/docs/proto3#maps
    /// [3]: https://doc.rust-lang.org/std/collections/struct.HashMap.html
    pub fn btree_map<I, S>(&mut self, paths: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.btree_map = paths.into_iter().map(|s| s.as_ref().to_string()).collect();
        self
    }

    /// Add additional attribute to matched fields.
    ///
    /// # Arguments
    ///
    /// **`path`** - a patch matching any number of fields. These fields get the attribute.
    /// For details about matching fields see [`btree_map`](#method.btree_map).
    ///
    /// **`attribute`** - an arbitrary string that'll be placed before each matched field. The
    /// expected usage are additional attributes, usually in concert with whole-type
    /// attributes set with [`type_attribute`](method.type_attribute), but it is not
    /// checked and anything can be put there.
    ///
    /// Note that the calls to this method are cumulative ‒ if multiple paths from multiple calls
    /// match the same field, the field gets all the corresponding attributes.
    ///
    /// # Examples
    ///
    /// ```
    /// # let mut config = prost_build::Config::new();
    /// // Prost renames fields named `in` to `in_`. But if serialized through serde,
    /// // they should as `in`.
    /// config.field_attribute("in", "#[serde(rename = \"in\")]");
    /// ```
    pub fn field_attribute<P, A>(&mut self, path: P, attribute: A) -> &mut Self
    where
        P: AsRef<str>,
        A: AsRef<str>,
    {
        self.field_attributes
            .push((path.as_ref().to_string(), attribute.as_ref().to_string()));
        self
    }

    /// Add additional attribute to matched messages, enums and one-ofs.
    ///
    /// # Arguments
    ///
    /// **`paths`** - a path matching any number of types. It works the same way as in
    /// [`btree_map`](#method.btree_map), just with the field name omitted.
    ///
    /// **`attribute`** - an arbitrary string to be placed before each matched type. The
    /// expected usage are additional attributes, but anything is allowed.
    ///
    /// The calls to this method are cumulative. They don't overwrite previous calls and if a
    /// type is matched by multiple calls of the method, all relevant attributes are added to
    /// it.
    ///
    /// For things like serde it might be needed to combine with [field
    /// attributes](#method.field_attribute).
    ///
    /// # Examples
    ///
    /// ```
    /// # let mut config = prost_build::Config::new();
    /// // Nothing around uses floats, so we can derive real `Eq` in addition to `PartialEq`.
    /// config.type_attribute(".", "#[derive(Eq)]");
    /// // Some messages want to be serializable with serde as well.
    /// config.type_attribute("my_messages.MyMessageType",
    ///                       "#[derive(Serialize)] #[serde(rename-all = \"snake_case\")]");
    /// config.type_attribute("my_messages.MyMessageType.MyNestedMessageType",
    ///                       "#[derive(Serialize)] #[serde(rename-all = \"snake_case\")]");
    /// ```
    ///
    /// # Oneof fields
    ///
    /// The `oneof` fields don't have a type name of their own inside Protobuf. Therefore, the
    /// field name can be used both with `type_attribute` and `field_attribute` ‒ the first is
    /// placed before the `enum` type definition, the other before the field inside corresponding
    /// message `struct`.
    ///
    /// In other words, to place an attribute on the `enum` implementing the `oneof`, the match
    /// would look like `my_messages.MyMessageType.oneofname`.
    pub fn type_attribute<P, A>(&mut self, path: P, attribute: A) -> &mut Self
    where
        P: AsRef<str>,
        A: AsRef<str>,
    {
        self.type_attributes
            .push((path.as_ref().to_string(), attribute.as_ref().to_string()));
        self
    }

    /// Configures the code generator to use the provided service generator.
    pub fn service_generator(&mut self, service_generator: Box<ServiceGenerator>) -> &mut Self {
        self.service_generator = Some(service_generator);
        self
    }

    /// Configures the code generator to not use the `prost_types` crate for Protobuf well-known
    /// types, and instead generate Protobuf well-known types from their `.proto` definitions.
    pub fn compile_well_known_types(&mut self) -> &mut Self {
        self.prost_types = false;
        self
    }

    /// Configures the code generator to not strip the enum name from variant names.
    ///
    /// Protobuf enum definitions commonly include the enum name as a prefix of every variant name.
    /// This style is non-idiomatic in Rust, so by default `prost` strips the enum name prefix from
    /// variants which include it. Configuring this option prevents `prost` from stripping the
    /// prefix.
    pub fn retain_enum_prefix(&mut self) -> &mut Self {
        self.strip_enum_prefix = false;
        self
    }

    /// Compile `.proto` files into Rust files during a Cargo build with additional code generator
    /// configuration options.
    ///
    /// This method is like the `prost_build::compile_protos` function, with the added ability to
    /// specify non-default code generation options. See that function for more information about
    /// the arguments and generated outputs.
    ///
    /// # Example `build.rs`
    ///
    /// ```norun
    /// extern crate prost_build;
    ///
    /// fn main() {
    ///     let mut prost_build = prost_build::Config::new();
    ///     prost_build.btree_map(&["."]);
    ///     prost_build.compile_protos(&["src/frontend.proto", "src/backend.proto"],
    ///                                &["src"]).unwrap();
    /// }
    /// ```
    pub fn compile_protos<P>(&mut self, protos: &[P], includes: &[P]) -> Result<()>
    where
        P: AsRef<Path>,
    {
        let target: PathBuf = env::var_os("OUT_DIR")
            .ok_or_else(|| Error::new(ErrorKind::Other, "OUT_DIR environment variable is not set"))?
            .into();

        // TODO: This should probably emit 'rerun-if-changed=PATH' directives for cargo, however
        // according to [1] if any are output then those paths replace the default crate root,
        // which is undesirable. Figure out how to do it in an additive way; perhaps gcc-rs has
        // this figured out.
        // [1]: http://doc.crates.io/build-script.html#outputs-of-the-build-script

        let tmp = tempdir::TempDir::new("prost-build")?;
        let descriptor_set = tmp.path().join("prost-descriptor-set");

        let mut cmd = Command::new(protoc());
        cmd.arg("--include_imports")
            .arg("--include_source_info")
            .arg("-o")
            .arg(&descriptor_set);

        for include in includes {
            cmd.arg("-I").arg(include.as_ref());
        }

        // Set the protoc include after the user includes in case the user wants to
        // override one of the built-in .protos.
        cmd.arg("-I").arg(protoc_include());

        for proto in protos {
            cmd.arg(proto.as_ref());
        }

        let output = cmd.output()?;
        if !output.status.success() {
            return Err(Error::new(
                ErrorKind::Other,
                format!("protoc failed: {}", String::from_utf8_lossy(&output.stderr)),
            ));
        }

        let mut buf = Vec::new();
        fs::File::open(descriptor_set)?.read_to_end(&mut buf)?;
        let descriptor_set = FileDescriptorSet::decode(&buf)?;

        let modules = self.generate(descriptor_set.file);
        for (module, content) in modules {
            let mut filename = module.join(".");
            filename.push_str(".rs");
            trace!("writing: {:?}", filename);
            let mut file = fs::File::create(target.join(filename))?;
            file.write_all(content.as_bytes())?;
            file.flush()?;
        }

        Ok(())
    }

    fn generate(&mut self, files: Vec<FileDescriptorProto>) -> HashMap<Module, String> {
        let mut modules = HashMap::new();

        let message_graph = MessageGraph::new(&files);

        for file in files {
            let module = module(&file);
            let mut buf = modules.entry(module).or_insert_with(String::new);
            CodeGenerator::generate(self, &message_graph, file, &mut buf);
        }
        modules
    }
}

impl default::Default for Config {
    fn default() -> Config {
        Config {
            service_generator: None,
            btree_map: Vec::new(),
            type_attributes: Vec::new(),
            field_attributes: Vec::new(),
            prost_types: true,
            strip_enum_prefix: true,
        }
    }
}

/// Compile `.proto` files into Rust files during a Cargo build.
///
/// The generated `.rs` files are written to the Cargo `OUT_DIR` directory, suitable for use with
/// the [include!][1] macro. See the [Cargo `build.rs` code generation][2] example for more info.
///
/// This function should be called in a project's `build.rs`.
///
/// # Arguments
///
/// **`protos`** - Paths to `.proto` files to compile. Any transitively [imported][3] `.proto`
/// files are automatically be included.
///
/// **`includes`** - Paths to directories in which to search for imports. Directories are searched
/// in order. The `.proto` files passed in **`protos`** must be found in one of the provided
/// include directories.
///
/// # Errors
///
/// This function can fail for a number of reasons:
///
///   - Failure to locate or download `protoc`.
///   - Failure to parse the `.proto`s.
///   - Failure to locate an imported `.proto`.
///
/// It's expected that this function call be `unwrap`ed in a `build.rs`; there is typically no
/// reason to gracefully recover from errors during a build.
///
/// # Example `build.rs`
///
/// ```norun
/// extern crate prost_build;
///
/// fn main() {
///     prost_build::compile_protos(&["src/frontend.proto", "src/backend.proto"],
///                                 &["src"]).unwrap();
/// }
/// ```
///
/// [1]: https://doc.rust-lang.org/std/macro.include.html
/// [2]: http://doc.crates.io/build-script.html#case-study-code-generation
/// [3]: https://developers.google.com/protocol-buffers/docs/proto3#importing-definitions
pub fn compile_protos<P>(protos: &[P], includes: &[P]) -> Result<()>
where
    P: AsRef<Path>,
{
    Config::new().compile_protos(protos, includes)
}

/// Returns the path to the `protoc` binary.
pub fn protoc() -> &'static Path {
    #[cfg(not(feature = "docs-rs"))]
    fn inner() -> &'static Path {
        Path::new(env!("PROTOC"))
    }
    #[cfg(feature = "docs-rs")]
    fn inner() -> &'static Path {
        Path::new("")
    }
    inner()
}

/// Returns the path to the Protobuf include directory.
pub fn protoc_include() -> &'static Path {
    #[cfg(not(feature = "docs-rs"))]
    fn inner() -> &'static Path {
        Path::new(env!("PROTOC_INCLUDE"))
    }
    #[cfg(feature = "docs-rs")]
    fn inner() -> &'static Path {
        Path::new("")
    }
    inner()
}

#[cfg(test)]
mod tests {
    extern crate env_logger;
    use super::*;

    /// An example service generator that generates a trait with methods corresponding to the
    /// service methods.
    struct ServiceTraitGenerator;
    impl ServiceGenerator for ServiceTraitGenerator {
        fn generate(&mut self, service: Service, buf: &mut String) {
            // Generate a trait for the service.
            service.comments.append_with_indent(0, buf);
            buf.push_str(&format!("trait {} {{\n", &service.name));

            // Generate the service methods.
            for method in service.methods {
                method.comments.append_with_indent(1, buf);
                buf.push_str(&format!(
                    "    fn {}({}) -> {};\n",
                    method.name, method.input_type, method.output_type
                ));
            }

            // Close out the trait.
            buf.push_str("}\n");
        }
        fn finalize(&mut self, buf: &mut String) {
            // Needs to be present only once, no matter how many services there are
            buf.push_str("pub mod utils { }\n");
        }
    }

    #[test]
    fn smoke_test() {
        let _ = env_logger::init();
        Config::new()
            .service_generator(Box::new(ServiceTraitGenerator))
            .compile_protos(&["src/smoke_test.proto"], &["src"])
            .unwrap();
    }
}
