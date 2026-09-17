# ctxvault Retrieval & Indexing Benchmark Report

- **Benchmark**: `codesearchnet`
- **Repository**: `coreos/go-omaha`
- **Evaluated Queries**: 3
- **Evaluation Cutoff**: K = 10

## 2. Retrieval Algorithm Quality & Latency Ablation

All IR metrics computed at cutoff **K = 10**:

| Mode | Recall@K | Precision@K | MRR@K | NDCG@K | Sep Ratio | Latency p50 | Latency p95 | Latency p99 | QPS |
|---|---|---|---|---|---|---|---|---|---|
| `bm25` | **1.000** | 0.100 | **1.000** | **1.000** | 1.75x | 4.29ms | 7.22ms | 7.48ms | **201** |
| `binary` | **0.000** | 0.000 | **0.000** | **0.000** | 1.14x | 0.14ms | 0.15ms | 0.15ms | **7187** |
| `ppr` | **1.000** | 0.100 | **1.000** | **1.000** | 45.75x | 7.85ms | 8.60ms | 8.67ms | **135** |
| `fast` | **1.000** | 0.100 | **1.000** | **1.000** | 1.22x | 7.99ms | 8.76ms | 8.83ms | **129** |

### Metric Descriptions
- **Recall@K**: Fraction of ground-truth relevant documents retrieved in the top K.
- **MRR@K**: Mean Reciprocal Rank (1/rank of first relevant result).
- **NDCG@K**: Normalized Discounted Cumulative Gain accounting for graded relevance.
- **Sep Ratio**: Score separation between top-1 hit and bottom top-K hit (confidence margin).
- **QPS**: Sustained queries per second.

