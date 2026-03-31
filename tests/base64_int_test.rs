//! Integration tests for base64 dependency usage.

use base64::Engine as _;
use base64::engine::general_purpose;

#[test]
fn test_base64_encode_decode_roundtrip() {
    let original = b"swe-gateway integration test data";
    let encoded = general_purpose::STANDARD.encode(original);
    let decoded = general_purpose::STANDARD.decode(&encoded).unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn test_base64_empty_input() {
    let encoded = general_purpose::STANDARD.encode(b"");
    assert_eq!(encoded, "");
    let decoded = general_purpose::STANDARD.decode(&encoded).unwrap();
    assert!(decoded.is_empty());
}
