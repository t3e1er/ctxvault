# ctxvault Retrieval & Indexing Benchmark Report

- **Benchmark**: `repobench`
- **Repository**: `jiawei-ren/dreamgaussian4d`
- **Evaluated Queries**: 1
- **Evaluation Cutoff**: K = 10

## 2. Retrieval Algorithm Quality & Latency Ablation

All IR metrics computed at cutoff **K = 10**:

| Mode | Recall@K | Precision@K | MRR@K | NDCG@K | Sep Ratio | Latency p50 | Latency p95 | Latency p99 | QPS |
|---|---|---|---|---|---|---|---|---|---|
| `bm25` | **0.000** | 0.000 | **0.000** | **0.000** | 1.28x | 53.34ms | 53.34ms | 53.34ms | **19** |
| `binary` | **0.000** | 0.000 | **0.000** | **0.000** | 1.04x | 1.01ms | 1.01ms | 1.01ms | **988** |
| `ppr` | **0.000** | 0.000 | **0.000** | **0.000** | 4.47x | 88.00ms | 88.00ms | 88.00ms | **11** |
| `fast` | **0.000** | 0.000 | **0.000** | **0.000** | 1.19x | 90.14ms | 90.14ms | 90.14ms | **11** |

### Metric Descriptions
- **Recall@K**: Fraction of ground-truth relevant documents retrieved in the top K.
- **MRR@K**: Mean Reciprocal Rank (1/rank of first relevant result).
- **NDCG@K**: Normalized Discounted Cumulative Gain accounting for graded relevance.
- **Sep Ratio**: Score separation between top-1 hit and bottom top-K hit (confidence margin).
- **QPS**: Sustained queries per second.

