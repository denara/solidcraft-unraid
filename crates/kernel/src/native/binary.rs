//! Bounded tagged binary values: integer and f64 bits stay exact, no base64/text numbers.
use super::*;
fn bytes<'a>(input: &mut &'a [u8], n: usize) -> Result<&'a [u8]> {
    let head = input.get(..n).ok_or_else(|| invalid("truncated value"))?;
    *input = input.get(n..).ok_or_else(|| invalid("truncated value"))?;
    Ok(head)
}
fn length(input: &mut &[u8]) -> Result<usize> {
    let raw: [u8; 4] = bytes(input, 4)?.try_into().map_err(invalid)?;
    usize::try_from(u32::from_le_bytes(raw)).map_err(invalid)
}
fn string(input: &mut &[u8]) -> Result<String> {
    let n = length(input)?;
    if n > 1 << 20 {
        return Err(invalid("string budget"));
    }
    Ok(std::str::from_utf8(bytes(input, n)?).map_err(invalid)?.to_owned())
}
pub(super) fn read(input: &mut &[u8], depth: usize, budget: &mut usize) -> Result<Value> {
    if depth > 48 || *budget == 0 {
        return Err(invalid("value budget/depth"));
    }
    *budget -= 1;
    let tag = *bytes(input, 1)?.first().ok_or_else(|| invalid("tag"))?;
    Ok(match tag {
        0 => Value::Null,
        1 => Value::Bool(false),
        2 => Value::Bool(true),
        3..=5 => {
            let raw: [u8; 8] = bytes(input, 8)?.try_into().map_err(invalid)?;
            Value::Number(match tag {
                3 => Number::from(i64::from_le_bytes(raw)),
                4 => Number::from(u64::from_le_bytes(raw)),
                _ => Number::from_f64(f64::from_le_bytes(raw)).ok_or_else(|| invalid("non-finite number"))?,
            })
        }
        6 => Value::String(string(input)?),
        7 | 8 => {
            let n = length(input)?;
            if n > MAX_ITEMS || n > input.len() || n > *budget {
                return Err(invalid("collection budget"));
            }
            if tag == 7 {
                let mut a = Vec::new();
                for _ in 0..n {
                    a.push(read(input, depth + 1, budget)?);
                }
                Value::Array(a)
            } else {
                let mut map = Map::new();
                for _ in 0..n {
                    let key = string(input)?;
                    if map.contains_key(&key) {
                        return Err(invalid("duplicate key"));
                    }
                    map.insert(key, read(input, depth + 1, budget)?);
                }
                Value::Object(map)
            }
        }
        _ => return Err(invalid("unknown binary tag")),
    })
}
fn count(n: usize, out: &mut Vec<u8>) -> Result<()> {
    out.extend_from_slice(&u32::try_from(n).map_err(invalid)?.to_le_bytes());
    Ok(())
}
fn text(s: &str, out: &mut Vec<u8>) -> Result<()> {
    count(s.len(), out)?;
    out.extend_from_slice(s.as_bytes());
    Ok(())
}
pub(super) fn write(value: &Value, out: &mut Vec<u8>, depth: usize) -> Result<()> {
    if depth > 48 || out.len() > MAX_BYTES {
        return Err(invalid("encoding budget"));
    }
    match value {
        Value::Null => out.push(0),
        Value::Bool(b) => out.push(if *b { 2 } else { 1 }),
        Value::Number(n) => {
            if n.is_f64() {
                out.push(5);
                out.extend_from_slice(&n.as_f64().ok_or_else(|| invalid("number"))?.to_le_bytes());
            } else if let Some(n) = n.as_u64() {
                out.push(4);
                out.extend_from_slice(&n.to_le_bytes());
            } else {
                out.push(3);
                out.extend_from_slice(&n.as_i64().ok_or_else(|| invalid("integer"))?.to_le_bytes());
            }
        }
        Value::String(s) => {
            out.push(6);
            text(s, out)?;
        }
        Value::Array(a) => {
            out.push(7);
            count(a.len(), out)?;
            for v in a {
                write(v, out, depth + 1)?;
            }
        }
        Value::Object(map) => {
            out.push(8);
            count(map.len(), out)?;
            for (k, v) in map {
                text(k, out)?;
                write(v, out, depth + 1)?;
            }
        }
    }
    if out.len() > MAX_BYTES {
        return Err(invalid("encoding size"));
    }
    Ok(())
}
