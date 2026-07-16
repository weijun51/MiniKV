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
    #[error("操作失败: {0}")]
    OperateError(String),
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
    pub fn delete(&mut self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        match self.map.remove(key) {
            None => Err(Error::OperateError("删除的 key 不存在".to_string())),
            Some(v) => Ok(Some(v)),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::Engine;
    use pretty_assertions::assert_eq;

    // cargo test test_put_get -- --show-output
    // put 后 get
    #[test]
    fn test_put_get() {
        let mut engine = Engine::open("./test").unwrap();
        engine.put(b"vv", b"I'm vv!").unwrap();
        let v = engine.get(b"vv").unwrap();
        let vv = v.clone()
            .map(|vec| String::from_utf8(vec))
            .transpose()
            .unwrap();
        println!("[test_put_get] 获取到的值: {:?}", vv);
        assert_eq!(v, Some(b"I'm vv!".to_vec()));
    }

    // get 不存在的 key
    #[test]
    fn test_get_missing() {
        let mut engine = Engine::open("./test").unwrap();
        let v = engine.get(b"noExit").unwrap();
        println!("[test_get_missing] 获取不存在的 key 值: {:?}", v);
        assert_eq!(engine.get(b"noExit").unwrap(), None);
    }

    // 覆盖同一个 key
    #[test]
    fn test_overwrite() {
        let mut engine = Engine::open("./test").unwrap();
        engine.put(b"vv", b"I'm vv!").unwrap();
        engine.put(b"vv", b"Hello World!").unwrap();
        engine.put(b"vv", b"Oh, my god!").unwrap();
        let value = engine.get(b"vv").unwrap();
        let value_bak = value.clone();
        let show = value_bak.map(|v| {
            String::from_utf8(v.to_vec())
        }).transpose()
            .unwrap_or_default();
        println!("[test_overwrite] 获取到的值: {:?}", show);
        assert_eq!(Some(Vec::from(&b"Oh, my god!"[..])), value);
    }

    // put → delete → get
    #[test]
    fn test_delete() {
        let mut engine = Engine::open("./test").unwrap();
        engine.put(b"vv", b"I'm vv!").unwrap();
        engine.delete(b"vv").unwrap();
        let value = engine.get(b"vv").unwrap();
        let value_bak = value.clone();
        if value.is_none() {
            println!("[test_delete] 获取到的 value 为空");
        } else {
            println!("[test_delete] 获取到 value 的值不为空");
        }
        assert_eq!(None, value_bak);
    }

    // 删不存在的 key
    #[test]
    fn test_delete_nonexistent() {
        let mut engine = Engine::open("./test").unwrap();
        let result = match engine.delete(b"vv") {
            Ok(num) => {
                println!("[test_delete_nonexistent] 删除成功: {:?}", num);
                Ok(num)
            },
            Err(e) => {
                println!("[test_delete_nonexistent] 删除失败: {:?}", e);
                Err(e)
            }
        };
        assert!(result.is_err())
    }

    // value 为空
    #[test]
    fn test_put_empty_value() {
        let mut engine = Engine::open("./test").unwrap();
        engine.put(b"vv", b"").unwrap();
        println!("[test_put_empty_value] 已放入空 value");
        let value = engine.get(b"vv")
            .unwrap()
            .map(|v| String::from_utf8(v.to_vec()))
            .transpose()
            .unwrap_or_default();
        println!("[test_put_empty_value] 获取 value：: {:?}", value);
        assert_eq!(Some("".to_string()), value);
    }

    // key 为空
    #[test]
    fn test_put_empty_key() {
        let mut engine = Engine::open("./test").unwrap();
        engine.put(b"", b"I'm vv").unwrap();
        println!("[test_put_empty_key] 已放入空 key");
        let value = engine.get(b"")
            .unwrap()
            .map(|v| String::from_utf8(v.to_vec()))
            .transpose()
            .unwrap_or_default();
        println!("[test_put_empty_key] 获取 value：: {:?}", value);
        assert_eq!(Some("I'm vv".to_string()), value);
    }

    // delete → put → get
    // #[test]
    // fn test_delete_then_put() {
    //
    // }

}