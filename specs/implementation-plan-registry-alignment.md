# sdkwork-discovery 行业专业注册中心对齐实施计划

## 架构概览

```
┌─────────────────────────────────────────────────────────┐
│  Proto Layer (proto/)                                   │
│  discovery_types.proto / registry_service.proto / ...    │
├─────────────────────────────────────────────────────────┤
│  RPC Adapter (crates/sdkwork-discovery-rpc/)             │
│  actor.rs / services.rs / codec.rs / watch.rs / server.rs│
├─────────────────────────────────────────────────────────┤
│  Core Control Plane (crates/sdkwork-discovery-core/)     │
│  control_plane.rs / permissions.rs / policy.rs           │
├─────────────────────────────────────────────────────────┤
│  Storage Contract (crates/sdkwork-discovery-storage-contract/) │
│  RegistryStore / ConfigStore / WatchEventStore           │
├──────────┬──────────────┬───────────────────────────────┤
│  Memory  │    SQLite    │         Postgres               │
│  Store   │    Store     │         Store                   │
└──────────┴──────────────┴───────────────────────────────┘
```

---

## Phase 1: P0 - 生产就绪 (4-6 周)

### 1.1 乐观并发控制 (CAS - Compare-And-Swap)

**目标**: 注册/更新操作支持 revision-based 条件更新，防止并发写覆盖。

**当前问题**: `register_instance` 使用 `ON CONFLICT ... DO UPDATE` 无条件覆盖，无 CAS 保护。

**实现方案**:

#### 1.1.1 Contract 层变更

**文件**: `crates/sdkwork-discovery-contract/src/registry.rs`

```rust
// RegisterInstanceCommand 增加可选 CAS 字段
pub struct RegisterInstanceCommand {
    // ... existing fields ...
    pub expected_revision: Option<u64>,  // None = 无条件更新, Some = CAS
}

// ReportInstanceStatusCommand 增加 CAS
pub struct ReportInstanceStatusCommand {
    // ... existing fields ...
    pub expected_revision: Option<u64>,
}

// 新增错误变体
pub enum DiscoveryError {
    // ... existing variants ...
    Conflict(String),  // CAS 失败时返回
}
```

#### 1.1.2 Storage Contract 层变更

**文件**: `crates/sdkwork-discovery-storage-contract/src/registry_store.rs`

trait 签名不变，`RegisterInstanceCommand` 内嵌的 `expected_revision` 由 store 实现解读。

#### 1.1.3 存储层实现

**SQLite** (`crates/sdkwork-discovery-storage-sqlite/src/sql.rs`):

```sql
-- REGISTER_INSTANCE CAS 变体
-- 当 expected_revision 有值时，在 ON CONFLICT 子句增加 WHERE 条件
ON CONFLICT (namespace, environment, service_name, instance_id)
DO UPDATE SET
    endpoint = excluded.endpoint,
    ...
WHERE excluded.expected_revision IS NULL
   OR discovery_service_instance.version = excluded.expected_revision
```

实际实现更安全的做法是拆分为两条 SQL：
- `REGISTER_INSTANCE_NEW`: 纯 INSERT（首次注册）
- `REGISTER_INSTANCE_CAS`: UPDATE ... WHERE revision = ? AND deleted_at IS NULL

**Postgres**: 同构 SQL，使用 `$N` 绑定参数。

**Memory** (`crates/sdkwork-discovery-storage-memory/src/registry.rs`):
在 `register_instance` 中检查 `expected_revision`，如果存在且不匹配当前 `ServiceInstance.revision`，返回 `DiscoveryError::Conflict`。

#### 1.1.4 Control Plane 层

**文件**: `crates/sdkwork-discovery-core/src/control_plane.rs`

无需修改，CAS 逻辑下沉到 store 层。

#### 1.1.5 Proto 层

**文件**: `proto/sdkwork/discovery/common/v1/discovery_types.proto`

```protobuf
message RegisterInstanceRequest {
    // ... existing fields ...
    optional uint64 expected_revision = 20;
}
```

#### 1.1.6 测试策略

- Memory store: 注册 → 并发更新（相同 expected_revision）→ 一个成功一个 Conflict
- SQLite/Postgres: 同上，验证 SQL 正确性
- 集成测试: gRPC 层传入 expected_revision → 验证 Conflict 错误码映射

**涉及文件**:
- `crates/sdkwork-discovery-contract/src/registry.rs`
- `crates/sdkwork-discovery-contract/src/error.rs`
- `crates/sdkwork-discovery-storage-contract/src/registry_store.rs`（签名不变，文档更新）
- `crates/sdkwork-discovery-storage-memory/src/registry.rs`
- `crates/sdkwork-discovery-storage-sqlite/src/sql.rs`
- `crates/sdkwork-discovery-storage-sqlite/src/store.rs`
- `crates/sdkwork-discovery-storage-postgres/src/sql.rs`
- `crates/sdkwork-discovery-storage-postgres/src/store.rs`
- `crates/sdkwork-discovery-rpc/src/codec.rs`
- `crates/sdkwork-discovery-rpc/src/error.rs`
- `proto/sdkwork/discovery/common/v1/discovery_types.proto`
- `proto/sdkwork/discovery/internal/v1/registry_service.proto`
- 各 crate 的 tests/ 目录

---

### 1.2 主动健康检查 (Active Health Checking)

**目标**: 除被动 self-report 外，支持 HTTP/TCP/gRPC 主动探测实例健康状态。

**当前问题**: 仅有 `ReportInstanceStatus` 被动上报，实例宕机后依赖 lease 过期才发现。

**实现方案**:

#### 1.2.1 新增 crate: `sdkwork-discovery-health-checker`

不在现有 crate 中嵌入，新建独立 crate，遵循项目 crate 拆分模式。

**目录**: `crates/sdkwork-discovery-health-checker/`

```rust
// crates/sdkwork-discovery-health-checker/src/lib.rs

pub struct HealthCheckConfig {
    pub check_interval_ms: u64,
    pub timeout_ms: u64,
    pub unhealthy_threshold: u32,
    pub healthy_threshold: u32,
}

pub enum HealthCheckProbe {
    Http { path: String, expected_status: u16 },
    Tcp,
    Grpc { service_name: String },
}

#[async_trait]
pub trait HealthChecker: Send + Sync {
    async fn check(&self, endpoint: &str, probe: &HealthCheckProbe) -> HealthCheckResult;
}

pub struct HealthCheckResult {
    pub healthy: bool,
    pub latency_ms: u64,
    pub message: Option<String>,
}
```

#### 1.2.2 Domain 层扩展

**文件**: `crates/sdkwork-discovery-contract/src/registry.rs`

```rust
// RegisterInstanceCommand 增加健康检查配置
pub struct RegisterInstanceCommand {
    // ... existing fields ...
    pub health_check: Option<HealthCheckConfig>,
}

// ServiceInstance 增加健康检查配置存储
pub struct ServiceInstance {
    // ... existing fields ...
    pub health_check: Option<HealthCheckConfig>,
}

// 新增类型
pub struct HealthCheckConfig {
    pub probe: HealthCheckProbe,
    pub interval_ms: u64,
    pub timeout_ms: u64,
    pub unhealthy_threshold: u32,
    pub healthy_threshold: u32,
}

pub enum HealthCheckProbe {
    Http { path: String, expected_status: u16 },
    Tcp,
    Grpc { service_name: String },
}
```

#### 1.2.3 存储层

**SQLite/Postgres migration** (新增 migration 文件):

```sql
ALTER TABLE discovery_service_instance
    ADD COLUMN health_check_json TEXT;
```

**Memory store**: `ServiceInstance` 已有 `health_check` 字段，直接存储。

#### 1.2.4 Actor 集成

**文件**: `crates/sdkwork-discovery-rpc/src/actor.rs`

在 `run_actor` 的 `tokio::select!` 中增加健康检查定时器：

```rust
// 新增 RuntimeCommand
HealthCheckTick {
    now_ms: u64,
    response: oneshot::Sender<()>,
}
```

Actor 循环中：
1. 查询所有带 `health_check` 配置的实例
2. 按 `check_interval_ms` 分组，到期的执行探测
3. 根据结果调用 `report_instance_status` 更新状态
4. 连续失败达 `unhealthy_threshold` → 设为 `NotServing`
5. 连续恢复达 `healthy_threshold` → 设为 `Serving`

#### 1.2.5 RuntimeConfig 扩展

**文件**: `crates/sdkwork-discovery-config/src/model.rs`

```rust
pub struct HealthCheckConfig {
    pub enabled: bool,
    pub default_interval_ms: u64,
    pub default_timeout_ms: u64,
    pub max_concurrent_checks: usize,
}
```

#### 1.2.6 测试策略

- 单元测试: mock `HealthChecker` trait，验证阈值逻辑
- 集成测试: 启动真实 HTTP/TCP server，验证探测流程
- 边界测试: 超时、连接拒绝、HTTP 非预期状态码

**涉及文件**:
- `crates/sdkwork-discovery-health-checker/` (新 crate)
- `crates/sdkwork-discovery-contract/src/registry.rs`
- `crates/sdkwork-discovery-storage-memory/src/registry.rs`
- `crates/sdkwork-discovery-storage-sqlite/src/sql.rs`
- `crates/sdkwork-discovery-storage-sqlite/src/store.rs`
- `crates/sdkwork-discovery-storage-postgres/src/sql.rs`
- `crates/sdkwork-discovery-storage-postgres/src/store.rs`
- `crates/sdkwork-discovery-rpc/src/actor.rs`
- `crates/sdkwork-discovery-config/src/model.rs`
- `Cargo.toml` (workspace members)
- 新 migration 文件

---

### 1.3 标签过滤发现 (Metadata Label Filtering)

**目标**: `DiscoverInstancesQuery` 支持 metadata 标签过滤，类似 AWS CloudMap / Consul tag filtering。

**当前问题**: `DiscoverInstancesQuery` 仅有 `protocol` 过滤，metadata 完全不参与查询。

**实现方案**:

#### 1.3.1 Contract 层

**文件**: `crates/sdkwork-discovery-contract/src/registry.rs`

```rust
pub struct DiscoverInstancesQuery {
    pub namespace: String,
    pub environment: String,
    pub service_name: String,
    pub healthy_only: bool,
    pub protocol: Option<String>,
    pub label_filters: Vec<LabelFilter>,  // 新增
}

pub struct LabelFilter {
    pub key: String,
    pub op: LabelFilterOp,
    pub value: String,
}

pub enum LabelFilterOp {
    Eq,       // key == value
    NotEq,    // key != value
    In,       // key in (comma-separated values)
    Exists,   // key exists
}
```

#### 1.3.2 Memory Store

**文件**: `crates/sdkwork-discovery-storage-memory/src/registry.rs`

在 `discover_instances` 过滤链中增加 label filter：

```rust
// 在现有 protocol 过滤之后
if !query.label_filters.is_empty() {
    instances.retain(|instance| {
        query.label_filters.iter().all(|filter| match &filter.op {
            LabelFilterOp::Eq => instance.metadata.get(&filter.key).map_or(false, |v| v == &filter.value),
            LabelFilterOp::NotEq => instance.metadata.get(&filter.key).map_or(true, |v| v != &filter.value),
            LabelFilterOp::In => {
                let values: Vec<&str> = filter.value.split(',').collect();
                instance.metadata.get(&filter.key).map_or(false, |v| values.contains(&v.as_str()))
            }
            LabelFilterOp::Exists => instance.metadata.contains_key(&filter.key),
        })
    });
}
```

#### 1.3.3 SQLite/Postgres

由于 metadata 存储为 JSON 文本，需要使用 JSON 函数：

**SQLite** (`crates/sdkwork-discovery-storage-sqlite/src/sql.rs`):

```sql
-- 在 DISCOVER_INSTANCES 的 WHERE 子句末尾追加
-- 对每个 label filter 生成一个条件
AND json_extract(metadata_json, '$.' || ?) = ?  -- Eq
AND (json_extract(metadata_json, '$.' || ?) IS NULL OR json_extract(metadata_json, '$.' || ?) != ?)  -- NotEq
AND json_extract(metadata_json, '$.' || ?) IN (SELECT value FROM json_each(?))  -- In
AND json_extract(metadata_json, '$.' || ?) IS NOT NULL  -- Exists
```

实际实现中，由于 label filter 数量动态，推荐在 Rust 层拼接 SQL（使用 `format!` 或 query builder）。

**Postgres**: 使用 `metadata_json::jsonb -> key = value` 语法。

#### 1.3.4 Proto 层

**文件**: `proto/sdkwork/discovery/common/v1/discovery_types.proto`

```protobuf
message DiscoverInstancesRequest {
    // ... existing fields ...
    repeated LabelFilter label_filters = 20;
}

message LabelFilter {
    string key = 1;
    LabelFilterOp op = 2;
    string value = 3;
}

enum LabelFilterOp {
    LABEL_FILTER_OP_UNSPECIFIED = 0;
    LABEL_FILTER_OP_EQ = 1;
    LABEL_FILTER_OP_NOT_EQ = 2;
    LABEL_FILTER_OP_IN = 3;
    LABEL_FILTER_OP_EXISTS = 4;
}
```

#### 1.3.5 Codec 层

**文件**: `crates/sdkwork-discovery-rpc/src/codec.rs`

在 `DiscoverInstancesRequest` → `DiscoverInstancesQuery` 转换中增加 label_filters 映射。

#### 1.3.6 测试策略

- Memory store: 注册带 metadata 实例 → 各种 filter 组合查询 → 验证结果
- SQL 测试: 验证 JSON 函数在 SQLite/Postgres 上的行为
- 边界: 空 metadata、不存在的 key、空 value

**涉及文件**:
- `crates/sdkwork-discovery-contract/src/registry.rs`
- `crates/sdkwork-discovery-storage-memory/src/registry.rs`
- `crates/sdkwork-discovery-storage-sqlite/src/sql.rs`
- `crates/sdkwork-discovery-storage-sqlite/src/store.rs`
- `crates/sdkwork-discovery-storage-postgres/src/sql.rs`
- `crates/sdkwork-discovery-storage-postgres/src/store.rs`
- `crates/sdkwork-discovery-rpc/src/codec.rs`
- `proto/sdkwork/discovery/common/v1/discovery_types.proto`
- `proto/sdkwork/discovery/internal/v1/registry_service.proto`

---

### 1.4 权重/优先级路由 (Weight/Priority Routing)

**目标**: `discover_instances` 结果按 priority 分组、weight 加权排序，支持客户端负载均衡。

**当前问题**: `weight`/`priority` 字段已存储但发现结果仅按 `instance_id` 排序。

**实现方案**:

#### 1.4.1 Contract 层

**文件**: `crates/sdkwork-discovery-contract/src/registry.rs`

```rust
pub struct DiscoverInstancesQuery {
    // ... existing fields ...
    pub sort_by: Option<DiscoverSortBy>,  // 新增
}

pub enum DiscoverSortBy {
    InstanceId,          // 默认，当前行为
    Priority,            // priority ASC, 同 priority 按 weight DESC
    Weight,              // weight DESC
    WeightedRandom,      // priority 分组内按 weight 加权随机
}
```

#### 1.4.2 存储层

**Memory** (`crates/sdkwork-discovery-storage-memory/src/registry.rs`):

```rust
match query.sort_by.unwrap_or(DiscoverSortBy::InstanceId) {
    DiscoverSortBy::InstanceId => instances.sort_by(|a, b| a.instance_id.cmp(&b.instance_id)),
    DiscoverSortBy::Priority => instances.sort_by(|a, b| {
        a.priority.cmp(&b.priority)
            .then_with(|| b.weight.cmp(&a.weight))
            .then_with(|| a.instance_id.cmp(&b.instance_id))
    }),
    DiscoverSortBy::Weight => instances.sort_by(|a, b| {
        b.weight.cmp(&a.weight)
            .then_with(|| a.instance_id.cmp(&b.instance_id))
    }),
    DiscoverSortBy::WeightedRandom => {
        // 按 priority 分组，组内按 weight 加权 shuffle
        instances.sort_by(|a, b| a.priority.cmp(&b.priority));
        // 组内 Fisher-Yates shuffle weighted
        weighted_shuffle(&mut instances);
    }
}
```

**SQLite/Postgres**: SQL ORDER BY 支持 `Priority` 和 `Weight` 排序。`WeightedRandom` 在 Rust 层实现（查询后 shuffle）。

```sql
-- Priority 排序
ORDER BY priority ASC, weight DESC, instance_id ASC

-- Weight 排序
ORDER BY weight DESC, instance_id ASC
```

#### 1.4.3 Proto 层

```protobuf
enum DiscoverSortBy {
    DISCOVER_SORT_BY_UNSPECIFIED = 0;  // 默认 InstanceId
    DISCOVER_SORT_BY_INSTANCE_ID = 1;
    DISCOVER_SORT_BY_PRIORITY = 2;
    DISCOVER_SORT_BY_WEIGHT = 3;
    DISCOVER_SORT_BY_WEIGHTED_RANDOM = 4;
}
```

#### 1.4.4 测试策略

- Memory: 注册不同 priority/weight 实例 → 验证排序结果
- 加权随机: 统计分布验证（大量调用后检查分布合理性）

**涉及文件**:
- `crates/sdkwork-discovery-contract/src/registry.rs`
- `crates/sdkwork-discovery-storage-memory/src/registry.rs`
- `crates/sdkwork-discovery-storage-sqlite/src/sql.rs`
- `crates/sdkwork-discovery-storage-sqlite/src/store.rs`
- `crates/sdkwork-discovery-storage-postgres/src/sql.rs`
- `crates/sdkwork-discovery-storage-postgres/src/store.rs`
- `crates/sdkwork-discovery-rpc/src/codec.rs`
- `proto/sdkwork/discovery/common/v1/discovery_types.proto`

---

### 1.5 Watch 事件 GC (Event Garbage Collection)

**目标**: 清理过期 watch 事件，防止 `discovery_watch_event` 表和 Memory `Vec<DiscoveryEvent>` 无限增长。

**当前问题**:
- SQLite/Postgres: `discovery_watch_event` 表只增不删
- Memory: `events: Vec<DiscoveryEvent>` 无限增长

**实现方案**:

#### 1.5.1 WatchEventStore 扩展

**文件**: `crates/sdkwork-discovery-storage-contract/src/watch_event_store.rs`

```rust
#[async_trait]
pub trait WatchEventStore {
    async fn watch_events(&self, query: WatchEventsQuery) -> DiscoveryResult<Vec<DiscoveryEvent>>;

    // 新增
    async fn gc_watch_events(
        &mut self,
        before_revision: u64,
        max_deletes: usize,
    ) -> DiscoveryResult<usize>;  // 返回删除数量
}
```

#### 1.5.2 存储实现

**Memory** (`crates/sdkwork-discovery-storage-memory/src/watch.rs`):

```rust
async fn gc_watch_events(&mut self, before_revision: u64, max_deletes: usize) -> DiscoveryResult<usize> {
    let before = self.events.len();
    self.events.retain(|e| e.revision > before_revision);
    // 如果超过 max_deletes，截断
    if self.events.len() > max_deletes {
        self.events.drain(0..self.events.len() - max_deletes);
    }
    Ok(before - self.events.len())
}
```

**SQLite** (`crates/sdkwork-discovery-storage-sqlite/src/sql.rs`):

```sql
-- 新增 SQL
pub const GC_WATCH_EVENTS: &str = r#"
DELETE FROM discovery_watch_event
WHERE revision IN (
    SELECT revision FROM discovery_watch_event
    WHERE revision <= ?
    AND deleted_at IS NULL
    ORDER BY revision ASC
    LIMIT ?
)
"#;
```

**Postgres**: 同构 SQL。

#### 1.5.3 Actor 集成

**文件**: `crates/sdkwork-discovery-rpc/src/actor.rs`

在 `DiscoveryRpcRuntimeConfig` 增加 GC 配置，在 `run_actor` 的 `tokio::select!` 中增加 GC 定时器：

```rust
// RuntimeConfig 增加
pub struct DiscoveryRpcRuntimeConfig {
    // ... existing fields ...
    pub event_gc_interval_ms: u64,        // 0 = disabled
    pub event_gc_retention_revision: u64,  // 保留最近 N 个 revision
    pub event_gc_batch_size: usize,
}
```

Actor 循环中增加第三个 select arm：
```rust
_ = gc_interval.tick() => {
    let current_revision = control_plane.current_revision();
    let before_revision = current_revision.saturating_sub(config.event_gc_retention_revision);
    let _ = control_plane.gc_watch_events(before_revision, config.event_gc_batch_size).await;
}
```

#### 1.5.4 Control Plane 扩展

**文件**: `crates/sdkwork-discovery-core/src/control_plane.rs`

```rust
pub async fn gc_watch_events(
    &mut self,
    before_revision: u64,
    max_deletes: usize,
) -> DiscoveryResult<usize> {
    self.store.gc_watch_events(before_revision, max_deletes).await
}
```

#### 1.5.5 RuntimeConfig 扩展

**文件**: `crates/sdkwork-discovery-config/src/model.rs`

```rust
pub struct WatchConfig {
    // ... existing fields ...
    pub event_gc_interval_ms: u64,
    pub event_gc_retention_count: u64,
    pub event_gc_batch_size: usize,
}
```

#### 1.5.6 测试策略

- Memory: 生成大量事件 → GC → 验证保留正确数量
- SQLite/Postgres: 同上，验证 SQL 正确性
- 边界: before_revision = 0（全删）、retention = MAX（不删）

**涉及文件**:
- `crates/sdkwork-discovery-storage-contract/src/watch_event_store.rs`
- `crates/sdkwork-discovery-storage-memory/src/watch.rs`
- `crates/sdkwork-discovery-storage-sqlite/src/sql.rs`
- `crates/sdkwork-discovery-storage-sqlite/src/store.rs`
- `crates/sdkwork-discovery-storage-postgres/src/sql.rs`
- `crates/sdkwork-discovery-storage-postgres/src/store.rs`
- `crates/sdkwork-discovery-core/src/control_plane.rs`
- `crates/sdkwork-discovery-rpc/src/actor.rs`
- `crates/sdkwork-discovery-config/src/model.rs`
- 新 migration 文件（Postgres: `DELETE` 操作无需 migration，直接执行）

---

## Phase 2: P1 - 竞争力提升 (3-4 周)

### 2.1 请求级限流 (Rate Limiting)

**目标**: 保护服务端不被突发流量击垮。

**实现方案**: 令牌桶算法，在 Actor 入口处限流。

**文件**: `crates/sdkwork-discovery-rpc/src/rate_limiter.rs`（新文件）

```rust
pub struct TokenBucketRateLimiter {
    capacity: u64,
    tokens: f64,
    refill_rate: f64,  // tokens per second
    last_refill: Instant,
}

impl TokenBucketRateLimiter {
    pub fn try_acquire(&mut self) -> bool;
}
```

**集成点**: `crates/sdkwork-discovery-rpc/src/services.rs`

在每个 gRPC 方法入口处调用 `rate_limiter.try_acquire()`，失败返回 `Status::ResourceExhausted`。

**配置**: `DiscoveryRuntimeConfig` 增加 `rate_limit` section。

**涉及文件**:
- `crates/sdkwork-discovery-rpc/src/rate_limiter.rs`（新）
- `crates/sdkwork-discovery-rpc/src/services.rs`
- `crates/sdkwork-discovery-rpc/src/server.rs`
- `crates/sdkwork-discovery-config/src/model.rs`

---

### 2.2 熔断器 (Circuit Breaker)

**目标**: 存储层不可用时快速失败，避免级联故障。

**实现方案**: 三态熔断器 (Closed → Open → Half-Open)。

**文件**: `crates/sdkwork-discovery-rpc/src/circuit_breaker.rs`（新文件）

```rust
pub enum CircuitState {
    Closed,    // 正常
    Open,      // 熔断，快速失败
    HalfOpen,  // 试探
}

pub struct CircuitBreaker {
    state: CircuitState,
    failure_count: u32,
    failure_threshold: u32,
    recovery_timeout: Duration,
    last_failure: Option<Instant>,
}
```

**集成点**: Actor 的 `dispatch_command` 中，wrap store 调用：

```rust
if circuit_breaker.is_open() {
    return DiscoveryError::Unavailable("storage circuit breaker is open");
}
match control_plane.xxx(...).await {
    Ok(r) => { circuit_breaker.record_success(); r }
    Err(e) => { circuit_breaker.record_failure(); Err(e) }
}
```

**新增错误变体**:

```rust
pub enum DiscoveryError {
    // ... existing ...
    Unavailable(String),
}
```

**涉及文件**:
- `crates/sdkwork-discovery-rpc/src/circuit_breaker.rs`（新）
- `crates/sdkwork-discovery-rpc/src/actor.rs`
- `crates/sdkwork-discovery-contract/src/error.rs`
- `crates/sdkwork-discovery-config/src/model.rs`

---

### 2.3 只读模式降级 (Read-Only Degradation)

**目标**: 存储故障时自动降级到只读模式，允许读操作继续服务。

**实现方案**: 基于熔断器状态，写操作被拒绝但读操作继续（使用缓存或 stale 数据）。

**文件**: `crates/sdkwork-discovery-rpc/src/actor.rs`

在 `dispatch_command` 中：
- 写操作检查 `circuit_breaker.is_open()` → 返回 `Unavailable`
- 读操作即使 circuit open 也尝试执行（可能返回 stale 数据）

**配置**: `DiscoveryRuntimeConfig` 增加：

```rust
pub struct DegradationConfig {
    pub read_only_on_storage_failure: bool,
    pub stale_read_max_age_ms: u64,
}
```

**涉及文件**:
- `crates/sdkwork-discovery-rpc/src/actor.rs`
- `crates/sdkwork-discovery-config/src/model.rs`

---

### 2.4 批量注册/注销 (Batch Operations)

**目标**: 提高批量操作效率，减少 RPC 往返。

**实现方案**:

#### 2.4.1 Proto 层

```protobuf
message BatchRegisterInstanceRequest {
    repeated RegisterInstanceRequest instances = 1;
}

message BatchRegisterInstanceResponse {
    repeated RegisterInstanceResponse results = 1;
    repeated BatchOperationError errors = 2;
}

message BatchOperationError {
    int32 index = 1;
    string error_code = 2;
    string error_message = 3;
}
```

#### 2.4.2 Actor 层

新增 `RuntimeCommand::BatchRegisterInstances`，在单个 actor 循环中顺序处理，共享同一个 revision 事务。

#### 2.4.3 Storage Contract

可选扩展 `RegistryStore`：

```rust
async fn batch_register_instances(
    &mut self,
    commands: Vec<RegisterInstanceCommand>,
) -> DiscoveryResult<Vec<RegisterInstanceResult>>;
```

**涉及文件**:
- `proto/sdkwork/discovery/internal/v1/registry_service.proto`
- `crates/sdkwork-discovery-storage-contract/src/registry_store.rs`
- `crates/sdkwork-discovery-storage-memory/src/registry.rs`
- `crates/sdkwork-discovery-storage-sqlite/src/store.rs`
- `crates/sdkwork-discovery-storage-postgres/src/store.rs`
- `crates/sdkwork-discovery-rpc/src/actor.rs`
- `crates/sdkwork-discovery-rpc/src/services.rs`
- `crates/sdkwork-discovery-rpc/src/codec.rs`
- `crates/sdkwork-discovery-core/src/control_plane.rs`

---

### 2.5 事件压缩 (Event Compaction)

**目标**: 对同一资源的连续事件合并，减少存储和传输开销。

**实现方案**: 在 GC 时，保留每个资源的最新事件，删除中间事件。

**文件**: `crates/sdkwork-discovery-storage-contract/src/watch_event_store.rs`

```rust
async fn compact_watch_events(
    &mut self,
    namespace: &str,
    environment: &str,
    max_events_per_resource: usize,
) -> DiscoveryResult<usize>;
```

**SQL** (SQLite):

```sql
-- 保留每个 resource_id 的最新 N 条事件
DELETE FROM discovery_watch_event
WHERE revision NOT IN (
    SELECT revision FROM (
        SELECT revision, ROW_NUMBER() OVER (
            PARTITION BY resource_id ORDER BY revision DESC
        ) as rn
        FROM discovery_watch_event
        WHERE namespace = ? AND environment = ?
    ) ranked
    WHERE rn <= ?
)
AND namespace = ? AND environment = ?
```

**涉及文件**:
- `crates/sdkwork-discovery-storage-contract/src/watch_event_store.rs`
- `crates/sdkwork-discovery-storage-memory/src/watch.rs`
- `crates/sdkwork-discovery-storage-sqlite/src/sql.rs`
- `crates/sdkwork-discovery-storage-sqlite/src/store.rs`
- `crates/sdkwork-discovery-storage-postgres/src/sql.rs`
- `crates/sdkwork-discovery-storage-postgres/src/store.rs`

---

## Phase 3: P2 - 生态完善 (2-3 周)

### 3.1 多节点集群 (Multi-Node Cluster)

**目标**: 无状态多实例部署 + 数据库 HA。

**实现方案**:

- **无状态**: Actor 已通过 channel 串行化，所有状态在数据库中。多实例部署时，每个实例独立运行 Actor，数据库提供一致性。
- **数据库 HA**: Postgres 主从复制 + 读写分离。
- **配置**: `DiscoveryRuntimeConfig` 增加集群配置。

**关键约束**: Watch 事件需要跨实例传播。当前 broadcast channel 是进程内的，需要引入外部 pub/sub（如 Postgres LISTEN/NOTIFY 或 Redis Pub/Sub）。

**新增 crate**: `crates/sdkwork-discovery-cluster/`（可选，处理跨实例 watch 同步）

**涉及文件**:
- `crates/sdkwork-discovery-config/src/model.rs`
- `crates/sdkwork-discovery-rpc/src/watch.rs`
- `crates/sdkwork-discovery-rpc/src/actor.rs`
- 可能新增 `crates/sdkwork-discovery-cluster/`

---

### 3.2 命名空间隔离 (Namespace Isolation)

**目标**: 资源配额和独立权限控制。

**实现方案**:

#### 3.2.1 Contract 层

```rust
pub struct NamespaceConfig {
    pub namespace: String,
    pub max_instances: usize,
    pub max_services: usize,
    pub max_config_releases: usize,
    pub permissions: NamespacePermissions,
}

pub struct NamespacePermissions {
    pub allowed_writers: Vec<String>,
    pub allowed_readers: Vec<String>,
}
```

#### 3.2.2 存储层

新增 `NamespaceStore` trait：

```rust
#[async_trait]
pub trait NamespaceStore {
    async fn create_namespace(&mut self, config: NamespaceConfig) -> DiscoveryResult<()>;
    async fn get_namespace(&self, namespace: &str) -> DiscoveryResult<Option<NamespaceConfig>>;
    async fn check_quota(&self, namespace: &str, resource: &str) -> DiscoveryResult<bool>;
}
```

#### 3.2.3 Control Plane

在 `register_instance` 等操作前增加配额检查。

**涉及文件**:
- `crates/sdkwork-discovery-contract/src/namespace.rs`（新）
- `crates/sdkwork-discovery-storage-contract/src/namespace_store.rs`（新）
- `crates/sdkwork-discovery-core/src/control_plane.rs`
- 各存储实现

---

### 3.3 持久实例模式 (Persistent Instance Mode)

**目标**: 支持非 lease-based 注册，实例不会自动过期。

**实现方案**:

```rust
pub struct RegisterInstanceCommand {
    // ... existing fields ...
    pub persistent: bool,  // true = 不依赖 lease，手动注销
}
```

**存储层**: 当 `persistent = true` 时，`expires_at_ms` 设为 `u64::MAX`，跳过 expiry scan。

**涉及文件**:
- `crates/sdkwork-discovery-contract/src/registry.rs`
- `crates/sdkwork-discovery-storage-memory/src/registry.rs`
- `crates/sdkwork-discovery-storage-sqlite/src/store.rs`
- `crates/sdkwork-discovery-storage-postgres/src/store.rs`
- `crates/sdkwork-discovery-rpc/src/actor.rs`
- `proto/sdkwork/discovery/common/v1/discovery_types.proto`

---

### 3.4 配置加密 (Config Encryption)

**目标**: 敏感配置值加密存储。

**实现方案**:

```rust
pub struct ConfigEncryptionConfig {
    pub enabled: bool,
    pub encryption_key_file: String,
    pub algorithm: EncryptionAlgorithm,
}

pub enum EncryptionAlgorithm {
    Aes256Gcm,
}
```

**集成点**:
- `create_config_draft`: 检查值是否标记为 secret → 加密后存储
- `retrieve_effective_config`: 解密后返回

**涉及文件**:
- `crates/sdkwork-discovery-config/src/model.rs`
- `crates/sdkwork-discovery-core/src/control_plane.rs`
- `crates/sdkwork-discovery-storage-sqlite/src/store.rs`
- `crates/sdkwork-discovery-storage-postgres/src/store.rs`

---

## 实施顺序和依赖关系

```
Phase 1 (P0) - 顺序执行，有依赖
┌─────────────────────────────────────────────────────────┐
│  1.5 Watch Event GC          ← 无依赖，可最先实施       │
│       ↓                                                  │
│  1.1 CAS                     ← 无依赖，可并行           │
│       ↓                                                  │
│  1.3 Label Filtering         ← 依赖 1.1 (CAS 稳定后)    │
│       ↓                                                  │
│  1.4 Weight/Priority         ← 依赖 1.3 (Query 扩展)    │
│       ↓                                                  │
│  1.2 Active Health Check     ← 最复杂，最后实施          │
└─────────────────────────────────────────────────────────┘

Phase 2 (P1) - 部分可并行
┌─────────────────────────────────────────────────────────┐
│  2.1 Rate Limiting           ← 无依赖                   │
│  2.2 Circuit Breaker         ← 无依赖，与 2.1 并行      │
│       ↓                                                  │
│  2.3 Read-Only Degradation   ← 依赖 2.2                 │
│       ↓                                                  │
│  2.4 Batch Operations        ← 无依赖，可并行           │
│  2.5 Event Compaction        ← 依赖 1.5 (GC 基础设施)   │
└─────────────────────────────────────────────────────────┘

Phase 3 (P2) - 独立模块
┌─────────────────────────────────────────────────────────┐
│  3.3 Persistent Instance     ← 最简单，可先做            │
│  3.2 Namespace Isolation     ← 独立                     │
│  3.4 Config Encryption       ← 独立                     │
│  3.1 Multi-Node Cluster      ← 最复杂，最后             │
└─────────────────────────────────────────────────────────┘
```

---

## 每个 Phase 的验收标准

### Phase 1 验收
- [ ] `cargo fmt --all -- --check` 通过
- [ ] `cargo test --workspace` 全部通过
- [ ] CAS: 并发注册返回 Conflict 错误
- [ ] Label Filtering: metadata 查询在三种存储后端均正确
- [ ] Weight/Priority: discover 结果按指定排序返回
- [ ] Watch GC: 事件表大小受控，不影响实时性
- [ ] Health Check: 实例宕机后自动标记 NotServing

### Phase 2 验收
- [ ] 限流: 超过阈值返回 ResourceExhausted
- [ ] 熔断: 存储故障时快速失败，恢复后自动关闭
- [ ] 只读降级: 写失败但读仍可用
- [ ] 批量操作: 性能优于逐个调用
- [ ] 事件压缩: 同资源事件数受控

### Phase 3 验收
- [ ] 多节点: 两个实例共享同一数据库，watch 事件跨实例传播
- [ ] 命名空间: 配额限制生效，权限隔离正确
- [ ] 持久实例: 不参与 expiry scan
- [ ] 配置加密: 加密存储、解密读取

---

## 新增 crate 汇总

| Crate | Phase | 用途 |
|---|---|---|
| `sdkwork-discovery-health-checker` | P0 | 主动健康检查探测器 |
| `sdkwork-discovery-cluster` | P2 (可选) | 跨实例 watch 同步 |

## 新增 Proto 字段/消息汇总

| Proto 文件 | 新增内容 | Phase |
|---|---|---|
| `discovery_types.proto` | `LabelFilter`, `LabelFilterOp`, `DiscoverSortBy`, `HealthCheckConfig`, `HealthCheckProbe` | P0 |
| `registry_service.proto` | `expected_revision`, `label_filters`, `sort_by`, `BatchRegister`, `BatchDeregister` | P0/P1 |
| `discovery_admin_service.proto` | `BatchDeregister` (可选) | P1 |

## 新增 Migration 汇总

| 数据库 | Migration | Phase |
|---|---|---|
| SQLite | `ALTER TABLE discovery_service_instance ADD COLUMN health_check_json TEXT` | P0 |
| Postgres | `ALTER TABLE discovery_service_instance ADD COLUMN health_check_json TEXT` | P0 |
| SQLite | `CREATE INDEX IF NOT EXISTS idx_discovery_watch_event_cleanup ON discovery_watch_event (revision)` | P0 |
| Postgres | 同上 | P0 |

---

## 风险与缓解

| 风险 | 影响 | 缓解措施 |
|---|---|---|
| CAS 引入破坏现有注册流程 | 高 | `expected_revision` 为 Option，None 保持旧行为 |
| Label filter SQL 注入 | 高 | 使用参数化查询，key/value 白名单校验 |
| Health check 探测风暴 | 中 | max_concurrent_checks 限制 + jitter |
| GC 删除仍在使用的事件 | 中 | 保留最近 N 个 revision，保守策略 |
| 熔断器误判 | 中 | 可配置阈值 + 手动 reset API |
