# ctxvault Retrieval & Indexing Benchmark Report

- **Benchmark**: `codesearchnet`
- **Repository**: `tylertreat/BoomFilters`
- **Evaluated Queries**: 1
- **Evaluation Cutoff**: K = 10

## 2. Retrieval Algorithm Quality & Latency Ablation

All IR metrics computed at cutoff **K = 10**:

| Mode | Recall@K | Precision@K | MRR@K | NDCG@K | Sep Ratio | Latency p50 | Latency p95 | Latency p99 | QPS |
|---|---|---|---|---|---|---|---|---|---|
| `bm25` | **1.000** | 0.100 | **1.000** | **1.000** | 1.59x | 5.70ms | 5.70ms | 5.70ms | **176** |
| `binary` | **1.000** | 0.100 | **1.000** | **1.000** | 1.20x | 0.10ms | 0.10ms | 0.10ms | **9891** |
| `ppr` | **1.000** | 0.100 | **1.000** | **1.000** | 21.76x | 3.03ms | 3.03ms | 3.03ms | **330** |
| `fast` | **1.000** | 0.100 | **1.000** | **1.000** | 1.17x | 6.76ms | 6.76ms | 6.76ms | **148** |

### Metric Descriptions
- **Recall@K**: Fraction of ground-truth relevant documents retrieved in the top K.
- **MRR@K**: Mean Reciprocal Rank (1/rank of first relevant result).
- **NDCG@K**: Normalized Discounted Cumulative Gain accounting for graded relevance.
- **Sep Ratio**: Score separation between top-1 hit and bottom top-K hit (confidence margin).
- **QPS**: Sustained queries per second.

