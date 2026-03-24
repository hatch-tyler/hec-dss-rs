# Array Records

Array records store generic integer, float, and/or double arrays. A single record can contain any combination of the three types.

## Writing

```rust
// Integer array
dss.write_array("/PROJECT/DATA/INDICES///V1/", &[1, 2, 3, 4, 5], &[], &[])?;

// Double array
dss.write_array("/PROJECT/DATA/VALUES///V1/", &[], &[], &[1.1, 2.2, 3.3])?;

// Mixed: integers + doubles
dss.write_array("/PROJECT/DATA/MIXED///V1/",
    &[10, 20, 30],           // integers
    &[],                      // floats (empty)
    &[1.5, 2.5, 3.5],        // doubles
)?;
```

**Python:**
```python
dss.write_array("/PROJECT/DATA/VALUES///V1/", double_values=[1.1, 2.2, 3.3])
dss.write_array("/PROJECT/DATA/MIXED///V1/",
                int_values=[10, 20, 30], double_values=[1.5, 2.5, 3.5])
```

## Reading

```rust
if let Some(arr) = dss.read_array("/PROJECT/DATA/MIXED///V1/")? {
    println!("Ints: {:?}", arr.int_values);
    println!("Floats: {:?}", arr.float_values);
    println!("Doubles: {:?}", arr.double_values);
}
```

**Python:**
```python
result = dss.read_array("/PROJECT/DATA/MIXED///V1/")
if result is not None:
    print(f"Ints: {result['int_values']}")
    print(f"Doubles: {result['double_values']}")
```
