# ctxvault Retrieval & Indexing Benchmark Report

- **Benchmark**: `repobench`
- **Repository**: `jianchang512/vocal-separate`
- **Evaluated Queries**: 1
- **Evaluation Cutoff**: K = 10

## 2. Retrieval Algorithm Quality & Latency Ablation

All IR metrics computed at cutoff **K = 10**:

| Mode | Recall@K | Precision@K | MRR@K | NDCG@K | Sep Ratio | Latency p50 | Latency p95 | Latency p99 | QPS |
|---|---|---|---|---|---|---|---|---|---|
| `bm25` | **1.000** | 0.100 | **1.000** | **1.000** | 3.11x | 6.80ms | 6.80ms | 6.80ms | **147** |
| `binary` | **1.000** | 0.100 | **0.333** | **0.500** | 1.38x | 1.95ms | 1.95ms | 1.95ms | **513** |
| `ppr` | **1.000** | 0.100 | **1.000** | **1.000** | 43.95x | 7.93ms | 7.93ms | 7.93ms | **126** |
| `fast` | **1.000** | 0.100 | **1.000** | **1.000** | 3.30x | 9.75ms | 9.75ms | 9.75ms | **103** |

### Metric Descriptions
- **Recall@K**: Fraction of ground-truth relevant documents retrieved in the top K.
- **MRR@K**: Mean Reciprocal Rank (1/rank of first relevant result).
- **NDCG@K**: Normalized Discounted Cumulative Gain accounting for graded relevance.
- **Sep Ratio**: Score separation between top-1 hit and bottom top-K hit (confidence margin).
- **QPS**: Sustained queries per second.

