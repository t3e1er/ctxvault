# ctxvault Retrieval & Indexing Benchmark Report

- **Benchmark**: `repobench`
- **Repository**: `DLYuanGod/TinyGPT-V`
- **Evaluated Queries**: 1
- **Evaluation Cutoff**: K = 10

## 2. Retrieval Algorithm Quality & Latency Ablation

All IR metrics computed at cutoff **K = 10**:

| Mode | Recall@K | Precision@K | MRR@K | NDCG@K | Sep Ratio | Latency p50 | Latency p95 | Latency p99 | QPS |
|---|---|---|---|---|---|---|---|---|---|
| `bm25` | **1.000** | 0.100 | **0.111** | **0.301** | 1.13x | 29.18ms | 29.18ms | 29.18ms | **34** |
| `binary` | **0.000** | 0.000 | **0.000** | **0.000** | 1.04x | 0.32ms | 0.32ms | 0.32ms | **3154** |
| `ppr` | **1.000** | 0.100 | **0.200** | **0.387** | 17.91x | 33.37ms | 33.37ms | 33.37ms | **30** |
| `fast` | **1.000** | 0.100 | **0.250** | **0.431** | 2.05x | 34.30ms | 34.30ms | 34.30ms | **29** |

### Metric Descriptions
- **Recall@K**: Fraction of ground-truth relevant documents retrieved in the top K.
- **MRR@K**: Mean Reciprocal Rank (1/rank of first relevant result).
- **NDCG@K**: Normalized Discounted Cumulative Gain accounting for graded relevance.
- **Sep Ratio**: Score separation between top-1 hit and bottom top-K hit (confidence margin).
- **QPS**: Sustained queries per second.

