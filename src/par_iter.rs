use crate::model::Record;
use crate::expr::Expr;
use crate::model::Value;
use rayon::prelude::*;

pub struct ParFilterIter<'a> {
    pub inner: crate::pipeline::RecordStream<'a>,
    pub ast: Expr,
    pub buffer: std::vec::IntoIter<anyhow::Result<Record>>,
}

impl<'a> Iterator for ParFilterIter<'a> {
    type Item = anyhow::Result<Record>;
    
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // Drain current buffer
            if let Some(item) = self.buffer.next() {
                return Some(item);
            }
            
            // Refill buffer from stream
            let mut chunk = Vec::with_capacity(10_000);
            for _ in 0..10_000 {
                if let Some(item) = self.inner.next() {
                    chunk.push(item);
                } else {
                    break;
                }
            }
            
            if chunk.is_empty() {
                return None;
            }
            
            // Process chunk in parallel using Rayon!
            let ast = &self.ast;
            let processed: Vec<_> = chunk.into_par_iter().filter_map(|res| {
                match res {
                    Ok(record) => {
                        if let Value::Boolean(true) = ast.evaluate(&record) {
                            Some(Ok(record))
                        } else {
                            None
                        }
                    }
                    Err(e) => Some(Err(e)),
                }
            }).collect();
            
            self.buffer = processed.into_iter();
        }
    }
}
