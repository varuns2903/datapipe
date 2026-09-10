# Quick Start

Process a stream of JSON records, filter them, sort them, and output as CSV:

```bash
cat examples/users.jsonl | dp filter '.age >= 21' | dp sort age --desc | dp csv
```

Each subcommand reads records from stdin and writes them to stdout, so stages chain together with ordinary shell pipes. For a pipeline with many stages, consider a [pipeline file](./pipeline-files.md) instead of a long one-liner.

Run `dp <command> --help` for full details on any command, or see the [Commands](./commands/README.md) reference.
