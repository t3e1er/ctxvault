# ctxvault Retrieval & Indexing Benchmark Report

- **Benchmark**: `codesearchnet`
- **Repository**: `grokify/gotilla`
- **Evaluated Queries**: 2
- **Evaluation Cutoff**: K = 10

## 2. Retrieval Algorithm Quality & Latency Ablation

All IR metrics computed at cutoff **K = 10**:

| Mode | Recall@K | Precision@K | MRR@K | NDCG@K | Sep Ratio | Latency p50 | Latency p95 | Latency p99 | QPS |
|---|---|---|---|---|---|---|---|---|---|
| `bm25` | **0.000** | 0.000 | **0.000** | **0.000** | 1.52x | 93.26ms | 108.39ms | 109.74ms | **11** |
| `binary` | **0.000** | 0.000 | **0.000** | **0.000** | 1.08x | 1.06ms | 1.28ms | 1.30ms | **942** |
| `ppr` | **0.500** | 0.050 | **0.250** | **0.315** | 26.43x | 87.68ms | 106.06ms | 107.70ms | **11** |
| `fast` | **0.500** | 0.050 | **0.056** | **0.151** | 1.19x | 90.59ms | 108.85ms | 110.47ms | **11** |

### Metric Descriptions
- **Recall@K**: Fraction of ground-truth relevant documents retrieved in the top K.
- **MRR@K**: Mean Reciprocal Rank (1/rank of first relevant result).
- **NDCG@K**: Normalized Discounted Cumulative Gain accounting for graded relevance.
- **Sep Ratio**: Score separation between top-1 hit and bottom top-K hit (confidence margin).
- **QPS**: Sustained queries per second.

