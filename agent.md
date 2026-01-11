# Wesolowski VDF 时间嘀嗒稳定性（tick stability / inter-tick jitter）统计平台规格书（Rust）

面向目标：把“时间单位（tick）”定义为一次固定迭代数 t 的 Wesolowski VDF 评估（Repeated Squaring），并统计相邻 tick 的墙钟时间波动（连续抖动）。本文件同时给出论文方案的关键实现细节（尽可能贴近原文算法），使 Codex 可据此从 0 开始实现一个可复现实验、可长期跑数、可导出报告的测量平台。

本规格默认基于论文：Benjamin Wesolowski, Efficient verifiable delay functions（Eurocrypt 2019 扩展版）。
本论文将“时间”主要抽象为顺序工作量（sequential work），并明确讨论真实硬件下的运行时间差异与模型误差（见论文 3.2 节的讨论）。平台统计的指标属于工程侧补充：在固定 t 下测量墙钟 tick 的波动与漂移。

---

## 1. 术语与指标定义

### 1.1 VDF tick（时间嘀嗒）
tick 定义为一次 VDF 评估过程，参数固定为：
- 输入：x（字节串），经哈希映射为群元素 g = H_G(x)
- 迭代数：t（正整数），代表 t 次“顺序平方”（sequential squarings）
- 输出：y = g^(2^t)
- 证明：π = g^{⌊2^t / ℓ⌋}，其中 ℓ = H_prime(bin(g) ||| bin(y)) 为哈希到素数

对应论文 Algorithm 3（eval_pk）：先做 t 次顺序平方得到 y，再计算 ℓ，再计算 π = g^{⌊2^t/ℓ⌋}（π 可用简单 Algorithm 4 或更快的 Algorithm 5）。论文中验证算法检查 π^ℓ * g^r = y，其中 r = 2^t mod ℓ。参见论文中 Algorithm 2/3/4 与 4.1 节描述。  
（论文要点：Algorithm 4 是“在线长除法”版本，代价在 t 到 2t 次群运算；Algorithm 5 用 base-2^κ 分解把证明计算压到 O(t/log t) 次群运算，并给出进一步的内存压缩思路。）

### 1.2 墙钟时间记录
对每个 tick i，记录墙钟耗时：
- d_i = end_i - start_i（单位：ns 或 µs，使用单调时钟 monotonic clock）
并记录：
- tick_index i
- input_mode（见 2.2）
- 参数：t、κ、γ、k（安全参数，影响 ℓ 位数/取素策略）
- 运行环境指纹：CPU 型号、核心数、频率信息（若可取）、OS、编译 profile、是否绑定 CPU 核、是否禁用 turbo 等

### 1.3 核心稳定性指标（你关心的“连续波动值”）
给出两类“相邻 tick”波动：

1) 绝对抖动（absolute inter-tick jitter）
- j_i = d_{i+1} - d_i

2) 相对抖动（relative inter-tick jitter）
- ρ_i = (d_{i+1} - d_i) / d_i

额外建议统计（用于报告更完整）：
- d_i 的均值、方差、标准差、变异系数 CV = std/mean
- j_i 的均值、方差、p50/p90/p99 分位数
- 漂移（drift）：在窗口 W 内的线性回归斜率（d_i 随 i 的趋势）
- 稳态检测：丢弃前 warm-up 的 N_warm ticks（如 50 或 100）后再统计

---

## 2. 平台功能需求（Platform requirements）

### 2.1 必须具备的能力（MVP）
1) Rust 实现 Wesolowski VDF（至少 RSA 组版本），包含：
- H_G：把输入映射到群元素
- H_prime：哈希到素数 ℓ
- eval：计算 y = g^(2^t)（t 次顺序平方）
- prove：计算 π = g^{⌊2^t/ℓ⌋}
  - 必须实现 Algorithm 4（在线长除法）作为正确性基线
  - 建议实现 Algorithm 5（base-2^κ 加速）以贴近论文的“原本技术细节”
- verify：检查 π^ℓ * g^r == y（r = 2^t mod ℓ）

2) tick 稳定性跑数器（runner）：
- 支持跑 N ticks（可配置）
- 每 tick 记录 d_i，写入本地数据库或 CSV/JSONL
- 可输出统计摘要（均值、std、p95、p99、jitter 分布）

3) 可复现实验：
- 统一配置文件（TOML）
- 固定随机种子（如果 input_mode 使用随机输入）
- 生成 run_id（时间戳 + git commit + 配置哈希）

### 2.2 输入模式（input modes）
至少支持三种模式，便于区分“算法内部波动”和“输入链式依赖”：

Mode A：fixed-input
- 全部 ticks 使用相同 x（因此 g 不变），仅重复执行 eval/prove
- 用于测“纯执行波动”

Mode B：random-input
- 每 tick 随机生成 x_i（固定 seed，可复现），测试哈希到群与证明计算在不同 g 下的波动

Mode C：chained（链式）
- x_{i+1} = H(x_i || bin(y_i) || i) 或等价构造
- 更贴近区块链里“连续时间嘀嗒”场景（每次输出喂给下一次）

（可选）Mode D：infusion（类似 Chia 的 infusion 语义）
- 每 tick 把外部“值 value_i”融合进 challenge（例如 x_{i+1} = H(bin(y_i) || value_i)）
- 该模式不要求完全复刻 Chia，但应在配置层支持“外部值流”接口

### 2.3 数据存储与导出
MVP 推荐：
- SQLite（rusqlite）为主存储，方便查询与长期积累
- 同时支持导出 CSV（便于你用 pandas/Excel 出图）

必须记录的表（最小字段集合）：
- runs(run_id, created_at, config_toml, git_commit, rustc_version, target_triple)
- ticks(run_id, tick_index, start_ns, end_ns, duration_ns, mode, t, k, kappa, gamma, proof_algo, ok_bool, err_msg)
- env(run_id, cpu_brand, cpu_cores, os, kernel, freq_hint, affinity, turbo_hint)

---

## 3. 论文方案实现细节（尽可能还原）

本节按论文的定义给出实现要点。关键公式与算法结构来自论文的 Construction 与 4.1 节（计算 π 的优化），以及 Algorithm 2/3/4/5 的思路。

### 3.1 群的选择与表示（RSA setup 为主）
论文给出 RSA setup 示例：G 取 (Z/NZ)^×/{±1}，公钥是 N，H_G(x) = int(H("residue"||x)) mod N，并说明技术原因需要在商群 {±1} 下工作。平台实现可以采取“规范代表”策略近似实现 {±1} 商：
- 令 a = a mod N，映射到 [0, N-1]
- 定义 canonical(a) = min(a, N-a)（把 a 与 -a 归一到同一代表）
- 群乘法 mul(a,b) = canonical((a*b) mod N)
- 群平方 sq(a) = canonical((a*a) mod N)
- 单位元 1_G = 1

注意：严格实现 (Z/NZ)^× 需要确保 gcd(a,N)=1；工程上可通过重哈希避免非可逆元素：
- 若 gcd(candidate, N) != 1，则把 candidate = H(candidate||counter) 再试，直到 gcd=1

（可选）Class group setup：
- 若未来要贴近 Chia 工程实现，可增加 class group 模式，但这超出 MVP；可以在 trait 设计上预留扩展点。

### 3.2 哈希函数定义

#### 3.2.1 H_G：输入到群元素
建议使用 SHA-256 或 BLAKE3：
- bytes = hash("residue" || x || counter)
- candidate = OS2IP(bytes) mod N
- candidate = canonical(candidate)
- 若 candidate in {0,1} 或 gcd(candidate,N)!=1，则 counter++ 重试

#### 3.2.2 H_prime：哈希到素数 ℓ
论文交互版中，验证者从 Primes(2k) 中均匀采样素数；Fiat-Shamir 后 ℓ = H_prime(bin(g)|||bin(y))。实现目标：确定性地产生“足够大”的奇素数 ℓ（建议位长约 2k，k 可设 128 或 160）。

建议实现（确定性、工程可行）：
- seed = hash("prime" || bin(g) || bin(y) || counter)
- 将 seed 扩展为 L 位（L ≈ 2k 位）的整数 u，强制最高位为 1、最低位为 1（保证位长与奇数）
- 做 Miller-Rabin（若使用大整数库，通常可用现成实现；否则需自写）
- 若不是素数，counter++ 重试

输出：ℓ（BigUint）

### 3.3 VDF evaluation（计算 y）
给定 g 与迭代数 t：
- y = g^(2^t)
实现即 t 次顺序平方：
- y_0 = g
- for i in 1..=t: y_i = sq(y_{i-1})
- 输出 y = y_t

### 3.4 证明与验证：Wesolowski PoE（Proof of Exponentiation）

#### 3.4.1 验证方程
令 ℓ = H_prime(bin(g)|||bin(y))，r = 2^t mod ℓ（least residue）
验证：
- π^ℓ * g^r == y
其中 pow_small(g, r) 可用常规快速幂（r 是 2k 位以内的素数模下的余数，规模可控）。

#### 3.4.2 证明 π 的计算目标
- π = g^{⌊2^t / ℓ⌋}

关键难点：2^t 极大，不能显式构造指数再做 pow_mod，因此需要“在线长除法”或 base-2^κ 分解算法。

---

## 4. 证明算法实现（必须实现 Algorithm 4，可选实现 Algorithm 5）

### 4.1 Algorithm 4：在线长除法（基线，强烈建议严格按论文写）
输入：群元素 g，素数 ℓ，迭代数 t  
输出：π = g^{⌊2^t/ℓ⌋}

状态变量：
- x ∈ G，初始 x = 1_G
- r ∈ Z，初始 r = 1

循环 i = 0..t-1：
- b = floor(2r / ℓ) ∈ {0,1}
- r = (2r) mod ℓ（取 least residue）
- x = x^2 * g^b
返回 x

实现要点：
- r 的计算在整数域上做（BigUint 或 u128 取决于 ℓ 位长；建议 BigUint 简化）
- b 仅 0/1，因此 g^b 要么是 1_G 要么是 g
- x^2 是群平方 sq(x)

复杂度：
- 每轮 1 次平方 + 至多 1 次乘法，总体在 t 到 2t 次群运算范围，贴合论文描述

### 4.2 Algorithm 5：base-2^κ 加速（贴近论文 4.1 节）
思想：把 ⌊2^t/ℓ⌋ 用 base 2^κ 表示：
- ⌊2^t/ℓ⌋ = Σ_i b_i 2^{κ i}
并将 π = g^{⌊2^t/ℓ⌋} 重写为分桶乘积，降低乘法次数。

论文给出（核心公式）：
1) b_i 的计算（以“least residue of 2^{t-κ(i+1)} mod ℓ”为输入）：
- b_i = floor( 2^κ * ( 2^{t-κ(i+1)} mod ℓ ) / ℓ )
2) 对每个 κ-bit 整数 b ∈ {0, …, 2^κ-1}，定义 I_b = { i | b_i = b }
3) 则
- g^{⌊2^t/ℓ⌋} = ∏_{b=0}^{2^κ-1} ( ∏_{i∈I_b} g^{2^{κ i}} )^b

实现分解为三步：

Step A：计算所有 b_i（i=0..m-1，m = ceil(t/κ)）
- 需要能得到 residues: a_i = 2^{t-κ(i+1)} mod ℓ
- 用迭代法避免对每个 i 做一次完整幂：
  - a_0 = 2^{t-κ} mod ℓ（用 fast pow_mod on integers；复杂度 O(log t)）
  - inv = (2^κ)^{-1} mod ℓ（ℓ 为奇素数，逆元存在）
  - a_{i+1} = a_i * inv mod ℓ
- b_i = floor( (2^κ * a_i) / ℓ )，b_i 是 κ-bit 小整数

Step B：在计算 y = g^{2^t} 的过程中获得 g^{2^{κ i}}
- 当你做顺序平方时，每 κ 次平方就得到一次 g^{2^{κ i}}
- 具体：
  - cur = g
  - for i in 0..m-1:
      - 做 κ 次 cur = sq(cur)
      - 此时 cur = g^{2^{κ(i+1)}}；若想要 g^{2^{κ i}}，可在进入 κ 次平方前保存
- 你可以保存 list_pow[i] = g^{2^{κ i}}（需要 t/κ 个群元素内存）

Step C：分桶聚合
- 初始化 buckets[b] = 1_G，b=0..2^κ-1
- 对每个 i： buckets[b_i] = buckets[b_i] * list_pow[i]
- 最终：
  - π = ∏_{b=0}^{2^κ-1} pow_small_group(buckets[b], b)
其中 pow_small_group(base, e) 的 e 是小整数（<2^κ），可用 square-and-multiply（在群上）。

参数选择建议：
- κ 取约 log2(t)/2，可使总体群运算约 t/κ + κ·2^κ，接近论文的 O(t/log t) 量级
- κ 太大 buckets 太多（2^κ 内存爆），κ 太小提速不明显

### 4.3 内存压缩（γ 技巧，论文 4.1 节的进一步优化）
论文给出思路：不保存每个 κ 步的群元素，而是每隔 κγ 保存一次，γ 可取 O(sqrt(t))，以 O(sqrt(t)) 内存换取近似同样加速。

平台实现建议：
- MVP 可先不实现（因为工程复杂度高），但应在代码结构里预留 prove_fast 的策略枚举：
  - ProofAlgo::Alg4Simple
  - ProofAlgo::Alg5Buckets { kappa }
  - ProofAlgo::Alg5BucketsLowMem { kappa, gamma }  （可后续补齐）

---

## 5. Rust 工程结构建议（Crates / Modules）

建议 workspace：

- crates/vdf-core
  - group.rs：Group trait + RSAGroup 实现
  - hash.rs：HG 与 H_prime
  - vdf.rs：WesolowskiVdf 实现（eval, prove_alg4, prove_alg5, verify）
  - serialize.rs：bin(g) 的规范序列化（BigUint -> big-endian 定长/变长）
  - math.rs：pow_mod_int、inv_mod、miller_rabin

- crates/vdf-runner
  - config.rs：TOML 配置解析
  - runner.rs：运行 N ticks，记录时间与结果
  - stats.rs：计算 jitter / quantiles / CV / drift
  - storage.rs：SQLite + CSV export

- crates/vdf-cli
  - 基于 clap：
    - vdf-cli bench --config run.toml
    - vdf-cli stats --run-id ...
    - vdf-cli export --run-id ... --format csv

（可选）crates/vdf-server
- 基于 axum 提供 REST：
  - GET /runs
  - GET /runs/{id}
  - GET /runs/{id}/ticks
  - GET /runs/{id}/stats

---

## 6. 配置文件（TOML）规范

示例 run.toml：

[tasks]
ticks = 2000
warmup = 100
mode = "fixed-input"        # fixed-input | random-input | chained
seed = 12345

[vdf]
group = "rsa"               # rsa (MVP) | classgroup (future)
n_bits = 2048               # RSA modulus bits
t = 500000                  # squaring iterations per tick
k = 128                     # security parameter for prime hash (ℓ ~ 2k bits)
proof_algo = "alg5"         # alg4 | alg5
kappa = 16                  # for alg5
gamma = 0                   # 0 = disabled; >0 enables low-mem variant (future)

[runner]
cpu_affinity = true
core_id = 2
priority = "high"           # best-effort, platform dependent
cooldown_ms = 0             # optional sleep between ticks

[storage]
sqlite_path = "runs.db"
export_dir = "exports/"

---

## 7. 正确性与验收标准（Acceptance criteria）

必须通过：
1) 单 tick 正确性
- 对任意 x,t：
  - (y, π) = eval(x,t)
  - verify(x,y,π,t) == true
（验证方程：π^ℓ * g^r == y）

2) 负例
- 改动 y 或 π 的任意一比特，verify 必须返回 false（高概率）

3) 可复现实验
- 相同配置与 seed，输出的 (g,y,π) 以及统计结果应一致（允许时间值因系统噪声差异而不同，但数据结构与校验结果必须一致）

---

## 8. 性能与测量注意事项（保证“稳定性统计”可信）

为了让 d_i / j_i 更有解释力，runner 应尽量减少外部噪声：
- 使用单调时钟：std::time::Instant
- 支持绑核（Linux 可用 sched_setaffinity；Windows 用 SetThreadAffinityMask）
- 记录 CPU 频率策略信息（如果能读到）
- 尽量固定编译 profile（--release）
- 预热：warmup ticks 丢弃
- 避免在 tick 内做 IO（写数据库应批量提交或异步队列，但注意你要测的是 VDF 计算而不是 IO）

---

## 9. 参考实现与对照（用于交叉验证，不要求直接复用）

以下资源可用于对照算法与工程实现（在需要时用于 sanity check 或性能对比）：

- Wesolowski 论文（ePrint）：
  https://eprint.iacr.org/2018/623

- POA Network 的 Rust VDF 实现（包含 class group + VDF + CLI，提及覆盖 Wesolowski 与 Pietrzak）：
  https://github.com/poanetwork/vdf
  https://lib.rs/crates/classgroup

- Chia VDF 工具（工程上使用 Wesolowski 并有分段/多阶段证明优化的描述，适合理解“连续 tick 链式运行”的系统形态）：
  https://github.com/Chia-Network/chiavdf
  https://docs.chia.net/proof-of-time/

说明：本平台的目标是“统计 tick 稳定性指标”，因此不强制复刻某条链的共识细节；但 VDF 核心应严格满足 Wesolowski 的 eval/prove/verify 关系与证明计算算法结构。

---

## 10. 实现提示（给 Codex 的直接任务拆解）

Phase 1：vdf-core（RSAGroup + Alg4）
- 完成 BigUint 模运算、canonical、mul、sq、pow_small_group
- 完成 H_G 与 H_prime（Miller-Rabin）
- 完成 eval（t squarings）
- 完成 prove_alg4（严格按 Algorithm 4）
- 完成 verify（π^ℓ * g^r == y）

Phase 2：vdf-runner（测量与存储）
- 配置解析
- 模式 fixed/random/chained
- 计时、记录、批量写 SQLite
- 统计输出（均值、std、p50/p90/p99、jitter）

Phase 3：prove_alg5（base-2^κ 分桶）
- 实现 b_i 计算（a_0 + inv 迭代）
- 实现 list_pow 抽取（每 κ 次平方保存一次）
- 实现 buckets 聚合与小指数幂
- 对比 Alg4/Alg5 输出 π 必须一致（同一 g,ℓ,t）

Phase 4：（可选）服务化与可视化
- axum REST
- 简单 HTML 或导出到外部绘图工具

完成标志：
- vdf-cli bench --config run.toml 可以跑完 N ticks 并输出 stats
- vdf-cli export 可以生成 CSV
- verify 在每 tick 必须为 true，否则记录错误并终止或标记失败（可配置）

---
