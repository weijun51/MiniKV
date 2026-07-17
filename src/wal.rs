use crate::{Error, Result};

pub enum WalEntry {
    Put(Vec<u8>, Vec<u8>), // 0x01 = Put, 0x02 = Delete
    Delete(Vec<u8>),
}

// 小端
// opcode(1B) + key_len(4B LE) + key(N B) + value_len(4B LE) + value(M B) + crc32(4B LE)
pub fn encode(entry: &WalEntry) -> Vec<u8> {
    let (opcode, key, value) = match entry {
        WalEntry::Put(k, v) => (0x01, k, Some(v)),
        WalEntry::Delete(k) => (0x02, k, None),
    };
    let key_len = key.len() as u32;
    let value_len = value.map_or(0, |v| v.len() as u32);
    let mut buf = Vec::new();
    buf.push(opcode);
    buf.extend_from_slice(&key_len.to_le_bytes()); // 将切片（Slice）中的所有元素复制并追加到 Vec 末尾
    buf.extend_from_slice(key);
    buf.extend_from_slice(&value_len.to_le_bytes());
    if let Some(v) = value {
        buf.extend_from_slice(v);
    }
    // CRC32 覆盖前 5 个字段
    let crc = crc32fast::hash(&buf);
    buf.extend_from_slice(&crc.to_le_bytes());
    buf
}

pub fn decode(bytes: &[u8]) -> Result<WalEntry> {
    if bytes.len() < 9 {
        return Err(Error::DataTooShort("WAL entry too short".into()));
    }

    let stored_crc = u32::from_le_bytes(bytes[bytes.len() - 4..].try_into().unwrap());
    let data = &bytes[..bytes.len() - 4];
    let actual_crc = crc32fast::hash(data);
    if stored_crc != actual_crc {
        return Err(Error::CorruptData("CRC mismatch".into()));
    }

    let opcode = bytes.get(0);
    if opcode.is_none() {
        return Err(Error::CorruptData("Invalid opcode".to_string()));
    }
    let opcode = opcode.unwrap();
    let key_len = u32::from_le_bytes(
        bytes.get(1..5)
            .ok_or_else(|| Error::DataTooShort("WAL entry too short".into()))?
            .try_into()
            .unwrap()
    );
    let kl = key_len as usize;
    let key = &bytes[5..5 + key_len as usize];
    let value_len = u32::from_le_bytes(
        bytes.get(5 + kl..9 + kl)
            .ok_or_else(|| Error::DataTooShort("value_len out of bounds".into()))?
            .try_into()
            .map_err(|_| Error::DataTooShort("value_len conversion failed".into()))?
    );

    match opcode {
        0x01 => {
            let value = bytes[9 + value_len as usize..].to_vec();
            Ok(WalEntry::Put(key.to_vec(), value))
        }
        0x02 => Ok(WalEntry::Delete(key.to_vec())),
        _ => Err(Error::CorruptData("Unknown opcode".into())),
    }
}