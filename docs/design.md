
# MiniKV — 完整设计文档

## 1. 项目概述

**MiniKV** 是一个基于 Rust 的 Key-Value 存储引擎，以学习 Rust 系统编程和求职为目标。

**核心设计理念：**
- 简单优先：从最小可行版本开始，逐步迭代
- 工程实践：测试先行、渐进式模块化、标准 Rust 生态工具链
- 面试引导：每个设计决策对应系统编程面试常见考点

## 2. 技术栈

| 类别 | 选择                                    | 理由 |
|------|---------------------------------------|------|
| 语言 | Rust (edition 2024)                   | 系统编程首选 |
| 错误处理 | `thiserror` (lib) + `anyhow` (binary) | 生态标准做法 |
| CRC32 | `crc32fast`                           | 最快的纯 Rust CRC32 实现 |
| CLI 框架 | `clap`                                | Rust 生态最成熟的 CLI 解析库 |
| 测试 | `#[cfg(test)]` + `tests/` 集成测试        | 标准 Rust 测试 |
| 构建 | Cargo workspace（单个 crate）             | 最简单且灵活 |
| 并发 | 无（调用者自包 `Mutex<Engine>`）              | MVP 阶段不做内部并发 |

## 3. 设计决策

### 3.1 项目范围

| 属性 | 值 |
|------|-----|
| MVP 范围 | 内存引擎 + WAL 持久化 + 自动快照恢复 |
| 后续扩展 | range scan、BTreeMap 索引、LSM-tree |
| Crate 类型 | 库（lib）+ 二进制（binary），同一 crate |
| 公开 API | `Engine::open`, `put`, `get`, `delete` |

### 3.2 数据模型

- Key: `Vec<u8>`
- Value: `Vec<u8>`
- 内存索引：`HashMap<Vec<u8>, Vec<u8>>`
- 删除语义：直接 `HashMap::remove()`（不做 tombstone）

### 3.3 并发模型

- 引擎本身不处理并发
- 所有方法签名用 `&mut self`
- 多线程场景下调用者自行 `Arc<Mutex<Engine>>`
- 理由：
    - MVP 阶段简化实现
    - 读操作不多时 `&mut self` 的开销可忽略
    - 未来可升级为 `RwLock` 分离读写路径

### 3.4 WAL（Write-Ahead Log）

#### 格式定义

```
每条记录二进制布局（小端序）：
+--------+----------+-----------+------------+-----------+-----------+
| opcode | key_len  |   key     | value_len  |   value   |  crc32    |
| (1 B)  | (4 B LE) | (N B)     | (4 B LE)   | (M B)     | (4 B LE)  |
+--------+----------+-----------+------------+-----------+-----------+
```

- `opcode`：`0x01` = Put, `0x02` = Delete
- `key_len`：u32 小端序，表示 key 的字节数
- `key`：原始字节（N = key_len）
- `value_len`：u32 小端序，表示 value 的字节数（Delete 操作 = 0）
- `value`：原始字节（M = value_len，Delete 操作不占空间）
- `crc32`：前 4 个字段（opcode + key_len + key + value_len + value）的 CRC32 校验值

#### 写入策略

- 每次 `put()` 或 `delete()` 同步追加一条记录到 WAL
- 每次写入后做 `file.sync_all()` 确保数据落盘
- 文件路径：`<data_dir>/wal.log`

#### 恢复流程（启动时）

1. 尝试打开并加载 `<data_dir>/snap.sst` 到 HashMap（若存在）
2. 打开 `<data_dir>/wal.log`，逐条回放：
    - 对每条记录计算 CRC32 与存储值比对，不匹配则标记损坏并停止回放
    - `Put` → `map.insert(key, value)`
    - `Delete` → `map.remove(key)`
3. 加载完成

### 3.5 快照（Snapshot）

#### 格式定义

```
+-----------+--------+------------+--------+---
| key_len   |  key   | value_len  | value  | 下一个...
| (4 B LE)  | (N B)  | (4 B LE)   | (M B)  |
+-----------+--------+------------+--------+---
```

- 按 `HashMap` 迭代顺序逐条写入
- 不包含 opcode（快照只包含 Put 条目）
- 不包含 crc32（完整性和 WAL 校验）
- 文件路径：`<data_dir>/snap.sst`

#### 触发阈值

- **条件**：WAL 文件大小超过 10 MB
- 时机：每次 `put()` / `delete()` 操作完成后检查文件大小
- 注意：文件大小检查不应增加显著开销，用 `metadata().len()`（元数据调用，不读内容）

#### 写入流程

1. 创建 `<data_dir>/snap.tmp`
2. 遍历 `HashMap`，写入快照格式的记录
3. `file.sync_all()` 确保快照落盘
4. 原子重命名 `snap.tmp` → `snap.sst`（`std::fs::rename`，NTFS 下是原子的）
5. Truncate WAL 文件：`file.set_len(0)` → 置文件指针到开头
6. 后续操作直接覆盖从 0 位置写入

#### 启动优先级

1. 加载 `snap.sst`（若存在）
2. 回放 `wal.log`（若存在）
3. 两者都不存在 → 空引擎

### 3.6 项目结构

```
minikv/
├── Cargo.toml
├── src/
│   ├── lib.rs          ← Engine 核心 + 模块声明
│   ├── wal.rs          ← WAL 写入/恢复 (M2 引入)
│   ├── snapshot.rs     ← 快照 dump/load (M3 引入)
│   └── main.rs         ← CLI binary (M4 引入)
├── tests/
│   └── integration.rs  ← 集成测试
└── README.md           ← 项目文档 (M4 引入)
```

**模块拆分策略（按里程碑）**：

| 里程碑 | 文件变化 |
|--------|----------|
| M1 | 仅 `lib.rs`，所有代码在单文件中 |
| M2 | 拆出 `wal.rs`，`lib.rs` 引入 `mod wal;` |
| M3 | 拆出 `snapshot.rs`，`lib.rs` 引入 `mod snapshot;` |
| M4 | 新增 `main.rs`，调整 `Cargo.toml` 为 `[[bin]]` + `[lib]` |

### 3.7 Public API 终稿

```rust
use std::path::Path;

pub struct Engine { /* ... */ }

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Corrupted data: {0}")]
    Corruption(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl Engine {
    /// 打开或创建数据目录，恢复上次的状态（快照 + WAL）
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
      todo!()
    }

    /// 写入 key-value，同步写入 WAL
    pub fn put(&mut self, key: &[u8], value: &[u8]) -> Result<()> {
      todo!()
    }

    /// 读取 key，返回 `Ok(None)` 表示不存在
    pub fn get(&mut self, key: &[u8]) -> Result<Option<Vec<u8>>> {
      todo!()
    }

    /// 删除 key，同步写入 WAL
    pub fn delete(&mut self, key: &[u8]) -> Result<()> {
      todo!()
    }
}
```

### 3.8 CLI 接口

```sh
# 写
minikv --db ./data put hello world

# 读
minikv --db ./data get hello

# 删
minikv --db ./data delete hello
```

- 参数 `--db` / `-d`：数据目录路径，默认 `./minikv-data`
- 子命令：`put`、`get`、`delete`
- 输出：`get` 成功时打印 value 到 stdout；`put`/`delete` 成功时无输出，失败时通过 anyhow 打印错误到 stderr

### 3.9 测试策略

#### 单元测试（`src/lib.rs`）

覆盖以下场景：

| 测试名 | 场景 | 验证点 |
|--------|------|--------|
| `test_put_get` | put 后 get | 返回值正确 |
| `test_get_missing` | get 不存在的 key | 返回 `Ok(None)` |
| `test_overwrite` | 覆盖同一个 key | 新值覆盖旧值 |
| `test_delete` | put → delete → get | 返回 `Ok(None)` |
| `test_delete_nonexistent` | 删不存在的 key | 不 panic，返回 `Ok(())` |
| `test_put_empty_value` | value 为空 | 存储正确 |
| `test_put_empty_key` | key 为空 | 存储正确 |
| `test_delete_then_put` | delete → put → get | 正常读写 |

#### 集成测试（`tests/integration.rs`）

| 测试名 | 场景 | 验证点 |
|--------|------|--------|
| `test_wal_recovery` | 写入 N 条 → 新 Engine 打开 | 所有数据恢复 |
| `test_snapshot_recovery` | 写入超过阈值条数 → 快照触发 → 重开 | 数据恢复且 WAL 较小 |
| `test_empty_recovery` | 空目录打开 | 不报错，get 空 |
| `test_corrupt_wal` | 手动破坏 WAL 末尾字节 | 回放到损坏点停止或报错 |
| `test_delete_in_wal` | 写入 A → 删 A → 重开 | A 不存在 |
| `test_snapshot_with_deletes` | 写入 A→B→C→删 A→快照→重开 | B/C 存在，A 不存在 |

集成测试要求：
- 每个测试用例在独立临时目录运行（`tempfile` crate，或 `std::env::temp_dir()` 手动）
- 测试后清理临时目录
- 快照相关测试需验证 `snap.sst` 文件确实被创建

### 3.10 快照触发阈值详细说明

```
每次 put/delete 操作最后：
1. 获取 WAL 文件当前大小（`metadata("wal.log")?.len()`）
2. 如果大小 > SNAPSHOT_THRESHOLD（10 MB）：
   a. dump HashMap 到 snap.tmp
   b. fsync
   c. rename snap.tmp → snap.sst
   d. truncate wal.log (set_len(0))
   e. seek wal 到开头
```

注意：
- 快照写入过程中发生 crash → 下次启动时 snap.tmp 不存，snap.sst 为旧快照，WAL 完整可回放（不丢数据）
- truncate 后 crash → 下次启动加载 snap.sst + 空 WAL，不丢已快照数据

---

## 4. 里程碑计划

### M1：核心引擎（单文件 `src/lib.rs`）

**依赖**：`Cargo.toml` 中仅 `thiserror`

**实现项**：
- `Engine` 结构体 + `HashMap<Vec<u8>, Vec<u8>>`
- `Engine::open(path)` — 创建数据目录（`create_dir_all`），无持久化恢复
- `put` / `get` / `delete`
- 7 个单元测试（见 3.9 节）

**不实现**：
- WAL
- 快照
- CLI
- 错误类型除 `Io` 外的变体

**验收标准**：
```
cargo test 全部通过
```

### M2：WAL 写入 + 恢复

**依赖**：新增 `crc32fast`

**实现项**：
- 新建 `src/wal.rs`，包含：
    - `WalWriter`：打开 wal.log，写入二进制记录，sync
    - `WalReader`：打开 wal.log，逐条读取 + CRC 校验
    - `WalEntry` 枚举：`Put(Vec<u8>, Vec<u8>)` / `Delete(Vec<u8>)`
- `lib.rs` 修改：
    - `Engine` 在 `open()` 时尝试回放 WAL
    - `put()` / `delete()` 追加 WAL 后 sync
    - `get()` 不需要修改
- 集成测试：`test_wal_recovery`、`test_delete_in_wal`、`test_corrupt_wal`、`test_empty_recovery`

**不实现**：
- 快照
- CLI

**验收标准**：
```
cargo test 全部通过
两次运行 cargo test 之间不残留状态（每个测试用独立目录）
```

### M3：快照

**依赖**：无新增

**实现项**：
- 新建 `src/snapshot.rs`，包含：
    - `dump_snapshot(map, path)` — 遍历 HashMap 写入文件
    - `load_snapshot(path)` — 读取并重建 HashMap
- `lib.rs` 修改：
    - `open()` 时先加载 snap.sst 再回放 WAL
    - `put()` / `delete()` 操作后检查 WAL 大小，超过阈值触发快照
    - 快照流程：dump → sync → rename → truncate
- 集成测试：`test_snapshot_recovery`、`test_snapshot_with_deletes`

**验收标准**：
```
大量写入后（>10MB），snap.sst 文件存在
重启后数据完整
重启后 WAL 文件很小
```

### M4：CLI + 完善

**依赖**：新增 `clap`

**实现项**：
- 新建 `src/main.rs`
- `clap` 定义参数和子命令
- `main()` 调用 `Engine::open` + 子命令分发
- `Cargo.toml` 配置 `[[bin]]` + `[lib]` 共存

**不实现**：
- 无进一步功能扩展

**验收标准**：
```sh
cargo run -- --db /tmp/test put hello world
cargo run -- --db /tmp/test get hello
# 输出：world
```

---

## 5. 错误处理策略

### Engine 错误类型

```rust
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Corrupted data at offset {offset}: {detail}")]
    Corruption { offset: u64, detail: String },
}
```

- `Io`：文件打开/写入/读取失败
- `Corruption`：WAL 回放时 CRC32 校验不通过
- `put`/`get`/`delete` 均为 `Result<()>` / `Result<Option<Vec<u8>>>`

### Binary 错误处理

- 使用 `anyhow::Result` 作为 `main()` 返回类型
- `?` 运算符自动转换 `Engine::Error` → `anyhow::Error`
- 错误信息输出到 stderr（`eprintln!` 或 `anyhow` 默认行为）

### 边界情况处理

| 情况 | 行为 |
|------|------|
| 数据目录不存在 | `open()` 时创建（`create_dir_all`） |
| WAL 为空文件 | 视为无记录，正常启动 |
| WAL 只有部分记录 | 回放完完整记录，忽略末尾不完整字节（报 Corruption） |
| 快照文件损坏 | 跳过快照，仅回放 WAL（WAL 恢复是安全的） |
| 快照不存在但 WAL 存在 | 仅回放 WAL |
| 快照存在但 WAL 不存在 | 仅加载快照 |
| 两者都不存在 | 空引擎 |
| 磁盘空间满 | 返回 `Io` 错误，上层处理 |
| Key/value 长度超过 u32 上限 | 目前不做硬限制（由序列化时的类型隐式限制，约 4GB） |

---

## 6. 文件目录布局（运行时）

```
<data_dir>/
├── wal.log        ← 写前日志（追加写入）
└── snap.sst       ← 快照文件（仅在快照后存在）
```

- 所有文件在 `<data_dir>/` 目录下
- `<data_dir>` 由用户在 `Engine::open(path)` 或 CLI `--db` 参数中指定

---

## 7. 后续扩展方向（M4 后）

这些不纳入当前计划，仅记录供参考：

| 方向 | 说明 |
|------|------|
| Range Scan | 替换 `HashMap` 为 `BTreeMap`，支持 `scan(from, to)` 有序遍历 |
| LSM-tree | 分层 SSTable、Compaction、Bloom Filter |
| 多线程 | `RwLock<HashMap>` + 独立 Compaction 线程 + Batch WAL Writer |
| gRPC Server | 用 `tonic` 包装为 gRPC 服务，支持网络访问 |
| Benchmark | 用 `criterion` 做性能基准测试 |
| WASM | 编译为 WASM 在浏览器中运行 |

