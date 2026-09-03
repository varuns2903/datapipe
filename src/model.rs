use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    Null,
    Boolean(bool),
    Integer(i64),
    Float(f64),
    String(String),
    Array(Vec<Value>),
    Object(IndexMap<String, Value>),
}

/// A Record is the top-level unit of data passing through the pipeline.
/// It is represented as an ordered map of field names to Values.
pub type Record = IndexMap<String, Value>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_value_equality() {
        let mut obj1 = IndexMap::new();
        obj1.insert("name".to_string(), Value::String("Alice".to_string()));
        obj1.insert("age".to_string(), Value::Integer(30));

        let mut obj2 = IndexMap::new();
        obj2.insert("name".to_string(), Value::String("Alice".to_string()));
        obj2.insert("age".to_string(), Value::Integer(30));

        assert_eq!(Value::Object(obj1), Value::Object(obj2));
    }
}
