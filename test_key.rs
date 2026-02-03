fn main() {
    let key = "gtjAoCT5GSIk9DhLAq46MEIy7RdmZirxZpj0iDknE7I=";
    let decoded = base64::engine::general_purpose::STANDARD.decode(key);
    match decoded {
        Ok(bytes) => println!("Decoded length: {}", bytes.len()),
        Err(e) => println!("Error decoding: {}", e),
    }
}
