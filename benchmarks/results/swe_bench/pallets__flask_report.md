# ctxvault Retrieval & Indexing Benchmark Report

- **Benchmark**: `swe_bench`
- **Repository**: `pallets/flask`
- **Evaluated Queries**: 3
- **Evaluation Cutoff**: K = 10

## 2. Retrieval Algorithm Quality & Latency Ablation

All IR metrics computed at cutoff **K = 10**:

| Mode | Recall@K | Precision@K | MRR@K | NDCG@K | Sep Ratio | Latency p50 | Latency p95 | Latency p99 | QPS |
|---|---|---|---|---|---|---|---|---|---|
| `bm25` | **0.333** | 0.033 | **0.333** | **0.333** | 1.46x | 28.12ms | 30.60ms | 30.82ms | **40** |
| `binary` | **0.667** | 0.067 | **0.194** | **0.310** | 1.09x | 5.85ms | 5.92ms | 5.93ms | **233** |
| `ppr` | **0.667** | 0.067 | **0.417** | **0.477** | 14.09x | 27.31ms | 30.61ms | 30.90ms | **41** |
| `fast` | **0.667** | 0.067 | **0.417** | **0.477** | 1.72x | 38.22ms | 42.25ms | 42.61ms | **28** |

### Metric Descriptions
- **Recall@K**: Fraction of ground-truth relevant documents retrieved in the top K.
- **MRR@K**: Mean Reciprocal Rank (1/rank of first relevant result).
- **NDCG@K**: Normalized Discounted Cumulative Gain accounting for graded relevance.
- **Sep Ratio**: Score separation between top-1 hit and bottom top-K hit (confidence margin).
- **QPS**: Sustained queries per second.

