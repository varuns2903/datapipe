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
