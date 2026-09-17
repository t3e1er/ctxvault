# ctxvault Retrieval & Indexing Benchmark Report

- **Benchmark**: `repobench`
- **Repository**: `jianchang512/vocal-separate`
- **Evaluated Queries**: 1
- **Evaluation Cutoff**: K = 10

## 2. Retrieval Algorithm Quality & Latency Ablation

All IR metrics computed at cutoff **K = 10**:

| Mode | Recall@K | Precision@K | MRR@K | NDCG@K | Sep Ratio | Latency p50 | Latency p95 | Latency p99 | QPS |
|---|---|---|---|---|---|---|---|---|---|
| `bm25` | **1.000** | 0.100 | **1.000** | **1.000** | 3.11x | 12.20ms | 12.20ms | 12.20ms | **82** |
| `binary` | **0.000** | 0.000 | **0.000** | **0.000** | 1.10x | 1.07ms | 1.07ms | 1.07ms | **933** |
| `ppr` | **1.000** | 0.100 | **1.000** | **1.000** | 43.95x | 9.60ms | 9.60ms | 9.60ms | **104** |
| `fast` | **1.000** | 0.100 | **0.500** | **0.631** | 1.59x | 14.40ms | 14.40ms | 14.40ms | **69** |

### Metric Descriptions
- **Recall@K**: Fraction of ground-truth relevant documents retrieved in the top K.
- **MRR@K**: Mean Reciprocal Rank (1/rank of first relevant result).
- **NDCG@K**: Normalized Discounted Cumulative Gain accounting for graded relevance.
- **Sep Ratio**: Score separation between top-1 hit and bottom top-K hit (confidence margin).
- **QPS**: Sustained queries per second.

