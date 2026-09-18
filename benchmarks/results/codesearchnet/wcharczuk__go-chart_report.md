# ctxvault Retrieval & Indexing Benchmark Report

- **Benchmark**: `codesearchnet`
- **Repository**: `wcharczuk/go-chart`
- **Evaluated Queries**: 2
- **Evaluation Cutoff**: K = 10

## 2. Retrieval Algorithm Quality & Latency Ablation

All IR metrics computed at cutoff **K = 10**:

| Mode | Recall@K | Precision@K | MRR@K | NDCG@K | Sep Ratio | Latency p50 | Latency p95 | Latency p99 | QPS |
|---|---|---|---|---|---|---|---|---|---|
| `bm25` | **0.500** | 0.050 | **0.500** | **0.500** | 1.68x | 20.25ms | 24.27ms | 24.62ms | **49** |
| `binary` | **0.500** | 0.050 | **0.500** | **0.500** | 1.15x | 0.63ms | 0.63ms | 0.63ms | **1599** |
| `ppr` | **0.500** | 0.050 | **0.500** | **0.500** | 48.46x | 19.06ms | 22.31ms | 22.60ms | **52** |
| `fast` | **0.500** | 0.050 | **0.500** | **0.500** | 1.85x | 24.19ms | 28.53ms | 28.92ms | **41** |

### Metric Descriptions
- **Recall@K**: Fraction of ground-truth relevant documents retrieved in the top K.
- **MRR@K**: Mean Reciprocal Rank (1/rank of first relevant result).
- **NDCG@K**: Normalized Discounted Cumulative Gain accounting for graded relevance.
- **Sep Ratio**: Score separation between top-1 hit and bottom top-K hit (confidence margin).
- **QPS**: Sustained queries per second.

