use synapse_rust::common::federation_test_keys::generate_federation_test_keypair;

fn main() {
    println!("Generating federation test keypair...");
    println!();

    let keypair = generate_federation_test_keypair();

    println!("Key ID: {}", keypair.key_id);
    println!("Secret Key (base64): {}", keypair.secret_key);
    println!("Public Key (base64): {}", keypair.public_key);
    println!();

    // Also output as environment variables for easy use
    println!("# Environment variables:");
    println!("export FEDERATION_KEY_ID=\"{}\"", keypair.key_id);
    println!("export FEDERATION_SECRET_KEY=\"{}\"", keypair.secret_key);
    println!("export FEDERATION_PUBLIC_KEY=\"{}\"", keypair.public_key);
}
