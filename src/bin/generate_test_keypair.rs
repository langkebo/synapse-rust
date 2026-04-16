#[cfg(feature = "test-utils")]
use synapse_rust::common::federation_test_keys::generate_federation_test_keypair;

#[cfg(feature = "test-utils")]
fn main() {
    println!("Generating federation test keypair...");
    println!();

    let keypair = generate_federation_test_keypair();

    println!("Key ID: {}", keypair.key_id);
    println!("Secret Key (base64): {}", keypair.secret_key);
    println!("Public Key (base64): {}", keypair.public_key);
    println!();

    println!("# Environment variables:");
    println!("export FEDERATION_KEY_ID=\"{}\"", keypair.key_id);
    println!("export FEDERATION_SECRET_KEY=\"{}\"", keypair.secret_key);
    println!("export FEDERATION_PUBLIC_KEY=\"{}\"", keypair.public_key);
}

#[cfg(not(feature = "test-utils"))]
fn main() {
    eprintln!("This tool is only available with --features test-utils.");
    eprintln!("Use: cargo run --bin generate_test_keypair --features test-utils");
    std::process::exit(1);
}
