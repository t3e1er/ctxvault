# ctxvault Retrieval & Indexing Benchmark Report

- **Benchmark**: `repobench`
- **Repository**: `DLYuanGod/TinyGPT-V`
- **Evaluated Queries**: 1
- **Evaluation Cutoff**: K = 10

## 2. Retrieval Algorithm Quality & Latency Ablation

All IR metrics computed at cutoff **K = 10**:

| Mode | Recall@K | Precision@K | MRR@K | NDCG@K | Sep Ratio | Latency p50 | Latency p95 | Latency p99 | QPS |
|---|---|---|---|---|---|---|---|---|---|
| `bm25` | **1.000** | 0.100 | **0.333** | **0.500** | 1.13x | 13.56ms | 13.56ms | 13.56ms | **74** |
| `binary` | **1.000** | 0.100 | **0.200** | **0.387** | 1.05x | 0.81ms | 0.81ms | 0.81ms | **1231** |
| `ppr` | **1.000** | 0.100 | **0.100** | **0.289** | 3.78x | 11.19ms | 11.19ms | 11.19ms | **89** |
| `fast` | **1.000** | 0.100 | **0.500** | **0.631** | 2.04x | 14.69ms | 14.69ms | 14.69ms | **68** |

### Metric Descriptions
- **Recall@K**: Fraction of ground-truth relevant documents retrieved in the top K.
- **MRR@K**: Mean Reciprocal Rank (1/rank of first relevant result).
- **NDCG@K**: Normalized Discounted Cumulative Gain accounting for graded relevance.
- **Sep Ratio**: Score separation between top-1 hit and bottom top-K hit (confidence margin).
- **QPS**: Sustained queries per second.

