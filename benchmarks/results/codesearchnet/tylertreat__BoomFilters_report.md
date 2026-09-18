# ctxvault Retrieval & Indexing Benchmark Report

- **Benchmark**: `codesearchnet`
- **Repository**: `tylertreat/BoomFilters`
- **Evaluated Queries**: 1
- **Evaluation Cutoff**: K = 10

## 2. Retrieval Algorithm Quality & Latency Ablation

All IR metrics computed at cutoff **K = 10**:

| Mode | Recall@K | Precision@K | MRR@K | NDCG@K | Sep Ratio | Latency p50 | Latency p95 | Latency p99 | QPS |
|---|---|---|---|---|---|---|---|---|---|
| `bm25` | **1.000** | 0.100 | **1.000** | **1.000** | 1.56x | 5.60ms | 5.60ms | 5.60ms | **179** |
| `binary` | **1.000** | 0.100 | **1.000** | **1.000** | 1.20x | 0.56ms | 0.56ms | 0.56ms | **1781** |
| `ppr` | **1.000** | 0.100 | **1.000** | **1.000** | 21.62x | 5.31ms | 5.31ms | 5.31ms | **188** |
| `fast` | **1.000** | 0.100 | **1.000** | **1.000** | 1.72x | 7.97ms | 7.97ms | 7.97ms | **125** |

### Metric Descriptions
- **Recall@K**: Fraction of ground-truth relevant documents retrieved in the top K.
- **MRR@K**: Mean Reciprocal Rank (1/rank of first relevant result).
- **NDCG@K**: Normalized Discounted Cumulative Gain accounting for graded relevance.
- **Sep Ratio**: Score separation between top-1 hit and bottom top-K hit (confidence margin).
- **QPS**: Sustained queries per second.

