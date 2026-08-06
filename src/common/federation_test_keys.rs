#[cfg(any(test, feature = "test-utils"))]
#[allow(unused_imports)] // re-export used by generate_test_keypair binary
pub use synapse_common::federation_test_keys::*;
