use crate::model::{Record, Value};
use crate::pipeline::{RecordStream, Stage};

pub struct FilterStage {
    pub ast: crate::expr::Expr,
}

impl Stage for FilterStage {
    fn process<'a>(&'a self, input: RecordStream<'a>) -> RecordStream<'a> {
        // Use our new parallel streaming filter!
        let par_iter = crate::par_iter::ParFilterIter {
            inner: input,
            ast: self.ast.clone(),
            buffer: Vec::new().into_iter(),
        };
        Box::new(par_iter)
    }
}

pub struct SelectStage {
    pub fields: Vec<String>,
}

impl Stage for SelectStage {
    fn process<'a>(&'a self, input: RecordStream<'a>) -> RecordStream<'a> {
        let fields = self.fields.clone();
        
        let mapped = input.map(move |res| {
            res.map(|record| {
                let mut new_record = indexmap::IndexMap::new();
                for field in &fields {
                    let val = record.get(field).cloned().unwrap_or(Value::Null);
                    new_record.insert(field.clone(), val);
                }
                new_record
            })
        });
        Box::new(mapped)
    }
}

pub struct LimitStage {
    pub max: usize,
}

impl Stage for LimitStage {
    fn process<'a>(&'a self, input: RecordStream<'a>) -> RecordStream<'a> {
        Box::new(input.take(self.max))
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


pub struct SortStage {
    pub field: String,
    pub desc: bool,
}

struct HeapItem {
    record: Record,
    file_idx: usize,
    field: String,
    desc: bool,
}

impl PartialEq for HeapItem {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == std::cmp::Ordering::Equal
    }
}
impl Eq for HeapItem {}
impl PartialOrd for HeapItem {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for HeapItem {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let val_a = self.record.get(&self.field).unwrap_or(&Value::Null);
        let val_b = other.record.get(&other.field).unwrap_or(&Value::Null);
        let mut ord = crate::model::cmp_values(val_a, val_b);
        if self.desc {
            ord = ord.reverse();
        }
        // Reverse because BinaryHeap is a MAX heap, and we want a MIN heap for K-way merge
        ord.reverse()
    }
}

pub struct ExternalSortIter<'a> {
    pub readers: Vec<RecordStream<'a>>,
    pub heap: std::collections::BinaryHeap<HeapItem>,
    pub field: String,
    pub desc: bool,
    pub initialized: bool,
}

impl<'a> Iterator for ExternalSortIter<'a> {
    type Item = anyhow::Result<Record>;
    fn next(&mut self) -> Option<Self::Item> {
        if !self.initialized {
            for (idx, reader) in self.readers.iter_mut().enumerate() {
                if let Some(Ok(rec)) = reader.next() {
                    self.heap.push(HeapItem { record: rec, file_idx: idx, field: self.field.clone(), desc: self.desc });
                }
            }
            self.initialized = true;
        }
        
        if let Some(min_item) = self.heap.pop() {
            let idx = min_item.file_idx;
            let record = min_item.record;
            
            if let Some(Ok(next_rec)) = self.readers[idx].next() {
                self.heap.push(HeapItem { record: next_rec, file_idx: idx, field: self.field.clone(), desc: self.desc });
            }
            return Some(Ok(record));
        }
        None
    }
}

impl Stage for SortStage {
    fn process<'a>(&'a self, mut input: RecordStream<'a>) -> RecordStream<'a> {
        let field = self.field.clone();
        let desc = self.desc;
        let mut temp_files = Vec::new();
        
        loop {
            let mut chunk = Vec::with_capacity(50_000);
            for _ in 0..50_000 {
                if let Some(Ok(rec)) = input.next() {
                    chunk.push(rec);
                } else { break; }
            }
            if chunk.is_empty() { break; }
            
            chunk.sort_by(|a, b| {
                let val_a = a.get(&field).unwrap_or(&Value::Null);
                let val_b = b.get(&field).unwrap_or(&Value::Null);
                let mut ord = crate::model::cmp_values(val_a, val_b);
                if desc { ord = ord.reverse(); }
                ord
            });
            
            let mut tmp = tempfile::NamedTempFile::new().unwrap();
            for rec in chunk {
                let json = serde_json::to_string(&rec).unwrap();
                use std::io::Write;
                writeln!(tmp, "{}", json).unwrap();
            }
            temp_files.push(tmp.into_temp_path());
        }
        
        if temp_files.is_empty() {
            return Box::new(std::iter::empty());
        }
        
        let mut readers: Vec<RecordStream<'a>> = Vec::new();
        for path in temp_files {
            let file = std::fs::File::open(path).unwrap();
            let reader = std::io::BufReader::new(file);
            let stream = crate::io::read_json_stream(reader);
            readers.push(Box::new(stream));
        }
        
        Box::new(ExternalSortIter {
            readers,
            heap: std::collections::BinaryHeap::new(),
            field,
            desc,
            initialized: false,
        })
    }
}

pub struct ExplodeStage {
    pub field: String,
}

impl Stage for ExplodeStage {
    fn process<'a>(&'a self, input: RecordStream<'a>) -> RecordStream<'a> {
        let field = self.field.clone();
        let iter = input.flat_map(move |res| {
            match res {
                Ok(record) => {
                    if let Some(Value::Array(arr)) = record.get(&field) {
                        let mut out = Vec::new();
                        for item in arr.iter() {
                            let mut new_rec = record.clone();
                            new_rec.insert(field.clone(), item.clone());
                            out.push(Ok(new_rec));
                        }
                        out.into_iter()
                    } else {
                        vec![Ok(record)].into_iter()
                    }
                }
                Err(e) => vec![Err(e)].into_iter(),
            }
        });
        Box::new(iter)
    }
}

pub struct MapStage {
    pub field: String,
    pub ast: crate::expr::Expr,
}

impl Stage for MapStage {
    fn process<'a>(&'a self, input: RecordStream<'a>) -> RecordStream<'a> {
        let field = self.field.clone();
        let ast = self.ast.clone();
        let iter = input.map(move |res| {
            match res {
                Ok(mut record) => {
                    let new_val = ast.evaluate(&record);
                    record.insert(field.clone(), new_val);
                    Ok(record)
                },
                Err(e) => Err(e),
            }
        });
        Box::new(iter)
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

pub struct GroupStage {
    pub by: String,
    pub sum: Option<String>,
    pub count: bool,
}

impl Stage for GroupStage {
    fn process<'a>(&'a self, input: RecordStream<'a>) -> RecordStream<'a> {
        let by = self.by.clone();
        let sum_field = self.sum.clone();
        let do_count = self.count;
        
        let mut groups: indexmap::IndexMap<String, (i64, f64, i64, bool)> = indexmap::IndexMap::new();

        for res in input {
            if let Ok(rec) = res {
                let group_key = match rec.get(&by) {
                    Some(Value::String(s)) => s.clone(),
                    Some(val) => serde_json::to_string(val).unwrap_or_default(),
                    None => "null".to_string(),
                };
                
                let entry = groups.entry(group_key).or_insert((0, 0.0, 0, false));
                entry.2 += 1; 
                
                if let Some(ref field) = sum_field {
                    if let Some(val) = rec.get(field) {
                        match val {
                            Value::Integer(i) => {
                                if entry.3 { entry.1 += *i as f64; }
                                else { entry.0 += i; }
                            },
                            Value::Float(f) => {
                                if !entry.3 {
                                    entry.3 = true;
                                    entry.1 = entry.0 as f64;
                                }
                                entry.1 += f;
                            },
                            _ => {}
                        }
                    }
                }
            }
        }
        
        let mut output = Vec::new();
        for (key, (sum_int, sum_float, count, is_float)) in groups {
            let mut rec = indexmap::IndexMap::new();
            rec.insert(by.clone(), Value::String(key));
            if do_count { rec.insert("count".to_string(), Value::Integer(count)); }
            if let Some(ref field) = sum_field {
                let final_sum = if is_float { Value::Float(sum_float) } else { Value::Integer(sum_int) };
                rec.insert(format!("sum_{}", field), final_sum);
            }
            output.push(Ok(rec));
        }
        Box::new(output.into_iter())
    }
}

pub struct JoinStage {
    pub hash_map: std::sync::Arc<std::collections::HashMap<String, Record>>,
    pub on: String,
}

impl Stage for JoinStage {
    fn process<'a>(&'a self, input: RecordStream<'a>) -> RecordStream<'a> {
        let hash_map = std::sync::Arc::clone(&self.hash_map);
        let on = self.on.clone();
        
        let iter = input.map(move |res| {
            match res {
                Ok(mut record) => {
                    let join_key = match record.get(&on) {
                        Some(Value::String(s)) => s.clone(),
                        Some(val) => serde_json::to_string(val).unwrap_or_default(),
                        None => return Ok(record),
                    };
                    
                    if let Some(right_record) = hash_map.get(&join_key) {
                        for (k, v) in right_record {
                            if k != &on {
                                record.insert(k.clone(), v.clone());
                            }
                        }
                    }
                    Ok(record)
                },
                Err(e) => Err(e),
            }
        });
        
        Box::new(iter)
    }
}

