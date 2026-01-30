use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub struct ResponseFilter;

impl ResponseFilter {
    pub fn filter_fields<T: Serialize>(
        data: &T,
        fields: Option<&[String]>,
    ) -> Result<Value, serde_json::Error> {
        let full_value = serde_json::to_value(data)?;

        match fields {
            Some(fields) => {
                let mut filtered = json!({});
                if let Value::Object(map) = full_value {
                    for field in fields {
                        if let Some(value) = map.get(field) {
                            if let Value::Object(ref mut filtered_map) = filtered {
                                filtered_map.insert(field.clone(), value.clone());
                            }
                        }
                    }
                }
                Ok(filtered)
            }
            None => Ok(full_value),
        }
    }

    pub fn extract_fields_from_query(query: &Value) -> Option<Vec<String>> {
        query
            .get("filter")
            .and_then(|f| f.as_str())
            .and_then(|s| serde_json::from_str(s).ok())
            .and_then(|v: Value| {
                v.get("fields").and_then(|f| f.as_array()).map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
            })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilteredResponse<T> {
    pub data: T,
    pub filtered: bool,
}

impl<T> FilteredResponse<T> {
    pub fn new(data: T, filtered: bool) -> Self {
        Self { data, filtered }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_fields_all() {
        let data = json!({
            "name": "John",
            "age": 30,
            "email": "john@example.com"
        });

        let result = ResponseFilter::filter_fields(&data, None).unwrap();
        assert_eq!(result, data);
    }

    #[test]
    fn test_filter_fields_specific() {
        let data = json!({
            "name": "John",
            "age": 30,
            "email": "john@example.com"
        });

        let fields = vec!["name".to_string(), "email".to_string()];
        let result = ResponseFilter::filter_fields(&data, Some(&fields)).unwrap();

        assert!(result.get("name").is_some());
        assert!(result.get("email").is_some());
        assert!(result.get("age").is_none());
    }

    #[test]
    fn test_filter_fields_empty() {
        let data = json!({
            "name": "John",
            "age": 30
        });

        let fields: Vec<String> = vec![];
        let result = ResponseFilter::filter_fields(&data, Some(&fields)).unwrap();
        assert_eq!(result, json!({}));
    }

    #[test]
    fn test_extract_fields_from_query() {
        let query = json!({
            "filter": r#"{"fields": ["name", "email"]}"#
        });

        let fields = ResponseFilter::extract_fields_from_query(&query);
        assert_eq!(fields, Some(vec!["name".to_string(), "email".to_string()]));
    }

    #[test]
    fn test_extract_fields_no_filter() {
        let query = json!({
            "limit": 10
        });

        let fields = ResponseFilter::extract_fields_from_query(&query);
        assert!(fields.is_none());
    }

    #[test]
    fn test_filtered_response() {
        let data = json!({"name": "John"});
        let response = FilteredResponse::new(data, true);
        assert!(response.filtered);
    }
}
