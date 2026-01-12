# 时间计量稳定性评估平台（Rust，从 0 搭建，面向 VDF/TDF/PoST 与 Chia/Spacemesh/Filecoin）

## 0. 目标与范围

目标：搭建一个可复现实验与可对比评估的平台，用于度量“时间嘀嗒的稳定性（tick stability）”，即：
1) 一个系统所定义的最小时间单位（tick/slot/epoch 等）的实际持续时间分布；
2) 相邻时间单位之间的连续波动（jitter），包括短期抖动与累计漂移（drift）；
3) 跨实现/跨硬件/跨节点（timelord/PoET server/矿工）之间的差异；
4) 将 PoSt 类论文中的理论约束（例如每个 time slot 长度在 [t0, t0+δT]）映射到可测指标，并给出是否满足定参约束的证据。

范围覆盖四类对象：
A. 纯 VDF/TDF 引擎层（本地基准测试，wall-clock jitter）
B. PoSt/ePoSt/stoRNA 等“顺序时间槽”协议层（slot 边界、t0、δ、定参）
C. Chia 协议时间节拍（signage points / infusion points / sub-slot）
D. Spacemesh 协议时间节拍（PoET tick 与轮次调度）
E. Filecoin 协议时间节拍（epoch、WindowPoSt/WinningPoSt 时间窗；不假设 VDF）

输出：统一 JSONL 事件流 + 可复现的分析报表（CSV/JSON + 生成图表的脚本接口）。

## 1. 核心概念与统一事件模型

### 1.1 统一“时间单位”定义

将“时间单位”抽象为 TimeUnit：
- unit_id：单调递增编号
- unit_type：vdf_tick / post_slot / chia_signage / chia_subslot / spacemesh_tick / spacemesh_round / filecoin_epoch / filecoin_deadline 等
- target：协议期望时长（秒）或期望操作数（iterations/steps）
- start_ts_ns / end_ts_ns：单调时钟（monotonic clock）时间戳
- duration_ns = end - start

额外字段（可选）：
- work_amount：该单位内执行的单位操作数（iterations / squarings / hash-steps）
- proof_size_bytes：该单位产生的证明大小
- verify_time_ns：验证耗时（如需要）
- metadata：系统特定字段（signage index、sub-slot iters、PoET tick count、Filecoin epoch height 等）

### 1.2 “连续波动值”与稳定性指标

对序列 {duration_i} 定义：
- jitter_i = duration_i - target_duration（若 target 是秒）
- adj_jitter_i = duration_{i+1} - duration_i（相邻差分，体现连续波动）
- drift_k = sum_{i=1..k}(duration_i - target_duration)（累计漂移）

统计指标（每个 unit_type 单独输出）：
- mean, std, coefficient_of_variation
- p50/p90/p99, max, min
- mean(|adj_jitter|), p99(|adj_jitter|)
- drift 的最大正/负偏移
- Allan deviation（可选，用于短期稳定性）
- 分段统计（按时间窗口/轮次/节点）

注意：对于 PoSt/PoET 这类“以操作数定义时间”的系统，还需要输出：
- ticks_per_second（或 iterations_per_second）
- ticks_per_second 的方差与分位数
- 跨节点差异（fastest vs median vs slowest）

## 2. 工程架构（Rust workspace）

workspace 建议结构：
- crates/
  - core_types/：TimeUnit、事件 schema、序列化（serde）
  - core_metrics/：在线统计（streaming stats）、分位数（TDigest 或 HDRHistogram）
  - collectors/：各系统数据采集器（RPC/日志/本地执行）
  - engines/：
    - vdf_rsa_wesolowski/：RSA VDF（迭代平方）+ Wesolowski proof
    - vdf_classgroup/：ClassGroup VDF（用于 Chia 风格）
    - tdf_trapdoor/：可选（用于论文里 TrapEval/对照）
  - protocol_models/：
    - post_slot_model/：PoSt slot 边界与 t0、δ、T、t 参数模型
    - chia_model/：signage/sub-slot/infusion 的时间结构模型
    - spacemesh_model/：PoET tick/round/cycle gap 模型
    - filecoin_model/：epoch/deadline 模型
  - cli/：统一命令行入口
  - report/：把 JSONL 事件转换为 CSV/JSON summary；预留导出给 Python/R 脚本

统一输出目录：
- out/
  - raw_events.jsonl
  - summaries/
  - configs/
  - logs/

## 3. 采集器（Collectors）设计

### 3.1 本地引擎基准采集（用于 A 类评估）

命令：
- timebench vdf local --engine rsa_weso --iters N --repeat M --pin-cpu 3 --warmup 10s
- timebench vdf local --engine classgroup --discriminant-bits 1024 --iters N ...

采集：
- 每次 tick（完成 N 次 unit op 或完成一次 proof 生成）记录 TimeUnit
- 区分 eval、prove、verify 三段耗时
- 记录 CPU 频率、核心编号、线程数、内存占用（尽量轻量）

重要：使用单调时钟（std::time::Instant）记录，避免系统时钟跳变。

### 3.2 PoSt 类论文协议采集（用于 B 类评估）

针对 PoSt “连续审计/连续可用性”的时间槽结构，采集两条线：
1) 协议 slot 边界事件：Ti、Ti+1、Ri（若论文定义）
2) 实际执行耗时：每个 slot 内 VDF/TDF 的 unit op 数与 wall-clock 时间

实现策略：
- 在 post_slot_model 中实现论文定义的 slot 逻辑：给定 T、t（审计频率）、δ、t0（强制延迟），构造 k = T/t0 轮的 slot 序列
- 在执行时，按“每个 slot 必须至少包含一次 VDF 评估”来记录 duration，并检测：
  - duration_i >= t0
  - duration_i <= t0 + δT（用测得的 δ_est 或用户输入 δ）
- 输出 “是否满足 Claim 1-4” 风格的检查报告（pass/fail + 证据）

定参与误差估计：
- 实现 calibration 子命令：
  - timebench post calibrate --unit-op squaring --target-seconds 3600 --delta 1e-4
  - 自动跑 microbench 得到 unit_op_time_ns 分布，并给出 s0（unit op 次数）使得 (s0 * unit_op_time) 最接近但不超过 (1+δ)*t0
- 输出参数建议，并提示：若观测到单位操作被专用硬件显著加速，则安全假设可能不成立（仅报告事实，不做价值判断）。

### 3.3 Chia 采集器（用于 C 类评估）

目标：评估 Chia 的协议时间节拍稳定性，至少覆盖：
- signage point 间隔（理论上 600s/64=9.375s）
- sub-slot 内 64 个 signage points 的抖动分布
- infusion point 相对 signage point 的延迟分布
- “有效节拍”由最快 timelord 推动时的网络可见抖动（如果能采集网络时间戳）

数据源选项（实现时二选一或都做）：
A) 解析 full node 日志（推荐先做，最容易落地）
B) 通过 RPC 拉取区块与 VDF 相关字段（如可用）

需要记录的字段（metadata）：
- sub_slot_iterations, sp_interval_iterations
- signage_index（0..63）
- 对应 VDF 的 iterations
- 观测到的发布时间戳（本机）与事件时间戳（链上/日志）

输出：
- chia_signage TimeUnit 序列
- chia_subslot TimeUnit（聚合 64 个 signage）
- infusion 延迟统计（如果能定位）

### 3.4 Spacemesh 采集器（用于 D 类评估）

目标：评估 PoET tick 的稳定性与轮次调度带来的系统波动：
- tick 产出速率（ticks/sec）
- 跨 PoET server 的 tick 速率差异
- round/cycle gap 期间的“系统不可用窗口”对时间节拍的影响

数据源选项：
A) 解析 go-spacemesh 节点日志（PoET 注册、取回、ATX 提交）
B) 调用 PoET server HTTP API（如果公开接口稳定）
C) 从链上 ATX 数据中提取 tick count（视实现可用性）

需要记录的字段：
- poet_server_id / endpoint
- round_id, round_start/end
- tick_count（若可得）
- 本地观测时间（注册/取回/生成 PoST/提交 ATX）

输出：
- spacemesh_round TimeUnit
- spacemesh_tick_rate 统计（非 TimeUnit，但作为派生指标输出）

### 3.5 Filecoin 采集器（用于 E 类评估）

强调：不把 Filecoin 强行等同于 VDF tick。Filecoin 的时间结构主要来自：
- epoch = 30 秒（链共识时间粒度）
- WindowPoSt 30 分钟 deadline（证明提交窗口）
- WinningPoSt 的出块时序（与 epoch 对齐）

数据源选项：
A) Lotus 节点 RPC 拉取 tipset/区块时间戳与高度（推荐）
B) 解析链数据导出（若用户提供）

需要记录的字段：
- epoch_height
- block_timestamp（链上）
- 本地观测到的获取时间戳（用于网络/节点延迟分离）
- deadline_index（若分析 WindowPoSt）

输出：
- filecoin_epoch TimeUnit（以链上时间戳/高度换算）
- filecoin_block_interarrival 分布（作为补充稳定性指标）
- deadline 内证明提交的时序分布（若能拿到证明消息时间）

## 4. 统一 CLI 设计（建议）

- timebench vdf local ...
- timebench post run --config post.yaml
- timebench post calibrate ...
- timebench chia collect --source log --path ~/.chia/mainnet/log/...
- timebench spacemesh collect --source log --path ...
- timebench filecoin collect --source lotus-rpc --endpoint ...
- timebench report --in out/raw_events.jsonl --out out/summaries/

## 5. 配置文件（YAML）

示例：post.yaml
- storage_time_T_seconds
- audit_frequency_t_seconds
- delta
- vdf_engine: rsa_weso | classgroup | external
- unit_op: squaring | hash_step | classgroup_square
- calibration:
  - warmup_runs
  - sample_runs
- output:
  - jsonl_path

示例：chia.yaml
- log_path 或 rpc_endpoint
- expected_subslot_seconds (default 600)
- signage_points_per_subslot (default 64)

示例：spacemesh.yaml
- node_log_path
- poet_servers: [ ... ]
- schedule_params（可选：phase-shift, cycle-gap）

示例：filecoin.yaml
- lotus_rpc
- epoch_seconds (default 30)

## 6. 报表与对比输出（最小交付件）

最小交付件必须包含：
- 每个 unit_type 的 duration 分布与 jitter 分布（CSV + JSON summary）
- 相邻差分 adj_jitter 的分布
- drift 的最大偏移与时间序列（可导出）
- 跨节点对比（Chia: timelord；Spacemesh: PoET server；Filecoin: miner/节点视角）

对比表（自动生成）字段建议：
- system, unit_type, target, mean, p99, max, mean(|adj_jitter|), drift_max_abs, sample_count

## 7. 验证点（确保“足够还原技术细节”）

PoSt 类：
- 必须实现“slot 上下界检查”与定参/校准工具（t0、δ、T、t 的关系）
- 必须能输出“单位操作耗时估计”与 s0 推荐值
- 必须能报告：若观测到单位操作被显著加速，风险在于强制延迟假设可能被破坏（仅基于测量数据）

Chia：
- 必须把 “signage points 9.375s、sp_interval_iterations=subslot_iterations/64、infusion 相对 signage 的延迟窗口”建模为可计算的 target，并输出偏差统计

Spacemesh：
- 必须把 tick 与 wall-clock 分开：tick_count 是协议时间，ticks/sec 是实现与硬件映射
- 必须支持跨 PoET server 的 tick rate 对比与稳定性对比

Filecoin：
- 必须把 epoch/deadline 作为时间单位，并统计出块间隔、epoch 对齐偏差、deadline 内消息时序（若可得）
- 不要求实现 VDF 引擎来评估 Filecoin（除非用户明确要评估其生态中的 VDF 组件）

## 8. 交付标准

- 可在单机运行：本地 VDF 基准 + JSONL 输出 + report 汇总
- 可在有节点数据时运行：Chia/Spacemesh/Filecoin 采集器至少能从日志或 RPC 跑通一种
- 所有输出可复现：config 固化，运行环境信息记录（CPU 型号、频率策略、OS、Rust 版本）
