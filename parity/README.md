# Parity Testing

This dir is to test parity with an exisiting python implementation of ULog parsing.

### Testing

```bash
python3 parity/profiler.py
```

This will populate the `/parity/results/` dir with files shaped like `<input_file_name>.ulg.py.json` which contain a json summary of the input ULog files. These can be used as test cases for ulog-rs.
