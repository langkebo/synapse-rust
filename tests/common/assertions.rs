use serde_json::Value;

pub fn assert_error_response(response: &Value, expected_code: &str) {
    assert!(
        response.get("errcode").is_some(),
        "Response should have errcode field"
    );
    assert_eq!(
        response["errcode"], expected_code,
        "Error code should match"
    );
}

pub fn assert_success_response(response: &Value) {
    assert!(
        !response.get("errcode").is_some(),
        "Success response should not have errcode field"
    );
}

pub fn assert_has_field(response: &Value, field: &str) {
    assert!(
        response.get(field).is_some(),
        "Response should have field: {}",
        field
    );
}

pub fn assert_field_equals(response: &Value, field: &str, expected: &Value) {
    assert_eq!(
        response.get(field),
        Some(expected),
        "Field {} should equal {:?}",
        field,
        expected
    );
}

pub fn assert_is_array(response: &Value, field: &str) {
    assert!(
        response.get(field).map(|v| v.is_array()).unwrap_or(false),
        "Field {} should be an array",
        field
    );
}

pub fn assert_array_length(response: &Value, field: &str, expected_len: usize) {
    if let Some(arr) = response.get(field).and_then(|v| v.as_array()) {
        assert_eq!(
            arr.len(),
            expected_len,
            "Array {} should have {} elements",
            field,
            expected_len
        );
    } else {
        panic!("Field {} is not an array", field);
    }
}

pub fn assert_is_string(response: &Value, field: &str) {
    assert!(
        response.get(field).map(|v| v.is_string()).unwrap_or(false),
        "Field {} should be a string",
        field
    );
}

pub fn assert_is_number(response: &Value, field: &str) {
    assert!(
        response.get(field).map(|v| v.is_number()).unwrap_or(false),
        "Field {} should be a number",
        field
    );
}

pub fn assert_is_object(response: &Value, field: &str) {
    assert!(
        response.get(field).map(|v| v.is_object()).unwrap_or(false),
        "Field {} should be an object",
        field
    );
}
