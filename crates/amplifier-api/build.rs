//! Build script for generating API client code from `OpenAPI` schema with custom modifications.
//!
//! This build script processes an `OpenAPI` schema file (`schema.yaml`) and generates Rust client
//! code using the `progenitor` crate. It applies custom transformations to handle Borsh
//! serialization for specific types and adds queue message ID implementations for task items.
//!
//! ## Generated Code
//!
//! The generated code is written to `$OUT_DIR/amplifier_api_client.rs` and includes:
//! - API client structures and methods
//! - Custom Borsh serialization attributes for JSON and `DateTime` types
//! - `QueueMsgId` trait implementations for `TaskItem` structs
//!
//! ## Custom Transformations
//!
//! 1. **Borsh Serialization**: Adds custom serialization/deserialization functions for:
//!    - `serde_json::Value` and `serde_json::Map` types
//!    - `chrono::DateTime<Utc>` types (both optional and non-optional)
//!
//! 2. **Queue Message IDs**: Automatically implements the `QueueMsgId` trait for `TaskItem` struct

use progenitor::InterfaceStyle;
use quote::{ToTokens as _, quote};
use syn::visit_mut::VisitMut;
use syn::{Field, Type, TypePath};

/// A visitor that adds Borsh serialization attributes to specific field types.
///
/// This visitor traverses the generated AST and adds custom Borsh serialization attributes
/// to fields that require special handling, such as JSON values and `DateTime` types.
/// It only processes structs that already have Borsh derive attributes.
struct HandleBorsh;

impl VisitMut for HandleBorsh {
    fn visit_item_struct_mut(&mut self, i: &mut syn::ItemStruct) {
        let r#struct = i;

        let has_borsh_derives = r#struct.attrs.iter().any(|attr| {
            if attr.path().is_ident("derive") {
                let mut has_borsh = false;
                attr.parse_nested_meta(|meta| {
                    let path_str = meta.path.to_token_stream().to_string();
                    if path_str.contains("BorshSerialize") || path_str.contains("BorshDeserialize")
                    {
                        has_borsh = true;
                    }
                    Ok(())
                })
                .expect("Failed to parse derive attributes");
                return has_borsh;
            }
            false
        });

        // Only process fields if the struct has borsh derives
        if has_borsh_derives {
            for field in &mut r#struct.fields {
                Self::process_field(field);
            }
        }

        syn::visit_mut::visit_item_struct_mut(self, r#struct);
    }
}

impl HandleBorsh {
    /// Processes individual fields and adds appropriate Borsh serialization attributes.
    ///
    /// This method examines field types and adds custom serialization functions for:
    /// - `serde_json::Value`: Uses `serialize_json_value`/`deserialize_json_value`
    /// - `serde_json::Map`: Uses `serialize_json_map`/`deserialize_json_map`
    /// - `Option<DateTime<Utc>>`: Uses `serialize_option_utc`/`deserialize_option_utc`
    /// - `DateTime<Utc>`: Uses `serialize_utc`/`deserialize_utc`
    ///
    /// # Arguments
    /// * `field` - The struct field to process
    fn process_field(field: &mut Field) {
        if let Type::Path(TypePath { path, .. }) = &field.ty {
            let path_str = quote!(#path).to_string();

            // Add borsh attributes for serde_json::Value
            if path_str.contains("serde_json") &&
                path_str.contains("Value") &&
                !path_str.contains("Map")
            {
                let attr = syn::parse_quote! {
                    #[borsh(
                        serialize_with = "crate::util::serialize_json_value",
                        deserialize_with = "crate::util::deserialize_json_value"
                    )]
                };
                field.attrs.push(attr);
            }
            // Add borsh attributes for serde_json::Map
            else if path_str.contains("serde_json") && path_str.contains("Map") {
                let attr = syn::parse_quote! {
                    #[borsh(
                        serialize_with = "crate::util::serialize_json_map",
                        deserialize_with = "crate::util::deserialize_json_map"
                    )]
                };
                field.attrs.push(attr);
            }
            // Add borsh attributes for Option<DateTime<Utc>>
            else if path_str.contains("Option") &&
                path_str.contains("chrono") &&
                path_str.contains("DateTime")
            {
                let attr = syn::parse_quote! {
                    #[borsh(
                        serialize_with = "crate::util::serialize_option_utc",
                        deserialize_with = "crate::util::deserialize_option_utc"
                    )]
                };
                field.attrs.push(attr);
            }
            // Add borsh attributes for DateTime<Utc> (non-optional)
            else if path_str.contains("chrono") &&
                path_str.contains("DateTime") &&
                !path_str.contains("Option")
            {
                let attr = syn::parse_quote! {
                    #[borsh(
                        serialize_with = "crate::util::serialize_utc",
                        deserialize_with = "crate::util::deserialize_utc"
                    )]
                };
                field.attrs.push(attr);
            } else {
                // We care only for the above types
            }
        }
    }
}

/// A visitor that adds `QueueMsgId` trait implementations to `TaskItem` struct.
///
/// This visitor searches for `types::TaskItem` struct in the generated code and automatically
/// adds an implementation of the `QueueMsgId` trait, which is required for queue message
/// handling in the infrastructure layer.
struct AddQueueMsgId;

impl VisitMut for AddQueueMsgId {
    fn visit_item_mod_mut(&mut self, i: &mut syn::ItemMod) {
        let module = i;

        if module.ident != "types" {
            // We only care about the `models` module
            return;
        }

        if let Some((_, ref mut items)) = module.content {
            let mut new_items = Vec::new();

            for item in items.iter() {
                new_items.push(item.clone());

                if let syn::Item::Struct(item_struct) = item &&
                    item_struct.ident == "TaskItem"
                {
                    let impl_item: syn::Item = syn::parse_quote! {
                        impl  ::infrastructure::interfaces::publisher::QueueMsgId for TaskItem {
                            type MessageId = ::std::string::String;

                            fn id(&self) -> Self::MessageId {
                                self.id.0.clone()
                            }
                        }
                    };
                    new_items.push(impl_item);
                }
            }

            *items = new_items;
        }

        // Continue visiting nested modules
        syn::visit_mut::visit_item_mod_mut(self, module);
    }
}

/// Main build script function that generates API client code from `OpenAPI` schema.
///
/// This function:
/// 1. Reads the `OpenAPI` schema from `schema.yaml`
/// 2. Configures the progenitor generator with Borsh serialization derives
/// 3. Generates the initial AST from the schema
/// 4. Applies custom transformations using the visitor patterns
/// 5. Writes the final generated code to `$OUT_DIR/amplifier_api_client.rs`
///
/// The generated code includes proper Borsh serialization support and queue message
/// ID implementations as needed by the application infrastructure.
///
/// # Returns
/// `Ok(())` on successful code generation, or an error describing what went wrong.
fn main() -> eyre::Result<()> {
    let src = "./schema.yaml";
    println!("cargo:rerun-if-changed={src}");

    let file = std::fs::File::open(src)?;
    let spec = serde_yaml::from_reader(file)?;
    let mut generator_settings = progenitor::GenerationSettings::new();

    generator_settings
        .with_interface(InterfaceStyle::Positional)
        .with_crate("borsh", progenitor::CrateVers::Any, None)
        .with_derive("Eq")
        .with_derive("PartialEq")
        .with_derive("::borsh::BorshSerialize")
        .with_derive("::borsh::BorshDeserialize");

    let mut generator = progenitor::Generator::new(&generator_settings);

    let tokens = generator.generate_tokens(&spec)?;
    let mut ast: syn::File = syn::parse2(tokens)?;

    let mut visitor = HandleBorsh;
    visitor.visit_file_mut(&mut ast);

    let mut queue_visitor = AddQueueMsgId;
    queue_visitor.visit_file_mut(&mut ast);

    let content = prettyplease::unparse(&ast);

    let out_file = std::path::Path::new(&std::env::var("OUT_DIR")?)
        .to_path_buf()
        .join("amplifier_api_client.rs");

    std::fs::write(out_file, content)?;

    Ok(())
}
