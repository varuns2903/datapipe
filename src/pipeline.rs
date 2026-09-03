use crate::model::Record;
use anyhow::Result;

/// A type alias for the dynamic iterator stream passing through the pipeline.
pub type RecordStream<'a> = Box<dyn Iterator<Item = Result<Record>> + 'a>;

/// A Stage represents a single operation in the pipeline (e.g., filter, select).
pub trait Stage {
    fn process<'a>(&'a self, input: RecordStream<'a>) -> RecordStream<'a>;
}

/// The Pipeline orchestrates pulling data through multiple stages.
pub struct Pipeline {
    stages: Vec<Box<dyn Stage>>,
}

impl Pipeline {
    pub fn new() -> Self {
        Self { stages: Vec::new() }
    }

    pub fn add_stage(&mut self, stage: Box<dyn Stage>) {
        self.stages.push(stage);
    }

    /// Consumes the stages and builds the final iterator chain.
    pub fn process<'a>(&'a self, mut stream: RecordStream<'a>) -> RecordStream<'a> {
        for stage in &self.stages {
            stream = stage.process(stream);
        }
        stream
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Value;
    use indexmap::IndexMap;

    /// A simple test stage that truncates the stream after `max` records.
    struct LimitStage {
        max: usize,
    }

    impl Stage for LimitStage {
        fn process<'a>(&'a self, input: RecordStream<'a>) -> RecordStream<'a> {
            Box::new(input.take(self.max))
        }
    }

    #[test]
    fn test_pipeline_chaining() {
        // Create 3 dummy records
        let mut rec = IndexMap::new();
        rec.insert("a".to_string(), Value::Integer(1));
        
        let inputs = vec![
            Ok(rec.clone()),
            Ok(rec.clone()),
            Ok(rec.clone()),
        ];
        
        // Wrap our vector into a Boxed Stream
        let stream: RecordStream = Box::new(inputs.into_iter());

        // Create a pipeline that limits to 2 records
        let mut pipeline = Pipeline::new();
        pipeline.add_stage(Box::new(LimitStage { max: 2 }));

        let result_stream = pipeline.process(stream);
        let results: Vec<_> = result_stream.collect();

        // Should only have 2 records left!
        assert_eq!(results.len(), 2);
    }
}
