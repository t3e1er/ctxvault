# ctxvault Retrieval & Indexing Benchmark Report

- **Benchmark**: `swe_bench`
- **Repository**: `psf/requests`
- **Evaluated Queries**: 6
- **Evaluation Cutoff**: K = 10

## 2. Retrieval Algorithm Quality & Latency Ablation

All IR metrics computed at cutoff **K = 10**:

| Mode | Recall@K | Precision@K | MRR@K | NDCG@K | Sep Ratio | Latency p50 | Latency p95 | Latency p99 | QPS |
|---|---|---|---|---|---|---|---|---|---|
| `bm25` | **0.800** | 0.080 | **0.540** | **0.604** | 1.46x | 23.00ms | 28.38ms | 29.18ms | **42** |
| `binary` | **0.833** | 0.083 | **0.275** | **0.399** | 1.16x | 2.85ms | 6.07ms | 6.69ms | **287** |
| `ppr` | **0.800** | 0.080 | **0.333** | **0.452** | 133.91x | 19.03ms | 29.65ms | 30.64ms | **48** |
| `fast` | **1.000** | 0.100 | **0.383** | **0.539** | 1.74x | 34.78ms | 42.70ms | 43.96ms | **29** |

### Metric Descriptions
- **Recall@K**: Fraction of ground-truth relevant documents retrieved in the top K.
- **MRR@K**: Mean Reciprocal Rank (1/rank of first relevant result).
- **NDCG@K**: Normalized Discounted Cumulative Gain accounting for graded relevance.
- **Sep Ratio**: Score separation between top-1 hit and bottom top-K hit (confidence margin).
- **QPS**: Sustained queries per second.

