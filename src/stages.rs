use crate::model::{Record, Value};
use crate::pipeline::{RecordStream, Stage};

/// Parses a comma-separated field list (`"a,b,c"`), used by `unique`,
/// `group by`, and `join --on` wherever multiple fields form a composite
/// key. Rejects empty entries (e.g. a trailing comma or `"a,,b"`) up front
/// rather than silently treating them as a field named `""`.
pub(crate) fn parse_field_list(spec: &str) -> anyhow::Result<Vec<String>> {
    let fields: Vec<String> = spec.split(',').map(|s| s.trim().to_string()).collect();
    if fields.iter().any(|f| f.is_empty()) {
        return Err(anyhow::anyhow!(
            "Invalid field list '{}': fields must be non-empty and comma-separated",
            spec
        ));
    }
    Ok(fields)
}

/// Parses a `sort`-style field spec: comma-separated fields, each optionally
/// suffixed with `:desc` or `:asc` (ascending is the default when omitted),
/// e.g. `"age"`, `"age:desc"`, or `"country,age:desc"`.
pub(crate) fn parse_sort_spec(spec: &str) -> anyhow::Result<Vec<(String, bool)>> {
    spec.split(',')
        .map(|part| {
            let part = part.trim();
            if let Some(name) = part.strip_suffix(":desc") {
                if name.is_empty() {
                    return Err(anyhow::anyhow!(
                        "Invalid sort spec '{}': empty field name before ':desc'",
                        spec
                    ));
                }
                Ok((name.to_string(), true))
            } else if let Some(name) = part.strip_suffix(":asc") {
                if name.is_empty() {
                    return Err(anyhow::anyhow!(
                        "Invalid sort spec '{}': empty field name before ':asc'",
                        spec
                    ));
                }
                Ok((name.to_string(), false))
            } else if part.is_empty() {
                Err(anyhow::anyhow!(
                    "Invalid sort spec '{}': fields must be non-empty and comma-separated",
                    spec
                ))
            } else {
                Ok((part.to_string(), false))
            }
        })
        .collect()
}

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
    /// When true, `fields` is an exclusion list (keep everything except
    /// these) instead of the default inclusion list.
    pub exclude: bool,
}

impl Stage for SelectStage {
    fn process<'a>(&'a self, input: RecordStream<'a>) -> RecordStream<'a> {
        let fields = self.fields.clone();
        let exclude = self.exclude;

        let mapped = input.map(move |res| {
            res.map(|record| {
                if exclude {
                    record
                        .into_iter()
                        .filter(|(k, _)| !fields.contains(k))
                        .collect()
                } else {
                    let mut new_record = indexmap::IndexMap::new();
                    for field in &fields {
                        let val = record.get(field).cloned().unwrap_or(Value::Null);
                        new_record.insert(field.clone(), val);
                    }
                    new_record
                }
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
        let mut count = 0i64;
        for res in input {
            match res {
                Ok(_) => count += 1,
                Err(e) => return Box::new(std::iter::once(Err(e))),
            }
        }
        let mut rec = indexmap::IndexMap::new();
        rec.insert("count".to_string(), Value::Integer(count));
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
            let rec = match res {
                Ok(rec) => rec,
                Err(e) => return Box::new(std::iter::once(Err(e))),
            };
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

        for res in input {
            let rec = match res {
                Ok(rec) => rec,
                Err(e) => return Box::new(std::iter::once(Err(e))),
            };
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

        for res in input {
            let rec = match res {
                Ok(rec) => rec,
                Err(e) => return Box::new(std::iter::once(Err(e))),
            };
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

        for res in input {
            let rec = match res {
                Ok(rec) => rec,
                Err(e) => return Box::new(std::iter::once(Err(e))),
            };
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
    /// Each entry is `(field, desc)`; earlier entries take precedence as
    /// the primary sort key, later ones only break ties.
    pub fields: Vec<(String, bool)>,
}

/// Compares two records across a multi-field sort spec, stopping at the
/// first field that isn't equal (standard lexicographic tie-breaking).
fn cmp_by_sort_fields(a: &Record, b: &Record, fields: &[(String, bool)]) -> std::cmp::Ordering {
    for (field, desc) in fields {
        let val_a = a.get(field).unwrap_or(&Value::Null);
        let val_b = b.get(field).unwrap_or(&Value::Null);
        let mut ord = crate::model::cmp_values(val_a, val_b);
        if *desc {
            ord = ord.reverse();
        }
        if ord != std::cmp::Ordering::Equal {
            return ord;
        }
    }
    std::cmp::Ordering::Equal
}

pub(crate) struct HeapItem {
    record: Record,
    file_idx: usize,
    // Shared rather than cloned per item: field lists are small, but a
    // BinaryHeap pushes/pops one HeapItem per record, so this avoids a
    // Vec<(String, bool)> allocation on every single push.
    fields: std::rc::Rc<Vec<(String, bool)>>,
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
        let ord = cmp_by_sort_fields(&self.record, &other.record, &self.fields);
        // Reverse because BinaryHeap is a MAX heap, and we want a MIN heap for K-way merge
        ord.reverse()
    }
}

pub struct ExternalSortIter<'a> {
    pub readers: Vec<RecordStream<'a>>,
    pub(crate) heap: std::collections::BinaryHeap<HeapItem>,
    pub(crate) fields: std::rc::Rc<Vec<(String, bool)>>,
    pub initialized: bool,
    // Kept alive for the full duration of reading (rather than dropped right
    // after opening each file) so temp-file cleanup happens deterministically
    // once every reader is done with it, instead of relying on platform-
    // specific delete-while-open semantics.
    pub(crate) _temp_files: Vec<tempfile::TempPath>,
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
                        fields: std::rc::Rc::clone(&self.fields),
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
                    fields: std::rc::Rc::clone(&self.fields),
                });
            }
            return Some(Ok(record));
        }
        None
    }
}

impl Stage for SortStage {
    fn process<'a>(&'a self, input: RecordStream<'a>) -> RecordStream<'a> {
        external_sort(input, self.fields.clone())
    }
}

/// The actual external-merge-sort logic, factored out of `SortStage::process`
/// as a free function so it can also be used to pre-sort a `'static` stream
/// (e.g. `join --merge`'s right-hand file) without being saddled with
/// `Stage::process`'s `&'a self` signature, which would otherwise force the
/// returned stream's lifetime to match a local `SortStage` value's lifetime
/// even though the implementation never actually borrows from `self`.
pub(crate) fn external_sort<'a>(
    mut input: RecordStream<'a>,
    fields: Vec<(String, bool)>,
) -> RecordStream<'a> {
    let fields = std::rc::Rc::new(fields);
    let mut temp_files = Vec::new();

    loop {
        let mut chunk = Vec::with_capacity(50_000);
        for _ in 0..50_000 {
            match input.next() {
                Some(Ok(rec)) => chunk.push(rec),
                Some(Err(e)) => return Box::new(std::iter::once(Err(e))),
                None => break,
            }
        }
        if chunk.is_empty() {
            break;
        }

        chunk.sort_by(|a, b| cmp_by_sort_fields(a, b, &fields));

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
    for path in &temp_files {
        let file = std::fs::File::open(path).unwrap();
        let reader = std::io::BufReader::new(file);
        let stream = crate::io::read_json_stream(reader);
        readers.push(Box::new(stream));
    }

    Box::new(ExternalSortIter {
        readers,
        heap: std::collections::BinaryHeap::new(),
        fields,
        initialized: false,
        _temp_files: temp_files,
    })
}

/// Ordered the same way `cmp_by_sort_fields` orders records (not reversed,
/// unlike `HeapItem`), so a plain `BinaryHeap`'s max is exactly the
/// "worst" record currently being kept - the one to evict first when a
/// better candidate arrives.
struct TopNItem {
    record: Record,
    fields: std::rc::Rc<Vec<(String, bool)>>,
}

impl PartialEq for TopNItem {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == std::cmp::Ordering::Equal
    }
}
impl Eq for TopNItem {}
impl PartialOrd for TopNItem {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for TopNItem {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        cmp_by_sort_fields(&self.record, &other.record, &self.fields)
    }
}

/// Keeps only the top `n` records by a `sort`-style field spec, without
/// buffering or sorting the whole stream: a bounded max-heap of at most
/// `n` items is maintained (O(n log k) time, O(k) memory where k = min(n,
/// stream length)), evicting the current worst-kept record whenever a
/// better one arrives once the heap is full. Equivalent to `sort <fields>
/// | limit <n>` in result, but doesn't need `sort`'s external-merge
/// temp-file spilling since it never holds more than `n` records at once.
pub struct TopNStage {
    pub fields: Vec<(String, bool)>,
    pub n: usize,
}

impl Stage for TopNStage {
    fn process<'a>(&'a self, input: RecordStream<'a>) -> RecordStream<'a> {
        let fields = std::rc::Rc::new(self.fields.clone());
        let n = self.n;
        let mut heap: std::collections::BinaryHeap<TopNItem> =
            std::collections::BinaryHeap::with_capacity(n.saturating_add(1));

        for res in input {
            let record = match res {
                Ok(rec) => rec,
                Err(e) => return Box::new(std::iter::once(Err(e))),
            };
            if n == 0 {
                continue;
            }
            if heap.len() < n {
                heap.push(TopNItem {
                    record,
                    fields: std::rc::Rc::clone(&fields),
                });
            } else if let Some(worst) = heap.peek() {
                if cmp_by_sort_fields(&record, &worst.record, &fields) == std::cmp::Ordering::Less {
                    heap.pop();
                    heap.push(TopNItem {
                        record,
                        fields: std::rc::Rc::clone(&fields),
                    });
                }
            }
        }

        let mut items: Vec<Record> = heap.into_iter().map(|item| item.record).collect();
        items.sort_by(|a, b| cmp_by_sort_fields(a, b, &fields));
        Box::new(items.into_iter().map(Ok))
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
    /// One or more `(field, expression)` assignments, applied in order.
    /// Later assignments can reference fields set by earlier ones in the
    /// same `map` invocation, since each is evaluated against the record
    /// as it stands after the previous assignment was applied.
    pub assignments: Vec<(String, crate::expr::Expr)>,
}

impl Stage for MapStage {
    fn process<'a>(&'a self, input: RecordStream<'a>) -> RecordStream<'a> {
        let assignments = self.assignments.clone();
        let iter = input.map(move |res| match res {
            Ok(mut record) => {
                for (field, ast) in &assignments {
                    let new_val = ast.evaluate(&record);
                    record.insert(field.clone(), new_val);
                }
                Ok(record)
            }
            Err(e) => Err(e),
        });
        Box::new(iter)
    }
}

pub struct UniqueStage {
    /// Distinctness is keyed on the combination of these fields.
    pub fields: Vec<String>,
}

impl Stage for UniqueStage {
    fn process<'a>(&'a self, input: RecordStream<'a>) -> RecordStream<'a> {
        let fields = self.fields.clone();
        let mut seen = std::collections::HashSet::new();

        let filtered = input.filter_map(move |res| match res {
            Ok(record) => {
                // A control character (never realistically part of a field's
                // own string content) joins each field's serialized value,
                // so a composite key can't collide across a field-count
                // boundary the way naive string concatenation could (e.g.
                // ("ab","c") vs ("a","bc")).
                let key: String = fields
                    .iter()
                    .map(|f| {
                        let val = record.get(f).unwrap_or(&Value::Null);
                        serde_json::to_string(val).unwrap_or_default()
                    })
                    .collect::<Vec<_>>()
                    .join("\u{1}");
                if seen.contains(&key) {
                    None
                } else {
                    seen.insert(key);
                    Some(Ok(record))
                }
            }
            Err(e) => Some(Err(e)),
        });

        Box::new(filtered)
    }
}

pub struct DedupStage;

impl Stage for DedupStage {
    fn process<'a>(&'a self, input: RecordStream<'a>) -> RecordStream<'a> {
        let mut seen = std::collections::HashSet::new();

        let filtered = input.filter_map(move |res| match res {
            Ok(record) => {
                // Whole-record equality via serialization, same approach
                // UniqueStage already uses per-field. Note this means two
                // records with identical fields in a different insertion
                // order are NOT considered duplicates - a rare edge case in
                // practice (homogeneous field order is the norm for both
                // CSV and typical JSONL), not worth the extra complexity of
                // a order-independent comparison for this scope.
                let key = serde_json::to_string(&record).unwrap_or_default();
                if seen.contains(&key) {
                    None
                } else {
                    seen.insert(key);
                    Some(Ok(record))
                }
            }
            Err(e) => Some(Err(e)),
        });

        Box::new(filtered)
    }
}

/// Keeps records where at least one field's value (rendered the same way
/// `csv`/`table` output does) contains the search text or matches the
/// regex. Exactly one of `literal`/`regex` is set; the regex, if any, is
/// compiled once by the caller (not per-record), same principle as the
/// `matches()` expression function.
pub struct SearchStage {
    pub literal: Option<String>,
    pub regex: Option<regex::Regex>,
}

impl Stage for SearchStage {
    fn process<'a>(&'a self, input: RecordStream<'a>) -> RecordStream<'a> {
        let literal = self.literal.clone();
        let regex = self.regex.clone();

        let filtered = input.filter_map(move |res| match res {
            Ok(record) => {
                let matched = record.values().any(|v| {
                    let s = crate::io::value_to_display_string(v);
                    if let Some(re) = &regex {
                        re.is_match(&s)
                    } else if let Some(lit) = &literal {
                        s.contains(lit.as_str())
                    } else {
                        false
                    }
                });
                if matched {
                    Some(Ok(record))
                } else {
                    None
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

        for res in input.take(10_000) {
            let rec = match res {
                Ok(rec) => rec,
                Err(e) => return Box::new(std::iter::once(Err(e))),
            };
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

#[derive(Default)]
struct FieldStats {
    /// Records where this field was present at all (any value, including null).
    count: i64,
    /// Records where this field was present and explicitly null.
    null_count: i64,
    numeric_count: i64,
    sum: f64,
    sum_sq: f64,
    min: Option<Value>,
    max: Option<Value>,
    /// Memory usage is proportional to the number of *distinct* values seen
    /// for this field, same tradeoff as `unique`/`group` - documented in
    /// the README rather than bounded, since typical field cardinality is
    /// small relative to stream length.
    distinct: std::collections::HashSet<String>,
}

pub struct StatsStage;

impl Stage for StatsStage {
    fn process<'a>(&'a self, input: RecordStream<'a>) -> RecordStream<'a> {
        let mut stats: indexmap::IndexMap<String, FieldStats> = indexmap::IndexMap::new();

        for res in input {
            let rec = match res {
                Ok(rec) => rec,
                Err(e) => return Box::new(std::iter::once(Err(e))),
            };
            for (key, val) in rec {
                let entry = stats.entry(key).or_default();
                entry.count += 1;

                match &val {
                    Value::Null => entry.null_count += 1,
                    Value::Integer(i) => {
                        entry.numeric_count += 1;
                        entry.sum += *i as f64;
                        entry.sum_sq += (*i as f64).powi(2);
                    }
                    Value::Float(f) => {
                        entry.numeric_count += 1;
                        entry.sum += f;
                        entry.sum_sq += f.powi(2);
                    }
                    _ => {}
                }

                if !matches!(val, Value::Null) {
                    entry.min = Some(match entry.min.take() {
                        Some(cur)
                            if crate::model::cmp_values(&val, &cur) != std::cmp::Ordering::Less =>
                        {
                            cur
                        }
                        _ => val.clone(),
                    });
                    entry.max = Some(match entry.max.take() {
                        Some(cur)
                            if crate::model::cmp_values(&val, &cur)
                                != std::cmp::Ordering::Greater =>
                        {
                            cur
                        }
                        _ => val.clone(),
                    });
                }

                entry
                    .distinct
                    .insert(serde_json::to_string(&val).unwrap_or_default());
            }
        }

        let mut output = Vec::new();
        for (field, s) in stats {
            let mut rec = indexmap::IndexMap::new();
            rec.insert("field".to_string(), Value::String(field));
            rec.insert("count".to_string(), Value::Integer(s.count));
            rec.insert("nulls".to_string(), Value::Integer(s.null_count));
            rec.insert(
                "distinct".to_string(),
                Value::Integer(s.distinct.len() as i64),
            );
            rec.insert("min".to_string(), s.min.unwrap_or(Value::Null));
            rec.insert("max".to_string(), s.max.unwrap_or(Value::Null));
            if s.numeric_count > 0 {
                let mean = s.sum / s.numeric_count as f64;
                let variance = (s.sum_sq / s.numeric_count as f64 - mean * mean).max(0.0);
                rec.insert("mean".to_string(), Value::Float(mean));
                rec.insert("stddev".to_string(), Value::Float(variance.sqrt()));
            } else {
                rec.insert("mean".to_string(), Value::Null);
                rec.insert("stddev".to_string(), Value::Null);
            }
            output.push(Ok(rec));
        }
        Box::new(output.into_iter())
    }
}

pub struct GroupStage {
    /// Grouping key is the combination of these fields.
    pub by: Vec<String>,
    pub sum: Option<String>,
    pub count: bool,
}

impl Stage for GroupStage {
    fn process<'a>(&'a self, input: RecordStream<'a>) -> RecordStream<'a> {
        let by = self.by.clone();
        let sum_field = self.sum.clone();
        let do_count = self.count;

        // Keyed on a composite string (one entry per distinct combination
        // of `by` values), but the original, untruncated Values for each
        // `by` field are kept alongside so the output preserves their real
        // type (e.g. an integer group key stays an integer) instead of
        // being flattened to a string.
        let mut groups: indexmap::IndexMap<String, (Vec<Value>, i64, f64, i64, bool)> =
            indexmap::IndexMap::new();

        for res in input {
            let rec = match res {
                Ok(rec) => rec,
                Err(e) => return Box::new(std::iter::once(Err(e))),
            };
            let key_values: Vec<Value> = by
                .iter()
                .map(|f| rec.get(f).cloned().unwrap_or(Value::Null))
                .collect();
            // Same control-character join as UniqueStage, to avoid
            // composite-key collisions across a field-count boundary.
            let group_key = key_values
                .iter()
                .map(|v| serde_json::to_string(v).unwrap_or_default())
                .collect::<Vec<_>>()
                .join("\u{1}");

            let entry = groups
                .entry(group_key)
                .or_insert_with(|| (key_values, 0, 0.0, 0, false));
            entry.3 += 1;

            if let Some(ref field) = sum_field {
                if let Some(val) = rec.get(field) {
                    match val {
                        Value::Integer(i) => {
                            if entry.4 {
                                entry.2 += *i as f64;
                            } else {
                                entry.1 += i;
                            }
                        }
                        Value::Float(f) => {
                            if !entry.4 {
                                entry.4 = true;
                                entry.2 = entry.1 as f64;
                            }
                            entry.2 += f;
                        }
                        _ => {}
                    }
                }
            }
        }

        let mut output = Vec::new();
        for (_, (key_values, sum_int, sum_float, count, is_float)) in groups {
            let mut rec = indexmap::IndexMap::new();
            for (field_name, val) in by.iter().zip(key_values) {
                rec.insert(field_name.clone(), val);
            }
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

pub struct FreqStage {
    pub field: String,
    /// Keep only the top N most frequent values.
    pub limit: Option<usize>,
}

impl Stage for FreqStage {
    fn process<'a>(&'a self, input: RecordStream<'a>) -> RecordStream<'a> {
        let field = self.field.clone();
        let mut counts: indexmap::IndexMap<String, i64> = indexmap::IndexMap::new();
        let mut total: i64 = 0;

        for res in input {
            let rec = match res {
                Ok(rec) => rec,
                Err(e) => return Box::new(std::iter::once(Err(e))),
            };
            let key = match rec.get(&field) {
                Some(Value::String(s)) => s.clone(),
                Some(val) => serde_json::to_string(val).unwrap_or_default(),
                None => "null".to_string(),
            };
            *counts.entry(key).or_insert(0) += 1;
            total += 1;
        }

        // Stable sort descending by count: ties keep first-seen order.
        let mut pairs: Vec<(String, i64)> = counts.into_iter().collect();
        pairs.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
        if let Some(n) = self.limit {
            pairs.truncate(n);
        }

        let output: Vec<_> = pairs
            .into_iter()
            .map(|(value, count)| {
                let mut rec = indexmap::IndexMap::new();
                rec.insert("value".to_string(), Value::String(value));
                rec.insert("count".to_string(), Value::Integer(count));
                let percent = if total > 0 {
                    (count as f64 / total as f64) * 100.0
                } else {
                    0.0
                };
                rec.insert("percent".to_string(), Value::Float(percent));
                Ok(rec)
            })
            .collect();

        Box::new(output.into_iter())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum, serde::Deserialize)]
#[clap(rename_all = "kebab-case")]
#[serde(rename_all = "lowercase")]
pub enum JoinType {
    /// Keep every left record; merge matching right fields when found.
    Left,
    /// Keep only left records that have a matching right record.
    Inner,
    /// Keep every right record; merge matching left fields when found.
    Right,
    /// Keep every left AND every right record, matched where possible.
    Full,
}

pub struct JoinStage {
    pub hash_map: std::sync::Arc<std::collections::HashMap<String, Record>>,
    pub on: Vec<String>,
    pub join_type: JoinType,
}

/// Builds the hash-join key for a record across one or more `on` fields.
/// Returns `None` (no possible match) if *any* of the fields is missing -
/// same policy the single-field version always had. For multiple fields,
/// each field's encoded value is joined with a control character that
/// can't realistically appear in real field content, so a composite key
/// can't collide across a field-count boundary the way naive
/// concatenation could.
pub(crate) fn join_key_for(record: &Record, on: &[String]) -> Option<String> {
    let mut parts = Vec::with_capacity(on.len());
    for field in on {
        match record.get(field) {
            Some(Value::String(s)) => parts.push(s.clone()),
            Some(val) => parts.push(serde_json::to_string(val).unwrap_or_default()),
            None => return None,
        }
    }
    Some(parts.join("\u{1}"))
}

/// Streams left-side records first (merging in matching right-side fields,
/// dropping or keeping unmatched lefts per `join_type`), then - for `Right`
/// and `Full` - emits any right-side records that were never matched, once
/// the left stream is exhausted. Tracking "which right keys matched" can
/// only be known once the left stream is fully drained, so the right-only
/// tail is computed lazily on first request rather than eagerly up front.
struct JoinIter<'a> {
    left: RecordStream<'a>,
    hash_map: std::sync::Arc<std::collections::HashMap<String, Record>>,
    on: Vec<String>,
    join_type: JoinType,
    matched_keys: std::collections::HashSet<String>,
    right_tail: Option<std::vec::IntoIter<anyhow::Result<Record>>>,
}

impl<'a> Iterator for JoinIter<'a> {
    type Item = anyhow::Result<Record>;
    fn next(&mut self) -> Option<Self::Item> {
        if let Some(tail) = &mut self.right_tail {
            return tail.next();
        }

        for res in self.left.by_ref() {
            let mut record = match res {
                Ok(r) => r,
                Err(e) => return Some(Err(e)),
            };
            let join_key = join_key_for(&record, &self.on);
            let right_match = join_key.as_ref().and_then(|k| self.hash_map.get(k));

            match right_match {
                Some(right_record) => {
                    if let Some(k) = join_key {
                        self.matched_keys.insert(k);
                    }
                    for (k, v) in right_record {
                        if !self.on.contains(k) {
                            record.insert(k.clone(), v.clone());
                        }
                    }
                    return Some(Ok(record));
                }
                None => {
                    if matches!(self.join_type, JoinType::Inner | JoinType::Right) {
                        continue; // drop unmatched left record
                    }
                    return Some(Ok(record));
                }
            }
        }

        // Left stream exhausted. For Right/Full, emit right-side records
        // that were never matched by any left record.
        if matches!(self.join_type, JoinType::Right | JoinType::Full) {
            let leftover: Vec<_> = self
                .hash_map
                .iter()
                .filter(|(k, _)| !self.matched_keys.contains(*k))
                .map(|(_, rec)| Ok(rec.clone()))
                .collect();
            let mut tail = leftover.into_iter();
            let first = tail.next();
            self.right_tail = Some(tail);
            first
        } else {
            None
        }
    }
}

impl Stage for JoinStage {
    fn process<'a>(&'a self, input: RecordStream<'a>) -> RecordStream<'a> {
        Box::new(JoinIter {
            left: input,
            hash_map: std::sync::Arc::clone(&self.hash_map),
            on: self.on.clone(),
            join_type: self.join_type,
            matched_keys: std::collections::HashSet::new(),
            right_tail: None,
        })
    }
}

fn join_key_values(record: &Record, on: &[String]) -> Vec<Value> {
    on.iter()
        .map(|f| record.get(f).cloned().unwrap_or(Value::Null))
        .collect()
}

/// Lexicographic comparison across a composite join key (one `Value` per
/// `on` field), stopping at the first field that differs.
fn cmp_key_values(a: &[Value], b: &[Value]) -> std::cmp::Ordering {
    for (av, bv) in a.iter().zip(b.iter()) {
        let ord = crate::model::cmp_values(av, bv);
        if ord != std::cmp::Ordering::Equal {
            return ord;
        }
    }
    std::cmp::Ordering::Equal
}

/// `join --merge`: a streaming sort-merge join, used instead of `JoinStage`'s
/// hash join when the right-hand file might be too large to hold entirely in
/// memory. Both sides are sorted by the join key first (`external_sort`,
/// bounded memory via temp-file spilling - the main stream is sorted lazily
/// inside `process`, the right-hand file is pre-sorted once at construction
/// time since it doesn't depend on the main stream). The merge itself then
/// only needs to buffer one key's worth of duplicates at a time on each
/// side, not the whole file.
///
/// Note a deliberate behavioral difference from the hash join: `JoinStage`
/// keeps only the *last* right-hand record for a duplicate key (a HashMap
/// insert overwrites earlier ones); `MergeJoinStage` instead produces the
/// full cross product for duplicate keys on either side, which is the
/// textbook-correct sort-merge join behavior. Documented in the README
/// rather than papered over, since it's a real difference a user might rely
/// on one way or the other.
pub struct MergeJoinStage {
    pub on: Vec<String>,
    pub join_type: JoinType,
    right_sorted: std::cell::RefCell<Option<RecordStream<'static>>>,
}

impl MergeJoinStage {
    pub fn new(on: Vec<String>, join_type: JoinType, right_sorted: RecordStream<'static>) -> Self {
        Self {
            on,
            join_type,
            right_sorted: std::cell::RefCell::new(Some(right_sorted)),
        }
    }
}

impl Stage for MergeJoinStage {
    fn process<'a>(&'a self, input: RecordStream<'a>) -> RecordStream<'a> {
        let sort_fields: Vec<(String, bool)> = self.on.iter().map(|f| (f.clone(), false)).collect();
        let left_sorted = external_sort(input, sort_fields);
        let right_sorted = self
            .right_sorted
            .borrow_mut()
            .take()
            .expect("MergeJoinStage::process called more than once");
        Box::new(MergeJoinIter {
            left: left_sorted.peekable(),
            right: right_sorted.peekable(),
            on: self.on.clone(),
            join_type: self.join_type,
            queue: std::collections::VecDeque::new(),
        })
    }
}

/// Drives the merge step by step: each call to `step()` is a complete,
/// self-contained unit of work (advance past one key, or one whole matching
/// key-group) that pushes zero or more ready records into `queue`, which
/// `next()` then drains before running another step. This avoids having to
/// hand-roll a resumable state machine across `next()` calls - each step
/// either fully finishes its unit of work or doesn't start one, so there's
/// no partial-progress state to track between calls.
struct MergeJoinIter<'a> {
    left: std::iter::Peekable<RecordStream<'a>>,
    right: std::iter::Peekable<RecordStream<'a>>,
    on: Vec<String>,
    join_type: JoinType,
    queue: std::collections::VecDeque<anyhow::Result<Record>>,
}

impl<'a> MergeJoinIter<'a> {
    /// Returns `false` only when both sides are exhausted and there is
    /// truly nothing left to do.
    fn step(&mut self) -> bool {
        match (self.left.peek(), self.right.peek()) {
            (None, None) => false,
            (Some(_), None) => {
                match self.left.next().unwrap() {
                    Err(e) => self.queue.push_back(Err(e)),
                    Ok(rec) => {
                        if matches!(self.join_type, JoinType::Left | JoinType::Full) {
                            self.queue.push_back(Ok(rec));
                        }
                    }
                }
                true
            }
            (None, Some(_)) => {
                match self.right.next().unwrap() {
                    Err(e) => self.queue.push_back(Err(e)),
                    Ok(rec) => {
                        if matches!(self.join_type, JoinType::Right | JoinType::Full) {
                            self.queue.push_back(Ok(rec));
                        }
                    }
                }
                true
            }
            (Some(Err(_)), _) => {
                self.queue
                    .push_back(Err(self.left.next().unwrap().unwrap_err()));
                true
            }
            (_, Some(Err(_))) => {
                self.queue
                    .push_back(Err(self.right.next().unwrap().unwrap_err()));
                true
            }
            (Some(Ok(l)), Some(Ok(r))) => {
                let lk = join_key_values(l, &self.on);
                let rk = join_key_values(r, &self.on);
                match cmp_key_values(&lk, &rk) {
                    std::cmp::Ordering::Less => {
                        let rec = self.left.next().unwrap().unwrap();
                        if matches!(self.join_type, JoinType::Left | JoinType::Full) {
                            self.queue.push_back(Ok(rec));
                        }
                        true
                    }
                    std::cmp::Ordering::Greater => {
                        let rec = self.right.next().unwrap().unwrap();
                        if matches!(self.join_type, JoinType::Right | JoinType::Full) {
                            self.queue.push_back(Ok(rec));
                        }
                        true
                    }
                    std::cmp::Ordering::Equal => {
                        let key = lk;
                        let mut left_group = Vec::new();
                        while let Some(Ok(rec)) = self.left.peek() {
                            if cmp_key_values(&join_key_values(rec, &self.on), &key)
                                != std::cmp::Ordering::Equal
                            {
                                break;
                            }
                            left_group.push(self.left.next().unwrap().unwrap());
                        }
                        let mut right_group = Vec::new();
                        while let Some(Ok(rec)) = self.right.peek() {
                            if cmp_key_values(&join_key_values(rec, &self.on), &key)
                                != std::cmp::Ordering::Equal
                            {
                                break;
                            }
                            right_group.push(self.right.next().unwrap().unwrap());
                        }
                        for l in &left_group {
                            for r in &right_group {
                                let mut merged = l.clone();
                                for (k, v) in r {
                                    if !self.on.contains(k) {
                                        merged.insert(k.clone(), v.clone());
                                    }
                                }
                                self.queue.push_back(Ok(merged));
                            }
                        }
                        true
                    }
                }
            }
        }
    }
}

impl<'a> Iterator for MergeJoinIter<'a> {
    type Item = anyhow::Result<Record>;
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(item) = self.queue.pop_front() {
                return Some(item);
            }
            if !self.step() {
                return None;
            }
        }
    }
}

pub struct RenameStage {
    /// (old_name, new_name) pairs.
    pub renames: Vec<(String, String)>,
}

impl Stage for RenameStage {
    fn process<'a>(&'a self, input: RecordStream<'a>) -> RecordStream<'a> {
        let renames = self.renames.clone();
        let iter = input.map(move |res| {
            res.map(|record| {
                let mut new_record = indexmap::IndexMap::new();
                for (key, val) in record {
                    let new_key = renames
                        .iter()
                        .find(|(old, _)| old == &key)
                        .map(|(_, new)| new.clone())
                        .unwrap_or(key);
                    new_record.insert(new_key, val);
                }
                new_record
            })
        });
        Box::new(iter)
    }
}

pub struct FlattenStage {
    pub separator: String,
}

fn flatten_into(
    out: &mut Record,
    prefix: &str,
    map: &indexmap::IndexMap<String, Value>,
    sep: &str,
) {
    for (key, val) in map {
        let flat_key = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}{sep}{key}")
        };
        match val {
            // Only nested objects are flattened; arrays are left as-is (use
            // `explode` for those) - flattening arrays would require
            // index-based keys, a different and separately useful operation.
            Value::Object(inner) => flatten_into(out, &flat_key, inner, sep),
            other => {
                out.insert(flat_key, other.clone());
            }
        }
    }
}

impl Stage for FlattenStage {
    fn process<'a>(&'a self, input: RecordStream<'a>) -> RecordStream<'a> {
        let sep = self.separator.clone();
        let iter = input.map(move |res| {
            res.map(|record| {
                let mut out = indexmap::IndexMap::new();
                flatten_into(&mut out, "", &record, &sep);
                out
            })
        });
        Box::new(iter)
    }
}

pub struct SampleStage {
    pub n: usize,
}

impl Stage for SampleStage {
    fn process<'a>(&'a self, input: RecordStream<'a>) -> RecordStream<'a> {
        // Reservoir sampling (Algorithm R): a single streaming pass yields a
        // uniformly random sample of `n` records without knowing the total
        // stream length in advance, using O(n) memory.
        use rand::RngExt;
        let mut rng = rand::rng();
        let mut reservoir: Vec<Record> = Vec::with_capacity(self.n);

        for (idx, res) in input.enumerate() {
            let rec = match res {
                Ok(r) => r,
                Err(e) => return Box::new(std::iter::once(Err(e))),
            };
            let seen = idx + 1; // 1-indexed count of records seen so far
            if reservoir.len() < self.n {
                reservoir.push(rec);
            } else if self.n > 0 {
                let j = rng.random_range(0..seen);
                if j < self.n {
                    reservoir[j] = rec;
                }
            }
        }

        Box::new(reservoir.into_iter().map(Ok))
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
            exclude: false,
        };
        let out = collect_ok(stage.process(input));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].get("a"), Some(&Value::Integer(1)));
        assert_eq!(out[0].get("c"), Some(&Value::Null));
        assert_eq!(out[0].get("b"), None);
    }

    #[test]
    fn select_exclude_drops_named_fields_keeps_rest() {
        let input = stream(vec![rec(&[
            ("name", Value::String("Alice".to_string())),
            ("password", Value::String("secret".to_string())),
            ("age", Value::Integer(30)),
        ])]);
        let stage = SelectStage {
            fields: vec!["password".to_string()],
            exclude: true,
        };
        let out = collect_ok(stage.process(input));
        assert_eq!(out[0].get("password"), None);
        assert_eq!(
            out[0].get("name"),
            Some(&Value::String("Alice".to_string()))
        );
        assert_eq!(out[0].get("age"), Some(&Value::Integer(30)));
    }

    #[test]
    fn select_exclude_preserves_field_order() {
        let input = stream(vec![rec(&[
            ("a", Value::Integer(1)),
            ("b", Value::Integer(2)),
            ("c", Value::Integer(3)),
        ])]);
        let stage = SelectStage {
            fields: vec!["b".to_string()],
            exclude: true,
        };
        let out = collect_ok(stage.process(input));
        let keys: Vec<_> = out[0].keys().cloned().collect();
        assert_eq!(keys, vec!["a".to_string(), "c".to_string()]);
    }

    #[test]
    fn select_exclude_nonexistent_field_is_a_no_op() {
        let input = stream(vec![rec(&[("a", Value::Integer(1))])]);
        let stage = SelectStage {
            fields: vec!["nonexistent".to_string()],
            exclude: true,
        };
        let out = collect_ok(stage.process(input));
        assert_eq!(out[0].get("a"), Some(&Value::Integer(1)));
        assert_eq!(out[0].len(), 1);
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
            fields: vec![("n".to_string(), false)],
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
            fields: vec![("n".to_string(), true)],
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
            fields: vec![("n".to_string(), false)],
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
            fields: vec![("n".to_string(), false)],
        };
        let out = collect_ok(stage.process(input));
        assert_eq!(out.len(), n as usize);
        for (i, r) in out.iter().enumerate() {
            assert_eq!(r.get("n"), Some(&Value::Integer(i as i64)));
        }
    }

    #[test]
    fn sort_multi_field_primary_then_secondary() {
        let input = stream(vec![
            rec(&[("a", Value::Integer(1)), ("b", Value::Integer(2))]),
            rec(&[("a", Value::Integer(1)), ("b", Value::Integer(1))]),
            rec(&[("a", Value::Integer(0)), ("b", Value::Integer(5))]),
        ]);
        let stage = SortStage {
            fields: vec![("a".to_string(), false), ("b".to_string(), false)],
        };
        let out = collect_ok(stage.process(input));
        let pairs: Vec<_> = out
            .iter()
            .map(|r| (r.get("a").cloned().unwrap(), r.get("b").cloned().unwrap()))
            .collect();
        assert_eq!(
            pairs,
            vec![
                (Value::Integer(0), Value::Integer(5)),
                (Value::Integer(1), Value::Integer(1)),
                (Value::Integer(1), Value::Integer(2)),
            ]
        );
    }

    #[test]
    fn sort_multi_field_mixed_asc_desc() {
        let input = stream(vec![
            rec(&[("a", Value::Integer(1)), ("b", Value::Integer(1))]),
            rec(&[("a", Value::Integer(1)), ("b", Value::Integer(2))]),
            rec(&[("a", Value::Integer(0)), ("b", Value::Integer(9))]),
        ]);
        let stage = SortStage {
            fields: vec![("a".to_string(), false), ("b".to_string(), true)],
        };
        let out = collect_ok(stage.process(input));
        let pairs: Vec<_> = out
            .iter()
            .map(|r| (r.get("a").cloned().unwrap(), r.get("b").cloned().unwrap()))
            .collect();
        assert_eq!(
            pairs,
            vec![
                (Value::Integer(0), Value::Integer(9)),
                (Value::Integer(1), Value::Integer(2)),
                (Value::Integer(1), Value::Integer(1)),
            ]
        );
    }

    #[test]
    fn topn_matches_sort_then_limit_descending() {
        let input = stream(vec![
            rec(&[("score", Value::Integer(5))]),
            rec(&[("score", Value::Integer(9))]),
            rec(&[("score", Value::Integer(1))]),
            rec(&[("score", Value::Integer(7))]),
            rec(&[("score", Value::Integer(3))]),
        ]);
        let stage = TopNStage {
            fields: vec![("score".to_string(), true)],
            n: 3,
        };
        let out = collect_ok(stage.process(input));
        let values: Vec<_> = out
            .iter()
            .map(|r| r.get("score").cloned().unwrap())
            .collect();
        assert_eq!(
            values,
            vec![Value::Integer(9), Value::Integer(7), Value::Integer(5)]
        );
    }

    #[test]
    fn topn_matches_sort_then_limit_ascending() {
        let input = stream(vec![
            rec(&[("score", Value::Integer(5))]),
            rec(&[("score", Value::Integer(9))]),
            rec(&[("score", Value::Integer(1))]),
        ]);
        let stage = TopNStage {
            fields: vec![("score".to_string(), false)],
            n: 2,
        };
        let out = collect_ok(stage.process(input));
        let values: Vec<_> = out
            .iter()
            .map(|r| r.get("score").cloned().unwrap())
            .collect();
        assert_eq!(values, vec![Value::Integer(1), Value::Integer(5)]);
    }

    #[test]
    fn topn_n_greater_than_stream_length_returns_everything_sorted() {
        let input = stream(vec![
            rec(&[("score", Value::Integer(2))]),
            rec(&[("score", Value::Integer(1))]),
        ]);
        let stage = TopNStage {
            fields: vec![("score".to_string(), false)],
            n: 10,
        };
        let out = collect_ok(stage.process(input));
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].get("score"), Some(&Value::Integer(1)));
        assert_eq!(out[1].get("score"), Some(&Value::Integer(2)));
    }

    #[test]
    fn topn_zero_yields_nothing() {
        let input = stream(vec![rec(&[("score", Value::Integer(1))])]);
        let stage = TopNStage {
            fields: vec![("score".to_string(), false)],
            n: 0,
        };
        assert_eq!(collect_ok(stage.process(input)).len(), 0);
    }

    #[test]
    fn topn_multi_field() {
        let input = stream(vec![
            rec(&[("a", Value::Integer(1)), ("b", Value::Integer(3))]),
            rec(&[("a", Value::Integer(1)), ("b", Value::Integer(1))]),
            rec(&[("a", Value::Integer(2)), ("b", Value::Integer(0))]),
        ]);
        let stage = TopNStage {
            fields: vec![("a".to_string(), true), ("b".to_string(), true)],
            n: 2,
        };
        let out = collect_ok(stage.process(input));
        let pairs: Vec<_> = out
            .iter()
            .map(|r| (r.get("a").cloned().unwrap(), r.get("b").cloned().unwrap()))
            .collect();
        assert_eq!(
            pairs,
            vec![
                (Value::Integer(2), Value::Integer(0)),
                (Value::Integer(1), Value::Integer(3)),
            ]
        );
    }

    #[test]
    fn topn_propagates_error_instead_of_ignoring_it() {
        let input = stream_with_error(
            vec![rec(&[("n", Value::Integer(1))])],
            vec![rec(&[("n", Value::Integer(2))])],
        );
        let stage = TopNStage {
            fields: vec![("n".to_string(), false)],
            n: 5,
        };
        let mut out = stage.process(input);
        assert!(out.next().unwrap().is_err());
    }

    #[test]
    fn parse_sort_spec_parses_mixed_directions() {
        let parsed = parse_sort_spec("age:desc,name,score:asc").unwrap();
        assert_eq!(
            parsed,
            vec![
                ("age".to_string(), true),
                ("name".to_string(), false),
                ("score".to_string(), false),
            ]
        );
    }

    #[test]
    fn parse_sort_spec_rejects_empty_field() {
        assert!(parse_sort_spec("age,,name").is_err());
        assert!(parse_sort_spec("").is_err());
    }

    #[test]
    fn parse_field_list_rejects_empty_field() {
        assert!(parse_field_list("a,,b").is_err());
        assert!(parse_field_list("").is_err());
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
            assignments: vec![("double".to_string(), crate::expr::parse(".n * 2").unwrap())],
        };
        let out = collect_ok(stage.process(input));
        assert_eq!(out[0].get("double"), Some(&Value::Integer(10)));
    }

    #[test]
    fn map_stage_applies_multiple_assignments_in_order() {
        let input = stream(vec![rec(&[("n", Value::Integer(5))])]);
        let stage = MapStage {
            assignments: vec![
                ("double".to_string(), crate::expr::parse(".n * 2").unwrap()),
                // References `double`, set by the previous assignment in
                // the same map call - confirms sequential evaluation.
                (
                    "quadruple".to_string(),
                    crate::expr::parse(".double * 2").unwrap(),
                ),
            ],
        };
        let out = collect_ok(stage.process(input));
        assert_eq!(out[0].get("double"), Some(&Value::Integer(10)));
        assert_eq!(out[0].get("quadruple"), Some(&Value::Integer(20)));
    }

    #[test]
    fn unique_keeps_first_occurrence_per_value() {
        let input = stream(vec![
            rec(&[("id", Value::Integer(1))]),
            rec(&[("id", Value::Integer(2))]),
            rec(&[("id", Value::Integer(1))]),
        ]);
        let stage = UniqueStage {
            fields: vec!["id".to_string()],
        };
        let out = collect_ok(stage.process(input));
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].get("id"), Some(&Value::Integer(1)));
        assert_eq!(out[1].get("id"), Some(&Value::Integer(2)));
    }

    #[test]
    fn unique_composite_key_across_multiple_fields() {
        let input = stream(vec![
            rec(&[("a", Value::Integer(1)), ("b", Value::Integer(2))]),
            rec(&[("a", Value::Integer(1)), ("b", Value::Integer(2))]),
            rec(&[("a", Value::Integer(1)), ("b", Value::Integer(3))]),
        ]);
        let stage = UniqueStage {
            fields: vec!["a".to_string(), "b".to_string()],
        };
        let out = collect_ok(stage.process(input));
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].get("b"), Some(&Value::Integer(2)));
        assert_eq!(out[1].get("b"), Some(&Value::Integer(3)));
    }

    #[test]
    fn dedup_drops_exact_duplicate_records() {
        let input = stream(vec![
            rec(&[("a", Value::Integer(1)), ("b", Value::Integer(2))]),
            rec(&[("a", Value::Integer(1)), ("b", Value::Integer(2))]),
            rec(&[("a", Value::Integer(1)), ("b", Value::Integer(3))]),
        ]);
        let out = collect_ok(DedupStage.process(input));
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].get("b"), Some(&Value::Integer(2)));
        assert_eq!(out[1].get("b"), Some(&Value::Integer(3)));
    }

    #[test]
    fn dedup_distinguishes_records_unique_keeps_would_collapse() {
        // Same "a" value but different "b" - unique(a) would collapse these
        // to one record, dedup must keep both since they aren't identical.
        let input = stream(vec![
            rec(&[("a", Value::Integer(1)), ("b", Value::Integer(2))]),
            rec(&[("a", Value::Integer(1)), ("b", Value::Integer(3))]),
        ]);
        let out = collect_ok(DedupStage.process(input));
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn dedup_on_empty_stream_yields_nothing() {
        let input = stream(vec![]);
        let out = collect_ok(DedupStage.process(input));
        assert_eq!(out.len(), 0);
    }

    #[test]
    fn dedup_propagates_errors() {
        let input = stream_with_error(
            vec![rec(&[("a", Value::Integer(1))])],
            vec![rec(&[("a", Value::Integer(2))])],
        );
        let out: Vec<_> = DedupStage.process(input).collect();
        assert!(out.iter().any(|r| r.is_err()));
    }

    #[test]
    fn search_literal_matches_any_field() {
        let input = stream(vec![
            rec(&[
                ("name", Value::String("Alice".to_string())),
                ("note", Value::String("likes Rust".to_string())),
            ]),
            rec(&[
                ("name", Value::String("Bob".to_string())),
                ("note", Value::String("likes Python".to_string())),
            ]),
        ]);
        let stage = SearchStage {
            literal: Some("Rust".to_string()),
            regex: None,
        };
        let out = collect_ok(stage.process(input));
        assert_eq!(out.len(), 1);
        assert_eq!(
            out[0].get("name"),
            Some(&Value::String("Alice".to_string()))
        );
    }

    #[test]
    fn search_matches_across_different_fields_per_record() {
        let input = stream(vec![
            rec(&[
                ("a", Value::String("foo".to_string())),
                ("b", Value::String("bar".to_string())),
            ]),
            rec(&[
                ("a", Value::String("baz".to_string())),
                ("b", Value::String("foo".to_string())),
            ]),
            rec(&[
                ("a", Value::String("nope".to_string())),
                ("b", Value::String("nope".to_string())),
            ]),
        ]);
        let stage = SearchStage {
            literal: Some("foo".to_string()),
            regex: None,
        };
        let out = collect_ok(stage.process(input));
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn search_matches_stringified_numeric_field() {
        let input = stream(vec![
            rec(&[("age", Value::Integer(30))]),
            rec(&[("age", Value::Integer(40))]),
        ]);
        let stage = SearchStage {
            literal: Some("30".to_string()),
            regex: None,
        };
        let out = collect_ok(stage.process(input));
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn search_regex_mode() {
        let input = stream(vec![
            rec(&[("email", Value::String("alice@example.com".to_string()))]),
            rec(&[("email", Value::String("bob@other.org".to_string()))]),
        ]);
        let re = regex::Regex::new(r"^.+@example\.com$").unwrap();
        let stage = SearchStage {
            literal: None,
            regex: Some(re),
        };
        let out = collect_ok(stage.process(input));
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn search_no_matches_yields_nothing() {
        let input = stream(vec![rec(&[("a", Value::String("x".to_string()))])]);
        let stage = SearchStage {
            literal: Some("zzz".to_string()),
            regex: None,
        };
        let out = collect_ok(stage.process(input));
        assert_eq!(out.len(), 0);
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

    fn stats_field_row<'a>(out: &'a [Record], field: &str) -> &'a Record {
        out.iter()
            .find(|r| r.get("field") == Some(&Value::String(field.to_string())))
            .unwrap()
    }

    #[test]
    fn stats_computes_mean_and_population_stddev() {
        // Classic textbook example: mean 5, population stddev 2.
        let input = stream(
            [2, 4, 4, 4, 5, 5, 7, 9]
                .iter()
                .map(|n| rec(&[("x", Value::Integer(*n))]))
                .collect(),
        );
        let out = collect_ok(StatsStage.process(input));
        let row = stats_field_row(&out, "x");
        assert_eq!(row.get("count"), Some(&Value::Integer(8)));
        assert_eq!(row.get("mean"), Some(&Value::Float(5.0)));
        assert_eq!(row.get("stddev"), Some(&Value::Float(2.0)));
        assert_eq!(row.get("min"), Some(&Value::Integer(2)));
        assert_eq!(row.get("max"), Some(&Value::Integer(9)));
    }

    #[test]
    fn stats_counts_nulls_and_missing_separately() {
        // "a" is present (once null) in 2 of 3 records; "b" only in the 3rd.
        let input = stream(vec![
            rec(&[("a", Value::Integer(1))]),
            rec(&[("a", Value::Null)]),
            rec(&[("b", Value::Integer(2))]),
        ]);
        let out = collect_ok(StatsStage.process(input));
        let a = stats_field_row(&out, "a");
        assert_eq!(a.get("count"), Some(&Value::Integer(2)));
        assert_eq!(a.get("nulls"), Some(&Value::Integer(1)));
        let b = stats_field_row(&out, "b");
        assert_eq!(b.get("count"), Some(&Value::Integer(1)));
        assert_eq!(b.get("nulls"), Some(&Value::Integer(0)));
    }

    #[test]
    fn stats_tracks_distinct_count() {
        let input = stream(vec![
            rec(&[("status", Value::String("a".to_string()))]),
            rec(&[("status", Value::String("a".to_string()))]),
            rec(&[("status", Value::String("b".to_string()))]),
        ]);
        let out = collect_ok(StatsStage.process(input));
        let row = stats_field_row(&out, "status");
        assert_eq!(row.get("distinct"), Some(&Value::Integer(2)));
    }

    #[test]
    fn stats_non_numeric_field_has_null_mean_and_stddev_but_real_min_max() {
        let input = stream(vec![
            rec(&[("name", Value::String("Alice".to_string()))]),
            rec(&[("name", Value::String("Bob".to_string()))]),
        ]);
        let out = collect_ok(StatsStage.process(input));
        let row = stats_field_row(&out, "name");
        assert_eq!(row.get("mean"), Some(&Value::Null));
        assert_eq!(row.get("stddev"), Some(&Value::Null));
        assert_eq!(row.get("min"), Some(&Value::String("Alice".to_string())));
        assert_eq!(row.get("max"), Some(&Value::String("Bob".to_string())));
    }

    #[test]
    fn stats_on_single_value_has_zero_stddev() {
        let input = stream(vec![rec(&[("x", Value::Integer(5))])]);
        let out = collect_ok(StatsStage.process(input));
        let row = stats_field_row(&out, "x");
        assert_eq!(row.get("stddev"), Some(&Value::Float(0.0)));
    }

    #[test]
    fn stats_on_empty_stream_yields_no_rows() {
        let input = stream(vec![]);
        let out = collect_ok(StatsStage.process(input));
        assert_eq!(out.len(), 0);
    }

    #[test]
    fn stats_propagates_error_instead_of_ignoring_it() {
        let input = stream_with_error(
            vec![rec(&[("a", Value::Integer(1))])],
            vec![rec(&[("a", Value::Integer(2))])],
        );
        let mut out = StatsStage.process(input);
        assert!(out.next().unwrap().is_err());
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
            by: vec!["category".to_string()],
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
    fn group_by_multiple_fields_and_preserves_value_types() {
        let input = stream(vec![
            rec(&[
                ("country", Value::String("IN".to_string())),
                ("city", Value::String("BLR".to_string())),
            ]),
            rec(&[
                ("country", Value::String("IN".to_string())),
                ("city", Value::String("BLR".to_string())),
            ]),
            rec(&[
                ("country", Value::String("IN".to_string())),
                ("city", Value::String("DEL".to_string())),
            ]),
        ]);
        let stage = GroupStage {
            by: vec!["country".to_string(), "city".to_string()],
            sum: None,
            count: true,
        };
        let out = collect_ok(stage.process(input));
        assert_eq!(out.len(), 2);
        let blr = out
            .iter()
            .find(|r| r.get("city") == Some(&Value::String("BLR".to_string())))
            .unwrap();
        assert_eq!(blr.get("country"), Some(&Value::String("IN".to_string())));
        assert_eq!(blr.get("count"), Some(&Value::Integer(2)));
    }

    #[test]
    fn group_by_single_field_preserves_original_type_not_stringified() {
        // Regression check: the by-field's original Value type (not a
        // stringified copy) should come through in the output.
        let input = stream(vec![
            rec(&[("code", Value::Integer(1))]),
            rec(&[("code", Value::Integer(1))]),
        ]);
        let stage = GroupStage {
            by: vec!["code".to_string()],
            sum: None,
            count: true,
        };
        let out = collect_ok(stage.process(input));
        assert_eq!(out[0].get("code"), Some(&Value::Integer(1)));
    }

    #[test]
    fn freq_sorts_by_count_descending() {
        let input = stream(vec![
            rec(&[("status", Value::String("active".to_string()))]),
            rec(&[("status", Value::String("active".to_string()))]),
            rec(&[("status", Value::String("banned".to_string()))]),
            rec(&[("status", Value::String("active".to_string()))]),
            rec(&[("status", Value::String("pending".to_string()))]),
        ]);
        let stage = FreqStage {
            field: "status".to_string(),
            limit: None,
        };
        let out = collect_ok(stage.process(input));
        assert_eq!(out.len(), 3);
        assert_eq!(
            out[0].get("value"),
            Some(&Value::String("active".to_string()))
        );
        assert_eq!(out[0].get("count"), Some(&Value::Integer(3)));
        assert_eq!(out[0].get("percent"), Some(&Value::Float(60.0)));
        // Both banned/pending have count 1 - order between ties isn't
        // asserted, just that active is strictly first.
    }

    #[test]
    fn freq_respects_limit() {
        let input = stream(vec![
            rec(&[("c", Value::String("a".to_string()))]),
            rec(&[("c", Value::String("a".to_string()))]),
            rec(&[("c", Value::String("b".to_string()))]),
            rec(&[("c", Value::String("c".to_string()))]),
        ]);
        let stage = FreqStage {
            field: "c".to_string(),
            limit: Some(2),
        };
        let out = collect_ok(stage.process(input));
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].get("value"), Some(&Value::String("a".to_string())));
    }

    #[test]
    fn freq_on_empty_stream_yields_no_rows() {
        let input = stream(vec![]);
        let stage = FreqStage {
            field: "x".to_string(),
            limit: None,
        };
        let out = collect_ok(stage.process(input));
        assert_eq!(out.len(), 0);
    }

    #[test]
    fn freq_missing_field_counted_as_null() {
        let input = stream(vec![
            rec(&[("a", Value::Integer(1))]),
            rec(&[("b", Value::Integer(2))]),
        ]);
        let stage = FreqStage {
            field: "a".to_string(),
            limit: None,
        };
        let out = collect_ok(stage.process(input));
        assert_eq!(out.len(), 2);
        assert!(out
            .iter()
            .any(|r| r.get("value") == Some(&Value::String("null".to_string()))));
    }

    #[test]
    fn freq_propagates_error_instead_of_ignoring_it() {
        let input = stream_with_error(
            vec![rec(&[("a", Value::String("x".to_string()))])],
            vec![rec(&[("a", Value::String("y".to_string()))])],
        );
        let stage = FreqStage {
            field: "a".to_string(),
            limit: None,
        };
        let mut out = stage.process(input);
        assert!(out.next().unwrap().is_err());
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
            on: vec!["id".to_string()],
            join_type: JoinType::Left,
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
            on: vec!["id".to_string()],
            join_type: JoinType::Left,
        };

        let input = stream(vec![rec(&[("id", Value::String("1".to_string()))])]);
        let out = collect_ok(stage.process(input));
        assert_eq!(out[0].get("id"), Some(&Value::String("1".to_string())));
        assert_eq!(out[0].len(), 1);
    }

    fn join_test_right_map() -> std::sync::Arc<std::collections::HashMap<String, Record>> {
        let mut right = std::collections::HashMap::new();
        right.insert(
            "1".to_string(),
            rec(&[
                ("id", Value::String("1".to_string())),
                ("name", Value::String("Alice".to_string())),
            ]),
        );
        right.insert(
            "2".to_string(),
            rec(&[
                ("id", Value::String("2".to_string())),
                ("name", Value::String("Bob".to_string())),
            ]),
        );
        std::sync::Arc::new(right)
    }

    #[test]
    fn inner_join_drops_unmatched_left_records() {
        let stage = JoinStage {
            hash_map: join_test_right_map(),
            on: vec!["id".to_string()],
            join_type: JoinType::Inner,
        };
        let input = stream(vec![
            rec(&[("id", Value::String("1".to_string()))]),
            rec(&[("id", Value::String("999".to_string()))]),
        ]);
        let out = collect_ok(stage.process(input));
        assert_eq!(out.len(), 1);
        assert_eq!(
            out[0].get("name"),
            Some(&Value::String("Alice".to_string()))
        );
    }

    #[test]
    fn right_join_drops_unmatched_left_and_appends_unmatched_right() {
        let stage = JoinStage {
            hash_map: join_test_right_map(),
            on: vec!["id".to_string()],
            join_type: JoinType::Right,
        };
        // Left has id "1" (matches) and "999" (no match, must be dropped).
        // Right has "1" (matched) and "2" (never matched, must appear at the end).
        let input = stream(vec![
            rec(&[
                ("id", Value::String("1".to_string())),
                ("order", Value::Integer(100)),
            ]),
            rec(&[("id", Value::String("999".to_string()))]),
        ]);
        let out = collect_ok(stage.process(input));
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].get("order"), Some(&Value::Integer(100)));
        assert_eq!(
            out[0].get("name"),
            Some(&Value::String("Alice".to_string()))
        );
        assert_eq!(out[1].get("name"), Some(&Value::String("Bob".to_string())));
        assert_eq!(out[1].get("order"), None);
    }

    #[test]
    fn full_join_keeps_unmatched_left_and_appends_unmatched_right() {
        let stage = JoinStage {
            hash_map: join_test_right_map(),
            on: vec!["id".to_string()],
            join_type: JoinType::Full,
        };
        let input = stream(vec![
            rec(&[
                ("id", Value::String("1".to_string())),
                ("order", Value::Integer(100)),
            ]),
            rec(&[("id", Value::String("999".to_string()))]),
        ]);
        let out = collect_ok(stage.process(input));
        assert_eq!(out.len(), 3);
        // Matched left record, merged.
        assert_eq!(
            out[0].get("name"),
            Some(&Value::String("Alice".to_string()))
        );
        // Unmatched left record, kept as-is.
        assert_eq!(out[1].get("id"), Some(&Value::String("999".to_string())));
        assert_eq!(out[1].get("name"), None);
        // Unmatched right record, appended at the end.
        assert_eq!(out[2].get("name"), Some(&Value::String("Bob".to_string())));
    }

    #[test]
    fn full_join_on_empty_left_stream_yields_all_right_records() {
        let stage = JoinStage {
            hash_map: join_test_right_map(),
            on: vec!["id".to_string()],
            join_type: JoinType::Full,
        };
        let input = stream(vec![]);
        let out = collect_ok(stage.process(input));
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn join_multi_field_on_matches_composite_key_only() {
        let on = vec!["region".to_string(), "id".to_string()];
        let right_us = rec(&[
            ("region", Value::String("us".to_string())),
            ("id", Value::Integer(1)),
            ("name", Value::String("Alice".to_string())),
        ]);
        let right_eu = rec(&[
            ("region", Value::String("eu".to_string())),
            ("id", Value::Integer(1)),
            ("name", Value::String("Bob".to_string())),
        ]);
        let mut hash_map = std::collections::HashMap::new();
        hash_map.insert(join_key_for(&right_us, &on).unwrap(), right_us);
        hash_map.insert(join_key_for(&right_eu, &on).unwrap(), right_eu);

        let stage = JoinStage {
            hash_map: std::sync::Arc::new(hash_map),
            on,
            join_type: JoinType::Left,
        };
        let input = stream(vec![
            rec(&[
                ("region", Value::String("us".to_string())),
                ("id", Value::Integer(1)),
            ]),
            rec(&[
                ("region", Value::String("eu".to_string())),
                ("id", Value::Integer(1)),
            ]),
        ]);
        let out = collect_ok(stage.process(input));
        assert_eq!(out.len(), 2);
        let us_match = out
            .iter()
            .find(|r| r.get("region") == Some(&Value::String("us".to_string())))
            .unwrap();
        assert_eq!(
            us_match.get("name"),
            Some(&Value::String("Alice".to_string()))
        );
        let eu_match = out
            .iter()
            .find(|r| r.get("region") == Some(&Value::String("eu".to_string())))
            .unwrap();
        assert_eq!(
            eu_match.get("name"),
            Some(&Value::String("Bob".to_string()))
        );
    }

    // --- merge join (join --merge) ---

    fn merge_join_stage(join_type: JoinType, right: Vec<Record>) -> MergeJoinStage {
        // MergeJoinStage requires its right-hand stream to already be
        // sorted by the join key (the production code path guarantees this
        // via external_sort before construction) - genuinely sort it here
        // too, rather than assuming test data happens to already be in the
        // right order. Note lexicographic string order isn't the same as
        // numeric order (e.g. "10" < "2"), which is exactly the assumption
        // an earlier, buggy version of this helper silently violated.
        let right_stream: RecordStream<'static> = Box::new(right.into_iter().map(Ok));
        let right_sorted = external_sort(right_stream, vec![("id".to_string(), false)]);
        MergeJoinStage::new(vec!["id".to_string()], join_type, right_sorted)
    }

    #[test]
    fn merge_join_left_matches_hash_join_for_unique_keys() {
        let left = stream(vec![
            rec(&[
                ("id", Value::String("1".to_string())),
                ("order", Value::Integer(100)),
            ]),
            rec(&[
                ("id", Value::String("2".to_string())),
                ("order", Value::Integer(200)),
            ]),
            rec(&[
                ("id", Value::String("4".to_string())),
                ("order", Value::Integer(400)),
            ]),
        ]);
        let right = vec![
            rec(&[
                ("id", Value::String("1".to_string())),
                ("name", Value::String("Alice".to_string())),
            ]),
            rec(&[
                ("id", Value::String("2".to_string())),
                ("name", Value::String("Bob".to_string())),
            ]),
            rec(&[
                ("id", Value::String("3".to_string())),
                ("name", Value::String("Carol".to_string())),
            ]),
        ];
        let stage = merge_join_stage(JoinType::Left, right);
        let out = collect_ok(stage.process(left));
        assert_eq!(out.len(), 3);
        let by_id = |id: &str| {
            out.iter()
                .find(|r| r.get("id") == Some(&Value::String(id.to_string())))
                .unwrap()
        };
        assert_eq!(
            by_id("1").get("name"),
            Some(&Value::String("Alice".to_string()))
        );
        assert_eq!(by_id("4").get("name"), None);
    }

    #[test]
    fn merge_join_inner_drops_unmatched() {
        let left = stream(vec![
            rec(&[("id", Value::String("1".to_string()))]),
            rec(&[("id", Value::String("999".to_string()))]),
        ]);
        let right = vec![rec(&[
            ("id", Value::String("1".to_string())),
            ("name", Value::String("Alice".to_string())),
        ])];
        let stage = merge_join_stage(JoinType::Inner, right);
        let out = collect_ok(stage.process(left));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].get("id"), Some(&Value::String("1".to_string())));
    }

    #[test]
    fn merge_join_right_appends_unmatched_right() {
        let left = stream(vec![rec(&[("id", Value::String("1".to_string()))])]);
        let right = vec![
            rec(&[
                ("id", Value::String("1".to_string())),
                ("name", Value::String("Alice".to_string())),
            ]),
            rec(&[
                ("id", Value::String("2".to_string())),
                ("name", Value::String("Bob".to_string())),
            ]),
        ];
        let stage = merge_join_stage(JoinType::Right, right);
        let out = collect_ok(stage.process(left));
        assert_eq!(out.len(), 2);
        assert_eq!(out[1].get("name"), Some(&Value::String("Bob".to_string())));
    }

    #[test]
    fn merge_join_full_keeps_both_unmatched_sides() {
        let left = stream(vec![
            rec(&[("id", Value::String("1".to_string()))]),
            rec(&[("id", Value::String("4".to_string()))]),
        ]);
        let right = vec![
            rec(&[
                ("id", Value::String("1".to_string())),
                ("name", Value::String("Alice".to_string())),
            ]),
            rec(&[
                ("id", Value::String("3".to_string())),
                ("name", Value::String("Carol".to_string())),
            ]),
        ];
        let stage = merge_join_stage(JoinType::Full, right);
        let out = collect_ok(stage.process(left));
        assert_eq!(out.len(), 3);
    }

    #[test]
    fn merge_join_produces_cross_product_for_duplicate_right_keys() {
        // Deliberately different from JoinStage's hash join, which keeps
        // only the last right record for a duplicate key.
        let left = stream(vec![rec(&[
            ("id", Value::String("1".to_string())),
            ("order", Value::Integer(100)),
        ])]);
        let right = vec![
            rec(&[
                ("id", Value::String("1".to_string())),
                ("tag", Value::String("a".to_string())),
            ]),
            rec(&[
                ("id", Value::String("1".to_string())),
                ("tag", Value::String("b".to_string())),
            ]),
        ];
        let stage = merge_join_stage(JoinType::Inner, right);
        let out = collect_ok(stage.process(left));
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].get("tag"), Some(&Value::String("a".to_string())));
        assert_eq!(out[1].get("tag"), Some(&Value::String("b".to_string())));
    }

    #[test]
    fn merge_join_produces_cross_product_for_duplicate_left_keys() {
        let left = stream(vec![
            rec(&[
                ("id", Value::String("1".to_string())),
                ("x", Value::String("p".to_string())),
            ]),
            rec(&[
                ("id", Value::String("1".to_string())),
                ("x", Value::String("q".to_string())),
            ]),
        ]);
        let right = vec![rec(&[
            ("id", Value::String("1".to_string())),
            ("name", Value::String("Alice".to_string())),
        ])];
        let stage = merge_join_stage(JoinType::Inner, right);
        let out = collect_ok(stage.process(left));
        assert_eq!(out.len(), 2);
        assert!(out
            .iter()
            .all(|r| r.get("name") == Some(&Value::String("Alice".to_string()))));
    }

    #[test]
    fn merge_join_on_empty_left_stream() {
        let left = stream(vec![]);
        let right = vec![rec(&[("id", Value::String("1".to_string()))])];
        let stage = merge_join_stage(JoinType::Full, right);
        let out = collect_ok(stage.process(left));
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn merge_join_on_empty_right_stream() {
        let left = stream(vec![rec(&[("id", Value::String("1".to_string()))])]);
        let stage = merge_join_stage(JoinType::Left, vec![]);
        let out = collect_ok(stage.process(left));
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn merge_join_multi_field_on_matches_composite_key_only() {
        let on = vec!["region".to_string(), "id".to_string()];
        let right = vec![
            rec(&[
                ("region", Value::String("us".to_string())),
                ("id", Value::Integer(1)),
                ("name", Value::String("Alice".to_string())),
            ]),
            rec(&[
                ("region", Value::String("eu".to_string())),
                ("id", Value::Integer(1)),
                ("name", Value::String("Bob".to_string())),
            ]),
        ];
        let right_stream: RecordStream<'static> = Box::new(right.into_iter().map(Ok));
        let sort_fields: Vec<(String, bool)> = on.iter().map(|f| (f.clone(), false)).collect();
        let right_sorted = external_sort(right_stream, sort_fields);
        let stage = MergeJoinStage::new(on, JoinType::Left, right_sorted);

        let left = stream(vec![
            rec(&[
                ("region", Value::String("us".to_string())),
                ("id", Value::Integer(1)),
            ]),
            rec(&[
                ("region", Value::String("eu".to_string())),
                ("id", Value::Integer(1)),
            ]),
        ]);
        let out = collect_ok(stage.process(left));
        assert_eq!(out.len(), 2);
        let us_match = out
            .iter()
            .find(|r| r.get("region") == Some(&Value::String("us".to_string())))
            .unwrap();
        assert_eq!(
            us_match.get("name"),
            Some(&Value::String("Alice".to_string()))
        );
        let eu_match = out
            .iter()
            .find(|r| r.get("region") == Some(&Value::String("eu".to_string())))
            .unwrap();
        assert_eq!(
            eu_match.get("name"),
            Some(&Value::String("Bob".to_string()))
        );
    }

    #[test]
    fn merge_join_at_scale_matches_every_record_exactly_once() {
        // Exercises the external-merge-sort path on both sides (chunk size
        // is 50_000) and confirms no records are lost or duplicated.
        let n: i64 = 60_000;
        let left_records: Vec<_> = (0..n)
            .rev()
            .map(|i| {
                rec(&[
                    ("id", Value::String(i.to_string())),
                    ("v", Value::Integer(i)),
                ])
            })
            .collect();
        let right_records: Vec<_> = (0..n)
            .map(|i| {
                rec(&[
                    ("id", Value::String(i.to_string())),
                    ("name", Value::String(format!("n{i}"))),
                ])
            })
            .collect();
        let stage = merge_join_stage(JoinType::Inner, right_records);
        let out = collect_ok(stage.process(stream(left_records)));
        assert_eq!(out.len(), n as usize);
    }

    // Note: a test asserting error propagation through merge join's left
    // side is deliberately not included here. `external_sort` (which both
    // `sort` and merge join's left-side pre-sort rely on) has a known,
    // pre-existing bug where it silently drops a malformed record instead
    // of propagating it - tracked as a separate follow-up fix rather than
    // scoped into this feature. Once that's fixed, this is worth adding.

    // --- rename / flatten / sample ---

    #[test]
    fn rename_renames_matching_fields_and_leaves_others_untouched() {
        let input = stream(vec![rec(&[
            ("a", Value::Integer(1)),
            ("b", Value::Integer(2)),
        ])]);
        let stage = RenameStage {
            renames: vec![("a".to_string(), "x".to_string())],
        };
        let out = collect_ok(stage.process(input));
        assert_eq!(out[0].get("x"), Some(&Value::Integer(1)));
        assert_eq!(out[0].get("a"), None);
        assert_eq!(out[0].get("b"), Some(&Value::Integer(2)));
    }

    #[test]
    fn rename_preserves_field_order() {
        let input = stream(vec![rec(&[
            ("a", Value::Integer(1)),
            ("b", Value::Integer(2)),
            ("c", Value::Integer(3)),
        ])]);
        let stage = RenameStage {
            renames: vec![("b".to_string(), "z".to_string())],
        };
        let out = collect_ok(stage.process(input));
        let keys: Vec<_> = out[0].keys().cloned().collect();
        assert_eq!(
            keys,
            vec!["a".to_string(), "z".to_string(), "c".to_string()]
        );
    }

    #[test]
    fn flatten_joins_nested_object_keys_with_default_separator() {
        let mut user = IndexMap::new();
        user.insert("name".to_string(), Value::String("Alice".to_string()));
        user.insert("age".to_string(), Value::Integer(30));
        let input = stream(vec![rec(&[
            ("user", Value::Object(user)),
            ("active", Value::Boolean(true)),
        ])]);
        let stage = FlattenStage {
            separator: ".".to_string(),
        };
        let out = collect_ok(stage.process(input));
        assert_eq!(
            out[0].get("user.name"),
            Some(&Value::String("Alice".to_string()))
        );
        assert_eq!(out[0].get("user.age"), Some(&Value::Integer(30)));
        assert_eq!(out[0].get("active"), Some(&Value::Boolean(true)));
        assert_eq!(out[0].get("user"), None);
    }

    #[test]
    fn flatten_handles_deep_nesting_and_custom_separator() {
        let mut inner = IndexMap::new();
        inner.insert("c".to_string(), Value::Integer(1));
        let mut mid = IndexMap::new();
        mid.insert("b".to_string(), Value::Object(inner));
        let input = stream(vec![rec(&[("a", Value::Object(mid))])]);
        let stage = FlattenStage {
            separator: "_".to_string(),
        };
        let out = collect_ok(stage.process(input));
        assert_eq!(out[0].get("a_b_c"), Some(&Value::Integer(1)));
    }

    #[test]
    fn flatten_leaves_arrays_unflattened() {
        let input = stream(vec![rec(&[(
            "tags",
            Value::Array(vec![Value::Integer(1), Value::Integer(2)]),
        )])]);
        let stage = FlattenStage {
            separator: ".".to_string(),
        };
        let out = collect_ok(stage.process(input));
        assert_eq!(
            out[0].get("tags"),
            Some(&Value::Array(vec![Value::Integer(1), Value::Integer(2)]))
        );
    }

    #[test]
    fn sample_yields_exactly_n_records_when_stream_is_larger() {
        let records: Vec<_> = (0..100).map(|i| rec(&[("n", Value::Integer(i))])).collect();
        let input = stream(records);
        let stage = SampleStage { n: 10 };
        let out = collect_ok(stage.process(input));
        assert_eq!(out.len(), 10);
    }

    #[test]
    fn sample_yields_every_record_when_n_exceeds_stream_length() {
        let input = stream(vec![
            rec(&[("a", Value::Integer(1))]),
            rec(&[("a", Value::Integer(2))]),
        ]);
        let stage = SampleStage { n: 10 };
        let out = collect_ok(stage.process(input));
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn sample_of_zero_yields_nothing() {
        let input = stream(vec![rec(&[("a", Value::Integer(1))])]);
        let stage = SampleStage { n: 0 };
        let out = collect_ok(stage.process(input));
        assert_eq!(out.len(), 0);
    }

    // --- error propagation: aggregation stages must not silently drop/miscount
    // malformed records that reach them (regression test for a bug where
    // `.flatten()`/`.count()` counted or ignored Err items without reporting them) ---

    fn stream_with_error(before: Vec<Record>, after: Vec<Record>) -> RecordStream<'static> {
        let err = std::iter::once(Err(anyhow::anyhow!("boom")));
        Box::new(
            before
                .into_iter()
                .map(Ok)
                .chain(err)
                .chain(after.into_iter().map(Ok)),
        )
    }

    #[test]
    fn count_propagates_error_instead_of_miscounting() {
        let input = stream_with_error(
            vec![rec(&[("a", Value::Integer(1))])],
            vec![rec(&[("a", Value::Integer(2))])],
        );
        let mut out = CountStage.process(input);
        let result = out.next().unwrap();
        assert!(result.is_err());
        assert!(out.next().is_none());
    }

    #[test]
    fn sum_propagates_error_instead_of_ignoring_it() {
        let input = stream_with_error(
            vec![rec(&[("n", Value::Integer(1))])],
            vec![rec(&[("n", Value::Integer(2))])],
        );
        let stage = SumStage {
            field: "n".to_string(),
        };
        let mut out = stage.process(input);
        assert!(out.next().unwrap().is_err());
    }

    #[test]
    fn group_propagates_error_instead_of_ignoring_it() {
        let input = stream_with_error(
            vec![rec(&[("k", Value::String("a".to_string()))])],
            vec![rec(&[("k", Value::String("b".to_string()))])],
        );
        let stage = GroupStage {
            by: vec!["k".to_string()],
            sum: None,
            count: true,
        };
        let mut out = stage.process(input);
        assert!(out.next().unwrap().is_err());
    }

    #[test]
    fn schema_propagates_error_instead_of_ignoring_it() {
        let input = stream_with_error(
            vec![rec(&[("a", Value::Integer(1))])],
            vec![rec(&[("a", Value::Integer(2))])],
        );
        let mut out = SchemaStage.process(input);
        assert!(out.next().unwrap().is_err());
    }
}
