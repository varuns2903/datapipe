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
