use std::collections::HashMap;
use std::path::Path;

pub struct Engine {
    map: HashMap<Vec<u8>, Vec<u8>>,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Corrupt data: {0}")]
    CorruptData(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl Engine {

    // 打开或创建数据目录，恢复上次的状态（快照 + WAL）
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        // 判断文件是否存在，如不存在则创建
        if !path.exists() {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::create_dir_all(path)?;
        }
        Ok(Self {
            map: HashMap::new()
        })
    }

    // 写入 key-value，同步写入 WAL
    pub fn put(&mut self, key: &[u8], value: &[u8]) -> Result<()> {
        self.map.insert(key.to_vec(), value.to_vec());
        Ok(())
    }

    // 读取 key，返回 `Ok(None)` 表示不存在
    pub fn get(&mut self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        Ok(self.map.get(key).cloned())
    }

    // 删除 key，同步写入 WAL
    pub fn delete(&mut self, key: &[u8]) -> Result<()> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use crate::Engine;

    // put 后 get
    #[test]
    fn test_put_get() {
        let mut engine = Engine::open("./test").unwrap();
        engine.put(b"vv", b"I'm vv!").unwrap();
        let v = engine.get(b"vv").unwrap();
        assert_eq!(v, Some(b"I'm vv!".to_vec()));
    }
}