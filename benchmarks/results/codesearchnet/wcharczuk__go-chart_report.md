# ctxvault Retrieval & Indexing Benchmark Report

- **Benchmark**: `codesearchnet`
- **Repository**: `wcharczuk/go-chart`
- **Evaluated Queries**: 2
- **Evaluation Cutoff**: K = 10

## 2. Retrieval Algorithm Quality & Latency Ablation

All IR metrics computed at cutoff **K = 10**:

| Mode | Recall@K | Precision@K | MRR@K | NDCG@K | Sep Ratio | Latency p50 | Latency p95 | Latency p99 | QPS |
|---|---|---|---|---|---|---|---|---|---|
| `bm25` | **0.500** | 0.050 | **0.500** | **0.500** | 1.68x | 13.81ms | 14.58ms | 14.65ms | **72** |
| `binary` | **0.500** | 0.050 | **0.125** | **0.215** | 1.18x | 1.09ms | 1.45ms | 1.48ms | **916** |
| `ppr` | **0.500** | 0.050 | **0.500** | **0.500** | 48.44x | 15.29ms | 18.86ms | 19.18ms | **65** |
| `fast` | **0.500** | 0.050 | **0.500** | **0.500** | 1.32x | 14.36ms | 15.67ms | 15.79ms | **70** |

### Metric Descriptions
- **Recall@K**: Fraction of ground-truth relevant documents retrieved in the top K.
- **MRR@K**: Mean Reciprocal Rank (1/rank of first relevant result).
- **NDCG@K**: Normalized Discounted Cumulative Gain accounting for graded relevance.
- **Sep Ratio**: Score separation between top-1 hit and bottom top-K hit (confidence margin).
- **QPS**: Sustained queries per second.

