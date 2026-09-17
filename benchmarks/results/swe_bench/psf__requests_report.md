# ctxvault Retrieval & Indexing Benchmark Report

- **Benchmark**: `swe_bench`
- **Repository**: `psf/requests`
- **Evaluated Queries**: 6
- **Evaluation Cutoff**: K = 10

## 2. Retrieval Algorithm Quality & Latency Ablation

All IR metrics computed at cutoff **K = 10**:

| Mode | Recall@K | Precision@K | MRR@K | NDCG@K | Sep Ratio | Latency p50 | Latency p95 | Latency p99 | QPS |
|---|---|---|---|---|---|---|---|---|---|
| `bm25` | **0.800** | 0.080 | **0.540** | **0.604** | 1.40x | 12.79ms | 24.30ms | 25.47ms | **63** |
| `binary` | **0.833** | 0.083 | **0.321** | **0.437** | 1.11x | 0.43ms | 1.59ms | 1.86ms | **1514** |
| `ppr` | **1.000** | 0.100 | **0.507** | **0.630** | 83.38x | 28.70ms | 38.43ms | 39.58ms | **35** |
| `fast` | **1.000** | 0.100 | **0.500** | **0.624** | 1.20x | 25.82ms | 53.70ms | 57.02ms | **31** |

### Metric Descriptions
- **Recall@K**: Fraction of ground-truth relevant documents retrieved in the top K.
- **MRR@K**: Mean Reciprocal Rank (1/rank of first relevant result).
- **NDCG@K**: Normalized Discounted Cumulative Gain accounting for graded relevance.
- **Sep Ratio**: Score separation between top-1 hit and bottom top-K hit (confidence margin).
- **QPS**: Sustained queries per second.

