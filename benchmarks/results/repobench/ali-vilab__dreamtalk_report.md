# ctxvault Retrieval & Indexing Benchmark Report

- **Benchmark**: `repobench`
- **Repository**: `ali-vilab/dreamtalk`
- **Evaluated Queries**: 1
- **Evaluation Cutoff**: K = 10

## 2. Retrieval Algorithm Quality & Latency Ablation

All IR metrics computed at cutoff **K = 10**:

| Mode | Recall@K | Precision@K | MRR@K | NDCG@K | Sep Ratio | Latency p50 | Latency p95 | Latency p99 | QPS |
|---|---|---|---|---|---|---|---|---|---|
| `bm25` | **1.000** | 0.100 | **0.250** | **0.431** | 1.83x | 8.49ms | 8.49ms | 8.49ms | **118** |
| `binary` | **1.000** | 0.100 | **0.333** | **0.500** | 1.19x | 0.81ms | 0.81ms | 0.81ms | **1227** |
| `ppr` | **1.000** | 0.100 | **0.500** | **0.631** | 42.84x | 6.69ms | 6.69ms | 6.69ms | **149** |
| `fast` | **1.000** | 0.100 | **0.500** | **0.631** | 1.81x | 11.03ms | 11.03ms | 11.03ms | **91** |

### Metric Descriptions
- **Recall@K**: Fraction of ground-truth relevant documents retrieved in the top K.
- **MRR@K**: Mean Reciprocal Rank (1/rank of first relevant result).
- **NDCG@K**: Normalized Discounted Cumulative Gain accounting for graded relevance.
- **Sep Ratio**: Score separation between top-1 hit and bottom top-K hit (confidence margin).
- **QPS**: Sustained queries per second.

