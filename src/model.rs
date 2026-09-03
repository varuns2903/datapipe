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

use std::cmp::Ordering;

/// Provides a total ordering for Values so they can be sorted.
pub fn cmp_values(a: &Value, b: &Value) -> Ordering {
    match (a, b) {
        // Same types
        (Value::Null, Value::Null) => Ordering::Equal,
        (Value::Boolean(a_b), Value::Boolean(b_b)) => a_b.cmp(b_b),
        (Value::Integer(a_i), Value::Integer(b_i)) => a_i.cmp(b_i),
        (Value::String(a_s), Value::String(b_s)) => a_s.cmp(b_s),
        
        // Number cross-types
        (Value::Integer(i), Value::Float(f)) => (*i as f64).partial_cmp(f).unwrap_or(Ordering::Equal),
        (Value::Float(f), Value::Integer(i)) => f.partial_cmp(&(*i as f64)).unwrap_or(Ordering::Equal),
        (Value::Float(f1), Value::Float(f2)) => f1.partial_cmp(f2).unwrap_or(Ordering::Equal),

        // Different types (define an arbitrary total order)
        // Null < Boolean < Number < String < Array < Object
        (Value::Null, _) => Ordering::Less,
        (_, Value::Null) => Ordering::Greater,
        
        (Value::Boolean(_), _) => Ordering::Less,
        (_, Value::Boolean(_)) => Ordering::Greater,
        
        (Value::Integer(_), _) | (Value::Float(_), _) => Ordering::Less,
        (_, Value::Integer(_)) | (_, Value::Float(_)) => Ordering::Greater,
        
        (Value::String(_), _) => Ordering::Less,
        (_, Value::String(_)) => Ordering::Greater,
        
        (Value::Array(_), Value::Object(_)) => Ordering::Less,
        (Value::Object(_), Value::Array(_)) => Ordering::Greater,
        
        _ => Ordering::Equal,
    }
}
