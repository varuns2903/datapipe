use crate::model::{Record, Value};
use crate::pipeline::{RecordStream, Stage};
use anyhow::Result;
use indexmap::IndexMap;

pub struct LimitStage {
    pub max: usize,
}

impl Stage for LimitStage {
    fn process<'a>(&'a self, input: RecordStream<'a>) -> RecordStream<'a> {
        Box::new(input.take(self.max))
    }
}

pub struct SelectStage {
    pub fields: Vec<String>,
}

impl Stage for SelectStage {
    fn process<'a>(&'a self, input: RecordStream<'a>) -> RecordStream<'a> {
        // We clone the fields vector so it can be moved into the closure
        // without worrying about borrowing `self`
        let fields = self.fields.clone();
        
        let mapped = input.map(move |res| {
            let record = res?;
            let mut new_record = IndexMap::new();
            
            for field in &fields {
                let val = record.get(field).cloned().unwrap_or(Value::Null);
                new_record.insert(field.clone(), val);
            }
            Ok(new_record)
        });
        
        Box::new(mapped)
    }
}

pub struct FilterStage {
    pub ast: crate::expr::Expr,
}

impl Stage for FilterStage {
    fn process<'a>(&'a self, input: RecordStream<'a>) -> RecordStream<'a> {
        // We need to clone the AST so the closure can own it
        let ast = self.ast.clone();
        
        let mapped = input.filter_map(move |res| match res {
            Ok(record) => {
                let eval_res = ast.evaluate(&record);
                if let Value::Boolean(true) = eval_res {
                    Some(Ok(record))
                } else {
                    None
                }
            }
            Err(e) => Some(Err(e)),
        });
        
        Box::new(mapped)
    }
}

pub struct SortStage {
    pub field: String,
    pub desc: bool,
}

impl Stage for SortStage {
    fn process<'a>(&'a self, input: RecordStream<'a>) -> RecordStream<'a> {
        let field = self.field.clone();
        let desc = self.desc;
        
        // Eagerly collect all records for sorting
        let mut records = match input.collect::<Result<Vec<_>>>() {
            Ok(r) => r,
            Err(e) => return Box::new(std::iter::once(Err(e))),
        };
        
        records.sort_by(|a, b| {
            let val_a = a.get(&field).unwrap_or(&Value::Null);
            let val_b = b.get(&field).unwrap_or(&Value::Null);
            let mut ord = crate::model::cmp_values(val_a, val_b);
            if desc {
                ord = ord.reverse();
            }
            ord
        });
        
        Box::new(records.into_iter().map(Ok))
    }
}

pub struct UniqueStage {
    pub field: String,
}

impl Stage for UniqueStage {
    fn process<'a>(&'a self, input: RecordStream<'a>) -> RecordStream<'a> {
        let field = self.field.clone();
        let mut seen = std::collections::HashSet::new();
        
        let filtered = input.filter_map(move |res| {
            match res {
                Ok(record) => {
                    let val = record.get(&field).unwrap_or(&Value::Null);
                    // Serialize to string to easily hash floats/nested objects
                    let val_str = serde_json::to_string(val).unwrap_or_default();
                    if seen.contains(&val_str) {
                        None
                    } else {
                        seen.insert(val_str);
                        Some(Ok(record))
                    }
                }
                Err(e) => Some(Err(e)),
            }
        });
        
        Box::new(filtered)
    }
}

pub struct CountStage;

impl Stage for CountStage {
    fn process<'a>(&'a self, input: RecordStream<'a>) -> RecordStream<'a> {
        let count = input.count(); 
        
        let mut rec = indexmap::IndexMap::new();
        rec.insert("count".to_string(), Value::Integer(count as i64));
        
        Box::new(std::iter::once(Ok(rec)))
    }
}

pub struct SumStage {
    pub field: String,
}

impl Stage for SumStage {
    fn process<'a>(&'a self, input: RecordStream<'a>) -> RecordStream<'a> {
        let field = self.field.clone();
        let mut sum_int = 0i64;
        let mut sum_float = 0f64;
        let mut is_float = false;

        for res in input {
            if let Ok(rec) = res {
                if let Some(val) = rec.get(&field) {
                    match val {
                        Value::Integer(i) => {
                            if is_float { sum_float += *i as f64; }
                            else { sum_int += i; }
                        },
                        Value::Float(f) => {
                            if !is_float {
                                is_float = true;
                                sum_float = sum_int as f64;
                            }
                            sum_float += f;
                        },
                        _ => {}
                    }
                }
            }
        }
        
        let mut result_rec = indexmap::IndexMap::new();
        let final_val = if is_float { Value::Float(sum_float) } else { Value::Integer(sum_int) };
        result_rec.insert(format!("sum_{}", field), final_val);
        
        Box::new(std::iter::once(Ok(result_rec)))
    }
}

pub struct AvgStage {
    pub field: String,
}

impl Stage for AvgStage {
    fn process<'a>(&'a self, input: RecordStream<'a>) -> RecordStream<'a> {
        let field = self.field.clone();
        let mut sum = 0f64;
        let mut count = 0i64;

        for res in input {
            if let Ok(rec) = res {
                if let Some(val) = rec.get(&field) {
                    match val {
                        Value::Integer(i) => { sum += *i as f64; count += 1; },
                        Value::Float(f) => { sum += f; count += 1; },
                        _ => {} 
                    }
                }
            }
        }
        
        let mut result_rec = indexmap::IndexMap::new();
        let final_val = if count == 0 { Value::Null } else { Value::Float(sum / count as f64) };
        result_rec.insert(format!("avg_{}", field), final_val);
        
        Box::new(std::iter::once(Ok(result_rec)))
    }
}

pub struct MinStage {
    pub field: String,
}

impl Stage for MinStage {
    fn process<'a>(&'a self, input: RecordStream<'a>) -> RecordStream<'a> {
        let field = self.field.clone();
        let mut min_val: Option<Value> = None;

        for res in input {
            if let Ok(rec) = res {
                if let Some(val) = rec.get(&field) {
                    if let Some(ref current_min) = min_val {
                        if crate::model::cmp_values(val, current_min) == std::cmp::Ordering::Less {
                            min_val = Some(val.clone());
                        }
                    } else {
                        min_val = Some(val.clone());
                    }
                }
            }
        }
        
        let mut result_rec = indexmap::IndexMap::new();
        result_rec.insert(format!("min_{}", field), min_val.unwrap_or(Value::Null));
        Box::new(std::iter::once(Ok(result_rec)))
    }
}

pub struct MaxStage {
    pub field: String,
}

impl Stage for MaxStage {
    fn process<'a>(&'a self, input: RecordStream<'a>) -> RecordStream<'a> {
        let field = self.field.clone();
        let mut max_val: Option<Value> = None;

        for res in input {
            if let Ok(rec) = res {
                if let Some(val) = rec.get(&field) {
                    if let Some(ref current_max) = max_val {
                        if crate::model::cmp_values(val, current_max) == std::cmp::Ordering::Greater {
                            max_val = Some(val.clone());
                        }
                    } else {
                        max_val = Some(val.clone());
                    }
                }
            }
        }
        
        let mut result_rec = indexmap::IndexMap::new();
        result_rec.insert(format!("max_{}", field), max_val.unwrap_or(Value::Null));
        Box::new(std::iter::once(Ok(result_rec)))
    }
}

pub struct SchemaStage;

impl Stage for SchemaStage {
    fn process<'a>(&'a self, input: RecordStream<'a>) -> RecordStream<'a> {
        let mut field_types: indexmap::IndexMap<String, std::collections::HashSet<String>> = indexmap::IndexMap::new();

        for res in input.take(10_000) {
            if let Ok(rec) = res {
                for (key, val) in rec {
                    let type_name = match val {
                        Value::Null => "null",
                        Value::Boolean(_) => "boolean",
                        Value::Integer(_) => "integer",
                        Value::Float(_) => "float",
                        Value::String(_) => "string",
                        Value::Array(_) => "array",
                        Value::Object(_) => "object",
                    };
                    field_types.entry(key)
                        .or_default()
                        .insert(type_name.to_string());
                }
            }
        }

        let mut result_rec = indexmap::IndexMap::new();
        for (field, types) in field_types {
            let mut types_vec: Vec<_> = types.into_iter().collect();
            types_vec.sort();
            result_rec.insert(
                field, 
                Value::String(types_vec.join(" | "))
            );
        }

        Box::new(std::iter::once(Ok(result_rec)))
    }
}
