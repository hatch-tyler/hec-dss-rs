# Text Records

Text records store arbitrary string data in a DSS file.

## Writing Text

```rust
use dss_core::NativeDssFile;

let mut dss = NativeDssFile::create("notes.dss")?;
dss.write_text("/PROJECT/SITE/NOTE///COMMENT/", "Dam inspection completed 01Jan2020")?;
dss.write_text("/PROJECT/SITE/NOTE///DESCRIPTION/", "Multi-line text\nLine 2\nLine 3")?;
```

**Python:**
```python
with hecdss_rs.DssFile.create("notes.dss") as dss:
    dss.write_text("/PROJECT/SITE/NOTE///COMMENT/", "Dam inspection completed")
```

## Reading Text

```rust
if let Some(text) = dss.read_text("/PROJECT/SITE/NOTE///COMMENT/")? {
    println!("Note: {text}");
} else {
    println!("Record not found");
}
```

**Python:**
```python
text = dss.read_text("/PROJECT/SITE/NOTE///COMMENT/")
if text is not None:
    print(f"Note: {text}")
```

## Notes

- Returns `None` (Rust `Option`) / `None` (Python) if the pathname doesn't exist
- Text records use DSS type code 300
- Maximum text size is limited by available disk space
- Pathname must follow the `/A/B/C/D/E/F/` format
