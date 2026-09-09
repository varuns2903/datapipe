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

        for rec in input.flatten() {
            if let Some(val) = rec.get(&field) {
                match val {
                    Value::Integer(i) => {
                        if is_float {
                            sum_float += *i as f64;
                        } else {
                            sum_int += i;
                        }
                    }
                    Value::Float(f) => {
                        if !is_float {
                            is_float = true;
                            sum_float = sum_int as f64;
                        }
                        sum_float += f;
                    }
                    _ => {}
                }
            }
        }

        let mut result_rec = indexmap::IndexMap::new();
        let final_val = if is_float {
            Value::Float(sum_float)
        } else {
            Value::Integer(sum_int)
        };
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

        for rec in input.flatten() {
            if let Some(val) = rec.get(&field) {
                match val {
                    Value::Integer(i) => {
                        sum += *i as f64;
                        count += 1;
                    }
                    Value::Float(f) => {
                        sum += f;
                        count += 1;
                    }
                    _ => {}
                }
            }
        }

        let mut result_rec = indexmap::IndexMap::new();
        let final_val = if count == 0 {
            Value::Null
        } else {
            Value::Float(sum / count as f64)
        };
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

        for rec in input.flatten() {
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

        for rec in input.flatten() {
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

        let mut result_rec = indexmap::IndexMap::new();
        result_rec.insert(format!("max_{}", field), max_val.unwrap_or(Value::Null));
        Box::new(std::iter::once(Ok(result_rec)))
    }
}

pub struct SortStage {
    pub field: String,
    pub desc: bool,
}

pub(crate) struct HeapItem {
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
    pub(crate) heap: std::collections::BinaryHeap<HeapItem>,
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
                    self.heap.push(HeapItem {
                        record: rec,
                        file_idx: idx,
                        field: self.field.clone(),
                        desc: self.desc,
                    });
                }
            }
            self.initialized = true;
        }

        if let Some(min_item) = self.heap.pop() {
            let idx = min_item.file_idx;
            let record = min_item.record;

            if let Some(Ok(next_rec)) = self.readers[idx].next() {
                self.heap.push(HeapItem {
                    record: next_rec,
                    file_idx: idx,
                    field: self.field.clone(),
                    desc: self.desc,
                });
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
                } else {
                    break;
                }
            }
            if chunk.is_empty() {
                break;
            }

            chunk.sort_by(|a, b| {
                let val_a = a.get(&field).unwrap_or(&Value::Null);
                let val_b = b.get(&field).unwrap_or(&Value::Null);
                let mut ord = crate::model::cmp_values(val_a, val_b);
                if desc {
                    ord = ord.reverse();
                }
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
        let iter = input.flat_map(move |res| match res {
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
        let iter = input.map(move |res| match res {
            Ok(mut record) => {
                let new_val = ast.evaluate(&record);
                record.insert(field.clone(), new_val);
                Ok(record)
            }
            Err(e) => Err(e),
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

        let filtered = input.filter_map(move |res| match res {
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
        });

        Box::new(filtered)
    }
}

pub struct SchemaStage;

impl Stage for SchemaStage {
    fn process<'a>(&'a self, input: RecordStream<'a>) -> RecordStream<'a> {
        let mut field_types: indexmap::IndexMap<String, std::collections::HashSet<String>> =
            indexmap::IndexMap::new();

        for rec in input.take(10_000).flatten() {
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
                field_types
                    .entry(key)
                    .or_default()
                    .insert(type_name.to_string());
            }
        }

        let mut result_rec = indexmap::IndexMap::new();
        for (field, types) in field_types {
            let mut types_vec: Vec<_> = types.into_iter().collect();
            types_vec.sort();
            result_rec.insert(field, Value::String(types_vec.join(" | ")));
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

        let mut groups: indexmap::IndexMap<String, (i64, f64, i64, bool)> =
            indexmap::IndexMap::new();

        for rec in input.flatten() {
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
                            if entry.3 {
                                entry.1 += *i as f64;
                            } else {
                                entry.0 += i;
                            }
                        }
                        Value::Float(f) => {
                            if !entry.3 {
                                entry.3 = true;
                                entry.1 = entry.0 as f64;
                            }
                            entry.1 += f;
                        }
                        _ => {}
                    }
                }
            }
        }

        let mut output = Vec::new();
        for (key, (sum_int, sum_float, count, is_float)) in groups {
            let mut rec = indexmap::IndexMap::new();
            rec.insert(by.clone(), Value::String(key));
            if do_count {
                rec.insert("count".to_string(), Value::Integer(count));
            }
            if let Some(ref field) = sum_field {
                let final_sum = if is_float {
                    Value::Float(sum_float)
                } else {
                    Value::Integer(sum_int)
                };
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

        let iter = input.map(move |res| match res {
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
            }
            Err(e) => Err(e),
        });

        Box::new(iter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;

    fn rec(pairs: &[(&str, Value)]) -> Record {
        let mut r = IndexMap::new();
        for (k, v) in pairs {
            r.insert(k.to_string(), v.clone());
        }
        r
    }

    fn stream(records: Vec<Record>) -> RecordStream<'static> {
        Box::new(records.into_iter().map(Ok))
    }

    fn collect_ok(s: RecordStream) -> Vec<Record> {
        s.map(|r| r.unwrap()).collect()
    }

    #[test]
    fn select_keeps_only_requested_fields_and_fills_missing_with_null() {
        let input = stream(vec![rec(&[
            ("a", Value::Integer(1)),
            ("b", Value::Integer(2)),
        ])]);
        let stage = SelectStage {
            fields: vec!["a".to_string(), "c".to_string()],
        };
        let out = collect_ok(stage.process(input));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].get("a"), Some(&Value::Integer(1)));
        assert_eq!(out[0].get("c"), Some(&Value::Null));
        assert_eq!(out[0].get("b"), None);
    }

    #[test]
    fn limit_truncates_stream() {
        let input = stream(vec![
            rec(&[("a", Value::Integer(1))]),
            rec(&[("a", Value::Integer(2))]),
            rec(&[("a", Value::Integer(3))]),
        ]);
        let stage = LimitStage { max: 2 };
        assert_eq!(collect_ok(stage.process(input)).len(), 2);
    }

    #[test]
    fn limit_zero_yields_nothing() {
        let input = stream(vec![rec(&[("a", Value::Integer(1))])]);
        let stage = LimitStage { max: 0 };
        assert_eq!(collect_ok(stage.process(input)).len(), 0);
    }

    #[test]
    fn count_on_empty_stream_is_zero() {
        let input = stream(vec![]);
        let out = collect_ok(CountStage.process(input));
        assert_eq!(out[0].get("count"), Some(&Value::Integer(0)));
    }

    #[test]
    fn count_counts_all_records() {
        let input = stream(vec![rec(&[]), rec(&[]), rec(&[])]);
        let out = collect_ok(CountStage.process(input));
        assert_eq!(out[0].get("count"), Some(&Value::Integer(3)));
    }

    #[test]
    fn sum_integer_field() {
        let input = stream(vec![
            rec(&[("n", Value::Integer(2))]),
            rec(&[("n", Value::Integer(3))]),
        ]);
        let stage = SumStage {
            field: "n".to_string(),
        };
        let out = collect_ok(stage.process(input));
        assert_eq!(out[0].get("sum_n"), Some(&Value::Integer(5)));
    }

    #[test]
    fn sum_mixed_int_and_float_upgrades_to_float() {
        let input = stream(vec![
            rec(&[("n", Value::Integer(2))]),
            rec(&[("n", Value::Float(1.5))]),
        ]);
        let stage = SumStage {
            field: "n".to_string(),
        };
        let out = collect_ok(stage.process(input));
        assert_eq!(out[0].get("sum_n"), Some(&Value::Float(3.5)));
    }

    #[test]
    fn sum_ignores_non_numeric_and_missing_values() {
        let input = stream(vec![
            rec(&[("n", Value::String("x".to_string()))]),
            rec(&[]),
            rec(&[("n", Value::Integer(4))]),
        ]);
        let stage = SumStage {
            field: "n".to_string(),
        };
        let out = collect_ok(stage.process(input));
        assert_eq!(out[0].get("sum_n"), Some(&Value::Integer(4)));
    }

    #[test]
    fn avg_computes_mean() {
        let input = stream(vec![
            rec(&[("n", Value::Integer(2))]),
            rec(&[("n", Value::Integer(4))]),
        ]);
        let stage = AvgStage {
            field: "n".to_string(),
        };
        let out = collect_ok(stage.process(input));
        assert_eq!(out[0].get("avg_n"), Some(&Value::Float(3.0)));
    }

    #[test]
    fn avg_on_empty_stream_is_null() {
        let input = stream(vec![]);
        let stage = AvgStage {
            field: "n".to_string(),
        };
        let out = collect_ok(stage.process(input));
        assert_eq!(out[0].get("avg_n"), Some(&Value::Null));
    }

    #[test]
    fn min_and_max_find_extremes() {
        let input = || {
            stream(vec![
                rec(&[("n", Value::Integer(5))]),
                rec(&[("n", Value::Integer(1))]),
                rec(&[("n", Value::Integer(3))]),
            ])
        };
        let min_out = collect_ok(
            (MinStage {
                field: "n".to_string(),
            })
            .process(input()),
        );
        assert_eq!(min_out[0].get("min_n"), Some(&Value::Integer(1)));

        let max_out = collect_ok(
            (MaxStage {
                field: "n".to_string(),
            })
            .process(input()),
        );
        assert_eq!(max_out[0].get("max_n"), Some(&Value::Integer(5)));
    }

    #[test]
    fn min_on_empty_stream_is_null() {
        let input = stream(vec![]);
        let stage = MinStage {
            field: "n".to_string(),
        };
        let out = collect_ok(stage.process(input));
        assert_eq!(out[0].get("min_n"), Some(&Value::Null));
    }

    #[test]
    fn sort_ascending() {
        let input = stream(vec![
            rec(&[("n", Value::Integer(3))]),
            rec(&[("n", Value::Integer(1))]),
            rec(&[("n", Value::Integer(2))]),
        ]);
        let stage = SortStage {
            field: "n".to_string(),
            desc: false,
        };
        let out = collect_ok(stage.process(input));
        let values: Vec<_> = out.iter().map(|r| r.get("n").cloned().unwrap()).collect();
        assert_eq!(
            values,
            vec![Value::Integer(1), Value::Integer(2), Value::Integer(3)]
        );
    }

    #[test]
    fn sort_descending() {
        let input = stream(vec![
            rec(&[("n", Value::Integer(1))]),
            rec(&[("n", Value::Integer(3))]),
            rec(&[("n", Value::Integer(2))]),
        ]);
        let stage = SortStage {
            field: "n".to_string(),
            desc: true,
        };
        let out = collect_ok(stage.process(input));
        let values: Vec<_> = out.iter().map(|r| r.get("n").cloned().unwrap()).collect();
        assert_eq!(
            values,
            vec![Value::Integer(3), Value::Integer(2), Value::Integer(1)]
        );
    }

    #[test]
    fn sort_on_empty_stream_yields_nothing() {
        let input = stream(vec![]);
        let stage = SortStage {
            field: "n".to_string(),
            desc: false,
        };
        assert_eq!(collect_ok(stage.process(input)).len(), 0);
    }

    #[test]
    fn sort_across_multiple_external_chunks() {
        // Exercise the external-merge path by exceeding the 50_000-record chunk size.
        let n: i64 = 60_000;
        let mut records = Vec::with_capacity(n as usize);
        for i in (0..n).rev() {
            records.push(rec(&[("n", Value::Integer(i))]));
        }
        let input = stream(records);
        let stage = SortStage {
            field: "n".to_string(),
            desc: false,
        };
        let out = collect_ok(stage.process(input));
        assert_eq!(out.len(), n as usize);
        for (i, r) in out.iter().enumerate() {
            assert_eq!(r.get("n"), Some(&Value::Integer(i as i64)));
        }
    }

    #[test]
    fn explode_expands_array_field() {
        let input = stream(vec![rec(&[
            ("id", Value::Integer(1)),
            (
                "tags",
                Value::Array(vec![
                    Value::String("a".to_string()),
                    Value::String("b".to_string()),
                ]),
            ),
        ])]);
        let stage = ExplodeStage {
            field: "tags".to_string(),
        };
        let out = collect_ok(stage.process(input));
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].get("tags"), Some(&Value::String("a".to_string())));
        assert_eq!(out[1].get("tags"), Some(&Value::String("b".to_string())));
    }

    #[test]
    fn explode_passes_through_non_array_field_unchanged() {
        let input = stream(vec![rec(&[("id", Value::Integer(1))])]);
        let stage = ExplodeStage {
            field: "tags".to_string(),
        };
        let out = collect_ok(stage.process(input));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].get("id"), Some(&Value::Integer(1)));
    }

    #[test]
    fn map_stage_sets_computed_field() {
        let input = stream(vec![rec(&[("n", Value::Integer(5))])]);
        let stage = MapStage {
            field: "double".to_string(),
            ast: crate::expr::parse(".n * 2").unwrap(),
        };
        let out = collect_ok(stage.process(input));
        assert_eq!(out[0].get("double"), Some(&Value::Integer(10)));
    }

    #[test]
    fn unique_keeps_first_occurrence_per_value() {
        let input = stream(vec![
            rec(&[("id", Value::Integer(1))]),
            rec(&[("id", Value::Integer(2))]),
            rec(&[("id", Value::Integer(1))]),
        ]);
        let stage = UniqueStage {
            field: "id".to_string(),
        };
        let out = collect_ok(stage.process(input));
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].get("id"), Some(&Value::Integer(1)));
        assert_eq!(out[1].get("id"), Some(&Value::Integer(2)));
    }

    #[test]
    fn schema_infers_field_types() {
        let input = stream(vec![
            rec(&[
                ("name", Value::String("a".to_string())),
                ("age", Value::Integer(1)),
            ]),
            rec(&[
                ("name", Value::String("b".to_string())),
                ("age", Value::Null),
            ]),
        ]);
        let out = collect_ok(SchemaStage.process(input));
        assert_eq!(
            out[0].get("name"),
            Some(&Value::String("string".to_string()))
        );
        assert_eq!(
            out[0].get("age"),
            Some(&Value::String("integer | null".to_string()))
        );
    }

    #[test]
    fn group_by_counts_and_sums_per_key() {
        let input = stream(vec![
            rec(&[
                ("category", Value::String("a".to_string())),
                ("amount", Value::Integer(10)),
            ]),
            rec(&[
                ("category", Value::String("a".to_string())),
                ("amount", Value::Integer(5)),
            ]),
            rec(&[
                ("category", Value::String("b".to_string())),
                ("amount", Value::Integer(1)),
            ]),
        ]);
        let stage = GroupStage {
            by: "category".to_string(),
            sum: Some("amount".to_string()),
            count: true,
        };
        let out = collect_ok(stage.process(input));
        assert_eq!(out.len(), 2);

        let group_a = out
            .iter()
            .find(|r| r.get("category") == Some(&Value::String("a".to_string())))
            .unwrap();
        assert_eq!(group_a.get("count"), Some(&Value::Integer(2)));
        assert_eq!(group_a.get("sum_amount"), Some(&Value::Integer(15)));

        let group_b = out
            .iter()
            .find(|r| r.get("category") == Some(&Value::String("b".to_string())))
            .unwrap();
        assert_eq!(group_b.get("count"), Some(&Value::Integer(1)));
        assert_eq!(group_b.get("sum_amount"), Some(&Value::Integer(1)));
    }

    #[test]
    fn join_merges_matching_right_record_fields() {
        let mut right = std::collections::HashMap::new();
        right.insert(
            "1".to_string(),
            rec(&[
                ("id", Value::String("1".to_string())),
                ("name", Value::String("Alice".to_string())),
            ]),
        );
        let stage = JoinStage {
            hash_map: std::sync::Arc::new(right),
            on: "id".to_string(),
        };

        let input = stream(vec![rec(&[
            ("id", Value::String("1".to_string())),
            ("order", Value::Integer(100)),
        ])]);
        let out = collect_ok(stage.process(input));
        assert_eq!(
            out[0].get("name"),
            Some(&Value::String("Alice".to_string()))
        );
        assert_eq!(out[0].get("order"), Some(&Value::Integer(100)));
    }

    #[test]
    fn join_leaves_record_unchanged_when_no_match() {
        let right: std::collections::HashMap<String, Record> = std::collections::HashMap::new();
        let stage = JoinStage {
            hash_map: std::sync::Arc::new(right),
            on: "id".to_string(),
        };

        let input = stream(vec![rec(&[("id", Value::String("1".to_string()))])]);
        let out = collect_ok(stage.process(input));
        assert_eq!(out[0].get("id"), Some(&Value::String("1".to_string())));
        assert_eq!(out[0].len(), 1);
    }
}
